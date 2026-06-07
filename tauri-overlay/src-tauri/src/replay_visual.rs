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

mod model_impls;
mod timeline;

use timeline::ReplayVisualTimelineBuilder;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayVisualMapSize {
    width: f64,
    height: f64,
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

pub struct ReplayVisualOps;

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
