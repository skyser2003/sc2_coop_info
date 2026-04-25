use crate::cache_overall_stats_generator::{
    AnalysisPlayerStatsSeries, CacheOverallStatsFile, CacheReplayEntry, PrettyCacheError,
};
use crate::dictionary_data::{
    CacheGenerationData, Sc2DictionaryData, UnitAddKillsToJson, UnitNamesJson,
};
use crate::tauri_replay_analysis_impl::{
    ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer, PlayerPositions, ReplayReport,
    ReplayReportDetailData, ReplayReportDetailedInput,
};
use chrono::{DateTime, Local};
use indexmap::IndexMap;
use s2protocol_port::{
    ProtocolStore, ProtocolStoreBuilder, ReplayDetails, ReplayEvent, ReplayInitData,
    ReplayMetadata, ReplayParseMode, ReplayParser, TrackerEvent,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use walkdir::WalkDir;
mod replay_event_handlers;

use crate::stats_counter_core::{
    ReplayDroneIdentifierCore, ReplayStatsCounterCore, StatsCounterDictionaries,
};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::ThreadPoolBuilder;
use replay_event_handlers::{
    IdentifiedWavesMap, ReplayEventHandlers, StatsCounterTarget, UnitBornOrInitEventFields,
    UnitDiedEventFields, UnitStateMap, UnitTypeChangeEventFields, UnitTypeCountMap, WaveUnitsState,
};

const LOCUST_SOURCE_UNITS: [&str; 5] = [
    "AbathurLocust",
    "Locust",
    "LocustMP",
    "DehakaCreeperFlying",
    "DehakaLocust",
];

const BROODLING_SOURCE_UNITS: [&str; 6] = [
    "BroodlingEscortStetmann",
    "BroodlingEscort",
    "Broodling",
    "KerriganInfestBroodling",
    "StukovInfestBroodling",
    "BroodlingStetmann",
];

const ZERATUL_ARTIFACT_PICKUPS: [&str; 4] = [
    "ZeratulArtifactPickup1",
    "ZeratulArtifactPickup2",
    "ZeratulArtifactPickup3",
    "ZeratulArtifactPickupUnlimited",
];

const ZERATUL_SHADE_PROJECTIONS: [&str; 2] = [
    "ZeratulKhaydarinMonolithProjection",
    "ZeratulPhotonCannonProjection",
];

const CUSTOM_KILL_ICON_KEYS: [&str; 10] = [
    "hfts",
    "tus",
    "propagators",
    "voidrifts",
    "turkey",
    "voidreanimators",
    "deadofnight",
    "minesweeper",
    "missilecommand",
    "shuttles",
];

type UnitStats = (i64, i64, i64, f64);

pub struct DetailedReplayAnalyzer;

#[derive(Clone)]
pub struct ReplayAnalysisResources {
    dictionary_data: Arc<Sc2DictionaryData>,
    hidden_created_lost: HashSet<String>,
    analysis_sets: ReplayAnalysisSets,
    stats_counter_dictionaries: Arc<StatsCounterDictionaries>,
    protocol_store: ProtocolStore,
}

#[derive(Debug, Clone)]
struct ReplayAnalysisSets {
    do_not_count_kills: HashSet<String>,
    duplicating_units: HashSet<String>,
    skip_tokens: Vec<String>,
    dont_count_morphs: HashSet<String>,
    self_killing_units: HashSet<String>,
    aoe_units: HashSet<String>,
    tychus_outlaws: HashSet<String>,
    units_killed_in_morph: HashSet<String>,
    dont_include_units: HashSet<String>,
    icon_units: HashSet<String>,
    salvage_units: HashSet<String>,
    unit_add_losses_to: HashSet<String>,
    commander_no_units_values: HashSet<String>,
}

impl ReplayAnalysisSets {
    fn new(data: &Sc2DictionaryData) -> Self {
        let replay_data = &data.replay_analysis_data;
        let mut commander_no_units_values = HashSet::new();
        for units in replay_data.commander_no_units.values() {
            commander_no_units_values.extend(units.iter().cloned());
        }

        Self {
            do_not_count_kills: replay_data.do_not_count_kills.iter().cloned().collect(),
            duplicating_units: replay_data.duplicating_units.iter().cloned().collect(),
            skip_tokens: replay_data
                .skip_strings
                .iter()
                .map(|value| value.to_lowercase())
                .collect(),
            dont_count_morphs: replay_data.dont_count_morphs.iter().cloned().collect(),
            self_killing_units: replay_data.self_killing_units.iter().cloned().collect(),
            aoe_units: replay_data.aoe_units.iter().cloned().collect(),
            tychus_outlaws: replay_data.tychus_outlaws.iter().cloned().collect(),
            units_killed_in_morph: replay_data.units_killed_in_morph.iter().cloned().collect(),
            dont_include_units: replay_data.dont_include_units.iter().cloned().collect(),
            icon_units: replay_data.icon_units.iter().cloned().collect(),
            salvage_units: replay_data.salvage_units.iter().cloned().collect(),
            unit_add_losses_to: replay_data.unit_add_losses_to.keys().cloned().collect(),
            commander_no_units_values,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProtocolBuildValue {
    Int(u32),
    Str(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayBuildInfo {
    replay_build: u32,
    protocol_build: ProtocolBuildValue,
}

pub struct ReplayTiming;

impl ReplayTiming {
    pub fn realtime_length_from_replay(
        accurate_length: f64,
        details: &ReplayDetails,
        init_data: &ReplayInitData,
    ) -> f64 {
        DetailedReplayAnalyzer::realtime_length_from_replay(accurate_length, details, init_data)
    }
}

pub struct ReplayFileIdentity;

impl ReplayFileIdentity {
    pub fn calculate_hash(path: &Path) -> String {
        DetailedReplayAnalyzer::calculate_replay_hash(path)
    }
}

impl ReplayBuildInfo {
    pub fn new(replay_build: u32, protocol_build: ProtocolBuildValue) -> Self {
        Self {
            replay_build,
            protocol_build,
        }
    }

    pub fn replay_build(&self) -> u32 {
        self.replay_build
    }

    pub fn protocol_build(&self) -> &ProtocolBuildValue {
        &self.protocol_build
    }
}

#[derive(Debug, Clone)]
struct ReplayParsedContext {
    details: ReplayDetails,
    init_data: ReplayInitData,
    metadata: ReplayMetadata,
}

#[derive(Debug, Clone)]
struct ReplayDetailedParseContext {
    events: Vec<ReplayEvent>,
    start_time: f64,
    end_time: f64,
}

#[derive(Debug, Clone)]
struct ReplayBaseParse {
    context: ReplayParsedContext,
    mutator_context: ReplayMutatorParseContext,
    build: ReplayBuildInfo,
    file: String,
    map_name: String,
    extension: bool,
    brutal_plus: u32,
    result: String,
    accurate_length: f64,
    accurate_length_force_float: bool,
    realtime_length: f64,
    form_alength: String,
    length: u64,
    mutators: Vec<String>,
    weekly: bool,
    raw_messages: Vec<ParsedReplayMessage>,
    hash: String,
    date: String,
    detailed: Option<ReplayDetailedParseContext>,
}

#[derive(Debug, Clone)]
struct ReplayParsedInputBundle {
    parser: ParsedReplayInput,
    all_players: Vec<ParsedReplayPlayer>,
    accurate_length_force_float: bool,
    metadata_duration: f64,
    metadata_player_apm: Vec<f64>,
    game_speed_code: i64,
    mutator_context: ReplayMutatorParseContext,
    realtime_length: f64,
    commander_found: bool,
    enemy_race_present: bool,
    cache_context: ReplayCacheContext,
    detailed: Option<ReplayDetailedParseContext>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ReplayCacheContext {
    is_mm_replay: bool,
    is_blizzard_map: bool,
    recover_disabled: bool,
}

#[derive(Debug, Clone)]
struct ReplayMutatorParseContext {
    cache_handles: Vec<String>,
    brutal_plus_difficulty: i64,
    retry_mutation_indexes: Vec<i64>,
}

impl ReplayMutatorParseContext {
    fn from_init_data(init_data: &ReplayInitData) -> Self {
        let game_description = &init_data.m_syncLobbyState.m_gameDescription;
        let slot0 = init_data.m_syncLobbyState.m_lobbyState.m_slots.first();

        Self {
            cache_handles: game_description.m_cacheHandles.clone(),
            brutal_plus_difficulty: slot0
                .map(|slot| slot.m_brutalPlusDifficulty)
                .unwrap_or_default(),
            retry_mutation_indexes: slot0
                .map(|slot| slot.m_retryMutationIndexes.clone())
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ReplayBaseParseFilters {
    only_blizzard: bool,
    require_recover_disabled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ReplayBaseParseOptions {
    include_events: bool,
    filters: ReplayBaseParseFilters,
}

impl ReplayBaseParseFilters {
    fn saved_cache() -> Self {
        Self {
            only_blizzard: true,
            require_recover_disabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplayBaseParseError {
    ProtocolStore(String),
    ReplayParse { path: String, message: String },
    InvalidReplayData(String),
    IoRead { path: PathBuf, message: String },
}

impl std::fmt::Display for ReplayBaseParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProtocolStore(message) => write!(f, "failed to build protocol store: {message}"),
            Self::ReplayParse { path, message } => {
                write!(f, "failed to parse replay '{path}': {message}")
            }
            Self::InvalidReplayData(message) => write!(f, "invalid replay data: {message}"),
            Self::IoRead { path, message } => {
                write!(f, "failed to read '{}': {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ReplayBaseParseError {}

impl ReplayBaseParseError {
    fn into_detailed_analysis_error(self) -> DetailedReplayAnalysisError {
        match self {
            Self::ProtocolStore(message) => DetailedReplayAnalysisError::ProtocolStore(message),
            Self::ReplayParse { path, message } => {
                DetailedReplayAnalysisError::ReplayParse { path, message }
            }
            Self::InvalidReplayData(message) => {
                DetailedReplayAnalysisError::InvalidReplayData(message)
            }
            Self::IoRead { path, message } => DetailedReplayAnalysisError::IoRead { path, message },
        }
    }
}

#[derive(Clone, Copy)]
enum ReplayNumericValue {
    Int(i64),
    Float(f64),
}

impl ReplayNumericValue {
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    fn subtract(self, rhs: &Self) -> Self {
        match (self, *rhs) {
            (Self::Int(left), Self::Int(right)) => Self::Int(left - right),
            _ => Self::Float(self.as_f64() - rhs.as_f64()),
        }
    }
}

impl ReplayAnalysisResources {
    pub fn from_dictionary_data(
        dictionary_data: Arc<Sc2DictionaryData>,
    ) -> Result<Self, DetailedReplayAnalysisError> {
        let hidden_created_lost = dictionary_data
            .replay_analysis_data
            .dont_show_created_lost
            .iter()
            .cloned()
            .collect::<HashSet<String>>();
        let protocol_store = ProtocolStoreBuilder::build().map_err(|error| {
            DetailedReplayAnalysisError::ProtocolStore(format!(
                "failed to build protocol store: {error}"
            ))
        })?;
        let analysis_sets = ReplayAnalysisSets::new(dictionary_data.as_ref());
        let stats_counter_dictionaries =
            Arc::new(DetailedReplayAnalyzer::build_stats_counter_dictionaries(
                &dictionary_data.cache_generation_data(),
            ));

        Ok(Self {
            dictionary_data,
            hidden_created_lost,
            analysis_sets,
            stats_counter_dictionaries,
            protocol_store,
        })
    }

    pub fn dictionary_data(&self) -> &Sc2DictionaryData {
        self.dictionary_data.as_ref()
    }

    pub fn cache_generation_data(&self) -> CacheGenerationData<'_> {
        self.dictionary_data.cache_generation_data()
    }

    pub fn hidden_created_lost(&self) -> &HashSet<String> {
        &self.hidden_created_lost
    }

    fn analysis_sets(&self) -> &ReplayAnalysisSets {
        &self.analysis_sets
    }

    fn stats_counter_dictionaries(&self) -> Arc<StatsCounterDictionaries> {
        Arc::clone(&self.stats_counter_dictionaries)
    }

    pub fn protocol_store(&self) -> &ProtocolStore {
        &self.protocol_store
    }

    fn parse_replay_base(
        &self,
        replay_path: &Path,
        options: ReplayBaseParseOptions,
    ) -> Result<Option<ReplayBaseParse>, ReplayBaseParseError> {
        let inputs = self.cache_generation_data();
        DetailedReplayAnalyzer::parse_replay_base(
            replay_path,
            &inputs,
            self.protocol_store(),
            options,
        )
    }
}

impl DetailedReplayAnalyzer {
    fn replay_game_speed_code(details: &ReplayDetails, init_data: &ReplayInitData) -> i64 {
        if matches!(details.m_gameSpeed, 0..=4) {
            details.m_gameSpeed
        } else if matches!(
            init_data.m_syncLobbyState.m_gameDescription.m_gameSpeed,
            0..=4
        ) {
            init_data.m_syncLobbyState.m_gameDescription.m_gameSpeed
        } else {
            4
        }
    }

    fn game_speed_multiplier(game_speed: i64) -> f64 {
        match game_speed {
            0 => 0.6,
            1 => 0.8,
            2 => 1.0,
            3 => 1.2,
            4 => 1.4,
            _ => 1.4,
        }
    }

    fn realtime_length_from_replay(
        accurate_length: f64,
        details: &ReplayDetails,
        init_data: &ReplayInitData,
    ) -> f64 {
        DetailedReplayAnalyzer::realtime_length_from_game_speed(
            accurate_length,
            DetailedReplayAnalyzer::replay_game_speed_code(details, init_data),
        )
    }

    fn realtime_length_from_game_speed(accurate_length: f64, game_speed_code: i64) -> f64 {
        if !accurate_length.is_finite() || accurate_length <= 0.0 {
            return 0.0;
        }

        let multiplier = DetailedReplayAnalyzer::game_speed_multiplier(game_speed_code);

        accurate_length / multiplier
    }

    fn parse_replay_base(
        replay_path: &Path,
        inputs: &CacheGenerationData<'_>,
        protocol_store: &ProtocolStore,
        options: ReplayBaseParseOptions,
    ) -> Result<Option<ReplayBaseParse>, ReplayBaseParseError> {
        if options.filters.only_blizzard && replay_path.to_string_lossy().contains("[MM]") {
            return Ok(None);
        }

        let (mut parsed, events) = if options.include_events {
            let mut parsed =
                ReplayParser::parse_file_with_store_ordered_events(replay_path, protocol_store)
                    .map_err(|error| ReplayBaseParseError::ReplayParse {
                        path: replay_path.display().to_string(),
                        message: error.to_string(),
                    })?;
            let events = parsed.take_events();
            (parsed.take_replay(), events)
        } else {
            let parsed = ReplayParser::parse_file_with_store(
                replay_path,
                protocol_store,
                ReplayParseMode::Simple,
            )
            .map_err(|error| ReplayBaseParseError::ReplayParse {
                path: replay_path.display().to_string(),
                message: error.to_string(),
            })?;
            (parsed, Vec::new())
        };

        let base_build = parsed.base_build();
        let details = parsed.take_details();
        let init_data = parsed.take_init_data();
        let metadata = parsed.take_metadata();
        let message_events = parsed.take_message_events();

        let details = details.ok_or_else(|| {
            ReplayBaseParseError::InvalidReplayData("missing replay.details".to_string())
        })?;
        let init_data = init_data.ok_or_else(|| {
            ReplayBaseParseError::InvalidReplayData("missing replay.initData".to_string())
        })?;
        let metadata = metadata.ok_or_else(|| {
            ReplayBaseParseError::InvalidReplayData("missing replay.gamemetadata.json".to_string())
        })?;

        if options.filters.only_blizzard && !details.m_isBlizzardMap {
            return Ok(None);
        }

        let disable_recover = details.m_disableRecoverGame.unwrap_or(false);
        if options.filters.require_recover_disabled && !disable_recover {
            return Ok(None);
        }

        let replay_build = i64::from(base_build);
        let latest_build = i64::from(
            protocol_store
                .latest()
                .map_err(|error| ReplayBaseParseError::ProtocolStore(error.to_string()))?
                .build(),
        );
        let selected_build = if protocol_store.build(base_build).is_ok() {
            replay_build
        } else {
            protocol_store
                .closest_build(base_build)
                .map(i64::from)
                .unwrap_or(latest_build)
        };

        let map_title = if metadata.Title.is_empty() {
            "Unknown map".to_string()
        } else {
            metadata.Title.clone()
        };
        let map_name = inputs
            .map_names
            .get(&map_title)
            .and_then(|row| row.get("EN"))
            .cloned()
            .unwrap_or(map_title);

        let extension = init_data
            .m_syncLobbyState
            .m_gameDescription
            .m_hasExtensionMod;
        let brutal_plus = init_data
            .m_syncLobbyState
            .m_lobbyState
            .m_slots
            .first()
            .map(|value| value.m_brutalPlusDifficulty as u32)
            .unwrap_or_default();

        let length_numeric = ReplayNumericValue::Float(metadata.Duration);
        let start_time = DetailedReplayAnalyzer::get_start_time(&events);
        let last_deselect_event = DetailedReplayAnalyzer::get_last_deselect_event(&events)
            .unwrap_or(ReplayNumericValue::Float(metadata.Duration));

        let metadata_players = &metadata.Players;
        if metadata_players.is_empty() {
            return Err(ReplayBaseParseError::InvalidReplayData(
                "metadata Players must be array".to_string(),
            ));
        }

        let player0_result = metadata_players
            .first()
            .map(|value| value.Result.clone())
            .unwrap_or_default();
        let player1_result = metadata_players
            .get(1)
            .map(|value| value.Result.clone())
            .unwrap_or_default();
        let result = if player0_result == "Win" || player1_result == "Win" {
            "Victory".to_string()
        } else {
            "Defeat".to_string()
        };

        let accurate_length_numeric = if result == "Victory" && options.include_events {
            last_deselect_event.subtract(&start_time)
        } else {
            length_numeric.subtract(&start_time)
        };
        let accurate_length = accurate_length_numeric.as_f64();
        let realtime_length = DetailedReplayAnalyzer::realtime_length_from_replay(
            accurate_length,
            &details,
            &init_data,
        );
        let end_time = if result == "Victory" && options.include_events {
            last_deselect_event.as_f64()
        } else {
            metadata.Duration
        };
        let mutator_context = ReplayMutatorParseContext::from_init_data(&init_data);

        let (mutators, weekly) = DetailedReplayAnalyzer::identify_mutators_for_replay(
            &events,
            &inputs.mutators_all,
            &inputs.mutators_ui,
            &inputs.mutator_ids,
            &inputs.cached_mutators,
            extension,
            replay_path.to_string_lossy().contains("[MM]"),
            Some(&mutator_context),
        );

        let raw_messages = message_events
            .iter()
            .filter_map(ParsedReplayMessage::from_message_event)
            .collect::<Vec<ParsedReplayMessage>>();

        Ok(Some(ReplayBaseParse {
            context: ReplayParsedContext {
                details,
                init_data,
                metadata,
            },
            mutator_context,
            build: ReplayBuildInfo::new(
                base_build,
                DetailedReplayAnalyzer::resolve_protocol_build(
                    replay_build,
                    latest_build,
                    selected_build,
                ),
            ),
            file: replay_path.display().to_string(),
            map_name,
            extension,
            brutal_plus,
            result,
            accurate_length,
            accurate_length_force_float: matches!(
                accurate_length_numeric,
                ReplayNumericValue::Float(_)
            ),
            realtime_length,
            form_alength: DetailedReplayAnalyzer::format_duration(accurate_length),
            length: CacheOverallStatsFile::duration_to_u64(length_numeric.as_f64()),
            mutators,
            weekly,
            raw_messages,
            hash: DetailedReplayAnalyzer::calculate_replay_hash(replay_path),
            date: DetailedReplayAnalyzer::file_date_string(replay_path).map_err(|error| {
                ReplayBaseParseError::IoRead {
                    path: replay_path.to_path_buf(),
                    message: error.to_string(),
                }
            })?,
            detailed: options
                .include_events
                .then_some(ReplayDetailedParseContext {
                    events,
                    start_time: start_time.as_f64(),
                    end_time,
                }),
        }))
    }

    fn resolve_protocol_build(
        replay_build: i64,
        latest_build: i64,
        selected_build: i64,
    ) -> ProtocolBuildValue {
        if let Some(mapped) = DetailedReplayAnalyzer::valid_protocol_mapping(replay_build) {
            if DetailedReplayAnalyzer::supported_legacy_protocol(mapped) {
                ProtocolBuildValue::Int(mapped as u32)
            } else {
                ProtocolBuildValue::Str(latest_build.to_string())
            }
        } else if replay_build == selected_build {
            ProtocolBuildValue::Int(replay_build as u32)
        } else {
            ProtocolBuildValue::Str(latest_build.to_string())
        }
    }

    fn collect_user_leave_times(events: &[ReplayEvent]) -> IndexMap<i64, f64> {
        let mut user_leave_times = IndexMap::new();
        for event in events {
            if ReplayEventKind::from_event(event) != ReplayEventKind::GameUserLeave {
                continue;
            }
            let user = DetailedReplayAnalyzer::event_user_id(event)
                .map(|value| value + 1)
                .unwrap_or_default();
            let leave_time = DetailedReplayAnalyzer::event_gameloop(event) as f64 / 16.0;
            user_leave_times.insert(user, leave_time);
        }
        user_leave_times
    }

    fn file_date_string(file: &Path) -> Result<String, std::io::Error> {
        let modified = fs::metadata(file)?.modified()?;
        let datetime: DateTime<Local> = DateTime::from(modified);
        Ok(datetime.format("%Y:%m:%d:%H:%M:%S").to_string())
    }

    fn calculate_replay_hash(path: &Path) -> String {
        match fs::read(path) {
            Ok(bytes) => format!("{:x}", md5::compute(bytes)),
            Err(_) => format!("{:x}", md5::compute(path.to_string_lossy().as_bytes())),
        }
    }

    fn parse_masteries(values: &[u32]) -> [u32; 6] {
        let mut out = [0_u32; 6];
        for (index, value) in values.iter().take(6).enumerate() {
            out[index] = *value;
        }
        out
    }

    fn event_name(event: &ReplayEvent) -> &str {
        event._event()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplayEventKind {
    GameUserLeave,
    GameSelectionDelta,
    GameTriggerDialogControl,
    GameCommand,
    GameCommandUpdateTargetUnit,
    TrackerPlayerStats,
    TrackerUpgrade,
    TrackerUnitBorn,
    TrackerUnitInit,
    TrackerUnitTypeChange,
    TrackerUnitOwnerChange,
    TrackerUnitDied,
    Other,
}

impl ReplayEventKind {
    fn from_name(event_name: &str) -> Self {
        match event_name {
            "NNet.Game.SGameUserLeaveEvent" => Self::GameUserLeave,
            "NNet.Game.SSelectionDeltaEvent" => Self::GameSelectionDelta,
            "NNet.Game.STriggerDialogControlEvent" => Self::GameTriggerDialogControl,
            "NNet.Game.SCmdEvent" => Self::GameCommand,
            "NNet.Game.SCmdUpdateTargetUnitEvent" => Self::GameCommandUpdateTargetUnit,
            "NNet.Replay.Tracker.SPlayerStatsEvent" => Self::TrackerPlayerStats,
            "NNet.Replay.Tracker.SUpgradeEvent" => Self::TrackerUpgrade,
            "NNet.Replay.Tracker.SUnitBornEvent" => Self::TrackerUnitBorn,
            "NNet.Replay.Tracker.SUnitInitEvent" => Self::TrackerUnitInit,
            "NNet.Replay.Tracker.SUnitTypeChangeEvent" => Self::TrackerUnitTypeChange,
            "NNet.Replay.Tracker.SUnitOwnerChangeEvent" => Self::TrackerUnitOwnerChange,
            "NNet.Replay.Tracker.SUnitDiedEvent" => Self::TrackerUnitDied,
            _ => Self::Other,
        }
    }

    fn from_event(event: &ReplayEvent) -> Self {
        Self::from_name(DetailedReplayAnalyzer::event_name(event))
    }

    fn needed_for_detailed_analysis_name(event_name: &str) -> bool {
        !matches!(Self::from_name(event_name), Self::Other)
    }
}

impl DetailedReplayAnalyzer {
    fn event_gameloop(event: &ReplayEvent) -> i64 {
        event._gameloop()
    }

    fn event_control_id(event: &ReplayEvent) -> Option<i64> {
        match event {
            ReplayEvent::Game(event) => event.m_control_id,
            ReplayEvent::Tracker(_) => None,
        }
    }

    fn event_event_type(event: &ReplayEvent) -> Option<i64> {
        match event {
            ReplayEvent::Game(event) => event.m_event_type,
            ReplayEvent::Tracker(_) => None,
        }
    }

    fn event_user_id(event: &ReplayEvent) -> Option<i64> {
        match event {
            ReplayEvent::Game(event) => event.user_id,
            ReplayEvent::Tracker(_) => None,
        }
    }

    fn difficulty_name(code: i64) -> &'static str {
        match code {
            1 => "Casual",
            2 => "Normal",
            3 => "Hard",
            4 => "Brutal",
            5 => "Custom",
            6 => "Cheater",
            _ => "Unknown",
        }
    }

    fn region_name(code: i64) -> &'static str {
        match code {
            1 => "NA",
            2 => "EU",
            3 => "KR",
            5 => "CN",
            98 => "PTR",
            _ => "",
        }
    }

    fn format_duration(seconds: f64) -> String {
        if !seconds.is_finite() || seconds <= 0.0 {
            return "00:00".to_string();
        }

        let total = seconds.floor() as u64;
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let secs = total % 60;
        if hours > 0 {
            format!("{hours:02}:{minutes:02}:{secs:02}")
        } else {
            format!("{minutes:02}:{secs:02}")
        }
    }

    fn valid_protocol_mapping(build: i64) -> Option<i64> {
        match build {
            81102 => Some(81433),
            80871 => Some(81433),
            76811 => Some(76114),
            80188 => Some(78285),
            79998 => Some(78285),
            81433 => Some(83830),
            84643 => Some(83830),
            _ => None,
        }
    }

    fn supported_legacy_protocol(build: i64) -> bool {
        matches!(build, 76114 | 78285 | 83830)
    }

    fn get_last_deselect_event(events: &[ReplayEvent]) -> Option<ReplayNumericValue> {
        let mut last_event = None;
        for event in events {
            if ReplayEventKind::from_event(event) == ReplayEventKind::GameSelectionDelta {
                last_event = Some(ReplayNumericValue::Float(
                    DetailedReplayAnalyzer::event_gameloop(event) as f64 / 16.0 - 2.0,
                ));
            }
        }
        last_event
    }

    fn get_start_time(events: &[ReplayEvent]) -> ReplayNumericValue {
        for event in events {
            if let ReplayEvent::Tracker(event) = event {
                let kind = ReplayEventKind::from_name(&event.event);
                if kind == ReplayEventKind::TrackerPlayerStats && event.m_player_id == Some(1) {
                    let minerals = event
                        .m_stats
                        .as_ref()
                        .and_then(|stats| stats.m_score_value_minerals_collection_rate)
                        .unwrap_or_default();
                    if minerals > 0.0 {
                        return ReplayNumericValue::Float(event.game_loop as f64 / 16.0);
                    }
                }

                if kind == ReplayEventKind::TrackerUpgrade
                    && matches!(event.m_player_id, Some(1 | 2))
                {
                    let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                    if upgrade_name.contains("Spray") {
                        return ReplayNumericValue::Float(event.game_loop as f64 / 16.0);
                    }
                }
            }
        }

        ReplayNumericValue::Int(0)
    }

    fn cache_handle_id(handle: &str) -> String {
        let tail = handle.rsplit('/').next().unwrap_or("");
        tail.split('.').next().unwrap_or("").to_string()
    }

    fn mutator_from_button(button: i64, panel: i64, mutators: &[String]) -> Option<String> {
        let idx = (button - 41) / 3 + (panel - 1) * 15;
        if idx < 0 {
            return None;
        }
        let Ok(index) = usize::try_from(idx) else {
            return None;
        };
        mutators.get(index).cloned()
    }

    fn identify_mutators_for_replay(
        events: &[ReplayEvent],
        mutators_all: &[String],
        mutators_ui: &[String],
        mutator_ids: &crate::dictionary_data::MutatorIdsJson,
        cached_mutators: &crate::dictionary_data::CachedMutatorsJson,
        extension: bool,
        mm: bool,
        mutator_context: Option<&ReplayMutatorParseContext>,
    ) -> (Vec<String>, bool) {
        let mut mutators = Vec::new();
        let mut weekly = false;

        if mm {
            for event in events {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                if ReplayEventKind::from_name(&event.event) != ReplayEventKind::TrackerUpgrade
                    || event.m_player_id != Some(0)
                {
                    continue;
                }
                let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                if !upgrade_name.contains("mutatorinfo") {
                    continue;
                }
                let mutator_key = upgrade_name.get(12..).unwrap_or_default();
                if mutator_ids.contains_key(mutator_key) {
                    mutators.push(mutator_key.to_string());
                }
            }
        }

        if extension {
            if let Some(context) = mutator_context {
                for handle in &context.cache_handles {
                    let cached = DetailedReplayAnalyzer::cache_handle_id(handle);
                    if cached.is_empty() {
                        continue;
                    }
                    if let Some(mutator_id) = cached_mutators.get(&cached) {
                        mutators.push(mutator_id.clone());
                        weekly = true;
                    }
                }
            }
        }

        if !extension {
            if let Some(context) = mutator_context {
                if context.brutal_plus_difficulty > 0 {
                    for key in &context.retry_mutation_indexes {
                        if *key <= 0 {
                            continue;
                        }
                        if let Ok(index) = usize::try_from(*key - 1) {
                            if let Some(mutator) = mutators_all.get(index) {
                                mutators.push(mutator.clone());
                            }
                        }
                    }
                }
            }
        }

        if extension {
            let mut actions = Vec::new();
            let mut offset = 0_i64;
            let mut last_gameloop = None;

            for event in events {
                let gameloop = DetailedReplayAnalyzer::event_gameloop(event);
                let kind = ReplayEventKind::from_event(event);

                if gameloop == 0
                    && kind == ReplayEventKind::GameTriggerDialogControl
                    && DetailedReplayAnalyzer::event_event_type(event) == Some(3)
                {
                    let contains_selection_changed = matches!(
                        event,
                        ReplayEvent::Game(event)
                            if event
                                .m_event_data
                                .as_ref()
                                .is_some_and(|data| data.contains_selection_changed)
                    );
                    if contains_selection_changed {
                        if let Some(control_id) = DetailedReplayAnalyzer::event_control_id(event) {
                            offset = 129 - control_id;
                        }
                        continue;
                    }
                }

                if gameloop > 0
                    && Some(gameloop) != last_gameloop
                    && kind == ReplayEventKind::GameTriggerDialogControl
                    && DetailedReplayAnalyzer::event_user_id(event) == Some(0)
                {
                    let contains_none = matches!(
                        event,
                        ReplayEvent::Game(event)
                            if event.m_event_data.as_ref().is_some_and(|data| data.contains_none)
                    );
                    if !contains_none {
                        if let Some(control_id) = DetailedReplayAnalyzer::event_control_id(event) {
                            actions.push(control_id + offset);
                            last_gameloop = Some(gameloop);
                        }
                        continue;
                    }
                }

                if let ReplayEvent::Tracker(event) = event {
                    if kind == ReplayEventKind::TrackerUpgrade
                        && matches!(event.m_player_id, Some(1 | 2))
                    {
                        let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                        if upgrade_name.contains("Spray") {
                            break;
                        }
                    }
                }
            }

            let mut panel = 1_i64;
            for action in actions {
                if (41..=83).contains(&action) {
                    if let Some(new_mutator) =
                        DetailedReplayAnalyzer::mutator_from_button(action, panel, mutators_ui)
                    {
                        if !mutators.contains(&new_mutator) || new_mutator == "Random" {
                            mutators.push(new_mutator);
                        } else if new_mutator != "Random" {
                            if let Some(position) =
                                mutators.iter().position(|value| value == &new_mutator)
                            {
                                mutators.remove(position);
                            }
                        }
                    }
                }

                if action == 123 && panel > 1 {
                    panel -= 1;
                }
                if action == 124 && panel < 4 {
                    panel += 1;
                }

                if (88..=106).contains(&action) {
                    if let Ok(index) = usize::try_from((action - 88) / 2) {
                        if index < mutators.len() {
                            mutators.remove(index);
                        }
                    }
                }
            }
        }

        (
            mutators
                .into_iter()
                .map(|mutator| {
                    mutator
                        .replace("Heroes from the Storm (old)", "Heroes from the Storm")
                        .replace("Extreme Caution", "Afraid of the Dark")
                })
                .collect(),
            weekly,
        )
    }
}

#[derive(Debug, Error)]
pub enum DetailedReplayAnalysisError {
    #[error("failed to build protocol store: {0}")]
    ProtocolStore(String),
    #[error("failed to parse replay '{path}': {message}")]
    ReplayParse { path: String, message: String },
    #[error("SC2 dictionary data directory was not found from '{0}'")]
    DictionaryDirNotFound(PathBuf),
    #[error("failed to read '{path}': {message}")]
    IoRead { path: PathBuf, message: String },
    #[error("failed to parse JSON '{path}': {message}")]
    JsonParse { path: PathBuf, message: String },
    #[error("invalid dictionary file '{file}': {message}")]
    InvalidDictionaryData { file: &'static str, message: String },
    #[error("invalid replay data: {0}")]
    InvalidReplayData(String),
}

#[derive(Debug, Clone)]
pub struct DetailedReplayAnalysisResult {
    report: ReplayReport,
    cache_entry: CacheReplayEntry,
    cache_persistable: bool,
}

impl DetailedReplayAnalysisResult {
    fn new(report: ReplayReport, cache_entry: CacheReplayEntry, cache_persistable: bool) -> Self {
        Self {
            report,
            cache_entry,
            cache_persistable,
        }
    }

    pub fn report(&self) -> &ReplayReport {
        &self.report
    }

    pub fn cache_entry(&self) -> &CacheReplayEntry {
        &self.cache_entry
    }

    pub fn into_cache_entry(self) -> CacheReplayEntry {
        self.cache_entry
    }

    pub fn cache_persistable(&self) -> bool {
        self.cache_persistable
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheConfig {
    account_dir: PathBuf,
    output_file: PathBuf,
    recent_replay_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheSummary {
    scanned_replays: usize,
    output_file: PathBuf,
    completed: bool,
}

#[derive(Debug, Default)]
pub struct GenerateCacheStopController {
    stop_requested: AtomicBool,
}

impl GenerateCacheStopController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_stop(&self) {
        self.stop_requested.store(true, AtomicOrdering::Release);
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested.load(AtomicOrdering::Acquire)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GenerateCacheRuntimeOptions {
    worker_count: Option<usize>,
    stop_controller: Option<Arc<GenerateCacheStopController>>,
}

impl GenerateCacheRuntimeOptions {
    pub fn with_worker_count(mut self, worker_count: usize) -> Self {
        self.worker_count = Some(worker_count);
        self
    }

    pub fn with_stop_controller(
        mut self,
        stop_controller: Arc<GenerateCacheStopController>,
    ) -> Self {
        self.stop_controller = Some(stop_controller);
        self
    }

    fn resolved_worker_count(&self, total_files: usize) -> usize {
        self.worker_count
            .map(|value| std::cmp::max(1, std::cmp::min(value, total_files)))
            .unwrap_or_else(|| Self::default_worker_count(total_files))
    }

    fn default_worker_count(total_files: usize) -> usize {
        std::cmp::max(1, std::cmp::min(Self::half_cpu_worker_cap(), total_files))
    }

    fn half_cpu_worker_cap() -> usize {
        let cpu_count = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        std::cmp::max(1, cpu_count / 2)
    }
}

#[derive(Debug, Error)]
pub enum GenerateCacheError {
    #[error("account directory does not exist or is not a directory: {0}")]
    InvalidAccountDirectory(PathBuf),
    #[error("failed to create output directory '{0}': {1}")]
    OutputDirectoryCreateFailed(PathBuf, #[source] io::Error),
    #[error("failed to load detailed-analysis cache formatting rules: {0}")]
    DetailedAnalysisConfig(String),
    #[error("failed to build rayon thread pool: {0}")]
    ThreadPoolBuildFailed(String),
    #[error("failed to serialize cache payload: {0}")]
    SerializeFailed(#[source] serde_json::Error),
    #[error("failed to write cache temp file '{0}': {1}")]
    TempWriteFailed(PathBuf, #[source] io::Error),
    #[error("failed to replace cache file '{1}' from temp '{0}': {2}")]
    TempMoveFailed(PathBuf, PathBuf, #[source] io::Error),
    #[error(transparent)]
    PrettyCache(#[from] PrettyCacheError),
    #[error("failed to read existing cache file '{0}': {1}")]
    ReadExistingCache(PathBuf, #[source] io::Error),
    #[error("failed to parse existing cache file '{0}': {1}")]
    ParseExistingCache(PathBuf, #[source] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FullAnalysisMode {
    Simple,
    Detailed,
}

impl DetailedReplayAnalyzer {
    pub fn analyze_full_simple(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        DetailedReplayAnalyzer::run_full_analysis(
            config,
            resources,
            logger,
            runtime,
            FullAnalysisMode::Simple,
        )
    }

    pub fn analyze_full_detailed(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        DetailedReplayAnalyzer::run_full_analysis(
            config,
            resources,
            logger,
            runtime,
            FullAnalysisMode::Detailed,
        )
    }

    pub fn analyze_single_detailed(
        replay_path: &Path,
        main_player_handles: &HashSet<String>,
        resources: &ReplayAnalysisResources,
    ) -> Result<DetailedReplayAnalysisResult, DetailedReplayAnalysisError> {
        let parsed = ReplayParsedInputBundle::parse_detailed_required(replay_path, resources)?;
        DetailedReplayAnalyzer::analyze_parsed_replay_with_cache_entry(
            parsed,
            main_player_handles,
            resources.hidden_created_lost(),
            None,
            resources,
        )
    }

    fn run_full_analysis(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
        mode: FullAnalysisMode,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        if !config.account_dir.is_dir() {
            return Err(GenerateCacheError::InvalidAccountDirectory(
                config.account_dir.clone(),
            ));
        }

        config.ensure_output_directory()?;
        let cache_output = DetailedReplayAnalyzer::analyze_replays_for_cache_output(
            config, logger, runtime, resources, mode,
        )?;
        CacheReplayEntry::write_entries(&cache_output.entries, &config.output_file)?;
        CacheOverallStatsFile::write_pretty_cache_file(&config.output_file, None)?;

        Ok(GenerateCacheSummary::new(
            cache_output.entries.len(),
            config.output_file.clone(),
            cache_output.completed,
        ))
    }
}

impl GenerateCacheConfig {
    pub fn new(account_dir: impl Into<PathBuf>, output_file: impl Into<PathBuf>) -> Self {
        Self {
            account_dir: account_dir.into(),
            output_file: output_file.into(),
            recent_replay_count: None,
        }
    }

    pub fn with_recent_replay_count(mut self, recent_replay_count: Option<usize>) -> Self {
        self.recent_replay_count = recent_replay_count;
        self
    }

    pub fn account_dir(&self) -> &Path {
        &self.account_dir
    }

    pub fn output_file(&self) -> &Path {
        &self.output_file
    }

    pub fn recent_replay_count(&self) -> Option<usize> {
        self.recent_replay_count
    }

    fn ensure_output_directory(&self) -> Result<(), GenerateCacheError> {
        if let Some(parent) = self.output_file.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                GenerateCacheError::OutputDirectoryCreateFailed(parent.to_path_buf(), error)
            })?;
        }
        Ok(())
    }

    pub fn collect_replay_files(&self) -> Vec<PathBuf> {
        DetailedReplayAnalyzer::collect_cache_replay_files(
            &self.account_dir,
            self.recent_replay_count,
        )
    }
}

impl GenerateCacheSummary {
    fn new(scanned_replays: usize, output_file: PathBuf, completed: bool) -> Self {
        Self {
            scanned_replays,
            output_file,
            completed,
        }
    }

    pub fn scanned_replays(&self) -> usize {
        self.scanned_replays
    }

    pub fn output_file(&self) -> &Path {
        &self.output_file
    }

    pub fn completed(&self) -> bool {
        self.completed
    }
}

struct GeneratedCacheOutput {
    entries: Vec<CacheReplayEntry>,
    completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayFileCandidate {
    path: PathBuf,
    modified: SystemTime,
}

impl ReplayFileCandidate {
    fn from_path(path: &Path) -> Self {
        let modified = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        Self {
            path: path.to_path_buf(),
            modified,
        }
    }

    fn compare_recent_first(left: &Self, right: &Self) -> std::cmp::Ordering {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.normalized_path().cmp(&right.normalized_path()))
    }

    fn normalized_path(&self) -> String {
        self.path.to_string_lossy().to_ascii_lowercase()
    }
}

struct GenerateCacheProgressReporter<'a> {
    logger: Option<&'a (dyn Fn(String) + Send + Sync + 'a)>,
    total_files: usize,
    report_interval: usize,
    temp_save_interval: usize,
    start_time: Instant,
    processed_files: AtomicUsize,
    next_report_target: AtomicUsize,
    next_temp_save_target: AtomicUsize,
    temp_file_path: PathBuf,
    temp_entries: std::sync::Mutex<Vec<CacheReplayEntry>>,
}

impl<'a> GenerateCacheProgressReporter<'a> {
    fn new(
        total_files: usize,
        initial_processed_files: usize,
        logger: Option<&'a (dyn Fn(String) + Send + Sync + 'a)>,
        temp_file_path: PathBuf,
    ) -> Self {
        let report_interval = if total_files <= 10 { 1 } else { 10 };
        let temp_save_interval = 100;
        let initial_processed_files = initial_processed_files.min(total_files);
        Self {
            logger,
            total_files,
            report_interval,
            temp_save_interval,
            start_time: Instant::now(),
            processed_files: AtomicUsize::new(initial_processed_files),
            next_report_target: AtomicUsize::new(Self::next_progress_target(
                total_files,
                report_interval,
                initial_processed_files,
            )),
            next_temp_save_target: AtomicUsize::new(Self::next_progress_target(
                total_files,
                temp_save_interval,
                initial_processed_files,
            )),
            temp_file_path,
            temp_entries: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn log_start(&self) {
        if self.total_files == 0 {
            self.log_completion();
            return;
        }

        self.emit("Starting detailed analysis!".to_string());
        self.emit(self.progress_message(self.processed_files.load(AtomicOrdering::Relaxed)));
    }

    fn record_processed_file(&self) {
        if self.logger.is_none() || self.total_files == 0 {
            return;
        }

        let processed = self.processed_files.fetch_add(1, AtomicOrdering::Relaxed) + 1;

        if processed == self.total_files {
            self.emit(self.progress_message(processed));
            let _ = self.save_temp_entries();
            return;
        }

        let mut target = self.next_report_target.load(AtomicOrdering::Relaxed);
        while processed >= target {
            let next_target = target.saturating_add(self.report_interval);
            match self.next_report_target.compare_exchange(
                target,
                next_target,
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
            ) {
                Ok(_) => {
                    self.emit(self.progress_message(processed));
                    break;
                }
                Err(current) => {
                    target = current;
                }
            }
        }

        let mut temp_target = self.next_temp_save_target.load(AtomicOrdering::Relaxed);
        while processed >= temp_target {
            let next_temp_target = temp_target.saturating_add(self.temp_save_interval);
            match self.next_temp_save_target.compare_exchange(
                temp_target,
                next_temp_target,
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
            ) {
                Ok(_) => {
                    if let Err(error) = self.save_temp_entries() {
                        self.emit(format!("Warning: failed to save temp entries: {error}"));
                    }
                    break;
                }
                Err(current) => {
                    temp_target = current;
                }
            }
        }
    }

    fn log_completion(&self) {
        self.emit(format!(
            "Detailed analysis completed! {}/{} | 100%",
            self.total_files, self.total_files
        ));
        self.emit(format!(
            "Detailed analysis completed in {:.0} seconds!",
            self.start_time.elapsed().as_secs_f64()
        ));
    }

    fn add_temp_entry(&self, entry: CacheReplayEntry) {
        if let Ok(mut temp_entries) = self.temp_entries.lock() {
            temp_entries.push(entry);
        }
    }

    fn save_temp_entries(&self) -> Result<(), std::io::Error> {
        let entries = match self.temp_entries.lock() {
            Ok(mut temp_entries) => temp_entries.drain(..).collect::<Vec<_>>(),
            Err(_) => return Ok(()),
        };

        if entries.is_empty() {
            return Ok(());
        }

        let mut content = String::new();
        for entry in entries {
            if let Ok(json) = serde_json::to_string(&entry) {
                content.push_str(&json);
                content.push('\n');
            }
        }

        if !content.is_empty() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.temp_file_path)?
                .write_all(content.as_bytes())?;
        }

        Ok(())
    }

    fn emit(&self, message: String) {
        if let Some(logger) = self.logger {
            logger(message);
        }
    }

    fn progress_message(&self, processed: usize) -> String {
        let percent = Self::progress_percent(processed, self.total_files);
        if processed >= self.report_interval && processed < self.total_files {
            format!(
                "Estimated remaining time: {}\nRunning... {processed}/{} ({percent}%)",
                Self::format_eta_duration(self.estimate_remaining(processed)),
                self.total_files,
            )
        } else {
            format!("Running... {processed}/{} ({percent}%)", self.total_files)
        }
    }

    fn estimate_remaining(&self, processed: usize) -> Duration {
        if processed == 0 || processed >= self.total_files {
            return Duration::ZERO;
        }

        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed <= 0.0 {
            return Duration::ZERO;
        }

        let average_seconds_per_replay = elapsed / processed as f64;
        Duration::from_secs_f64(
            average_seconds_per_replay * self.total_files.saturating_sub(processed) as f64,
        )
    }

    fn next_progress_target(
        total_files: usize,
        report_interval: usize,
        processed_files: usize,
    ) -> usize {
        if total_files == 0 || processed_files >= total_files {
            return total_files;
        }
        if processed_files == 0 {
            return report_interval.min(total_files);
        }

        let remainder = processed_files % report_interval;
        if remainder == 0 {
            processed_files
                .saturating_add(report_interval)
                .min(total_files)
        } else {
            processed_files
                .saturating_add(report_interval - remainder)
                .min(total_files)
        }
    }

    fn progress_percent(processed: usize, total: usize) -> usize {
        if total == 0 {
            return 100;
        }

        (((processed as f64 / total as f64) * 100.0).round() as usize).min(100)
    }

    fn format_eta_duration(duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

#[derive(Debug, Clone)]
struct CandidateReplay {
    path: PathBuf,
    parsed: ReplayParsedInputBundle,
}

impl CandidateReplay {
    fn collect(replay_path: &Path, resources: &ReplayAnalysisResources) -> Option<Self> {
        let (_, parsed) = CacheReplayEntry::parse_with_options(
            replay_path,
            resources,
            ReplayBaseParseOptions {
                include_events: false,
                filters: ReplayBaseParseFilters::saved_cache(),
            },
        )?;

        Some(Self {
            path: replay_path.to_path_buf(),
            parsed,
        })
    }

    fn analyze(
        self,
        main_handles: &HashSet<String>,
        resources: &ReplayAnalysisResources,
    ) -> CacheReplayEntry {
        let Self { path, parsed } = self;
        let basic = parsed.cache_entry();

        let detailed = parsed
            .with_detailed_events(&path, resources)
            .and_then(|parsed| {
                DetailedReplayAnalyzer::analyze_parsed_replay_with_cache_entry(
                    parsed,
                    main_handles,
                    resources.hidden_created_lost(),
                    Some(&basic),
                    resources,
                )
            });

        if let Ok(result) = detailed {
            if result.report().has_non_empty_player_stats() {
                return result.into_cache_entry();
            }
        }

        basic
    }

    fn partition_cached(
        candidates: Vec<Self>,
        existing_entries: &HashMap<String, CacheReplayEntry>,
    ) -> (HashMap<String, CacheReplayEntry>, Vec<(String, Self)>) {
        let mut reused_entries = HashMap::new();
        let mut pending_candidates = Vec::new();

        for candidate in candidates {
            let hash = candidate.parsed.parser.hash.clone().unwrap_or_default();
            if hash.is_empty() {
                pending_candidates.push((hash, candidate));
                continue;
            }

            if let Some(existing_entry) = existing_entries.get(&hash) {
                reused_entries.insert(hash, existing_entry.refreshed_for_parsed(&candidate.parsed));
            } else {
                pending_candidates.push((hash, candidate));
            }
        }

        (reused_entries, pending_candidates)
    }
}

impl DetailedReplayAnalyzer {
    fn collect_cache_replay_files(
        account_dir: &Path,
        recent_replay_count: Option<usize>,
    ) -> Vec<PathBuf> {
        let mut replay_files = WalkDir::new(account_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.path().to_path_buf())
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension == "SC2Replay")
            })
            .collect::<Vec<PathBuf>>();

        if let Some(recent_replay_count) = recent_replay_count {
            let mut candidates = replay_files
                .into_iter()
                .map(|path| ReplayFileCandidate::from_path(path.as_path()))
                .collect::<Vec<ReplayFileCandidate>>();
            candidates.sort_unstable_by(ReplayFileCandidate::compare_recent_first);
            candidates.truncate(recent_replay_count);
            return candidates
                .into_iter()
                .map(|candidate| candidate.path)
                .collect::<Vec<PathBuf>>();
        }

        replay_files.sort_by(|left, right| {
            let left_norm = left.to_string_lossy().to_ascii_lowercase();
            let right_norm = right.to_string_lossy().to_ascii_lowercase();
            left_norm.cmp(&right_norm)
        });
        replay_files
    }

    fn resolve_main_handles(account_dir: &Path) -> HashSet<String> {
        let scan_root = DetailedReplayAnalyzer::main_handle_scan_root(account_dir);
        let mut handles = HashSet::new();

        for entry in WalkDir::new(&scan_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir())
        {
            if DetailedReplayAnalyzer::path_contains_component(entry.path(), "Banks") {
                continue;
            }

            let directory_name = entry.file_name().to_string_lossy();
            if directory_name.matches('-').count() < 3 {
                continue;
            }
            if directory_name.contains("Crash")
                || directory_name.contains("Desync")
                || directory_name.contains("Error")
            {
                continue;
            }

            handles.insert(directory_name.to_string());
        }

        handles
    }

    fn main_handle_scan_root(account_dir: &Path) -> PathBuf {
        let mut folder = account_dir.to_path_buf();
        loop {
            let Some(parent) = folder.parent() else {
                break;
            };
            if parent.to_string_lossy().contains("StarCraft") {
                folder = parent.to_path_buf();
            } else {
                break;
            }
        }
        folder
    }

    fn path_contains_component(path: &Path, target: &str) -> bool {
        path.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|value| value == target)
        })
    }

    fn cache_output_temp_file_path(output_file: &Path) -> PathBuf {
        output_file.with_extension("temp.jsonl")
    }

    fn analyze_replays_for_cache_output(
        config: &GenerateCacheConfig,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
        resources: &ReplayAnalysisResources,
        mode: FullAnalysisMode,
    ) -> Result<GeneratedCacheOutput, GenerateCacheError> {
        let replay_files = DetailedReplayAnalyzer::collect_cache_replay_files(
            &config.account_dir,
            config.recent_replay_count,
        );
        if mode == FullAnalysisMode::Simple {
            return DetailedReplayAnalyzer::analyze_simple_replays_for_cache_output(
                replay_files,
                runtime,
                resources,
            );
        }

        let main_handles = DetailedReplayAnalyzer::resolve_main_handles(&config.account_dir);
        let existing_detailed_cache_entries =
            CacheReplayEntry::load_existing_detailed_cache_entries(
                config.output_file.as_path(),
                logger,
            );
        let temp_file_path =
            DetailedReplayAnalyzer::cache_output_temp_file_path(config.output_file.as_path());

        let stop_controller = runtime.stop_controller.clone();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let entries = if replay_files.is_empty() {
            let progress = GenerateCacheProgressReporter::new(0, 0, logger, temp_file_path.clone());
            progress.log_completion();
            HashMap::new()
        } else {
            let worker_count = runtime.resolved_worker_count(replay_files.len());
            let thread_pool = ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .build()
                .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))?;
            let stop_requested_for_candidates = stop_requested.clone();
            let stop_controller_for_candidates = stop_controller.clone();
            let candidate_replays = thread_pool.install(|| {
                replay_files
                    .par_iter()
                    .filter_map(|path| {
                        if stop_controller_for_candidates
                            .as_ref()
                            .is_some_and(|controller| controller.stop_requested())
                        {
                            stop_requested_for_candidates.store(true, AtomicOrdering::Release);
                            return None;
                        }
                        CandidateReplay::collect(path, &resources)
                    })
                    .collect::<Vec<CandidateReplay>>()
            });
            let total_candidates = candidate_replays.len();
            let (mut reused_entries, pending_candidates) = CandidateReplay::partition_cached(
                candidate_replays,
                &existing_detailed_cache_entries,
            );
            let progress = Arc::new(GenerateCacheProgressReporter::new(
                total_candidates,
                reused_entries.len(),
                logger,
                temp_file_path.clone(),
            ));

            if total_candidates == 0 {
                progress.log_completion();
                HashMap::new()
            } else {
                progress.log_start();
                let analyzed_entries = if pending_candidates.is_empty() {
                    HashMap::new()
                } else {
                    let progress_for_workers = Arc::clone(&progress);
                    let stop_requested_for_workers = stop_requested.clone();
                    let stop_controller_for_workers = stop_controller.clone();

                    thread_pool.install(|| {
                        pending_candidates
                            .into_par_iter()
                            .filter_map(|(_, candidate)| {
                                if stop_controller_for_workers
                                    .as_ref()
                                    .is_some_and(|controller| controller.stop_requested())
                                {
                                    stop_requested_for_workers.store(true, AtomicOrdering::Release);
                                    return None;
                                }
                                let entry = candidate.analyze(&main_handles, &resources);
                                if entry.detailed_analysis {
                                    progress_for_workers.add_temp_entry(entry.clone());
                                }
                                progress_for_workers.record_processed_file();
                                Some((entry.hash.clone(), entry))
                            })
                            .collect::<HashMap<_, _>>()
                    })
                };

                reused_entries.extend(analyzed_entries);
                if stop_requested.load(AtomicOrdering::Acquire) {
                    if let Some(logger) = logger {
                        logger(
                            "Detailed analysis stopped after the current work finished."
                                .to_string(),
                        );
                    }
                } else {
                    progress.log_completion();
                }
                reused_entries
            }
        };

        let mut all_entries = if config.recent_replay_count.is_some() {
            HashMap::new()
        } else {
            existing_detailed_cache_entries
        };
        all_entries.extend(entries);

        let mut all_entries = all_entries.into_values().collect::<Vec<_>>();
        all_entries.sort_by(|left, right| left.cmp_cache_order(right));

        if temp_file_path.exists() {
            let _ = fs::remove_file(&temp_file_path);
        }

        Ok(GeneratedCacheOutput {
            entries: all_entries,
            completed: !stop_requested.load(AtomicOrdering::Acquire),
        })
    }

    fn analyze_simple_replays_for_cache_output(
        replay_files: Vec<PathBuf>,
        runtime: &GenerateCacheRuntimeOptions,
        resources: &ReplayAnalysisResources,
    ) -> Result<GeneratedCacheOutput, GenerateCacheError> {
        if replay_files.is_empty() {
            return Ok(GeneratedCacheOutput {
                entries: Vec::new(),
                completed: true,
            });
        }

        let worker_count = runtime.resolved_worker_count(replay_files.len());
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
            .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))?;
        let stop_controller = runtime.stop_controller.clone();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop_requested_for_workers = stop_requested.clone();
        let entries = thread_pool.install(|| {
            replay_files
                .par_iter()
                .filter_map(|path| {
                    if stop_controller
                        .as_ref()
                        .is_some_and(|controller| controller.stop_requested())
                    {
                        stop_requested_for_workers.store(true, AtomicOrdering::Release);
                        return None;
                    }

                    CacheReplayEntry::parse_with_options(
                        path,
                        resources,
                        ReplayBaseParseOptions {
                            include_events: false,
                            filters: ReplayBaseParseFilters::saved_cache(),
                        },
                    )
                    .map(|(entry, _)| entry)
                })
                .collect::<Vec<CacheReplayEntry>>()
        });

        let mut entries = entries;
        entries.sort_by(|left, right| left.cmp_cache_order(right));

        Ok(GeneratedCacheOutput {
            entries,
            completed: !stop_requested.load(AtomicOrdering::Acquire),
        })
    }
}

impl ReplayParsedInputBundle {
    fn parser_players_from_all_players(players: &[ParsedReplayPlayer]) -> Vec<ParsedReplayPlayer> {
        ParsedReplayPlayer::normalize_slots(players, true, Some(2))
    }

    fn normalized_cache_players(&self) -> Vec<ParsedReplayPlayer> {
        ParsedReplayPlayer::normalize_slots(&self.all_players, true, None)
    }

    fn normalized_cache_messages(&self) -> Vec<ParsedReplayMessage> {
        let user_leave_times = self
            .detailed
            .as_ref()
            .map(|context| DetailedReplayAnalyzer::collect_user_leave_times(&context.events))
            .unwrap_or_default();
        ParsedReplayMessage::sorted_with_leave_events(&self.parser.messages, &user_leave_times)
    }

    fn cache_entry(&self) -> CacheReplayEntry {
        CacheReplayEntry::from_parsed_bundle(self)
    }

    fn supports_cache_filters(&self, filters: ReplayBaseParseFilters) -> bool {
        if filters.only_blizzard
            && (self.cache_context.is_mm_replay || !self.cache_context.is_blizzard_map)
        {
            return false;
        }

        if filters.require_recover_disabled && !self.cache_context.recover_disabled {
            return false;
        }

        true
    }

    fn is_cache_candidate(&self, filters: ReplayBaseParseFilters) -> bool {
        self.supports_cache_filters(filters)
            && self.parser.accurate_length != 0.0
            && (!filters.only_blizzard || self.commander_found)
    }

    fn is_saved_cache_candidate(&self) -> bool {
        self.is_cache_candidate(ReplayBaseParseFilters::saved_cache())
    }

    fn from_base_parse(
        base: ReplayBaseParse,
        dictionaries: CacheGenerationData<'_>,
    ) -> Result<Self, ReplayBaseParseError> {
        let details = &base.context.details;
        let init_data = &base.context.init_data;
        let metadata = &base.context.metadata;
        let player_list = if details.m_playerList.is_empty() {
            Err(ReplayBaseParseError::InvalidReplayData(
                "details player list must be array".to_string(),
            ))
        } else {
            Ok(&details.m_playerList)
        }?;

        let length = metadata.Duration;
        let accurate_length = base.accurate_length;
        let game_speed_code = DetailedReplayAnalyzer::replay_game_speed_code(details, init_data);
        let metadata_player_apm = metadata
            .Players
            .iter()
            .map(|player| player.APM)
            .collect::<Vec<_>>();
        let cache_context = ReplayCacheContext {
            is_mm_replay: base.file.contains("[MM]"),
            is_blizzard_map: details.m_isBlizzardMap,
            recover_disabled: details.m_disableRecoverGame.unwrap_or(false),
        };

        let mut all_players = metadata
            .Players
            .iter()
            .enumerate()
            .map(|(index, player)| {
                let pid = (index + 1) as u8;
                let apm = if accurate_length == 0.0 {
                    0
                } else {
                    (player.APM * length / accurate_length).round_ties_even() as u32
                };
                ParsedReplayPlayer {
                    pid,
                    apm,
                    result: player.Result.clone(),
                    ..ParsedReplayPlayer::empty(pid)
                }
            })
            .collect::<Vec<_>>();

        let mut region = String::new();
        for (index, player) in player_list.iter().enumerate() {
            let Some(target) = all_players.get_mut(index) else {
                continue;
            };
            target.name = player.m_name.clone();
            target.race = player.m_race.clone();
            target.observer = player.m_observe != 0;

            if index == 0 {
                let region_code = player
                    .m_toon
                    .as_ref()
                    .map(|value| value.m_region)
                    .unwrap_or_default();
                region = DetailedReplayAnalyzer::region_name(region_code).to_string();
            }
        }

        let slots = &init_data.m_syncLobbyState.m_lobbyState.m_slots;
        let mut commander_found = false;
        for (index, slot) in slots.iter().enumerate() {
            let Some(target) = all_players.get_mut(index) else {
                continue;
            };
            let commander = slot.m_commander.clone();
            let commander_level = slot.m_commanderLevel;
            let commander_mastery_level = slot.m_commanderMasteryLevel;
            let prestige = slot.m_selectedCommanderPrestige;
            target.commander = commander.clone();
            target.commander_level = commander_level as u32;
            target.commander_mastery_level = commander_mastery_level as u32;
            target.prestige = prestige as u32;
            target.prestige_name = dictionaries
                .prestige_names
                .get(&commander)
                .and_then(|row| row.get(&prestige))
                .cloned()
                .unwrap_or_default();
            target.handle = slot.m_toonHandle.clone();
            target.masteries =
                DetailedReplayAnalyzer::parse_masteries(&slot.m_commanderMasteryTalents);

            if !commander.is_empty() {
                commander_found = true;
            }
        }

        let user_initial = &init_data.m_syncLobbyState.m_userInitialData;
        for (index, user) in user_initial.iter().enumerate() {
            let Some(target) = all_players.get_mut(index) else {
                continue;
            };
            let user_name = user.m_name.clone();
            if !user_name.is_empty() {
                target.name = user_name;
            }
        }

        let enemy_race_present = all_players.get(2).is_some();
        let enemy_race = all_players
            .get(2)
            .map(|player| player.race.clone())
            .unwrap_or_default();

        let difficulty_from_slot =
            |index: usize| -> Option<i64> { slots.get(index).map(|slot| slot.m_difficulty) };
        let mut diff_1_code = difficulty_from_slot(2);
        let mut diff_2_code = difficulty_from_slot(3);
        if diff_1_code.is_none() {
            diff_1_code = difficulty_from_slot(0).or_else(|| difficulty_from_slot(1));
        }
        if diff_2_code.is_none() {
            diff_2_code = difficulty_from_slot(1);
        }
        let diff_1_name =
            DetailedReplayAnalyzer::difficulty_name(diff_1_code.unwrap_or(4)).to_string();
        let diff_2_name =
            DetailedReplayAnalyzer::difficulty_name(diff_2_code.unwrap_or(4)).to_string();
        let ext_difficulty = if base.brutal_plus > 0 {
            format!("B+{}", base.brutal_plus)
        } else if diff_1_name == diff_2_name {
            diff_1_name.clone()
        } else {
            format!("{diff_1_name}/{diff_2_name}")
        };

        let parser = ParsedReplayInput {
            file: base.file,
            map_name: base.map_name,
            extension: base.extension,
            brutal_plus: base.brutal_plus,
            result: base.result,
            players: Self::parser_players_from_all_players(&all_players),
            difficulty: (diff_1_name, diff_2_name),
            accurate_length,
            form_alength: base.form_alength,
            length: base.length,
            mutators: base.mutators,
            weekly: base.weekly,
            messages: base.raw_messages,
            hash: Some(base.hash),
            build: base.build,
            date: base.date,
            enemy_race,
            ext_difficulty,
            region,
        };

        Ok(Self {
            parser,
            all_players,
            accurate_length_force_float: base.accurate_length_force_float,
            metadata_duration: length,
            metadata_player_apm,
            game_speed_code,
            mutator_context: base.mutator_context,
            realtime_length: base.realtime_length,
            commander_found,
            enemy_race_present,
            cache_context,
            detailed: base.detailed,
        })
    }

    fn apply_event_derived_fields(
        &mut self,
        events: Vec<ReplayEvent>,
        dictionaries: CacheGenerationData<'_>,
    ) {
        let start_time = DetailedReplayAnalyzer::get_start_time(&events);
        let last_deselect_event = DetailedReplayAnalyzer::get_last_deselect_event(&events)
            .unwrap_or(ReplayNumericValue::Float(self.metadata_duration));
        let length_numeric = ReplayNumericValue::Float(self.metadata_duration);
        let accurate_length_numeric = if self.parser.result == "Victory" {
            last_deselect_event.subtract(&start_time)
        } else {
            length_numeric.subtract(&start_time)
        };
        let accurate_length = accurate_length_numeric.as_f64();
        let end_time = if self.parser.result == "Victory" {
            last_deselect_event.as_f64()
        } else {
            self.metadata_duration
        };
        let (mutators, weekly) = DetailedReplayAnalyzer::identify_mutators_for_replay(
            &events,
            &dictionaries.mutators_all,
            &dictionaries.mutators_ui,
            &dictionaries.mutator_ids,
            &dictionaries.cached_mutators,
            self.parser.extension,
            self.cache_context.is_mm_replay,
            Some(&self.mutator_context),
        );

        self.parser.accurate_length = accurate_length;
        self.parser.form_alength = DetailedReplayAnalyzer::format_duration(accurate_length);
        self.parser.mutators = mutators;
        self.parser.weekly = weekly;
        self.accurate_length_force_float =
            matches!(accurate_length_numeric, ReplayNumericValue::Float(_));
        self.realtime_length = DetailedReplayAnalyzer::realtime_length_from_game_speed(
            accurate_length,
            self.game_speed_code,
        );
        self.recompute_player_apm();
        self.detailed = Some(ReplayDetailedParseContext {
            events,
            start_time: start_time.as_f64(),
            end_time,
        });
    }

    fn recompute_player_apm(&mut self) {
        for (index, player) in self.all_players.iter_mut().enumerate() {
            let raw_apm = self
                .metadata_player_apm
                .get(index)
                .copied()
                .unwrap_or_default();
            player.apm = if self.parser.accurate_length == 0.0 {
                0
            } else {
                (raw_apm * self.metadata_duration / self.parser.accurate_length).round_ties_even()
                    as u32
            };
        }
        self.parser.players = Self::parser_players_from_all_players(&self.all_players);
    }

    fn with_detailed_events(
        mut self,
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Result<Self, DetailedReplayAnalysisError> {
        let events = ReplayParser::parse_ordered_events_with_store_filtered(
            replay_path,
            resources.protocol_store(),
            ReplayEventKind::needed_for_detailed_analysis_name,
        )
        .map_err(|error| DetailedReplayAnalysisError::ReplayParse {
            path: replay_path.display().to_string(),
            message: error.to_string(),
        })?;
        self.apply_event_derived_fields(events, resources.cache_generation_data());
        Ok(self)
    }

    fn parse(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        options: ReplayBaseParseOptions,
    ) -> Result<Option<Self>, ReplayBaseParseError> {
        let Some(base) = resources.parse_replay_base(replay_path, options)? else {
            return Ok(None);
        };

        Self::from_base_parse(base, resources.cache_generation_data()).map(Some)
    }

    fn parse_detailed_required(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Result<Self, DetailedReplayAnalysisError> {
        Self::parse(
            replay_path,
            resources,
            ReplayBaseParseOptions {
                include_events: true,
                ..ReplayBaseParseOptions::default()
            },
        )
        .map_err(ReplayBaseParseError::into_detailed_analysis_error)?
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData(
                "detailed replay parsing unexpectedly skipped the replay".to_string(),
            )
        })
    }
}

impl CacheReplayEntry {
    pub fn parse_basic_with_resources(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Option<Self> {
        Self::parse_with_options(
            replay_path,
            resources,
            ReplayBaseParseOptions {
                include_events: false,
                filters: ReplayBaseParseFilters::saved_cache(),
            },
        )
        .map(|(entry, _)| entry)
    }
    fn from_parsed_bundle(parsed: &ReplayParsedInputBundle) -> Self {
        let players = parsed.normalized_cache_players();
        let messages = parsed.normalized_cache_messages();
        Self::from_parser_projection(
            &parsed.parser,
            &players,
            &messages,
            parsed.accurate_length_force_float,
            parsed.enemy_race_present,
            false,
        )
    }

    fn parse_with_options(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        options: ReplayBaseParseOptions,
    ) -> Option<(Self, ReplayParsedInputBundle)> {
        let parsed = ReplayParsedInputBundle::parse(replay_path, resources, options)
            .ok()
            .flatten()?;

        if !parsed.is_cache_candidate(options.filters) {
            return None;
        }

        let entry = parsed.cache_entry();
        Some((entry, parsed))
    }

    fn refreshed_for_parsed(&self, parsed: &ReplayParsedInputBundle) -> Self {
        let mut reused_entry = self.clone();
        reused_entry.file =
            CacheOverallStatsFile::normalized_path_string(Path::new(&parsed.parser.file));
        reused_entry.hash = parsed.parser.hash.clone().unwrap_or_default();
        reused_entry
    }
}

impl DetailedReplayAnalyzer {
    fn map_name_has_amon_override(map_name: &str, candidate: &str) -> bool {
        map_name.contains(candidate)
            || (map_name.contains("[MM] Lnl") && candidate == "Lock & Load")
    }

    fn replay_unitid_from_event(event: &TrackerEvent, killer: bool, creator: bool) -> Option<i64> {
        let (index, recycle_index) = if killer {
            (
                event.m_killer_unit_tag_index?,
                event.m_killer_unit_tag_recycle?,
            )
        } else if creator {
            (
                event.m_creator_unit_tag_index?,
                event.m_creator_unit_tag_recycle?,
            )
        } else {
            (event.m_unit_tag_index?, event.m_unit_tag_recycle?)
        };
        Some(recycle_index * 100_000 + index)
    }

    fn clamp_nonnegative_to_u64(value: i64) -> u64 {
        if value <= 0 {
            0
        } else {
            value as u64
        }
    }

    fn count_for_pid(values: &[i64], pid: i64) -> i64 {
        usize::try_from(pid)
            .ok()
            .and_then(|index| values.get(index))
            .copied()
            .unwrap_or_default()
    }

    fn round_to_digits_half_even(value: f64, digits: i32) -> f64 {
        if !value.is_finite() {
            return value;
        }
        let Ok(digits_u32) = u32::try_from(digits) else {
            return value;
        };
        let Some(scale10) = 10_u128.checked_pow(digits_u32) else {
            return value;
        };

        let bits = value.to_bits();
        let sign_negative = (bits >> 63) != 0;
        let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
        let mantissa_bits = bits & ((1_u64 << 52) - 1);

        let (mantissa, exponent2) = if exponent_bits == 0 {
            (mantissa_bits as u128, -1074_i32)
        } else {
            (
                (mantissa_bits | (1_u64 << 52)) as u128,
                exponent_bits - 1075,
            )
        };
        if mantissa == 0 {
            return if sign_negative { -0.0 } else { 0.0 };
        }

        let Some(mut numerator) = mantissa.checked_mul(scale10) else {
            return value;
        };
        let mut denominator = 1_u128;
        if exponent2 >= 0 {
            let Ok(shift) = u32::try_from(exponent2) else {
                return value;
            };
            let Some(shifted) = numerator.checked_shl(shift) else {
                return value;
            };
            numerator = shifted;
        } else {
            let Ok(shift) = u32::try_from(-exponent2) else {
                return value;
            };
            let Some(shifted) = denominator.checked_shl(shift) else {
                return 0.0;
            };
            denominator = shifted;
        }

        let quotient = numerator / denominator;
        let remainder = numerator % denominator;
        let rounded = match remainder.checked_mul(2) {
            Some(double_remainder) if double_remainder < denominator => quotient,
            Some(double_remainder) if double_remainder > denominator => quotient + 1,
            Some(_) => {
                if quotient % 2 == 0 {
                    quotient
                } else {
                    quotient + 1
                }
            }
            None => quotient,
        };

        let factor = 10_f64.powi(digits);
        if !factor.is_finite() || factor == 0.0 {
            return value;
        }

        let rounded_value = rounded as f64 / factor;
        if sign_negative {
            -rounded_value
        } else {
            rounded_value
        }
    }

    fn format_mm_ss(seconds: f64) -> String {
        if !seconds.is_finite() || seconds <= 0.0 {
            return "00:00".to_string();
        }
        let total = seconds as u64;
        let minutes = (total / 60) % 60;
        let secs = total % 60;
        format!("{minutes:02}:{secs:02}")
    }

    fn contains_skip_strings_text(unit_name: &str, skip_tokens: &[String]) -> bool {
        let lowered = unit_name.to_lowercase();
        skip_tokens.iter().any(|token| lowered.contains(token))
    }

    fn increment_icon_count(icons: &mut BTreeMap<String, u64>, key: &str, delta: i64) {
        if delta == 0 {
            return;
        }

        let current = icons.get(key).copied().unwrap_or_default() as i64;
        let next = current + delta;
        if next <= 0 {
            icons.remove(key);
        } else {
            icons.insert(key.to_string(), next as u64);
        }
    }

    fn set_icon_count(icons: &mut BTreeMap<String, u64>, key: &str, value: i64) {
        if value > 0 {
            icons.insert(key.to_string(), value as u64);
        } else {
            icons.remove(key);
        }
    }

    fn build_stats_counter_dictionaries(
        dictionaries: &CacheGenerationData<'_>,
    ) -> StatsCounterDictionaries {
        StatsCounterDictionaries {
            unit_base_costs: dictionaries.unit_base_costs.clone(),
            royal_guards: dictionaries.royal_guards.clone(),
            horners_units: dictionaries.horners_units.clone(),
            tychus_base_upgrades: dictionaries.tychus_base_upgrades.clone(),
            tychus_ultimate_upgrades: dictionaries.tychus_ultimate_upgrades.clone(),
            outlaws: dictionaries.outlaws.clone(),
        }
    }

    fn switched_unit_counts(
        counts: &UnitTypeCountMap,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        dont_include_units: &HashSet<String>,
    ) -> HashMap<String, [i64; 4]> {
        let mut switched: HashMap<String, [i64; 4]> = HashMap::new();

        for (unit_name, values) in counts {
            if dont_include_units.contains(unit_name) {
                continue;
            }

            let mut added = false;
            if let Some(target) = unit_add_kills_to.get(unit_name) {
                let entry = switched.entry(target.clone()).or_insert([0_i64; 4]);
                entry[2] += values[2];
                added = true;
            }

            if let Some(target) = unit_add_losses_to.get(unit_name) {
                let entry = switched.entry(target.clone()).or_insert([0_i64; 4]);
                entry[1] += values[1];
                added = true;
            }

            if !added {
                let mapped_name = unit_name_dict
                    .get(unit_name)
                    .cloned()
                    .unwrap_or_else(|| unit_name.clone());
                let entry = switched.entry(mapped_name).or_insert([0_i64; 4]);
                for (index, value) in values.iter().enumerate() {
                    entry[index] += *value;
                }
            }
        }

        switched
    }

    fn sorted_switch_name_entries(
        counts: &UnitTypeCountMap,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        dont_include_units: &HashSet<String>,
    ) -> Vec<(String, i64, i64, i64)> {
        let mut rows = DetailedReplayAnalyzer::switched_unit_counts(
            counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            dont_include_units,
        )
        .into_iter()
        .map(|(unit_name, values)| (unit_name, values[0], values[1], values[2]))
        .collect::<Vec<(String, i64, i64, i64)>>();

        rows.sort_by(|left, right| {
            right
                .3
                .cmp(&left.3)
                .then_with(|| right.1.cmp(&left.1))
                .then_with(|| left.0.cmp(&right.0))
        });
        rows
    }

    fn unit_stats_tuple(created: i64, lost: i64, kills: i64, kill_fraction: f64) -> UnitStats {
        (created, lost, kills, kill_fraction)
    }

    fn fill_unit_kills_and_icons(
        base_icons: &BTreeMap<String, u64>,
        player: i64,
        main_player: i64,
        unit_counts: &UnitTypeCountMap,
        ally_kills_counted_toward_main: i64,
        killcounts: &[i64],
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        dont_include_units: &HashSet<String>,
        icon_units: &HashSet<String>,
    ) -> (BTreeMap<String, UnitStats>, BTreeMap<String, u64>) {
        let mut icons = base_icons.clone();
        for (unit_name, values) in unit_counts {
            let created = values[0];
            if LOCUST_SOURCE_UNITS.contains(&unit_name.as_str()) {
                DetailedReplayAnalyzer::increment_icon_count(&mut icons, "locust", created);
            } else if BROODLING_SOURCE_UNITS.contains(&unit_name.as_str()) {
                DetailedReplayAnalyzer::increment_icon_count(&mut icons, "broodling", created);
            }
        }

        for icon_key in ["broodling", "locust"] {
            let count = icons.get(icon_key).copied().unwrap_or_default();
            if count > 0 && count < 200 {
                icons.remove(icon_key);
            }
        }

        let rows = DetailedReplayAnalyzer::sorted_switch_name_entries(
            unit_counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            dont_include_units,
        );
        let player_kills = DetailedReplayAnalyzer::count_for_pid(killcounts, player);
        let dehaka_created_lost = rows
            .iter()
            .find(|(unit_name, _, _, _)| unit_name == "Dehaka")
            .map(|(_, created, lost, _)| (*created, *lost));

        let mut units = BTreeMap::new();
        for (unit_name, mut created, mut lost, kills) in rows {
            let denominator = if ally_kills_counted_toward_main > 0 && player != main_player {
                player_kills + ally_kills_counted_toward_main
            } else if ally_kills_counted_toward_main > 0 && player == main_player {
                player_kills - ally_kills_counted_toward_main
            } else {
                player_kills
            };

            let kill_fraction = if denominator > 0 {
                DetailedReplayAnalyzer::round_to_digits_half_even(
                    kills as f64 / denominator as f64,
                    2,
                )
            } else {
                0.0
            };

            if unit_name == "Zweihaka" {
                if let Some((dehaka_created, dehaka_lost)) = dehaka_created_lost {
                    created = dehaka_created;
                    lost = dehaka_lost;
                }
            }

            units.insert(
                unit_name.clone(),
                DetailedReplayAnalyzer::unit_stats_tuple(created, lost, kills, kill_fraction),
            );

            if icon_units.contains(&unit_name) {
                DetailedReplayAnalyzer::set_icon_count(&mut icons, &unit_name, created);
            }
        }

        let mut artifacts_collected = 0_i64;
        for (unit_name, values) in unit_counts {
            let created = values[0];
            let lost = values[1];

            if ZERATUL_ARTIFACT_PICKUPS.contains(&unit_name.as_str()) {
                artifacts_collected += lost;
            }
            if ZERATUL_SHADE_PROJECTIONS.contains(&unit_name.as_str()) {
                DetailedReplayAnalyzer::increment_icon_count(
                    &mut icons,
                    "ShadeProjection",
                    created,
                );
            }
        }
        if artifacts_collected > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut icons, "Artifact", artifacts_collected);
        }

        (units, icons)
    }

    fn fill_amon_units(
        unit_counts: &UnitTypeCountMap,
        killcounts: &[i64],
        amon_players: &HashSet<i64>,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        dont_include_units: &HashSet<String>,
        skip_tokens: &[String],
    ) -> BTreeMap<String, UnitStats> {
        let rows = DetailedReplayAnalyzer::sorted_switch_name_entries(
            unit_counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            dont_include_units,
        );

        let mut total_amon_kills = amon_players
            .iter()
            .map(|player| DetailedReplayAnalyzer::count_for_pid(killcounts, *player))
            .sum::<i64>();
        if total_amon_kills == 0 {
            total_amon_kills = 1;
        }

        let mut amon_units = BTreeMap::new();
        for (unit_name, created, lost, kills) in rows {
            if DetailedReplayAnalyzer::contains_skip_strings_text(&unit_name, skip_tokens) {
                continue;
            }
            let kill_fraction = DetailedReplayAnalyzer::round_to_digits_half_even(
                kills as f64 / total_amon_kills as f64,
                2,
            );
            amon_units.insert(
                unit_name,
                DetailedReplayAnalyzer::unit_stats_tuple(created, lost, kills, kill_fraction),
            );
        }
        amon_units
    }

    fn enemy_comp_from_identified_waves(
        identified_waves: &IdentifiedWavesMap,
        unit_comp_dict: &HashMap<String, Vec<HashSet<String>>>,
    ) -> String {
        let mut ai_order = unit_comp_dict.keys().cloned().collect::<Vec<String>>();
        ai_order.sort();
        let mut results = HashMap::new();
        for ai in &ai_order {
            results.insert(ai.clone(), 0.0_f64);
        }

        for wave in identified_waves.values() {
            let types = wave.iter().cloned().collect::<HashSet<String>>();
            if types.is_empty() {
                continue;
            }

            for ai in &ai_order {
                let Some(waves) = unit_comp_dict.get(ai) else {
                    continue;
                };
                let score = results.entry(ai.clone()).or_insert(0.0);
                for wave_row in waves {
                    let mut wave_set = wave_row.clone();
                    wave_set.remove("Medivac");
                    if types == wave_set {
                        *score += wave_set.len() as f64;
                    } else if types.is_subset(&wave_set)
                        && wave_set.len().saturating_sub(types.len()) == 1
                    {
                        *score += 0.25 * wave_set.len() as f64;
                    }
                }
            }
        }

        let mut best_ai: Option<String> = None;
        let mut best_score = 0.0_f64;
        for ai in ai_order {
            let score = results.get(&ai).copied().unwrap_or_default();
            if score > best_score {
                best_score = score;
                best_ai = Some(ai);
            }
        }

        best_ai.unwrap_or_else(|| "Unidentified AI".to_string())
    }

    fn apply_custom_kill_icons(
        main_icons: &mut BTreeMap<String, u64>,
        ally_icons: &mut BTreeMap<String, u64>,
        custom_kill_count: &replay_event_handlers::NestedPlayerCountMap,
        unit_type_dict_amon: &UnitTypeCountMap,
        map_name: &str,
        main_player: i64,
        ally_player: i64,
    ) {
        for key in CUSTOM_KILL_ICON_KEYS {
            let Some(player_counts) = custom_kill_count.get(key) else {
                continue;
            };
            if key == "deadofnight" && !map_name.contains("Dead of Night") {
                continue;
            }
            if key == "minesweeper" && !unit_type_dict_amon.contains_key("MutatorSpiderMine") {
                continue;
            }

            main_icons.insert(
                key.to_string(),
                player_counts
                    .get(&main_player)
                    .copied()
                    .unwrap_or_default()
                    .max(0) as u64,
            );
            ally_icons.insert(
                key.to_string(),
                player_counts
                    .get(&ally_player)
                    .copied()
                    .unwrap_or_default()
                    .max(0) as u64,
            );
        }
    }

    fn analyze_parsed_replay_with_cache_entry(
        parsed: ReplayParsedInputBundle,
        main_player_handles: &HashSet<String>,
        hidden_created_lost: &HashSet<String>,
        basic_cache_entry: Option<&CacheReplayEntry>,
        resources: &ReplayAnalysisResources,
    ) -> Result<DetailedReplayAnalysisResult, DetailedReplayAnalysisError> {
        let dictionaries = resources.cache_generation_data();
        let fallback_basic = basic_cache_entry
            .cloned()
            .unwrap_or_else(|| parsed.cache_entry());
        let cache_persistable = parsed.is_saved_cache_candidate();
        let report = DetailedReplayAnalyzer::analyze_replay_file_impl(
            main_player_handles,
            parsed,
            &dictionaries,
            resources.analysis_sets(),
            resources.stats_counter_dictionaries(),
        )?;
        let cache_entry = CacheReplayEntry::from_report_with_basic(
            &report,
            Some(&fallback_basic),
            hidden_created_lost,
        );

        Ok(DetailedReplayAnalysisResult::new(
            report,
            cache_entry,
            cache_persistable,
        ))
    }

    fn analyze_replay_file_impl(
        main_player_handles: &HashSet<String>,
        parsed: ReplayParsedInputBundle,
        dictionaries: &CacheGenerationData<'_>,
        analysis_sets: &ReplayAnalysisSets,
        counter_dicts: Arc<StatsCounterDictionaries>,
    ) -> Result<ReplayReport, DetailedReplayAnalysisError> {
        let ReplayParsedInputBundle {
            mut parser,
            realtime_length,
            detailed,
            ..
        } = parsed;
        let ReplayDetailedParseContext {
            events,
            start_time,
            end_time,
        } = detailed.ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData(
                "detailed replay parsing did not include event context".to_string(),
            )
        })?;

        let main_player = i64::from(parser.selected_main_player_pid(main_player_handles));
        let ally_player = if main_player == 2 { 1 } else { 2 };

        let main_player_row = parser.player(main_player as u8);
        let ally_player_row = parser.player(ally_player as u8);
        let main_commander = main_player_row
            .map(|player| player.commander.clone())
            .filter(|value| !value.is_empty());
        let ally_commander = ally_player_row
            .map(|player| player.commander.clone())
            .filter(|value| !value.is_empty());
        let main_masteries = main_player_row
            .map(|player| player.masteries)
            .unwrap_or([0_u32; 6]);
        let ally_masteries = ally_player_row
            .map(|player| player.masteries)
            .unwrap_or([0_u32; 6]);

        let mut vespene_drone_identifier =
            ReplayDroneIdentifierCore::new(main_commander.clone(), ally_commander.clone());
        let mut main_stats_counter =
            ReplayStatsCounterCore::new(counter_dicts.clone(), main_masteries, main_commander);
        let mut ally_stats_counter =
            ReplayStatsCounterCore::new(counter_dicts, ally_masteries, ally_commander);
        if parser.file.contains("[MM]") {
            main_stats_counter.set_enable_updates(true);
            ally_stats_counter.set_enable_updates(true);
        }

        let do_not_count_kills_set = &analysis_sets.do_not_count_kills;
        let duplicating_units_set = &analysis_sets.duplicating_units;
        let skip_tokens = &analysis_sets.skip_tokens;
        let dont_count_morphs_set = &analysis_sets.dont_count_morphs;
        let self_killing_units_set = &analysis_sets.self_killing_units;
        let aoe_units_set = &analysis_sets.aoe_units;
        let tychus_outlaws_set = &analysis_sets.tychus_outlaws;
        let units_killed_in_morph_set = &analysis_sets.units_killed_in_morph;
        let dont_include_units_set = &analysis_sets.dont_include_units;
        let icon_units_set = &analysis_sets.icon_units;
        let salvage_units_set = &analysis_sets.salvage_units;
        let unit_add_losses_to_set = &analysis_sets.unit_add_losses_to;
        let commander_no_units_values_set = &analysis_sets.commander_no_units_values;

        let mut amon_player_ids_set: HashSet<i64> = HashSet::from([3_i64, 4_i64]);
        for (mission_name, player_ids) in dictionaries.amon_player_ids.iter() {
            if !DetailedReplayAnalyzer::map_name_has_amon_override(&parser.map_name, mission_name) {
                continue;
            }
            amon_player_ids_set.extend(player_ids.iter().copied());
            break;
        }

        let mut unit_type_dict_main: UnitTypeCountMap = IndexMap::new();
        let mut unit_type_dict_ally: UnitTypeCountMap = IndexMap::new();
        let mut unit_type_dict_amon: UnitTypeCountMap = IndexMap::new();
        let mut unit_dict: UnitStateMap = IndexMap::new();
        let mut dt_ht_ignore = vec![0_i64; 17];
        let mut killcounts = vec![0_i64; 18];
        let mut commander_by_player = HashMap::<i64, String>::new();
        let mut mastery_by_player = HashMap::from([(1_i64, [0_i64; 6]), (2_i64, [0_i64; 6])]);
        let mut prestige_by_player = HashMap::<i64, String>::new();
        let mut outlaw_order: Vec<String> = Vec::new();
        let mut wave_units = WaveUnitsState::default();
        let mut identified_waves: IdentifiedWavesMap = BTreeMap::new();
        let mut killbot_feed = vec![0_i64, 0, 0];
        let mut custom_kill_count: replay_event_handlers::NestedPlayerCountMap = IndexMap::new();
        let mut used_mutator_spider_mines: HashSet<i64> = HashSet::new();
        let mut bonus_timings: Vec<f64> = Vec::new();
        let mut research_vessel_landed_timing: Option<i64> = None;
        let mut unit_id = 0_i64;
        let mut last_biomass_position = [0_i64, 0, 0];
        let mut abathur_kill_locusts = HashSet::new();
        let mut mutator_dehaka_drag_unit_ids = HashSet::new();
        let mut mw_bonus_initial_timing = [0.0_f64, 0.0_f64];
        let mut murvar_spawns = HashSet::new();
        let mut glevig_spawns = HashSet::new();
        let mut broodlord_broodlings = HashSet::new();
        let mut user_leave_times: IndexMap<i64, f64> = IndexMap::new();
        let mut mind_controlled_units = HashSet::new();
        let mut zagaras_dummy_zerglings = HashSet::new();
        let mut unit_killed_by: replay_event_handlers::TextListMapping = IndexMap::new();
        let mut ally_kills_counted_toward_main = 0_i64;
        let mut last_aoe_unit_killed: Vec<Option<(String, f64)>> = vec![None; 17];
        let mut main_icons_base = BTreeMap::<String, u64>::new();
        let mut ally_icons_base = BTreeMap::<String, u64>::new();

        for event in &events {
            let current_event_kind = ReplayEventKind::from_event(event);

            if current_event_kind == ReplayEventKind::GameUserLeave {
                let user_id = DetailedReplayAnalyzer::event_user_id(event).unwrap_or_default();
                let gameloop = DetailedReplayAnalyzer::event_gameloop(event) as f64;
                ReplayEventHandlers::replay_handle_game_user_leave_event_fields(
                    user_id,
                    gameloop,
                    &mut user_leave_times,
                );
            }

            if DetailedReplayAnalyzer::event_gameloop(event) as f64 / 16.0 > end_time {
                continue;
            }

            if matches!(
                current_event_kind,
                ReplayEventKind::GameCommand | ReplayEventKind::GameCommandUpdateTargetUnit
            ) {
                let ReplayEvent::Game(game_event) = event else {
                    continue;
                };
                vespene_drone_identifier.event(game_event);
            }

            if let ReplayEvent::Tracker(event) = event {
                if current_event_kind == ReplayEventKind::TrackerPlayerStats {
                    let player = event.m_player_id.unwrap_or_default();
                    if let Some(stats) = event.m_stats.as_ref() {
                        let supply_used =
                            stats.m_score_value_food_used.unwrap_or_default() / 4096.0;
                        let collection_rate = stats
                            .m_score_value_minerals_collection_rate
                            .unwrap_or_default()
                            + stats
                                .m_score_value_vespene_collection_rate
                                .unwrap_or_default();

                        if let Some(update) =
                            ReplayEventHandlers::replay_handle_player_stats_event_fields(
                                player,
                                main_player,
                                ally_player,
                                supply_used,
                                collection_rate,
                                &killcounts,
                            )
                        {
                            match update.target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.set_unit_dict(&unit_type_dict_main);
                                    main_stats_counter.add_stats(
                                        &vespene_drone_identifier,
                                        update.kills,
                                        update.supply_used,
                                        update.collection_rate,
                                    );
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.set_unit_dict(&unit_type_dict_ally);
                                    ally_stats_counter.add_stats(
                                        &vespene_drone_identifier,
                                        update.kills,
                                        update.supply_used,
                                        update.collection_rate,
                                    );
                                }
                            }
                        }
                    }
                }

                if current_event_kind == ReplayEventKind::TrackerUpgrade
                    && matches!(event.m_player_id, Some(1 | 2))
                {
                    let upg_name = event.m_upgrade_type_name.clone().unwrap_or_default();
                    let upg_pid = event.m_player_id.unwrap_or_default();
                    let upgrade_count = event.m_count.unwrap_or_default();
                    let update = ReplayEventHandlers::replay_handle_upgrade_event_fields(
                        upg_name.as_str(),
                        upg_pid,
                        upgrade_count,
                        main_player,
                        ally_player,
                        &dictionaries.replay_analysis_data.commander_upgrades,
                        &dictionaries.co_mastery_upgrades,
                        &dictionaries.prestige_upgrades,
                    );

                    if let Some(target) = update.target {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.upgrade_event(upg_name.as_str())
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.upgrade_event(upg_name.as_str())
                            }
                        }
                    }

                    if let Some(commander_name) = update.commander_name.as_deref() {
                        commander_by_player.insert(upg_pid, commander_name.to_string());
                        vespene_drone_identifier.update_commanders(upg_pid, commander_name);

                        if let Some(target) = update.target {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.update_commander(commander_name);
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.update_commander(commander_name);
                                }
                            }
                        }
                    }

                    if let Some(mastery_idx) = update.mastery_index {
                        if let Some(row) = mastery_by_player.get_mut(&upg_pid) {
                            if let Ok(index) = usize::try_from(mastery_idx) {
                                if index < row.len() {
                                    row[index] = update.upgrade_count;
                                }
                            }
                        }

                        if let Some(target) = update.target {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter
                                        .update_mastery(mastery_idx, update.upgrade_count);
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter
                                        .update_mastery(mastery_idx, update.upgrade_count);
                                }
                            }
                        }
                    }

                    if let Some(prestige_name) = update.prestige_name.as_deref() {
                        prestige_by_player.insert(upg_pid, prestige_name.to_string());
                        if let Some(target) = update.target {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.update_prestige(prestige_name);
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.update_prestige(prestige_name);
                                }
                            }
                        }
                    }
                }
            }

            if matches!(
                current_event_kind,
                ReplayEventKind::TrackerUnitBorn | ReplayEventKind::TrackerUnitInit
            ) {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                let event_fields = UnitBornOrInitEventFields {
                    unit_type: event.m_unit_type_name.clone().unwrap_or_default(),
                    ability_name: event.m_creator_ability_name.clone(),
                    unit_id: DetailedReplayAnalyzer::replay_unitid_from_event(event, false, false)
                        .unwrap_or_default(),
                    creator_unit_id: DetailedReplayAnalyzer::replay_unitid_from_event(
                        event, false, true,
                    ),
                    control_pid: event.m_control_player_id.unwrap_or_default(),
                    gameloop: event.game_loop,
                    event_x: event.m_x.unwrap_or_default(),
                    event_y: event.m_y.unwrap_or_default(),
                };
                let update = ReplayEventHandlers::replay_handle_unit_born_or_init_event_fields(
                    &event_fields,
                    main_player,
                    ally_player,
                    &amon_player_ids_set,
                    &mut unit_dict,
                    start_time,
                    &mut unit_type_dict_main,
                    &mut unit_type_dict_ally,
                    &mut unit_type_dict_amon,
                    &mut mutator_dehaka_drag_unit_ids,
                    &mut murvar_spawns,
                    &mut glevig_spawns,
                    &mut broodlord_broodlings,
                    &mut outlaw_order,
                    &mut wave_units,
                    &mut identified_waves,
                    &mut abathur_kill_locusts,
                    last_biomass_position,
                    &dictionaries.replay_analysis_data.revival_types,
                    &dictionaries.replay_analysis_data.primal_combat_predecessors,
                    tychus_outlaws_set,
                    &dictionaries.units_in_waves,
                );
                unit_id = update.unit_id;
                last_biomass_position = update.last_biomass_position;

                if let Some((target, unit_type)) = update.created_event.as_ref() {
                    match target {
                        StatsCounterTarget::Main => {
                            main_stats_counter.unit_created_event(unit_type.as_str(), event);
                        }
                        StatsCounterTarget::Ally => {
                            ally_stats_counter.unit_created_event(unit_type.as_str(), event);
                        }
                    }
                }
            }

            if current_event_kind == ReplayEventKind::TrackerUnitInit {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                let event_unit_type = event.m_unit_type_name.clone().unwrap_or_default();
                if event_unit_type == "Archon" {
                    let control_pid = event.m_control_player_id.unwrap_or_default();
                    ReplayEventHandlers::replay_handle_archon_init_event_control_pid(
                        control_pid,
                        &mut dt_ht_ignore,
                    );
                }
            }

            let event_unit_id = match event {
                ReplayEvent::Tracker(event) => {
                    DetailedReplayAnalyzer::replay_unitid_from_event(event, false, false)
                }
                ReplayEvent::Game(_) => None,
            };
            if current_event_kind == ReplayEventKind::TrackerUnitTypeChange
                && event_unit_id
                    .map(|value| unit_dict.contains_key(&value))
                    .unwrap_or(false)
            {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                let event_fields = UnitTypeChangeEventFields {
                    event_unit_id: event_unit_id.unwrap_or_default(),
                    unit_type: event.m_unit_type_name.clone().unwrap_or_default(),
                    gameloop: event.game_loop,
                };
                let update = ReplayEventHandlers::replay_handle_unit_type_change_event_fields(
                    &event_fields,
                    parser.map_name.as_str(),
                    main_player,
                    ally_player,
                    &amon_player_ids_set,
                    &mut unit_dict,
                    &mut unit_type_dict_main,
                    &mut unit_type_dict_ally,
                    &mut unit_type_dict_amon,
                    start_time,
                    &mut bonus_timings,
                    unit_id,
                    &glevig_spawns,
                    &murvar_spawns,
                    &mut zagaras_dummy_zerglings,
                    &broodlord_broodlings,
                    research_vessel_landed_timing,
                    units_killed_in_morph_set,
                    &dictionaries.unit_name_dict,
                    unit_add_losses_to_set,
                    dont_count_morphs_set,
                );
                research_vessel_landed_timing = update.landed_timing;

                if let Some((target, new_unit, old_unit)) = update.unit_change_event.as_ref() {
                    match target {
                        StatsCounterTarget::Main => {
                            main_stats_counter
                                .unit_change_event(new_unit.as_str(), old_unit.as_str());
                        }
                        StatsCounterTarget::Ally => {
                            ally_stats_counter
                                .unit_change_event(new_unit.as_str(), old_unit.as_str());
                        }
                    }
                }
            }

            if current_event_kind == ReplayEventKind::TrackerUnitOwnerChange
                && event_unit_id
                    .map(|value| unit_dict.contains_key(&value))
                    .unwrap_or(false)
            {
                if let Some(changed_unit_id) = event_unit_id {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let control_pid = event.m_control_player_id.unwrap_or_default();
                    let game_time = event.game_loop as f64 / 16.0 - start_time;
                    let update = ReplayEventHandlers::replay_handle_unit_owner_change_event_fields(
                        changed_unit_id,
                        parser.map_name.as_str(),
                        control_pid,
                        main_player,
                        ally_player,
                        &amon_player_ids_set,
                        &mut unit_dict,
                        game_time,
                        &mut bonus_timings,
                        &mut mw_bonus_initial_timing,
                    );

                    if let Some(mindcontrolled_unit_id) = update.mind_controlled_unit_id {
                        mind_controlled_units.insert(mindcontrolled_unit_id);
                        match update.icon_target {
                            Some(StatsCounterTarget::Main) => {
                                DetailedReplayAnalyzer::increment_icon_count(
                                    &mut main_icons_base,
                                    "mc",
                                    1,
                                );
                            }
                            Some(StatsCounterTarget::Ally) => {
                                DetailedReplayAnalyzer::increment_icon_count(
                                    &mut ally_icons_base,
                                    "mc",
                                    1,
                                );
                            }
                            None => {}
                        }
                    }
                }
            }

            if current_event_kind == ReplayEventKind::TrackerUnitDied {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                let unit_in_dict = event_unit_id
                    .map(|value| unit_dict.contains_key(&value))
                    .unwrap_or(false);
                if !unit_in_dict {
                    let killed_unit_type = event.m_unit_type_name.clone().unwrap_or_default();
                    if !do_not_count_kills_set.contains(killed_unit_type.as_str()) {
                        if let Some(killer_player) = event.m_killer_player_id {
                            if let Ok(index) = usize::try_from(killer_player) {
                                if let Some(value) = killcounts.get_mut(index) {
                                    *value += 1;
                                }
                            }
                        }
                    }
                }

                ally_kills_counted_toward_main =
                    ReplayEventHandlers::replay_handle_unit_died_kill_stats_event_fields(
                        event_unit_id,
                        event.m_killer_player_id,
                        event.game_loop,
                        main_player,
                        ally_player,
                        &amon_player_ids_set,
                        &unit_dict,
                        &mut killcounts,
                        &user_leave_times,
                        end_time,
                        &mut last_aoe_unit_killed,
                        ally_kills_counted_toward_main,
                        do_not_count_kills_set,
                        aoe_units_set,
                    );
            }

            if current_event_kind == ReplayEventKind::TrackerUnitDied
                && event_unit_id
                    .map(|value| unit_dict.contains_key(&value))
                    .unwrap_or(false)
            {
                if let Some(detail_unit_id) = event_unit_id {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_fields = UnitDiedEventFields {
                        event_unit_id: detail_unit_id,
                        killing_unit_id: DetailedReplayAnalyzer::replay_unitid_from_event(
                            event, true, false,
                        ),
                        killing_player: event.m_killer_player_id,
                        gameloop: event.game_loop,
                        event_x: event.m_x.unwrap_or_default(),
                        event_y: event.m_y.unwrap_or_default(),
                    };
                    let update = ReplayEventHandlers::replay_handle_unit_died_detail_event_fields(
                        &event_fields,
                        parser.map_name.as_str(),
                        main_player,
                        ally_player,
                        &amon_player_ids_set,
                        unit_id,
                        &mut unit_type_dict_main,
                        &mut unit_type_dict_ally,
                        &mut unit_type_dict_amon,
                        &unit_dict,
                        &mut dt_ht_ignore,
                        start_time,
                        &commander_by_player,
                        &mut killbot_feed,
                        &mut custom_kill_count,
                        &mut used_mutator_spider_mines,
                        &mut bonus_timings,
                        &abathur_kill_locusts,
                        &mutator_dehaka_drag_unit_ids,
                        &murvar_spawns,
                        &glevig_spawns,
                        &broodlord_broodlings,
                        &mut unit_killed_by,
                        &mind_controlled_units,
                        &zagaras_dummy_zerglings,
                        &last_aoe_unit_killed,
                        &dictionaries.replay_analysis_data.commander_no_units,
                        commander_no_units_values_set,
                        &dictionaries.hfts_units,
                        &dictionaries.tus_units,
                        do_not_count_kills_set,
                        self_killing_units_set,
                        duplicating_units_set,
                        salvage_units_set,
                    );
                    unit_id = update.current_unit_id;

                    if let Some((target, unit_name)) = update.salvaged_unit.as_ref() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.append_salvaged_unit(unit_name.as_str());
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.append_salvaged_unit(unit_name.as_str());
                            }
                        }
                    }

                    if let Some((target, unit_name)) = update.mindcontrolled_unit_died.as_ref() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.mindcontrolled_unit_dies(unit_name.as_str());
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.mindcontrolled_unit_dies(unit_name.as_str());
                            }
                        }
                    }
                }
            }
        }

        parser.apply_player_overrides(
            &commander_by_player,
            &mastery_by_player,
            &prestige_by_player,
        );
        parser.messages =
            ParsedReplayMessage::sorted_with_leave_events(&parser.messages, &user_leave_times);

        let main_name = parser
            .player(main_player as u8)
            .map(|player| player.name.clone())
            .unwrap_or_default();
        let ally_name = parser
            .player(ally_player as u8)
            .map(|player| player.name.clone())
            .unwrap_or_default();

        let mut player_stats = BTreeMap::<u8, AnalysisPlayerStatsSeries>::new();
        player_stats.insert(1, main_stats_counter.get_stats(main_name.as_str()));
        player_stats.insert(2, ally_stats_counter.get_stats(ally_name.as_str()));

        let bonus = bonus_timings
            .iter()
            .map(|value| DetailedReplayAnalyzer::format_mm_ss(*value))
            .collect::<Vec<String>>();
        let comp = DetailedReplayAnalyzer::enemy_comp_from_identified_waves(
            &identified_waves,
            &dictionaries.unit_comp_dict,
        );

        DetailedReplayAnalyzer::apply_custom_kill_icons(
            &mut main_icons_base,
            &mut ally_icons_base,
            &custom_kill_count,
            &unit_type_dict_amon,
            parser.map_name.as_str(),
            main_player,
            ally_player,
        );

        let (main_units, mut main_icons) = DetailedReplayAnalyzer::fill_unit_kills_and_icons(
            &main_icons_base,
            main_player,
            main_player,
            &unit_type_dict_main,
            ally_kills_counted_toward_main,
            &killcounts,
            &dictionaries.unit_name_dict,
            &dictionaries.unit_add_kills_to,
            &dictionaries.replay_analysis_data.unit_add_losses_to,
            dont_include_units_set,
            icon_units_set,
        );
        let (ally_units, mut ally_icons) = DetailedReplayAnalyzer::fill_unit_kills_and_icons(
            &ally_icons_base,
            ally_player,
            main_player,
            &unit_type_dict_ally,
            ally_kills_counted_toward_main,
            &killcounts,
            &dictionaries.unit_name_dict,
            &dictionaries.unit_add_kills_to,
            &dictionaries.replay_analysis_data.unit_add_losses_to,
            dont_include_units_set,
            icon_units_set,
        );

        let main_killbot_feed = DetailedReplayAnalyzer::count_for_pid(&killbot_feed, main_player);
        if main_killbot_feed > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut main_icons, "killbots", main_killbot_feed);
        }
        let ally_killbot_feed = DetailedReplayAnalyzer::count_for_pid(&killbot_feed, ally_player);
        if ally_killbot_feed > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut ally_icons, "killbots", ally_killbot_feed);
        }

        let amon_units = DetailedReplayAnalyzer::fill_amon_units(
            &unit_type_dict_amon,
            &killcounts,
            &amon_player_ids_set,
            &dictionaries.unit_name_dict,
            &dictionaries.unit_add_kills_to,
            &dictionaries.replay_analysis_data.unit_add_losses_to,
            dont_include_units_set,
            skip_tokens,
        );

        let mut detailed_input = ReplayReportDetailedInput::from_parser(parser);
        detailed_input.positions = Some(PlayerPositions {
            main: main_player as u8,
            ally: ally_player as u8,
        });
        detailed_input.detail = Some(ReplayReportDetailData {
            length: realtime_length,
            bonus,
            comp,
            replay_hash: None,
            main_kills: DetailedReplayAnalyzer::clamp_nonnegative_to_u64(
                DetailedReplayAnalyzer::count_for_pid(&killcounts, main_player),
            ),
            ally_kills: DetailedReplayAnalyzer::clamp_nonnegative_to_u64(
                DetailedReplayAnalyzer::count_for_pid(&killcounts, ally_player),
            ),
            main_icons,
            ally_icons,
            main_units,
            ally_units,
            amon_units,
            player_stats,
            outlaw_order,
        });

        Ok(ReplayReport::from_detailed_input(
            &detailed_input.parser.file,
            &detailed_input,
            main_player_handles,
        ))
    }
}
