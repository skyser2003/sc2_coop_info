use s2coop_analyzer::detailed_replay_analysis::ReplayAnalysisResources;
use s2coop_analyzer::dictionary_data::{Sc2DictionaryData, UnitNamesJson};
use s2protocol_port::{
    ReplayDetails, ReplayEvent, ReplayInitData, ReplayMetadata, ReplayParser, TrackerEvent,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use ts_rs::TS;

const FRAME_INTERVAL_GAME_LOOPS: i64 = 16;
const ASSAULT_MIN_GAME_SECONDS: f64 = 60.0;
const ASSAULT_MIN_UNITS: usize = 6;
const GAME_LOOPS_PER_SECOND: f64 = 16.0;

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

#[derive(Debug)]
struct ReplayVisualTimelineBuilder {
    input: ReplayVisualBuildInput,
    dictionaries: ReplayVisualDictionaries,
    unit_id_by_tag_index: BTreeMap<i64, i64>,
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
        Self {
            unit_names,
            units_in_waves,
            amon_player_ids,
            omitted_unit_types,
        }
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
        self.omitted_unit_types.contains(unit_type)
            || ReplayVisualOps::should_skip_unit_type(unit_type, display_name)
    }
}

impl ReplayVisualBuildInput {
    pub fn new(
        file: impl Into<String>,
        map: impl Into<String>,
        result: impl Into<String>,
        duration_seconds: f64,
        map_width: f64,
        map_height: f64,
        players: Vec<ReplayVisualPlayer>,
        main_player_id: i64,
    ) -> Self {
        Self {
            file: file.into(),
            map: map.into(),
            result: result.into(),
            duration_seconds,
            map_width,
            map_height,
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

    fn set_position(&mut self, x: i64, y: i64) {
        self.x = x as f64;
        self.y = y as f64;
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
        }
    }
}

impl ReplayVisualTimelineBuilder {
    fn new(input: ReplayVisualBuildInput, dictionaries: ReplayVisualDictionaries) -> Self {
        Self {
            input,
            dictionaries,
            unit_id_by_tag_index: BTreeMap::new(),
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

            self.last_game_loop = event_game_loop;
            match event {
                ReplayEvent::Tracker(tracker) => self.process_tracker_event(tracker),
                ReplayEvent::Game(_) => {}
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
        for chunk in event.m_position_items.chunks_exact(3) {
            tag_index += chunk[0];
            let Some(unit_id) = self.unit_id_by_tag_index.get(&tag_index).copied() else {
                continue;
            };
            if let Some(live_unit) = self.live_units.get_mut(&unit_id) {
                live_unit.set_position(chunk[1], chunk[2]);
                self.frame_dirty = true;
            }
        }
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

    fn replay_event_unit_id(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unit_id(event.m_unit_tag_index, event.m_unit_tag_recycle)
    }

    fn event_name_is_needed(event_name: &str) -> bool {
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
            context.file(),
            map_name,
            result,
            duration_seconds,
            map_width,
            map_height,
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

    fn should_skip_unit_type(unit_type: &str, display_name: &str) -> bool {
        let lower_type = unit_type.to_ascii_lowercase();
        let lower_name = display_name.to_ascii_lowercase();
        if lower_type.starts_with("beacon") {
            return true;
        }
        if lower_type.starts_with("coopcaster") || lower_type.starts_with("soacaster") {
            return true;
        }
        let skip_terms = [
            "mineralfield",
            "vespenegeyser",
            "pickup",
            "unbuildable",
            "pathingblocker",
            "dummy",
            "placeholder",
            "cocoon",
            "egg",
            "larva",
            "top bar",
        ];
        skip_terms
            .iter()
            .any(|term| lower_type.contains(term) || lower_name.contains(term))
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
