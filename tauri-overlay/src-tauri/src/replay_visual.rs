use s2coop_analyzer::detailed_replay_analysis::ReplayAnalysisResources;
use s2coop_analyzer::dictionary_data::{Sc2DictionaryData, UnitNamesJson};
use s2protocol_port::{
    GameEvent, ReplayDetails, ReplayEvent, ReplayInitData, ReplayMetadata, ReplayParser,
    SelectionRemoveMask, SnapshotPoint, SnapshotPointValue, TrackerEvent, UnitTag,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use ts_rs::TS;

const FRAME_INTERVAL_GAME_LOOPS: i64 = 16;
const ASSAULT_MIN_GAME_SECONDS: f64 = 60.0;
const ASSAULT_MIN_UNITS: usize = 6;
const GAME_LOOPS_PER_SECOND: f64 = 16.0;
const GAME_POINT_FIXED_SCALE: f64 = 4096.0;
const ABATHUR_DEEP_TUNNEL_ABILITY_LINK: i64 = 2307;
const ABATHUR_DEEP_TUNNEL_TRAVEL_GAME_LOOPS: i64 = 16;
const ABATHUR_DEEP_TUNNEL_PENDING_TARGET_GAME_LOOPS: i64 = 320;
const ABATHUR_DEEP_TUNNEL_TRACKER_MIN_DISTANCE: f64 = 15.0;
const TYCHUS_MEDIVAC_ABILITY_LINKS: [i64; 3] = [3101, 3115, 3125];
const TELEPORT_TRACKER_ACCEPT_DISTANCE: f64 = 8.0;
const TYCHUS_MEDIVAC_TRACKER_ACCEPT_DISTANCE: f64 = 12.0;
const TYCHUS_MEDIVAC_TRACKER_MIN_DISTANCE: f64 = 15.0;
const TYCHUS_MEDIVAC_PENDING_TARGET_GAME_LOOPS: i64 = 320;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../src/bindings/overlay.ts",
    rename_all = "snake_case"
)]
pub enum ReplayVisualOwnerKind {
    Main,
    Ally,
    Amon,
    Neutral,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../src/bindings/overlay.ts",
    rename_all = "snake_case"
)]
pub enum ReplayVisualUnitGroup {
    Buildings,
    AttackUnits,
    DefenseBuildings,
    EnemyAssaults,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayVisualPlayer {
    #[ts(type = "number")]
    pub player_id: i64,
    pub label: String,
    pub owner_kind: ReplayVisualOwnerKind,
    pub color: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayVisualUnit {
    pub id: String,
    pub unit_type: String,
    pub display_name: String,
    #[ts(type = "number")]
    pub owner_player_id: i64,
    pub owner_kind: ReplayVisualOwnerKind,
    pub group: ReplayVisualUnitGroup,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub interpolate_from_previous: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayVisualFrame {
    #[ts(type = "number")]
    pub game_loop: i64,
    pub seconds: f64,
    pub units: Vec<ReplayVisualUnit>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayVisualUnitCount {
    pub unit_type: String,
    pub display_name: String,
    #[ts(type = "number")]
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayVisualAssault {
    pub id: String,
    #[ts(type = "number")]
    pub game_loop: i64,
    pub seconds: f64,
    pub x: f64,
    pub y: f64,
    #[ts(type = "number")]
    pub unit_count: u64,
    pub units: Vec<ReplayVisualUnitCount>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayVisualPayload {
    pub file: String,
    pub map: String,
    pub result: String,
    pub duration_seconds: f64,
    pub map_width: f64,
    pub map_height: f64,
    pub players: Vec<ReplayVisualPlayer>,
    pub frames: Vec<ReplayVisualFrame>,
    pub assaults: Vec<ReplayVisualAssault>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayVisualContext {
    file: String,
    map: String,
    result: String,
    duration_seconds: u64,
    main_player_id: i64,
}

#[derive(Clone, Debug)]
pub struct ReplayVisualDictionaries {
    unit_names: HashMap<String, String>,
    units_in_waves: HashSet<String>,
    amon_player_ids: HashSet<i64>,
    omitted_unit_types: HashSet<String>,
    omitted_unit_type_prefixes: Vec<String>,
    omitted_unit_type_or_name_fragments: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ReplayVisualBuildInput {
    file: String,
    map: String,
    result: String,
    duration_seconds: f64,
    map_width: f64,
    map_height: f64,
    players: Vec<ReplayVisualPlayer>,
    main_player_id: i64,
}

#[derive(Clone, Debug)]
pub struct ReplayVisualReplayInfo {
    file: String,
    map: String,
    result: String,
    duration_seconds: f64,
}

impl ReplayVisualReplayInfo {
    pub fn new(
        file: impl Into<String>,
        map: impl Into<String>,
        result: impl Into<String>,
        duration_seconds: f64,
    ) -> Self {
        Self {
            file: file.into(),
            map: map.into(),
            result: result.into(),
            duration_seconds,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayVisualMapSize {
    width: f64,
    height: f64,
}

impl ReplayVisualMapSize {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Debug)]
struct ReplayVisualLiveUnit {
    id: i64,
    tag_index: i64,
    unit_type: String,
    display_name: String,
    owner_player_id: i64,
    owner_kind: ReplayVisualOwnerKind,
    group: ReplayVisualUnitGroup,
    x: f64,
    y: f64,
    radius: f64,
    interpolate_from_previous: bool,
    teleport_target: Option<ReplayVisualPoint>,
}

#[derive(Clone, Copy, Debug)]
struct ReplayVisualPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Debug)]
struct ReplayVisualAssaultUnit {
    unit_type: String,
    display_name: String,
    x: f64,
    y: f64,
}

#[derive(Clone, Debug)]
struct ReplayVisualAssaultDraft {
    game_loop: i64,
    units: Vec<ReplayVisualAssaultUnit>,
}

#[derive(Clone, Copy, Debug)]
struct ReplayVisualPendingTeleportTarget {
    game_loop: i64,
    owner_player_id: i64,
    x: f64,
    y: f64,
}

#[derive(Clone, Debug)]
struct ReplayVisualPendingDeepTunnelTarget {
    game_loop: i64,
    owner_player_id: i64,
    x: f64,
    y: f64,
    candidate_unit_ids: Vec<i64>,
}

#[derive(Debug)]
struct ReplayVisualTimelineBuilder {
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

pub struct ReplayVisualOps;

impl ReplayVisualContext {
    pub fn new(
        file: impl Into<String>,
        map: impl Into<String>,
        result: impl Into<String>,
        duration_seconds: u64,
        main_player_id: i64,
    ) -> Self {
        Self {
            file: file.into(),
            map: map.into(),
            result: result.into(),
            duration_seconds,
            main_player_id,
        }
    }

    fn file(&self) -> &str {
        self.file.as_str()
    }

    fn map(&self) -> &str {
        self.map.as_str()
    }

    fn result(&self) -> &str {
        self.result.as_str()
    }

    fn duration_seconds(&self) -> u64 {
        self.duration_seconds
    }

    fn main_player_id(&self) -> i64 {
        self.main_player_id
    }
}

impl ReplayVisualDictionaries {
    pub fn new(
        unit_names: HashMap<String, String>,
        units_in_waves: HashSet<String>,
        amon_player_ids: HashSet<i64>,
    ) -> Self {
        Self::new_with_omitted_units(unit_names, units_in_waves, amon_player_ids, HashSet::new())
    }

    pub fn new_with_omitted_units(
        unit_names: HashMap<String, String>,
        units_in_waves: HashSet<String>,
        amon_player_ids: HashSet<i64>,
        omitted_unit_types: HashSet<String>,
    ) -> Self {
        let mut omitted_unit_types = omitted_unit_types;
        omitted_unit_types.extend(Self::visualizer_omitted_unit_types());
        Self {
            unit_names,
            units_in_waves,
            amon_player_ids,
            omitted_unit_types,
            omitted_unit_type_prefixes: Self::visualizer_omitted_unit_type_prefixes(),
            omitted_unit_type_or_name_fragments:
                Self::visualizer_omitted_unit_type_or_name_fragments(),
        }
    }

    fn visualizer_omitted_unit_types() -> HashSet<String> {
        HashSet::from(["CreepTumorStukov".to_string()])
    }

    fn visualizer_omitted_unit_type_prefixes() -> Vec<String> {
        vec![
            "AbathurSymbiote".to_string(),
            "Beacon".to_string(),
            "CoopCaster".to_string(),
            "SOACaster".to_string(),
        ]
    }

    fn visualizer_omitted_unit_type_or_name_fragments() -> Vec<String> {
        vec![
            "cocoon".to_string(),
            "dummy".to_string(),
            "egg".to_string(),
            "larva".to_string(),
            "mineralfield".to_string(),
            "pathingblocker".to_string(),
            "pickup".to_string(),
            "placeholder".to_string(),
            "top bar".to_string(),
            "unbuildable".to_string(),
            "vespenegeyser".to_string(),
        ]
    }

    fn from_dictionary(dictionary: &Sc2DictionaryData, map_name: &str) -> Self {
        let unit_names = Self::clone_unit_names(&dictionary.unit_name_dict);
        let units_in_waves = dictionary.units_in_waves.clone();
        let omitted_unit_types = dictionary
            .replay_analysis_data
            .dont_include_units
            .iter()
            .cloned()
            .collect();
        let mut amon_player_ids = HashSet::from([3_i64, 4_i64]);
        for (mission_name, player_ids) in dictionary.amon_player_ids.iter() {
            if !ReplayVisualOps::map_name_has_amon_override(map_name, mission_name) {
                continue;
            }
            amon_player_ids.extend(player_ids.iter().copied());
            break;
        }

        Self::new_with_omitted_units(
            unit_names,
            units_in_waves,
            amon_player_ids,
            omitted_unit_types,
        )
    }

    fn clone_unit_names(unit_names: &UnitNamesJson) -> HashMap<String, String> {
        unit_names
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn display_name(&self, unit_type: &str) -> String {
        self.unit_names
            .get(unit_type)
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| unit_type.to_string())
    }

    fn is_amon_player(&self, player_id: i64) -> bool {
        self.amon_player_ids.contains(&player_id)
    }

    fn is_wave_unit(&self, unit_type: &str) -> bool {
        self.units_in_waves.contains(unit_type)
    }

    fn should_omit_unit(&self, unit_type: &str, display_name: &str) -> bool {
        if self.omitted_unit_types.contains(unit_type) {
            return true;
        }
        if self
            .omitted_unit_type_prefixes
            .iter()
            .any(|prefix| unit_type.starts_with(prefix))
        {
            return true;
        }
        let lower_type = unit_type.to_ascii_lowercase();
        let lower_name = display_name.to_ascii_lowercase();
        self.omitted_unit_type_or_name_fragments
            .iter()
            .any(|fragment| lower_type.contains(fragment) || lower_name.contains(fragment))
    }
}

impl ReplayVisualBuildInput {
    pub fn new(
        replay: ReplayVisualReplayInfo,
        map_size: ReplayVisualMapSize,
        players: Vec<ReplayVisualPlayer>,
        main_player_id: i64,
    ) -> Self {
        Self {
            file: replay.file,
            map: replay.map,
            result: replay.result,
            duration_seconds: replay.duration_seconds,
            map_width: map_size.width,
            map_height: map_size.height,
            players,
            main_player_id,
        }
    }
}

impl ReplayVisualPlayer {
    fn new(player_id: i64, label: impl Into<String>, owner_kind: ReplayVisualOwnerKind) -> Self {
        Self {
            player_id,
            label: label.into(),
            owner_kind,
            color: ReplayVisualOps::owner_color(owner_kind).to_string(),
        }
    }
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
    fn new(input: ReplayVisualBuildInput, dictionaries: ReplayVisualDictionaries) -> Self {
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

    fn process_events(mut self, events: &[ReplayEvent]) -> ReplayVisualPayload {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplayVisualTrackerEventKind {
    UnitBornOrInit,
    UnitTypeChange,
    UnitOwnerChange,
    UnitPositions,
    UnitDied,
    Other,
}

impl ReplayVisualOps {
    pub fn payload_from_file(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        dictionary: &Sc2DictionaryData,
        context: &ReplayVisualContext,
    ) -> Result<ReplayVisualPayload, String> {
        let mut parsed = ReplayParser::parse_file_with_store_ordered_events_filtered(
            replay_path,
            resources.protocol_store(),
            Self::event_name_is_needed,
        )
        .map_err(|error| format!("Failed to parse replay visual events: {error}"))?;
        let events = parsed.take_events();
        let mut replay = parsed.take_replay();
        let details = replay
            .take_details()
            .ok_or_else(|| "Replay visual data is missing replay.details.".to_string())?;
        let init_data = replay
            .take_init_data()
            .ok_or_else(|| "Replay visual data is missing replay.initData.".to_string())?;
        let metadata = replay
            .take_metadata()
            .ok_or_else(|| "Replay visual data is missing gamemetadata.json.".to_string())?;

        let map_name = Self::resolved_map_name(context.map(), &metadata, dictionary);
        let dictionaries = ReplayVisualDictionaries::from_dictionary(dictionary, map_name.as_str());
        let (map_width, map_height) = Self::infer_map_bounds(&events);
        let input = Self::build_input_from_parsed(
            context, map_name, &details, &init_data, &metadata, map_width, map_height,
        );
        Ok(Self::payload_from_events(input, dictionaries, &events))
    }

    pub fn payload_from_events(
        input: ReplayVisualBuildInput,
        dictionaries: ReplayVisualDictionaries,
        events: &[ReplayEvent],
    ) -> ReplayVisualPayload {
        ReplayVisualTimelineBuilder::new(input, dictionaries).process_events(events)
    }

    pub fn replay_unit_id(index: Option<i64>, recycle_index: Option<i64>) -> Option<i64> {
        let index = index?;
        let recycle_index = recycle_index?;
        Some(recycle_index * 100_000 + index)
    }

    fn unit_id_from_game_unit_tag(unit_tag: i64) -> Option<i64> {
        let index = i64::try_from(UnitTag::index(unit_tag.into())).ok()?;
        let recycle = i64::try_from(UnitTag::recycle(unit_tag.into())).ok()?;
        Self::replay_unit_id(Some(index), Some(recycle))
    }

    fn replay_event_unit_id(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unit_id(event.m_unit_tag_index, event.m_unit_tag_recycle)
    }

    fn event_name_is_needed(event_name: &str) -> bool {
        if matches!(
            event_name,
            "NNet.Game.SCmdEvent" | "NNet.Game.SSelectionDeltaEvent"
        ) {
            return true;
        }
        !matches!(
            Self::tracker_event_kind(event_name),
            ReplayVisualTrackerEventKind::Other
        )
    }

    fn tracker_event_kind(event_name: &str) -> ReplayVisualTrackerEventKind {
        match event_name {
            "NNet.Replay.Tracker.SUnitBornEvent" | "NNet.Replay.Tracker.SUnitInitEvent" => {
                ReplayVisualTrackerEventKind::UnitBornOrInit
            }
            "NNet.Replay.Tracker.SUnitTypeChangeEvent" => {
                ReplayVisualTrackerEventKind::UnitTypeChange
            }
            "NNet.Replay.Tracker.SUnitOwnerChangeEvent" => {
                ReplayVisualTrackerEventKind::UnitOwnerChange
            }
            "NNet.Replay.Tracker.SUnitPositionsEvent" => {
                ReplayVisualTrackerEventKind::UnitPositions
            }
            "NNet.Replay.Tracker.SUnitDiedEvent" => ReplayVisualTrackerEventKind::UnitDied,
            _ => ReplayVisualTrackerEventKind::Other,
        }
    }

    fn build_input_from_parsed(
        context: &ReplayVisualContext,
        map_name: String,
        details: &ReplayDetails,
        _init_data: &ReplayInitData,
        metadata: &ReplayMetadata,
        map_width: f64,
        map_height: f64,
    ) -> ReplayVisualBuildInput {
        let result = if context.result().trim().is_empty() {
            Self::result_from_metadata(metadata)
        } else {
            context.result().to_string()
        };
        let duration_seconds = if context.duration_seconds() > 0 {
            context.duration_seconds() as f64
        } else {
            metadata.Duration
        };
        ReplayVisualBuildInput::new(
            ReplayVisualReplayInfo::new(context.file(), map_name, result, duration_seconds),
            ReplayVisualMapSize::new(map_width, map_height),
            Self::players_from_details(details, context.main_player_id()),
            context.main_player_id(),
        )
    }

    fn resolved_map_name(
        context_map: &str,
        metadata: &ReplayMetadata,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        if !context_map.trim().is_empty() {
            return context_map.to_string();
        }
        let title = metadata.Title.as_str();
        dictionary
            .map_names
            .get(title)
            .and_then(|row| row.get("EN"))
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if title.trim().is_empty() {
                    "Unknown map".to_string()
                } else {
                    title.to_string()
                }
            })
    }

    fn result_from_metadata(metadata: &ReplayMetadata) -> String {
        let player0_result = metadata
            .Players
            .first()
            .map(|player| player.Result.as_str())
            .unwrap_or_default();
        let player1_result = metadata
            .Players
            .get(1)
            .map(|player| player.Result.as_str())
            .unwrap_or_default();
        if player0_result == "Win" || player1_result == "Win" {
            "Victory".to_string()
        } else {
            "Defeat".to_string()
        }
    }

    fn players_from_details(
        details: &ReplayDetails,
        main_player_id: i64,
    ) -> Vec<ReplayVisualPlayer> {
        let mut players = Vec::with_capacity(3);
        for player_id in [1_i64, 2_i64] {
            let index = usize::try_from(player_id - 1).unwrap_or_default();
            let name = details
                .m_playerList
                .get(index)
                .map(|player| player.m_name.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("Player {player_id}"));
            let owner_kind = if player_id == main_player_id {
                ReplayVisualOwnerKind::Main
            } else {
                ReplayVisualOwnerKind::Ally
            };
            players.push(ReplayVisualPlayer::new(player_id, name, owner_kind));
        }
        players.push(ReplayVisualPlayer::new(
            3,
            "Amon",
            ReplayVisualOwnerKind::Amon,
        ));
        players
    }

    fn event_game_loop(event: &ReplayEvent) -> i64 {
        match event {
            ReplayEvent::Game(game_event) => game_event.game_loop,
            ReplayEvent::Tracker(tracker_event) => tracker_event.game_loop,
        }
    }

    fn game_event_target_point(event: &GameEvent) -> Option<(f64, f64)> {
        let point = event
            .m_data
            .as_ref()
            .and_then(|data| data.TargetPoint.as_ref())
            .or_else(|| {
                event
                    .m_data
                    .as_ref()
                    .and_then(|data| data.TargetUnit.as_ref())
                    .and_then(|target| target.m_snapshotPoint.as_ref())
            })
            .or_else(|| {
                event
                    .m_target
                    .as_ref()
                    .and_then(|target| target.m_snapshotPoint.as_ref())
            })?;
        Self::snapshot_point_xy(point)
    }

    fn snapshot_point_xy(point: &SnapshotPoint) -> Option<(f64, f64)> {
        let x = point.values.first().and_then(Self::snapshot_point_number)?;
        let y = point.values.get(1).and_then(Self::snapshot_point_number)?;
        Some((x / GAME_POINT_FIXED_SCALE, y / GAME_POINT_FIXED_SCALE))
    }

    fn snapshot_point_number(value: &SnapshotPointValue) -> Option<f64> {
        match value {
            SnapshotPointValue::Int(value) => Some(*value as f64),
            SnapshotPointValue::Float(value) if value.is_finite() => Some(*value),
            SnapshotPointValue::Float(_) => None,
        }
    }

    fn is_deep_tunnel_unit(unit_type: &str) -> bool {
        unit_type.to_ascii_lowercase().contains("brutalisk")
    }

    fn is_tychus_medivac_ability_link(ability_link: i64) -> bool {
        TYCHUS_MEDIVAC_ABILITY_LINKS.contains(&ability_link)
    }

    fn apply_selection_remove_mask(selection: &mut Vec<i64>, remove_mask: &SelectionRemoveMask) {
        match remove_mask {
            SelectionRemoveMask::None => {}
            SelectionRemoveMask::Mask(mask) => {
                let mut index = 0_usize;
                selection.retain(|_| {
                    let should_remove = mask.get(index).copied().unwrap_or(false);
                    index += 1;
                    !should_remove
                });
            }
            SelectionRemoveMask::OneIndices(indices) => {
                let indices_to_remove = Self::selection_mask_indices(indices);
                let mut index = 0_usize;
                selection.retain(|_| {
                    let should_remove = indices_to_remove.contains(&index);
                    index += 1;
                    !should_remove
                });
            }
            SelectionRemoveMask::ZeroIndices(indices) => {
                let indices_to_keep = Self::selection_mask_indices(indices);
                let mut index = 0_usize;
                selection.retain(|_| {
                    let should_keep = indices_to_keep.contains(&index);
                    index += 1;
                    should_keep
                });
            }
        }
    }

    fn selection_mask_indices(indices: &[i64]) -> HashSet<usize> {
        indices
            .iter()
            .filter_map(|index| usize::try_from(*index).ok())
            .collect()
    }

    fn is_tychus_medivac_passenger_unit(unit: &ReplayVisualLiveUnit) -> bool {
        if unit.group != ReplayVisualUnitGroup::AttackUnits {
            return false;
        }
        let lower_type = unit.unit_type.to_ascii_lowercase();
        if !lower_type.starts_with("tychus") {
            return false;
        }
        let excluded_terms = ["scv", "medivac", "platform", "turret", "dummy", "caster"];
        !excluded_terms.iter().any(|term| lower_type.contains(term))
    }

    fn is_tychus_medivac_proxy_unit(unit: &ReplayVisualLiveUnit) -> bool {
        if Self::is_tychus_medivac_passenger_unit(unit) {
            return false;
        }
        unit.unit_type.to_ascii_lowercase().starts_with("tychus")
    }

    fn infer_map_bounds(events: &[ReplayEvent]) -> (f64, f64) {
        let mut max_x = 0_i64;
        let mut max_y = 0_i64;
        for event in events {
            let ReplayEvent::Tracker(tracker) = event else {
                continue;
            };
            if let Some(x) = tracker.m_x {
                max_x = max_x.max(x);
            }
            if let Some(y) = tracker.m_y {
                max_y = max_y.max(y);
            }
            for chunk in tracker.m_position_items.chunks_exact(3) {
                max_x = max_x.max(chunk[1]);
                max_y = max_y.max(chunk[2]);
            }
        }
        let width = max_x as f64;
        let height = max_y as f64;
        (width, height)
    }

    fn seconds_from_game_loop(game_loop: i64) -> f64 {
        game_loop as f64 / GAME_LOOPS_PER_SECOND
    }

    fn owner_kind(
        player_id: i64,
        main_player_id: i64,
        dictionaries: &ReplayVisualDictionaries,
    ) -> ReplayVisualOwnerKind {
        if player_id == main_player_id {
            ReplayVisualOwnerKind::Main
        } else if matches!(player_id, 1 | 2) {
            ReplayVisualOwnerKind::Ally
        } else if dictionaries.is_amon_player(player_id) {
            ReplayVisualOwnerKind::Amon
        } else if player_id == 0 {
            ReplayVisualOwnerKind::Neutral
        } else {
            ReplayVisualOwnerKind::Other
        }
    }

    fn unit_group(
        unit_type: &str,
        display_name: &str,
        owner_kind: ReplayVisualOwnerKind,
        dictionaries: &ReplayVisualDictionaries,
    ) -> ReplayVisualUnitGroup {
        if owner_kind == ReplayVisualOwnerKind::Amon && dictionaries.is_wave_unit(unit_type) {
            return ReplayVisualUnitGroup::EnemyAssaults;
        }
        if Self::is_defense_structure(unit_type, display_name) {
            return ReplayVisualUnitGroup::DefenseBuildings;
        }
        if Self::is_structure(unit_type, display_name) {
            return ReplayVisualUnitGroup::Buildings;
        }
        ReplayVisualUnitGroup::AttackUnits
    }

    fn unit_radius(group: ReplayVisualUnitGroup) -> f64 {
        match group {
            ReplayVisualUnitGroup::Buildings => 1.45,
            ReplayVisualUnitGroup::AttackUnits => 0.68,
            ReplayVisualUnitGroup::DefenseBuildings => 1.1,
            ReplayVisualUnitGroup::EnemyAssaults => 0.85,
        }
    }

    fn should_render_unit(
        unit: &ReplayVisualLiveUnit,
        input: &ReplayVisualBuildInput,
        dictionaries: &ReplayVisualDictionaries,
    ) -> bool {
        if !unit.x.is_finite() || !unit.y.is_finite() {
            return false;
        }
        if unit.x < 0.0 || unit.y < 0.0 || unit.x > input.map_width || unit.y > input.map_height {
            return false;
        }
        if dictionaries.should_omit_unit(unit.unit_type.as_str(), unit.display_name.as_str()) {
            return false;
        }
        !matches!(
            unit.owner_kind,
            ReplayVisualOwnerKind::Neutral | ReplayVisualOwnerKind::Other
        ) || matches!(
            unit.group,
            ReplayVisualUnitGroup::Buildings | ReplayVisualUnitGroup::DefenseBuildings
        )
    }

    fn is_defense_structure(unit_type: &str, display_name: &str) -> bool {
        let haystack = format!(
            "{} {}",
            unit_type.to_ascii_lowercase(),
            display_name.to_ascii_lowercase()
        );
        let defense_terms = [
            "turret",
            "cannon",
            "bunker",
            "crawler",
            "tower",
            "battery",
            "monolith",
            "toxic nest",
            "toxicnest",
            "bile launcher",
            "bilelauncher",
            "missile",
            "stasis ward",
            "stasisward",
            "laser drill",
            "laserdrill",
            "perdition",
            "flaming betty",
            "flamingbetty",
            "blaster billy",
            "blasterbilly",
            "spinning dizzy",
            "spinningdizzy",
            "auto turret",
            "autoturret",
            "railgun turret",
            "railgunturret",
        ];
        defense_terms.iter().any(|term| haystack.contains(term))
    }

    fn is_structure(unit_type: &str, display_name: &str) -> bool {
        let haystack = format!(
            "{} {}",
            unit_type.to_ascii_lowercase(),
            display_name.to_ascii_lowercase()
        );
        let structure_terms = [
            "commandcenter",
            "command center",
            "hatchery",
            "lair",
            "hive",
            "nexus",
            "pylon",
            "depot",
            "barracks",
            "factory",
            "starport",
            "gateway",
            "warpgate",
            "warp gate",
            "forge",
            "assimilator",
            "refinery",
            "extractor",
            "spawningpool",
            "spawning pool",
            "roachwarren",
            "roach warren",
            "evolutionchamber",
            "evolution chamber",
            "hydraliskden",
            "hydralisk den",
            "banelingnest",
            "baneling nest",
            "spire",
            "ultraliskcavern",
            "ultralisk cavern",
            "cyberneticscore",
            "cybernetics core",
            "twilightcouncil",
            "twilight council",
            "robotics",
            "fleetbeacon",
            "fleet beacon",
            "templararchive",
            "templar archive",
            "engineeringbay",
            "engineering bay",
            "armory",
            "ghostacademy",
            "ghost academy",
            "techlab",
            "tech lab",
            "reactor",
            "structure",
            "building",
            "compound",
            "nydus",
            "omega worm",
            "omegaworm",
            "creep tumor",
            "creeptumor",
            "creep colony",
            "creepcolony",
            "altar",
            "solar forge",
            "solarforge",
            "den",
        ];
        structure_terms.iter().any(|term| haystack.contains(term))
    }

    fn owner_color(owner_kind: ReplayVisualOwnerKind) -> &'static str {
        match owner_kind {
            ReplayVisualOwnerKind::Main => "#38bdf8",
            ReplayVisualOwnerKind::Ally => "#22c55e",
            ReplayVisualOwnerKind::Amon => "#ef4444",
            ReplayVisualOwnerKind::Neutral => "#94a3b8",
            ReplayVisualOwnerKind::Other => "#cbd5e1",
        }
    }

    fn map_name_has_amon_override(map_name: &str, candidate: &str) -> bool {
        map_name.contains(candidate)
            || (map_name.contains("[MM] Lnl") && candidate == "Lock & Load")
    }
}
