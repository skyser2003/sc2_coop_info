use super::*;

#[derive(Debug)]
pub(super) struct ReplayVisualTimelineBuilder {
    input: ReplayVisualBuildInput,
    dictionaries: ReplayVisualDictionaries,
    unit_id_by_tag_index: BTreeMap<i64, i64>,
    selected_unit_ids_by_user_id: HashMap<i64, Vec<i64>>,
    pending_deep_tunnel_targets: Vec<ReplayVisualPendingDeepTunnelTarget>,
    last_tychus_medivac_passenger_unit_ids_by_user_id: HashMap<i64, Vec<i64>>,
    pending_tychus_medivac_targets: Vec<ReplayVisualPendingTeleportTarget>,
    live_units: BTreeMap<i64, ReplayVisualLiveUnit>,
    frames: Vec<ReplayVisualFrame>,
    assaults: Vec<ReplayVisualAssault>,
    assault_draft: Option<ReplayVisualAssaultDraft>,
    next_frame_loop: i64,
    last_game_loop: i64,
    frame_dirty: bool,
}

impl ReplayVisualPoint {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance_to(self, other: ReplayVisualPoint) -> f64 {
        let x_delta = self.x - other.x;
        let y_delta = self.y - other.y;
        (x_delta * x_delta + y_delta * y_delta).sqrt()
    }
}

impl ReplayVisualPendingTeleportTarget {
    fn point(self) -> ReplayVisualPoint {
        ReplayVisualPoint::new(self.x, self.y)
    }
}

impl ReplayVisualPendingDeepTunnelTarget {
    fn point(&self) -> ReplayVisualPoint {
        ReplayVisualPoint::new(self.x, self.y)
    }

    fn has_candidate_unit(&self, unit_id: i64) -> bool {
        self.candidate_unit_ids.contains(&unit_id)
    }
}

impl ReplayVisualLiveUnit {
    fn from_event(
        event: &TrackerEvent,
        unit_id: i64,
        dictionaries: &ReplayVisualDictionaries,
        main_player_id: i64,
    ) -> Self {
        let unit_type = event.m_unit_type_name.clone().unwrap_or_default();
        let display_name = dictionaries.display_name(unit_type.as_str());
        let owner_player_id = event.m_control_player_id.unwrap_or_default();
        let owner_kind = ReplayVisualOps::owner_kind(owner_player_id, main_player_id, dictionaries);
        let group = ReplayVisualOps::unit_group(
            unit_type.as_str(),
            display_name.as_str(),
            owner_kind,
            dictionaries,
        );
        let radius = ReplayVisualOps::unit_radius(group);
        Self {
            id: unit_id,
            tag_index: event.m_unit_tag_index.unwrap_or_default(),
            unit_type,
            display_name,
            owner_player_id,
            owner_kind,
            group,
            x: event.m_x.unwrap_or_default() as f64,
            y: event.m_y.unwrap_or_default() as f64,
            radius,
            interpolate_from_previous: true,
            teleport_target: None,
        }
    }

    fn set_unit_type(&mut self, unit_type: String, dictionaries: &ReplayVisualDictionaries) {
        self.unit_type = unit_type;
        self.display_name = dictionaries.display_name(self.unit_type.as_str());
        self.group = ReplayVisualOps::unit_group(
            self.unit_type.as_str(),
            self.display_name.as_str(),
            self.owner_kind,
            dictionaries,
        );
        self.radius = ReplayVisualOps::unit_radius(self.group);
    }

    fn set_owner(
        &mut self,
        owner_player_id: i64,
        dictionaries: &ReplayVisualDictionaries,
        main_player_id: i64,
    ) {
        self.owner_player_id = owner_player_id;
        self.owner_kind =
            ReplayVisualOps::owner_kind(owner_player_id, main_player_id, dictionaries);
        self.group = ReplayVisualOps::unit_group(
            self.unit_type.as_str(),
            self.display_name.as_str(),
            self.owner_kind,
            dictionaries,
        );
        self.radius = ReplayVisualOps::unit_radius(self.group);
    }

    fn set_position(&mut self, x: i64, y: i64) -> bool {
        let position = ReplayVisualPoint::new(x as f64, y as f64);
        if let Some(target) = self.teleport_target {
            if target.distance_to(position) > TELEPORT_TRACKER_ACCEPT_DISTANCE {
                return false;
            }
            self.teleport_target = None;
        }
        self.x = x as f64;
        self.y = y as f64;
        self.interpolate_from_previous = true;
        true
    }

    fn set_snap_position(&mut self, x: i64, y: i64) {
        self.x = x as f64;
        self.y = y as f64;
        self.interpolate_from_previous = false;
        self.teleport_target = None;
    }

    fn set_teleport_position(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        self.interpolate_from_previous = false;
        self.teleport_target = Some(ReplayVisualPoint::new(x, y));
    }

    fn set_command_movement_position(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        self.interpolate_from_previous = true;
        self.teleport_target = Some(ReplayVisualPoint::new(x, y));
    }

    fn as_payload(&self) -> ReplayVisualUnit {
        ReplayVisualUnit {
            id: self.id.to_string(),
            unit_type: self.unit_type.clone(),
            display_name: self.display_name.clone(),
            owner_player_id: self.owner_player_id,
            owner_kind: self.owner_kind,
            group: self.group,
            x: self.x,
            y: self.y,
            radius: self.radius,
            interpolate_from_previous: self.interpolate_from_previous,
        }
    }
}

impl ReplayVisualTimelineBuilder {
    pub(super) fn new(
        input: ReplayVisualBuildInput,
        dictionaries: ReplayVisualDictionaries,
    ) -> Self {
        Self {
            input,
            dictionaries,
            unit_id_by_tag_index: BTreeMap::new(),
            selected_unit_ids_by_user_id: HashMap::new(),
            pending_deep_tunnel_targets: Vec::new(),
            last_tychus_medivac_passenger_unit_ids_by_user_id: HashMap::new(),
            pending_tychus_medivac_targets: Vec::new(),
            live_units: BTreeMap::new(),
            frames: Vec::new(),
            assaults: Vec::new(),
            assault_draft: None,
            next_frame_loop: 0,
            last_game_loop: 0,
            frame_dirty: false,
        }
    }

    pub(super) fn process_events(mut self, events: &[ReplayEvent]) -> ReplayVisualPayload {
        let mut current_game_loop = None;
        for event in events {
            let event_game_loop = ReplayVisualOps::event_game_loop(event);
            match current_game_loop {
                Some(active_game_loop) if active_game_loop != event_game_loop => {
                    self.capture_frames_through_loop(active_game_loop);
                    self.capture_frames_before_loop(event_game_loop);
                    current_game_loop = Some(event_game_loop);
                }
                None => {
                    self.align_first_frame_loop(event_game_loop);
                    current_game_loop = Some(event_game_loop);
                }
                Some(_) => {}
            }

            self.last_game_loop = self.last_game_loop.max(event_game_loop);
            match event {
                ReplayEvent::Tracker(tracker) => self.process_tracker_event(tracker),
                ReplayEvent::Game(game) => self.process_game_event(game),
            }
        }
        if let Some(active_game_loop) = current_game_loop {
            self.capture_frames_through_loop(active_game_loop);
        }
        self.finalize_assault_draft();
        self.capture_final_frame();
        self.into_payload()
    }

    fn process_tracker_event(&mut self, event: &TrackerEvent) {
        match ReplayVisualOps::tracker_event_kind(event.event.as_str()) {
            ReplayVisualTrackerEventKind::UnitBornOrInit => self.handle_unit_born_or_init(event),
            ReplayVisualTrackerEventKind::UnitTypeChange => self.handle_unit_type_change(event),
            ReplayVisualTrackerEventKind::UnitOwnerChange => self.handle_unit_owner_change(event),
            ReplayVisualTrackerEventKind::UnitPositions => self.handle_unit_positions(event),
            ReplayVisualTrackerEventKind::UnitDied => self.handle_unit_died(event),
            ReplayVisualTrackerEventKind::Other => {}
        }
    }

    fn process_game_event(&mut self, event: &GameEvent) {
        match event.event.as_str() {
            "NNet.Game.SCmdEvent" => self.handle_game_command(event),
            "NNet.Game.SSelectionDeltaEvent" => self.handle_selection_delta(event),
            _ => {}
        }
    }

    fn handle_selection_delta(&mut self, event: &GameEvent) {
        let Some(user_id) = event.user_id else {
            return;
        };
        let Some(delta) = event.m_delta.as_ref() else {
            return;
        };
        let mut selected_unit_ids = self
            .selected_unit_ids_by_user_id
            .remove(&user_id)
            .unwrap_or_default();
        ReplayVisualOps::apply_selection_remove_mask(&mut selected_unit_ids, &delta.m_remove_mask);
        for unit_id in delta
            .m_add_unit_tags
            .iter()
            .filter_map(|unit_tag| ReplayVisualOps::unit_id_from_game_unit_tag(*unit_tag))
        {
            if !selected_unit_ids.contains(&unit_id) {
                selected_unit_ids.push(unit_id);
            }
        }

        if selected_unit_ids.is_empty() {
            return;
        }
        self.remember_tychus_medivac_passenger_selection(user_id, &selected_unit_ids);
        self.selected_unit_ids_by_user_id
            .insert(user_id, selected_unit_ids);
    }

    fn handle_game_command(&mut self, event: &GameEvent) {
        let Some(ability_link) = event.m_abil.as_ref().map(|ability| ability.m_abilLink) else {
            return;
        };
        let Some((x, y)) = ReplayVisualOps::game_event_target_point(event) else {
            return;
        };
        let Some(control_player_id) = event.user_id.map(|user_id| user_id + 1) else {
            return;
        };

        if ability_link == ABATHUR_DEEP_TUNNEL_ABILITY_LINK {
            let candidate_unit_ids = self.deep_tunnel_candidate_unit_ids(event, control_player_id);
            if candidate_unit_ids.len() == 1 {
                self.capture_frame(event.game_loop);
                self.set_deep_tunnel_unit(candidate_unit_ids[0], x, y);
                self.frame_dirty = true;
                self.next_frame_loop = event.game_loop + ABATHUR_DEEP_TUNNEL_TRAVEL_GAME_LOOPS;
                self.last_game_loop = self.last_game_loop.max(self.next_frame_loop);
            } else if !candidate_unit_ids.is_empty() {
                self.remember_pending_deep_tunnel_target(
                    event.game_loop,
                    control_player_id,
                    x,
                    y,
                    candidate_unit_ids,
                );
            }
            return;
        }

        let changed = if ReplayVisualOps::is_tychus_medivac_ability_link(ability_link) {
            self.remember_pending_tychus_medivac_target(event.game_loop, control_player_id, x, y);
            self.set_selected_tychus_medivac_passenger_units(event, control_player_id, x, y)
        } else {
            false
        };
        if changed {
            self.frame_dirty = true;
            self.capture_frame(event.game_loop);
        }
    }

    fn deep_tunnel_candidate_unit_ids(
        &self,
        event: &GameEvent,
        control_player_id: i64,
    ) -> Vec<i64> {
        let selected_unit_ids = event
            .user_id
            .and_then(|user_id| self.selected_unit_ids_by_user_id.get(&user_id))
            .cloned();
        if let Some(selected_unit_ids) = selected_unit_ids.as_ref() {
            let selected = self.selected_deep_tunnel_unit_ids(selected_unit_ids, control_player_id);
            if !selected.is_empty() {
                return selected;
            }
        } else {
            return self.owned_deep_tunnel_unit_ids(control_player_id);
        }
        self.owned_deep_tunnel_unit_ids(control_player_id)
    }

    fn selected_deep_tunnel_unit_ids(
        &self,
        selected_unit_ids: &[i64],
        control_player_id: i64,
    ) -> Vec<i64> {
        selected_unit_ids
            .iter()
            .copied()
            .filter(|unit_id| {
                self.live_units.get(unit_id).is_some_and(|live_unit| {
                    live_unit.owner_player_id == control_player_id
                        && ReplayVisualOps::is_deep_tunnel_unit(live_unit.unit_type.as_str())
                })
            })
            .collect()
    }

    fn owned_deep_tunnel_unit_ids(&self, control_player_id: i64) -> Vec<i64> {
        self.live_units
            .iter()
            .filter_map(|(unit_id, live_unit)| {
                (live_unit.owner_player_id == control_player_id
                    && ReplayVisualOps::is_deep_tunnel_unit(live_unit.unit_type.as_str()))
                .then_some(*unit_id)
            })
            .collect()
    }

    fn set_deep_tunnel_unit(&mut self, unit_id: i64, x: f64, y: f64) {
        let Some(live_unit) = self.live_units.get_mut(&unit_id) else {
            return;
        };
        live_unit.set_command_movement_position(x, y);
    }

    fn remember_pending_deep_tunnel_target(
        &mut self,
        game_loop: i64,
        owner_player_id: i64,
        x: f64,
        y: f64,
        candidate_unit_ids: Vec<i64>,
    ) {
        self.prune_pending_deep_tunnel_targets(game_loop);
        self.pending_deep_tunnel_targets
            .push(ReplayVisualPendingDeepTunnelTarget {
                game_loop,
                owner_player_id,
                x,
                y,
                candidate_unit_ids,
            });
    }

    fn prune_pending_deep_tunnel_targets(&mut self, game_loop: i64) {
        self.pending_deep_tunnel_targets.retain(|target| {
            game_loop.saturating_sub(target.game_loop)
                <= ABATHUR_DEEP_TUNNEL_PENDING_TARGET_GAME_LOOPS
        });
    }

    fn remember_tychus_medivac_passenger_selection(
        &mut self,
        user_id: i64,
        selected_unit_ids: &[i64],
    ) {
        let control_player_id = user_id + 1;
        let passenger_unit_ids =
            self.tychus_medivac_passenger_unit_ids(selected_unit_ids, control_player_id);
        if !passenger_unit_ids.is_empty() {
            self.last_tychus_medivac_passenger_unit_ids_by_user_id
                .insert(user_id, passenger_unit_ids);
        }
    }

    fn tychus_medivac_passenger_unit_ids(
        &self,
        selected_unit_ids: &[i64],
        control_player_id: i64,
    ) -> Vec<i64> {
        selected_unit_ids
            .iter()
            .copied()
            .filter(|unit_id| {
                self.live_units.get(unit_id).is_some_and(|live_unit| {
                    live_unit.owner_player_id == control_player_id
                        && ReplayVisualOps::is_tychus_medivac_passenger_unit(live_unit)
                })
            })
            .collect()
    }

    fn tychus_medivac_candidate_unit_ids(
        &self,
        user_id: i64,
        control_player_id: i64,
    ) -> Option<Vec<i64>> {
        let selected_unit_ids = self.selected_unit_ids_by_user_id.get(&user_id)?;
        let passenger_unit_ids =
            self.tychus_medivac_passenger_unit_ids(selected_unit_ids, control_player_id);
        if !passenger_unit_ids.is_empty() {
            return Some(passenger_unit_ids);
        }
        if !self.selection_contains_tychus_medivac_proxy(selected_unit_ids) {
            return None;
        }
        self.last_tychus_medivac_passenger_unit_ids_by_user_id
            .get(&user_id)
            .map(|cached_unit_ids| {
                self.tychus_medivac_passenger_unit_ids(cached_unit_ids, control_player_id)
            })
            .filter(|cached_unit_ids| !cached_unit_ids.is_empty())
    }

    fn selection_contains_tychus_medivac_proxy(&self, selected_unit_ids: &[i64]) -> bool {
        selected_unit_ids.iter().any(|unit_id| {
            self.live_units
                .get(unit_id)
                .is_some_and(ReplayVisualOps::is_tychus_medivac_proxy_unit)
        })
    }

    fn set_selected_tychus_medivac_passenger_units(
        &mut self,
        event: &GameEvent,
        control_player_id: i64,
        x: f64,
        y: f64,
    ) -> bool {
        let Some(user_id) = event.user_id else {
            return false;
        };
        let Some(unit_ids) = self.tychus_medivac_candidate_unit_ids(user_id, control_player_id)
        else {
            return false;
        };
        let mut changed = false;
        for unit_id in unit_ids {
            let Some(live_unit) = self.live_units.get_mut(&unit_id) else {
                continue;
            };
            if live_unit.owner_player_id != control_player_id
                || !ReplayVisualOps::is_tychus_medivac_passenger_unit(live_unit)
            {
                continue;
            }
            live_unit.set_teleport_position(x, y);
            changed = true;
        }
        changed
    }

    fn remember_pending_tychus_medivac_target(
        &mut self,
        game_loop: i64,
        owner_player_id: i64,
        x: f64,
        y: f64,
    ) {
        self.prune_pending_tychus_medivac_targets(game_loop);
        self.pending_tychus_medivac_targets
            .push(ReplayVisualPendingTeleportTarget {
                game_loop,
                owner_player_id,
                x,
                y,
            });
    }

    fn prune_pending_tychus_medivac_targets(&mut self, game_loop: i64) {
        self.pending_tychus_medivac_targets.retain(|target| {
            game_loop.saturating_sub(target.game_loop) <= TYCHUS_MEDIVAC_PENDING_TARGET_GAME_LOOPS
        });
    }

    fn pending_tychus_medivac_tracker_target(
        &self,
        game_loop: i64,
        live_unit: &ReplayVisualLiveUnit,
        new_position: ReplayVisualPoint,
    ) -> Option<ReplayVisualPendingTeleportTarget> {
        if !ReplayVisualOps::is_tychus_medivac_passenger_unit(live_unit) {
            return None;
        }
        let previous_position = ReplayVisualPoint::new(live_unit.x, live_unit.y);
        if previous_position.distance_to(new_position) < TYCHUS_MEDIVAC_TRACKER_MIN_DISTANCE {
            return None;
        }
        self.pending_tychus_medivac_targets
            .iter()
            .rev()
            .copied()
            .find(|target| {
                target.owner_player_id == live_unit.owner_player_id
                    && game_loop >= target.game_loop
                    && game_loop.saturating_sub(target.game_loop)
                        <= TYCHUS_MEDIVAC_PENDING_TARGET_GAME_LOOPS
                    && target.point().distance_to(new_position)
                        <= TYCHUS_MEDIVAC_TRACKER_ACCEPT_DISTANCE
            })
    }

    fn handle_unit_born_or_init(&mut self, event: &TrackerEvent) {
        let Some(unit_id) = ReplayVisualOps::replay_event_unit_id(event) else {
            return;
        };
        let live_unit = ReplayVisualLiveUnit::from_event(
            event,
            unit_id,
            &self.dictionaries,
            self.input.main_player_id,
        );
        if self.dictionaries.should_omit_unit(
            live_unit.unit_type.as_str(),
            live_unit.display_name.as_str(),
        ) {
            return;
        }
        if let Some(tag_index) = event.m_unit_tag_index {
            self.unit_id_by_tag_index.insert(tag_index, unit_id);
        }
        self.track_assault_unit(event.game_loop, &live_unit);
        self.live_units.insert(unit_id, live_unit);
        self.frame_dirty = true;
    }

    fn handle_unit_type_change(&mut self, event: &TrackerEvent) {
        let Some(unit_id) = ReplayVisualOps::replay_event_unit_id(event) else {
            return;
        };
        let Some(unit_type) = event.m_unit_type_name.clone() else {
            return;
        };
        let mut should_remove = false;
        if let Some(live_unit) = self.live_units.get_mut(&unit_id) {
            live_unit.set_unit_type(unit_type, &self.dictionaries);
            should_remove = self.dictionaries.should_omit_unit(
                live_unit.unit_type.as_str(),
                live_unit.display_name.as_str(),
            );
            self.frame_dirty = true;
        }
        if should_remove {
            self.remove_live_unit(unit_id);
        }
    }

    fn handle_unit_owner_change(&mut self, event: &TrackerEvent) {
        let Some(unit_id) = ReplayVisualOps::replay_event_unit_id(event) else {
            return;
        };
        let Some(owner_player_id) = event.m_control_player_id else {
            return;
        };
        if let Some(live_unit) = self.live_units.get_mut(&unit_id) {
            live_unit.set_owner(
                owner_player_id,
                &self.dictionaries,
                self.input.main_player_id,
            );
            self.frame_dirty = true;
        }
    }

    fn handle_unit_positions(&mut self, event: &TrackerEvent) {
        let Some(mut tag_index) = event.m_first_unit_index else {
            return;
        };
        self.prune_pending_deep_tunnel_targets(event.game_loop);
        self.prune_pending_tychus_medivac_targets(event.game_loop);
        for chunk in event.m_position_items.chunks_exact(3) {
            tag_index += chunk[0];
            let Some(unit_id) = self.unit_id_by_tag_index.get(&tag_index).copied() else {
                continue;
            };
            let new_position = ReplayVisualPoint::new(chunk[1] as f64, chunk[2] as f64);
            let deep_tunnel_tracker_target =
                self.live_units
                    .get(&unit_id)
                    .cloned()
                    .and_then(|live_unit| {
                        let start_unit = ReplayVisualOps::should_render_unit(
                            &live_unit,
                            &self.input,
                            &self.dictionaries,
                        )
                        .then(|| live_unit.as_payload());
                        self.take_pending_deep_tunnel_tracker_target(
                            event.game_loop,
                            unit_id,
                            &live_unit,
                            new_position,
                        )
                        .zip(start_unit)
                    });
            let medivac_tracker_target = self.live_units.get(&unit_id).and_then(|live_unit| {
                self.pending_tychus_medivac_tracker_target(event.game_loop, live_unit, new_position)
            });
            if let Some((target, start_unit)) = deep_tunnel_tracker_target.as_ref() {
                self.backpatch_unit_movement_frames(
                    target.game_loop,
                    event.game_loop,
                    start_unit,
                    target.x,
                    target.y,
                );
            }
            let snap_unit = medivac_tracker_target.and_then(|_| {
                self.live_units.get(&unit_id).and_then(|live_unit| {
                    ReplayVisualOps::should_render_unit(live_unit, &self.input, &self.dictionaries)
                        .then(|| {
                            let mut unit = live_unit.as_payload();
                            unit.x = new_position.x;
                            unit.y = new_position.y;
                            unit.interpolate_from_previous = false;
                            unit
                        })
                })
            });
            if let (Some(target), Some(unit)) = (medivac_tracker_target, snap_unit.as_ref()) {
                self.backpatch_unit_snap_frames(target.game_loop, event.game_loop, unit);
            }
            if let Some(live_unit) = self.live_units.get_mut(&unit_id) {
                if deep_tunnel_tracker_target.is_some() {
                    live_unit.set_position(chunk[1], chunk[2]);
                    self.frame_dirty = true;
                } else if medivac_tracker_target.is_some() {
                    live_unit.set_snap_position(chunk[1], chunk[2]);
                    self.frame_dirty = true;
                } else if live_unit.set_position(chunk[1], chunk[2]) {
                    self.frame_dirty = true;
                }
            }
        }
    }

    fn take_pending_deep_tunnel_tracker_target(
        &mut self,
        game_loop: i64,
        unit_id: i64,
        live_unit: &ReplayVisualLiveUnit,
        new_position: ReplayVisualPoint,
    ) -> Option<ReplayVisualPendingDeepTunnelTarget> {
        if !ReplayVisualOps::is_deep_tunnel_unit(live_unit.unit_type.as_str()) {
            return None;
        }
        let previous_position = ReplayVisualPoint::new(live_unit.x, live_unit.y);
        if previous_position.distance_to(new_position) < ABATHUR_DEEP_TUNNEL_TRACKER_MIN_DISTANCE {
            return None;
        }

        let mut best_match = None;
        for (index, target) in self.pending_deep_tunnel_targets.iter().enumerate() {
            if target.owner_player_id != live_unit.owner_player_id {
                continue;
            }
            if !target.has_candidate_unit(unit_id) {
                continue;
            }
            if game_loop < target.game_loop {
                continue;
            }
            if game_loop.saturating_sub(target.game_loop)
                > ABATHUR_DEEP_TUNNEL_PENDING_TARGET_GAME_LOOPS
            {
                continue;
            }
            let distance = target.point().distance_to(new_position);
            if distance > TELEPORT_TRACKER_ACCEPT_DISTANCE {
                continue;
            }
            if match best_match {
                Some((_, best_distance)) => distance < best_distance,
                None => true,
            } {
                best_match = Some((index, distance));
            }
        }

        best_match.map(|(index, _)| self.pending_deep_tunnel_targets.remove(index))
    }

    fn backpatch_unit_snap_frames(
        &mut self,
        from_game_loop: i64,
        until_game_loop: i64,
        unit: &ReplayVisualUnit,
    ) {
        if from_game_loop >= until_game_loop {
            return;
        }
        self.ensure_backpatch_frame(from_game_loop, unit);
        for frame in &mut self.frames {
            if frame.game_loop >= from_game_loop && frame.game_loop < until_game_loop {
                Self::replace_or_insert_frame_unit(frame, unit);
            }
        }
    }

    fn backpatch_unit_movement_frames(
        &mut self,
        from_game_loop: i64,
        until_game_loop: i64,
        start_unit: &ReplayVisualUnit,
        arrival_x: f64,
        arrival_y: f64,
    ) {
        if from_game_loop >= until_game_loop {
            return;
        }
        let arrival_game_loop =
            (from_game_loop + ABATHUR_DEEP_TUNNEL_TRAVEL_GAME_LOOPS).min(until_game_loop);
        let mut arrival_unit = start_unit.clone();
        arrival_unit.x = arrival_x;
        arrival_unit.y = arrival_y;
        arrival_unit.interpolate_from_previous = true;

        self.ensure_backpatch_frame(from_game_loop, start_unit);
        self.ensure_backpatch_frame(arrival_game_loop, &arrival_unit);
        for frame in &mut self.frames {
            if frame.game_loop >= from_game_loop && frame.game_loop < arrival_game_loop {
                Self::replace_or_insert_frame_unit(frame, start_unit);
            } else if frame.game_loop >= arrival_game_loop && frame.game_loop < until_game_loop {
                Self::replace_or_insert_frame_unit(frame, &arrival_unit);
            }
        }
    }

    fn ensure_backpatch_frame(&mut self, game_loop: i64, unit: &ReplayVisualUnit) {
        if self.frames.iter().any(|frame| frame.game_loop == game_loop) {
            return;
        }
        let insert_index = self
            .frames
            .iter()
            .position(|frame| frame.game_loop > game_loop)
            .unwrap_or(self.frames.len());
        let units = insert_index
            .checked_sub(1)
            .and_then(|previous_index| self.frames.get(previous_index))
            .map(|frame| frame.units.clone())
            .unwrap_or_default();
        let mut frame = ReplayVisualFrame {
            game_loop,
            seconds: ReplayVisualOps::seconds_from_game_loop(game_loop),
            units,
        };
        Self::replace_or_insert_frame_unit(&mut frame, unit);
        self.frames.insert(insert_index, frame);
    }

    fn replace_or_insert_frame_unit(frame: &mut ReplayVisualFrame, unit: &ReplayVisualUnit) {
        if let Some(existing) = frame
            .units
            .iter_mut()
            .find(|existing| existing.id == unit.id)
        {
            *existing = unit.clone();
            return;
        }
        frame.units.push(unit.clone());
    }

    fn handle_unit_died(&mut self, event: &TrackerEvent) {
        let Some(unit_id) = ReplayVisualOps::replay_event_unit_id(event) else {
            return;
        };
        self.remove_live_unit(unit_id);
    }

    fn remove_live_unit(&mut self, unit_id: i64) {
        if let Some(live_unit) = self.live_units.remove(&unit_id) {
            if self.unit_id_by_tag_index.get(&live_unit.tag_index) == Some(&unit_id) {
                self.unit_id_by_tag_index.remove(&live_unit.tag_index);
            }
            self.frame_dirty = true;
        }
    }

    fn track_assault_unit(&mut self, game_loop: i64, live_unit: &ReplayVisualLiveUnit) {
        if live_unit.owner_kind != ReplayVisualOwnerKind::Amon {
            return;
        }
        if !self.dictionaries.is_wave_unit(live_unit.unit_type.as_str()) {
            return;
        }
        if ReplayVisualOps::seconds_from_game_loop(game_loop) <= ASSAULT_MIN_GAME_SECONDS {
            return;
        }

        match self.assault_draft.as_ref() {
            Some(draft) if draft.game_loop == game_loop => {}
            Some(_) => self.finalize_assault_draft(),
            None => {}
        }

        let draft = self
            .assault_draft
            .get_or_insert_with(|| ReplayVisualAssaultDraft {
                game_loop,
                units: Vec::new(),
            });
        draft.units.push(ReplayVisualAssaultUnit {
            unit_type: live_unit.unit_type.clone(),
            display_name: live_unit.display_name.clone(),
            x: live_unit.x,
            y: live_unit.y,
        });
    }

    fn finalize_assault_draft(&mut self) {
        let Some(draft) = self.assault_draft.take() else {
            return;
        };
        if draft.units.len() < ASSAULT_MIN_UNITS {
            return;
        }

        let mut counts = BTreeMap::<String, (String, u64)>::new();
        let mut x_sum = 0.0_f64;
        let mut y_sum = 0.0_f64;
        for unit in &draft.units {
            let count = counts
                .entry(unit.unit_type.clone())
                .or_insert_with(|| (unit.display_name.clone(), 0));
            count.1 = count.1.saturating_add(1);
            x_sum += unit.x;
            y_sum += unit.y;
        }
        let unit_count = u64::try_from(draft.units.len()).unwrap_or(u64::MAX);
        let divisor = draft.units.len() as f64;
        let units = counts
            .into_iter()
            .map(|(unit_type, (display_name, count))| ReplayVisualUnitCount {
                unit_type,
                display_name,
                count,
            })
            .collect::<Vec<_>>();
        let index = self.assaults.len() + 1;
        self.assaults.push(ReplayVisualAssault {
            id: format!("assault-{}-{index}", draft.game_loop),
            game_loop: draft.game_loop,
            seconds: ReplayVisualOps::seconds_from_game_loop(draft.game_loop),
            x: x_sum / divisor,
            y: y_sum / divisor,
            unit_count,
            units,
        });
    }

    fn align_first_frame_loop(&mut self, game_loop: i64) {
        if self.frames.is_empty() && !self.frame_dirty && self.live_units.is_empty() {
            self.next_frame_loop = game_loop;
        }
    }

    fn capture_frames_before_loop(&mut self, game_loop: i64) {
        if self.frames.is_empty() && !self.frame_dirty && self.live_units.is_empty() {
            self.next_frame_loop = game_loop;
            return;
        }

        while self.next_frame_loop < game_loop {
            self.capture_frame(self.next_frame_loop);
            self.next_frame_loop += FRAME_INTERVAL_GAME_LOOPS;
        }
    }

    fn capture_frames_through_loop(&mut self, game_loop: i64) {
        if self.frames.is_empty() && !self.frame_dirty && self.live_units.is_empty() {
            self.next_frame_loop = game_loop;
            return;
        }

        while self.next_frame_loop <= game_loop {
            self.capture_frame(self.next_frame_loop);
            self.next_frame_loop += FRAME_INTERVAL_GAME_LOOPS;
        }
    }

    fn capture_final_frame(&mut self) {
        if self
            .frames
            .last()
            .is_some_and(|frame| frame.game_loop == self.last_game_loop)
            && !self.frame_dirty
        {
            return;
        }
        self.capture_frame(self.last_game_loop);
    }

    fn capture_frame(&mut self, game_loop: i64) {
        let units = self
            .live_units
            .values()
            .filter(|unit| {
                ReplayVisualOps::should_render_unit(unit, &self.input, &self.dictionaries)
            })
            .map(ReplayVisualLiveUnit::as_payload)
            .collect::<Vec<_>>();
        self.frames.push(ReplayVisualFrame {
            game_loop,
            seconds: ReplayVisualOps::seconds_from_game_loop(game_loop),
            units,
        });
        for live_unit in self.live_units.values_mut() {
            live_unit.interpolate_from_previous = true;
        }
        self.frame_dirty = false;
    }

    fn into_payload(self) -> ReplayVisualPayload {
        ReplayVisualPayload {
            file: self.input.file,
            map: self.input.map,
            result: self.input.result,
            duration_seconds: self.input.duration_seconds,
            map_width: self.input.map_width,
            map_height: self.input.map_height,
            players: self.input.players,
            frames: self.frames,
            assaults: self.assaults,
        }
    }
}
