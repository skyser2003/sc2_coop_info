use crate::cache_overall_stats_generator::{
    AnalysisPlayerStatsSeries, CacheOverallStatsFile, CacheReplayEntry,
    CanonicalCachePayloadTiming, PrettyCacheError,
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
    ReplayMetadata, ReplayParseMode, ReplayParseTiming, ReplayParser, TrackerEvent,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use walkdir::WalkDir;
mod replay_event_handlers;

use crate::stats_counter_core::{
    ReplayDroneCommandEventKind, ReplayDroneIdentifierCore, ReplayStatsCounterCore,
    StatsCounterDictionaries,
};
use rayon::ThreadPoolBuilder;
use rayon::iter::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};
use replay_event_handlers::{
    IdentifiedWavesMap, ReplayEventHandlers, ReplayEventStringSets, ReplayMapAnalysisFlags,
    ReplayPlayerIdSet, StatsCounterTarget, UnitBornOrInitEventFields, UnitDiedEventFields,
    UnitStateMap, UnitTypeChangeEventFields, UnitTypeCountMap, WaveUnitsState,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayAnalysisFilePriority {
    size_bytes: u64,
    normalized_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayFileDigest {
    hash: String,
    size_bytes: u64,
}

impl ReplayAnalysisFilePriority {
    fn from_path(path: &Path) -> Self {
        let size_bytes = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        Self::from_size_and_path(size_bytes, path)
    }

    fn from_size_and_path(size_bytes: u64, path: &Path) -> Self {
        let normalized_path = path.to_string_lossy().to_ascii_lowercase();

        Self {
            size_bytes,
            normalized_path,
        }
    }

    fn compare_largest_first(&self, other: &Self) -> Ordering {
        other
            .size_bytes
            .cmp(&self.size_bytes)
            .then_with(|| self.normalized_path.cmp(&other.normalized_path))
    }
}

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
    mastery_upgrade_indices: HashMap<String, i64>,
    prestige_upgrade_names: HashMap<String, String>,
    locust_source_units: HashSet<String>,
    broodling_source_units: HashSet<String>,
    zeratul_artifact_pickups: HashSet<String>,
    zeratul_shade_projections: HashSet<String>,
    event_string_sets: ReplayEventStringSets,
}

impl ReplayAnalysisSets {
    fn new(data: &Sc2DictionaryData) -> Self {
        let replay_data = &data.replay_analysis_data;
        let mut commander_no_units_values = HashSet::new();
        for units in replay_data.commander_no_units.values() {
            commander_no_units_values.extend(units.iter().cloned());
        }
        let mut mastery_upgrade_indices = HashMap::new();
        for upgrades in data.co_mastery_upgrades.values() {
            for (index, upgrade_name) in upgrades.iter().enumerate() {
                mastery_upgrade_indices
                    .entry(upgrade_name.clone())
                    .or_insert(index as i64);
            }
        }
        let mut prestige_upgrade_names = HashMap::new();
        for upgrades in data.prestige_upgrades.values() {
            for (upgrade_name, prestige_name) in upgrades {
                prestige_upgrade_names
                    .entry(upgrade_name.clone())
                    .or_insert_with(|| prestige_name.clone());
            }
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
            mastery_upgrade_indices,
            prestige_upgrade_names,
            locust_source_units: Self::string_set(&LOCUST_SOURCE_UNITS),
            broodling_source_units: Self::string_set(&BROODLING_SOURCE_UNITS),
            zeratul_artifact_pickups: Self::string_set(&ZERATUL_ARTIFACT_PICKUPS),
            zeratul_shade_projections: Self::string_set(&ZERATUL_SHADE_PROJECTIONS),
            event_string_sets: ReplayEventStringSets::new(),
        }
    }

    fn string_set(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ReplayBaseParseTiming {
    total: Duration,
    early_filter: Duration,
    decode_replay: Duration,
    decode_replay_detail: ReplayParseTiming,
    extract_fields: Duration,
    validate_filters: Duration,
    resolve_build: Duration,
    map_lookup: Duration,
    lobby_metadata: Duration,
    length_events: Duration,
    identify_mutators: Duration,
    collect_messages: Duration,
    hash_file: Duration,
    file_date: Duration,
    detailed_event_filter: Duration,
    build_base: Duration,
}

impl ReplayBaseParseTiming {
    fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    fn add(&mut self, other: &Self) {
        self.total += other.total;
        self.early_filter += other.early_filter;
        self.decode_replay += other.decode_replay;
        self.decode_replay_detail.add(&other.decode_replay_detail);
        self.extract_fields += other.extract_fields;
        self.validate_filters += other.validate_filters;
        self.resolve_build += other.resolve_build;
        self.map_lookup += other.map_lookup;
        self.lobby_metadata += other.lobby_metadata;
        self.length_events += other.length_events;
        self.identify_mutators += other.identify_mutators;
        self.collect_messages += other.collect_messages;
        self.hash_file += other.hash_file;
        self.file_date += other.file_date;
        self.detailed_event_filter += other.detailed_event_filter;
        self.build_base += other.build_base;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ReplayEntryParseTiming {
    total: Duration,
    base: ReplayBaseParseTiming,
    bundle_projection: Duration,
    candidate_filter: Duration,
    cache_entry_projection: Duration,
}

impl ReplayEntryParseTiming {
    fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    fn add(&mut self, other: &Self) {
        self.total += other.total;
        self.base.add(&other.base);
        self.bundle_projection += other.bundle_projection;
        self.candidate_filter += other.candidate_filter;
        self.cache_entry_projection += other.cache_entry_projection;
    }
}

#[derive(Debug, Clone)]
struct TimedReplayEntryParse {
    parsed: Option<(CacheReplayEntry, ReplayParsedInputBundle)>,
    timing: ReplayEntryParseTiming,
}

impl TimedReplayEntryParse {
    fn new(
        parsed: Option<(CacheReplayEntry, ReplayParsedInputBundle)>,
        timing: ReplayEntryParseTiming,
    ) -> Self {
        Self { parsed, timing }
    }

    fn timing(&self) -> &ReplayEntryParseTiming {
        &self.timing
    }

    fn into_parts(
        self,
    ) -> (
        Option<(CacheReplayEntry, ReplayParsedInputBundle)>,
        ReplayEntryParseTiming,
    ) {
        (self.parsed, self.timing)
    }
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
        self.parse_replay_base_timed(replay_path, options).0
    }

    fn parse_replay_base_timed(
        &self,
        replay_path: &Path,
        options: ReplayBaseParseOptions,
    ) -> (
        Result<Option<ReplayBaseParse>, ReplayBaseParseError>,
        ReplayBaseParseTiming,
    ) {
        let inputs = self.cache_generation_data();
        DetailedReplayAnalyzer::parse_replay_base_timed(
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

    fn parse_replay_base_timed(
        replay_path: &Path,
        inputs: &CacheGenerationData<'_>,
        protocol_store: &ProtocolStore,
        options: ReplayBaseParseOptions,
    ) -> (
        Result<Option<ReplayBaseParse>, ReplayBaseParseError>,
        ReplayBaseParseTiming,
    ) {
        let total_start = Instant::now();
        let mut timing = ReplayBaseParseTiming::default();
        let result = (|| -> Result<Option<ReplayBaseParse>, ReplayBaseParseError> {
            let early_filter_start = Instant::now();
            let is_mm_replay = replay_path.to_string_lossy().contains("[MM]");
            if options.filters.only_blizzard && is_mm_replay {
                timing.early_filter = early_filter_start.elapsed();
                return Ok(None);
            }
            timing.early_filter = early_filter_start.elapsed();

            let decode_replay_start = Instant::now();
            let (mut parsed, events) = if options.include_events {
                let mut parsed = ReplayParser::parse_file_with_store_ordered_events_filtered(
                    replay_path,
                    protocol_store,
                    ReplayEventKind::needed_for_detailed_analysis_name,
                )
                .map_err(|error| ReplayBaseParseError::ReplayParse {
                    path: replay_path.display().to_string(),
                    message: error.to_string(),
                })?;
                timing.decode_replay_detail.add(parsed.timing());
                let events = parsed.take_events();
                (parsed.take_replay(), events)
            } else {
                let parsed = ReplayParser::parse_file_with_store_timed(
                    replay_path,
                    protocol_store,
                    ReplayParseMode::Simple,
                )
                .map_err(|error| ReplayBaseParseError::ReplayParse {
                    path: replay_path.display().to_string(),
                    message: error.to_string(),
                })?;
                timing.decode_replay_detail.add(parsed.timing());
                (parsed.take_replay(), Vec::new())
            };
            timing.decode_replay = decode_replay_start.elapsed();

            let extract_fields_start = Instant::now();
            let base_build = parsed.base_build();
            let details = parsed.take_details();
            let init_data = parsed.take_init_data();
            let metadata = parsed.take_metadata();
            let message_events = parsed.take_message_events();
            timing.extract_fields = extract_fields_start.elapsed();

            let validate_filters_start = Instant::now();
            let details = details.ok_or_else(|| {
                ReplayBaseParseError::InvalidReplayData("missing replay.details".to_string())
            })?;
            let init_data = init_data.ok_or_else(|| {
                ReplayBaseParseError::InvalidReplayData("missing replay.initData".to_string())
            })?;
            let metadata = metadata.ok_or_else(|| {
                ReplayBaseParseError::InvalidReplayData(
                    "missing replay.gamemetadata.json".to_string(),
                )
            })?;

            if options.filters.only_blizzard && !details.m_isBlizzardMap {
                timing.validate_filters = validate_filters_start.elapsed();
                return Ok(None);
            }

            let disable_recover = details.m_disableRecoverGame.unwrap_or(false);
            if options.filters.require_recover_disabled && !disable_recover {
                timing.validate_filters = validate_filters_start.elapsed();
                return Ok(None);
            }
            timing.validate_filters = validate_filters_start.elapsed();

            let resolve_build_start = Instant::now();
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
            let build = ReplayBuildInfo::new(
                base_build,
                DetailedReplayAnalyzer::resolve_protocol_build(
                    replay_build,
                    latest_build,
                    selected_build,
                ),
            );
            timing.resolve_build = resolve_build_start.elapsed();

            let map_lookup_start = Instant::now();
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
            timing.map_lookup = map_lookup_start.elapsed();

            let lobby_metadata_start = Instant::now();
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
            timing.lobby_metadata = lobby_metadata_start.elapsed();

            let length_events_start = Instant::now();
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
            let form_alength = DetailedReplayAnalyzer::format_duration(accurate_length);
            let length = CacheOverallStatsFile::duration_to_u64(length_numeric.as_f64());
            timing.length_events = length_events_start.elapsed();

            let identify_mutators_start = Instant::now();
            let mutator_context = ReplayMutatorParseContext::from_init_data(&init_data);
            let (mutators, weekly) = DetailedReplayAnalyzer::identify_mutators_for_replay(
                &events,
                &inputs.mutators_all,
                &inputs.mutators_ui,
                &inputs.mutator_ids,
                &inputs.cached_mutators,
                extension,
                is_mm_replay,
                Some(&mutator_context),
            );
            timing.identify_mutators = identify_mutators_start.elapsed();

            let collect_messages_start = Instant::now();
            let raw_messages = message_events
                .iter()
                .filter_map(ParsedReplayMessage::from_message_event)
                .collect::<Vec<ParsedReplayMessage>>();
            timing.collect_messages = collect_messages_start.elapsed();

            let hash_file_start = Instant::now();
            let hash = DetailedReplayAnalyzer::calculate_replay_hash(replay_path);
            timing.hash_file = hash_file_start.elapsed();

            let file_date_start = Instant::now();
            let date = DetailedReplayAnalyzer::file_date_string(replay_path).map_err(|error| {
                ReplayBaseParseError::IoRead {
                    path: replay_path.to_path_buf(),
                    message: error.to_string(),
                }
            })?;
            timing.file_date = file_date_start.elapsed();

            let detailed_event_filter_start = Instant::now();
            let detailed = options.include_events.then(|| ReplayDetailedParseContext {
                events: events
                    .into_iter()
                    .filter(ReplayEventKind::needed_for_replay_report_analysis_event)
                    .collect(),
                start_time: start_time.as_f64(),
                end_time,
            });
            timing.detailed_event_filter = detailed_event_filter_start.elapsed();

            let build_base_start = Instant::now();
            let base = ReplayBaseParse {
                context: ReplayParsedContext {
                    details,
                    init_data,
                    metadata,
                },
                build,
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
                form_alength,
                length,
                mutators,
                weekly,
                raw_messages,
                hash,
                date,
                detailed,
            };
            timing.build_base = build_base_start.elapsed();

            Ok(Some(base))
        })();
        (result, timing.finish(total_start.elapsed()))
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
        Self::calculate_replay_file_digest(path).hash
    }

    fn calculate_replay_file_digest(path: &Path) -> ReplayFileDigest {
        match fs::read(path) {
            Ok(bytes) => ReplayFileDigest {
                hash: format!("{:x}", md5::compute(&bytes)),
                size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            },
            Err(_) => ReplayFileDigest {
                hash: format!("{:x}", md5::compute(path.to_string_lossy().as_bytes())),
                size_bytes: fs::metadata(path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
            },
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

const REPLAY_EVENT_KIND_TIMING_COUNT_NAMES: [&str; 13] = [
    "count.game_user_leave",
    "count.game_selection_delta",
    "count.game_trigger_dialog_control",
    "count.game_command",
    "count.game_command_update_target_unit",
    "count.tracker_player_stats",
    "count.tracker_upgrade",
    "count.tracker_unit_born",
    "count.tracker_unit_init",
    "count.tracker_unit_type_change",
    "count.tracker_unit_owner_change",
    "count.tracker_unit_died",
    "count.other",
];

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

    fn needed_for_replay_report_analysis_event(event: &ReplayEvent) -> bool {
        matches!(
            Self::from_event(event),
            Self::GameUserLeave
                | Self::GameCommand
                | Self::GameCommandUpdateTargetUnit
                | Self::TrackerPlayerStats
                | Self::TrackerUpgrade
                | Self::TrackerUnitBorn
                | Self::TrackerUnitInit
                | Self::TrackerUnitTypeChange
                | Self::TrackerUnitOwnerChange
                | Self::TrackerUnitDied
        )
    }

    fn timing_count_index(self) -> usize {
        match self {
            Self::GameUserLeave => 0,
            Self::GameSelectionDelta => 1,
            Self::GameTriggerDialogControl => 2,
            Self::GameCommand => 3,
            Self::GameCommandUpdateTargetUnit => 4,
            Self::TrackerPlayerStats => 5,
            Self::TrackerUpgrade => 6,
            Self::TrackerUnitBorn => 7,
            Self::TrackerUnitInit => 8,
            Self::TrackerUnitTypeChange => 9,
            Self::TrackerUnitOwnerChange => 10,
            Self::TrackerUnitDied => 11,
            Self::Other => 12,
        }
    }
}

#[derive(Debug)]
struct ReplayAnalysisTimingCollector {
    label: String,
    started: Instant,
    spans: BTreeMap<&'static str, Duration>,
    event_counts: [usize; 13],
    extra_counts: BTreeMap<&'static str, usize>,
}

#[derive(Debug, Default)]
struct ReplayAnalysisNoopTimingCollector;

trait ReplayAnalysisTiming {
    type SpanStart;

    fn new(label: &str) -> Self;
    fn start(&self) -> Self::SpanStart;
    fn finish(&mut self, name: &'static str, started: Self::SpanStart);
    fn increment_event_kind(&mut self, event_kind: ReplayEventKind);
    fn add_count(&mut self, name: &'static str, value: usize);
    fn print(&self);
}

impl ReplayAnalysisTimingCollector {
    fn enabled_from_env() -> bool {
        std::env::var_os("S2COOP_ANALYZER_TIMINGS")
            .and_then(|value| value.into_string().ok())
            .map(|value| {
                let trimmed = value.trim();
                !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false")
            })
            .unwrap_or(false)
    }
}

impl ReplayAnalysisTiming for ReplayAnalysisTimingCollector {
    type SpanStart = Instant;

    fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            started: Instant::now(),
            spans: BTreeMap::new(),
            event_counts: [0; 13],
            extra_counts: BTreeMap::new(),
        }
    }

    #[inline(always)]
    fn start(&self) -> Self::SpanStart {
        Instant::now()
    }

    #[inline(always)]
    fn finish(&mut self, name: &'static str, started: Self::SpanStart) {
        let elapsed = started.elapsed();
        let value = self.spans.entry(name).or_default();
        *value += elapsed;
    }

    #[inline(always)]
    fn increment_event_kind(&mut self, event_kind: ReplayEventKind) {
        self.event_counts[event_kind.timing_count_index()] += 1;
    }

    #[inline(always)]
    fn add_count(&mut self, name: &'static str, value: usize) {
        *self.extra_counts.entry(name).or_default() += value;
    }

    fn print(&self) {
        eprintln!(
            "[s2coop timing] analyze_replay_file_impl label=\"{}\" total={:.3}ms",
            self.label,
            self.started.elapsed().as_secs_f64() * 1000.0
        );
        for (name, duration) in &self.spans {
            eprintln!(
                "[s2coop timing] span.{name}={:.3}ms",
                duration.as_secs_f64() * 1000.0
            );
        }
        for (name, count) in &self.extra_counts {
            eprintln!("[s2coop timing] {name}={count}");
        }
        for (index, count) in self.event_counts.iter().enumerate() {
            if *count > 0 {
                let name = REPLAY_EVENT_KIND_TIMING_COUNT_NAMES[index];
                eprintln!("[s2coop timing] {name}={count}");
            }
        }
    }
}

impl ReplayAnalysisTiming for ReplayAnalysisNoopTimingCollector {
    type SpanStart = ();

    #[inline(always)]
    fn new(_label: &str) -> Self {
        Self
    }

    #[inline(always)]
    fn start(&self) -> Self::SpanStart {}

    #[inline(always)]
    fn finish(&mut self, _name: &'static str, _started: Self::SpanStart) {}

    #[inline(always)]
    fn increment_event_kind(&mut self, _event_kind: ReplayEventKind) {}

    #[inline(always)]
    fn add_count(&mut self, _name: &'static str, _value: usize) {}

    #[inline(always)]
    fn print(&self) {}
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

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateCacheSummary {
    scanned_replays: usize,
    output_file: PathBuf,
    entries: Vec<CacheReplayEntry>,
    completed: bool,
    timing_report: GenerateCacheTimingReport,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GenerateCacheTimingReport {
    worker_count: usize,
    total_replay_files: usize,
    candidate_count: usize,
    reused_candidate_count: usize,
    pending_candidate_count: usize,
    analyzed_entry_count: usize,
    output_directory_setup: Duration,
    collect_replay_files: Duration,
    resolve_main_handles: Duration,
    load_existing_cache: Duration,
    build_thread_pool: Duration,
    build_canonicalize_thread_pool: Duration,
    collect_candidates_parallel: Duration,
    collect_candidates_worker: Duration,
    collect_candidates_hash_lookup: Duration,
    collect_candidates_priority: Duration,
    partition_candidates: Duration,
    sort_pending_candidates: Duration,
    replay_analysis_parallel: Duration,
    replay_analysis_worker: Duration,
    replay_analysis_parse_detailed: Duration,
    replay_analysis_parse_detailed_breakdown: ReplayEntryParseTiming,
    replay_analysis_parse_basic_fallback: Duration,
    replay_analysis_parse_basic_fallback_breakdown: ReplayEntryParseTiming,
    replay_analysis_detailed_report: Duration,
    replay_analysis_temp_entry_write: Duration,
    replay_analysis_progress_record: Duration,
    collect_analyzed_entries: Duration,
    merge_entries: Duration,
    sort_entries: Duration,
    cleanup_temp_file: Duration,
    simple_analysis_parallel: Duration,
    simple_analysis_worker: Duration,
    simple_analysis_parse: Duration,
    simple_analysis_parse_breakdown: ReplayEntryParseTiming,
    canonicalize_entries: Duration,
    canonicalize_worker_count: usize,
    canonicalize_entries_parallel: Duration,
    canonicalize_entries_worker: Duration,
    canonicalize_to_json_value_worker: Duration,
    canonicalize_json_value_worker: Duration,
    canonicalize_serialize_payload: Duration,
    canonicalize_deserialize_payload: Duration,
    write_entries: Duration,
    total: Duration,
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
    #[error("failed to canonicalize cache payload: {0}")]
    CanonicalizeFailed(#[source] serde_json::Error),
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

    #[doc(hidden)]
    pub fn sort_replay_paths_by_detailed_analysis_priority(replay_paths: &mut [PathBuf]) {
        let mut prioritized_paths = replay_paths
            .iter()
            .cloned()
            .map(|path| (ReplayAnalysisFilePriority::from_path(&path), path))
            .collect::<Vec<_>>();

        prioritized_paths.sort_by(|left, right| left.0.compare_largest_first(&right.0));

        for (target, (_, path)) in replay_paths.iter_mut().zip(prioritized_paths) {
            *target = path;
        }
    }

    fn run_full_analysis(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
        mode: FullAnalysisMode,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        let total_start = Instant::now();
        if !config.account_dir.is_dir() {
            return Err(GenerateCacheError::InvalidAccountDirectory(
                config.account_dir.clone(),
            ));
        }

        let output_directory_setup_start = Instant::now();
        config.ensure_output_directory()?;
        let output_directory_setup = output_directory_setup_start.elapsed();
        let mut cache_output = DetailedReplayAnalyzer::analyze_replays_for_cache_output(
            config, logger, runtime, resources, mode,
        )?;
        cache_output.timing_report.output_directory_setup = output_directory_setup;
        let scanned_replays = cache_output.entries.len();

        let canonical_worker_count = if cache_output.timing_report.worker_count == 0 {
            runtime.resolved_worker_count(std::cmp::max(1, cache_output.entries.len()))
        } else {
            cache_output.timing_report.worker_count
        };
        let build_canonicalize_thread_pool_start = Instant::now();
        let canonicalize_thread_pool = ThreadPoolBuilder::new()
            .num_threads(canonical_worker_count)
            .build()
            .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))?;
        cache_output.timing_report.build_canonicalize_thread_pool =
            build_canonicalize_thread_pool_start.elapsed();

        let canonicalize_entries_start = Instant::now();
        let canonical_payload = canonicalize_thread_pool.install(|| {
            CacheReplayEntry::canonicalized_entries_with_payload(&cache_output.entries)
        });
        let canonical_payload =
            canonical_payload.map_err(GenerateCacheError::CanonicalizeFailed)?;
        cache_output.timing_report.canonicalize_entries = canonicalize_entries_start.elapsed();
        cache_output
            .timing_report
            .apply_canonical_payload_timing(canonical_payload.timing());
        let (cache_entries, cache_payload) = canonical_payload.into_parts();

        let write_entries_start = Instant::now();
        CacheReplayEntry::write_payload(&cache_payload, &config.output_file)?;
        cache_output.timing_report.write_entries = write_entries_start.elapsed();
        let timing_report = cache_output.timing_report.finish(total_start.elapsed());

        Ok(GenerateCacheSummary::new(
            scanned_replays,
            config.output_file.clone(),
            cache_entries,
            cache_output.completed,
            timing_report,
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

impl GenerateCacheTimingReport {
    fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    fn apply_canonical_payload_timing(&mut self, timing: CanonicalCachePayloadTiming) {
        self.canonicalize_worker_count = timing.worker_count();
        self.canonicalize_entries_parallel = timing.canonicalize_entries_parallel();
        self.canonicalize_entries_worker = timing.canonicalize_entries_worker();
        self.canonicalize_to_json_value_worker = timing.to_json_value_worker();
        self.canonicalize_json_value_worker = timing.canonicalize_json_value_worker();
        self.canonicalize_serialize_payload = timing.serialize_payload();
        self.canonicalize_deserialize_payload = timing.deserialize_payload();
    }

    fn add_candidate_collection_timing(&mut self, timing: &CandidateReplayCollectionTiming) {
        self.collect_candidates_worker += timing.total();
        self.collect_candidates_hash_lookup += timing.hash_lookup();
        self.collect_candidates_priority += timing.priority();
    }

    fn add_replay_analysis_timing(&mut self, timing: &CandidateReplayAnalysisTiming) {
        self.replay_analysis_worker += timing.total();
        self.replay_analysis_parse_detailed += timing.parse_detailed();
        self.replay_analysis_parse_detailed_breakdown
            .add(timing.parse_detailed_breakdown());
        self.replay_analysis_parse_basic_fallback += timing.parse_basic_fallback();
        self.replay_analysis_parse_basic_fallback_breakdown
            .add(timing.parse_basic_fallback_breakdown());
        self.replay_analysis_detailed_report += timing.detailed_report();
        self.replay_analysis_temp_entry_write += timing.temp_entry_write();
        self.replay_analysis_progress_record += timing.progress_record();
    }

    fn add_simple_analysis_timing(&mut self, timing: &SimpleReplayAnalysisTiming) {
        self.simple_analysis_worker += timing.total();
        self.simple_analysis_parse += timing.parse();
        self.simple_analysis_parse_breakdown
            .add(timing.parse_breakdown());
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn total_replay_files(&self) -> usize {
        self.total_replay_files
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    pub fn reused_candidate_count(&self) -> usize {
        self.reused_candidate_count
    }

    pub fn pending_candidate_count(&self) -> usize {
        self.pending_candidate_count
    }

    pub fn analyzed_entry_count(&self) -> usize {
        self.analyzed_entry_count
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn output_directory_setup(&self) -> Duration {
        self.output_directory_setup
    }

    pub fn collect_replay_files(&self) -> Duration {
        self.collect_replay_files
    }

    pub fn resolve_main_handles(&self) -> Duration {
        self.resolve_main_handles
    }

    pub fn load_existing_cache(&self) -> Duration {
        self.load_existing_cache
    }

    pub fn build_thread_pool(&self) -> Duration {
        self.build_thread_pool
    }

    pub fn build_canonicalize_thread_pool(&self) -> Duration {
        self.build_canonicalize_thread_pool
    }

    pub fn collect_candidates_parallel(&self) -> Duration {
        self.collect_candidates_parallel
    }

    pub fn collect_candidates_worker(&self) -> Duration {
        self.collect_candidates_worker
    }

    pub fn collect_candidates_hash_lookup(&self) -> Duration {
        self.collect_candidates_hash_lookup
    }

    pub fn collect_candidates_priority(&self) -> Duration {
        self.collect_candidates_priority
    }

    pub fn partition_candidates(&self) -> Duration {
        self.partition_candidates
    }

    pub fn sort_pending_candidates(&self) -> Duration {
        self.sort_pending_candidates
    }

    pub fn replay_analysis_parallel(&self) -> Duration {
        self.replay_analysis_parallel
    }

    pub fn replay_analysis_worker(&self) -> Duration {
        self.replay_analysis_worker
    }

    pub fn replay_analysis_parse_detailed(&self) -> Duration {
        self.replay_analysis_parse_detailed
    }

    pub fn replay_analysis_parse_basic_fallback(&self) -> Duration {
        self.replay_analysis_parse_basic_fallback
    }

    pub fn replay_analysis_detailed_report(&self) -> Duration {
        self.replay_analysis_detailed_report
    }

    pub fn replay_analysis_temp_entry_write(&self) -> Duration {
        self.replay_analysis_temp_entry_write
    }

    pub fn replay_analysis_progress_record(&self) -> Duration {
        self.replay_analysis_progress_record
    }

    pub fn collect_analyzed_entries(&self) -> Duration {
        self.collect_analyzed_entries
    }

    pub fn merge_entries(&self) -> Duration {
        self.merge_entries
    }

    pub fn sort_entries(&self) -> Duration {
        self.sort_entries
    }

    pub fn cleanup_temp_file(&self) -> Duration {
        self.cleanup_temp_file
    }

    pub fn simple_analysis_parallel(&self) -> Duration {
        self.simple_analysis_parallel
    }

    pub fn simple_analysis_worker(&self) -> Duration {
        self.simple_analysis_worker
    }

    pub fn simple_analysis_parse(&self) -> Duration {
        self.simple_analysis_parse
    }

    pub fn canonicalize_entries(&self) -> Duration {
        self.canonicalize_entries
    }

    pub fn canonicalize_worker_count(&self) -> usize {
        self.canonicalize_worker_count
    }

    pub fn canonicalize_entries_parallel(&self) -> Duration {
        self.canonicalize_entries_parallel
    }

    pub fn canonicalize_entries_worker(&self) -> Duration {
        self.canonicalize_entries_worker
    }

    pub fn canonicalize_to_json_value_worker(&self) -> Duration {
        self.canonicalize_to_json_value_worker
    }

    pub fn canonicalize_json_value_worker(&self) -> Duration {
        self.canonicalize_json_value_worker
    }

    pub fn canonicalize_serialize_payload(&self) -> Duration {
        self.canonicalize_serialize_payload
    }

    pub fn canonicalize_deserialize_payload(&self) -> Duration {
        self.canonicalize_deserialize_payload
    }

    pub fn write_entries(&self) -> Duration {
        self.write_entries
    }

    pub fn parallelizable_wall_time(&self) -> Duration {
        self.collect_candidates_parallel
            + self.replay_analysis_parallel
            + self.simple_analysis_parallel
            + self.canonicalize_entries_parallel
    }

    pub fn serial_wall_estimate(&self) -> Duration {
        self.total.saturating_sub(self.parallelizable_wall_time())
    }

    pub fn serial_wall_fraction(&self) -> f64 {
        Self::duration_fraction(self.serial_wall_estimate(), self.total)
    }

    pub fn parallelizable_wall_fraction(&self) -> f64 {
        Self::duration_fraction(self.parallelizable_wall_time(), self.total)
    }

    pub fn amdahl_max_speedup_from_serial_fraction(&self) -> Option<f64> {
        let serial_fraction = self.serial_wall_fraction();
        (serial_fraction > 0.0).then_some(1.0 / serial_fraction)
    }

    pub fn format_amdahl_summary(&self) -> String {
        let max_speedup = self
            .amdahl_max_speedup_from_serial_fraction()
            .map(|value| format!("{value:.2}x"))
            .unwrap_or_else(|| "unbounded".to_string());
        let merge_and_sort = self.merge_entries + self.sort_entries;
        let candidate_other = Self::saturating_duration_sub_all(
            self.collect_candidates_worker,
            &[
                self.collect_candidates_hash_lookup,
                self.collect_candidates_priority,
            ],
        );
        let replay_other = Self::saturating_duration_sub_all(
            self.replay_analysis_worker,
            &[
                self.replay_analysis_parse_detailed,
                self.replay_analysis_parse_basic_fallback,
                self.replay_analysis_detailed_report,
                self.replay_analysis_temp_entry_write,
                self.replay_analysis_progress_record,
            ],
        );

        let mut output = format!(
            concat!(
                "Amdahl timings:\n",
                "  total={:.3}s workers={} files={} candidates={} pending={} reused={} analyzed={}\n",
                "  serial_wall_estimate={:.3}s ({:.1}%) parallelizable_wall={:.3}s ({:.1}%) max_speedup_from_this_serial_fraction={}\n",
                "  phases: collect_files={:.3}s resolve_handles={:.3}s load_cache={:.3}s build_pool={:.3}s build_canonical_pool={:.3}s collect_candidates_parallel={:.3}s replay_analysis_parallel={:.3}s collect_results={:.3}s merge_sort={:.3}s canonicalize_total={:.3}s write_file={:.3}s"
            ),
            Self::duration_seconds(self.total),
            self.worker_count,
            self.total_replay_files,
            self.candidate_count,
            self.pending_candidate_count,
            self.reused_candidate_count,
            self.analyzed_entry_count,
            Self::duration_seconds(self.serial_wall_estimate()),
            self.serial_wall_fraction() * 100.0,
            Self::duration_seconds(self.parallelizable_wall_time()),
            self.parallelizable_wall_fraction() * 100.0,
            max_speedup,
            Self::duration_seconds(self.collect_replay_files),
            Self::duration_seconds(self.resolve_main_handles),
            Self::duration_seconds(self.load_existing_cache),
            Self::duration_seconds(self.build_thread_pool),
            Self::duration_seconds(self.build_canonicalize_thread_pool),
            Self::duration_seconds(self.collect_candidates_parallel),
            Self::duration_seconds(self.replay_analysis_parallel),
            Self::duration_seconds(self.collect_analyzed_entries),
            Self::duration_seconds(merge_and_sort),
            Self::duration_seconds(self.canonicalize_entries),
            Self::duration_seconds(self.write_entries),
        );

        output.push_str(&format!(
            concat!(
                "\n  parallel core use: ",
                "collect_candidates worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "replay_analysis worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "simple_analysis worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "canonicalize_json workers={} worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%"
            ),
            Self::duration_seconds(self.collect_candidates_worker),
            Self::effective_cores(
                self.collect_candidates_worker,
                self.collect_candidates_parallel
            ),
            Self::core_efficiency_percent(
                self.collect_candidates_worker,
                self.collect_candidates_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_worker),
            Self::effective_cores(self.replay_analysis_worker, self.replay_analysis_parallel),
            Self::core_efficiency_percent(
                self.replay_analysis_worker,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.simple_analysis_worker),
            Self::effective_cores(self.simple_analysis_worker, self.simple_analysis_parallel),
            Self::core_efficiency_percent(
                self.simple_analysis_worker,
                self.simple_analysis_parallel,
                self.worker_count
            ),
            self.canonicalize_worker_count,
            Self::duration_seconds(self.canonicalize_entries_worker),
            Self::effective_cores(
                self.canonicalize_entries_worker,
                self.canonicalize_entries_parallel
            ),
            Self::core_efficiency_percent(
                self.canonicalize_entries_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
        ));

        output.push_str(&format!(
            concat!(
                "\n  candidate parts: ",
                "hash_lookup={:.3}s capacity_eff={:.1}% ",
                "priority={:.3}s capacity_eff={:.1}% ",
                "other={:.3}s capacity_eff={:.1}%"
            ),
            Self::duration_seconds(self.collect_candidates_hash_lookup),
            Self::core_efficiency_percent(
                self.collect_candidates_hash_lookup,
                self.collect_candidates_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.collect_candidates_priority),
            Self::core_efficiency_percent(
                self.collect_candidates_priority,
                self.collect_candidates_parallel,
                self.worker_count
            ),
            Self::duration_seconds(candidate_other),
            Self::core_efficiency_percent(
                candidate_other,
                self.collect_candidates_parallel,
                self.worker_count
            ),
        ));

        output.push_str(&format!(
            concat!(
                "\n  replay parts: ",
                "parse_detailed={:.3}s capacity_eff={:.1}% ",
                "detailed_report={:.3}s capacity_eff={:.1}% ",
                "parse_basic_fallback={:.3}s capacity_eff={:.1}% ",
                "temp_entry_write={:.3}s capacity_eff={:.1}% ",
                "progress_record={:.3}s capacity_eff={:.1}% ",
                "other={:.3}s capacity_eff={:.1}%"
            ),
            Self::duration_seconds(self.replay_analysis_parse_detailed),
            Self::core_efficiency_percent(
                self.replay_analysis_parse_detailed,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_detailed_report),
            Self::core_efficiency_percent(
                self.replay_analysis_detailed_report,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_parse_basic_fallback),
            Self::core_efficiency_percent(
                self.replay_analysis_parse_basic_fallback,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_temp_entry_write),
            Self::core_efficiency_percent(
                self.replay_analysis_temp_entry_write,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_progress_record),
            Self::core_efficiency_percent(
                self.replay_analysis_progress_record,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(replay_other),
            Self::core_efficiency_percent(
                replay_other,
                self.replay_analysis_parallel,
                self.worker_count
            ),
        ));

        output.push('\n');
        output.push_str(&Self::format_parse_timing_breakdown(
            "parse_detailed parts",
            &self.replay_analysis_parse_detailed_breakdown,
            self.replay_analysis_parallel,
            self.worker_count,
        ));

        output.push('\n');
        output.push_str(&Self::format_parse_timing_breakdown(
            "parse_basic_fallback parts",
            &self.replay_analysis_parse_basic_fallback_breakdown,
            self.replay_analysis_parallel,
            self.worker_count,
        ));

        if self.simple_analysis_worker > Duration::ZERO {
            output.push('\n');
            output.push_str(&Self::format_parse_timing_breakdown(
                "simple_parse parts",
                &self.simple_analysis_parse_breakdown,
                self.simple_analysis_parallel,
                self.worker_count,
            ));
        }

        output.push_str(&format!(
            concat!(
                "\n  canonicalize parts: ",
                "json_parallel_wall={:.3}s json_worker={:.3}s capacity_eff={:.1}% ",
                "to_json={:.3}s capacity_eff={:.1}% ",
                "canonicalize_value={:.3}s capacity_eff={:.1}% ",
                "serialize_payload={:.3}s deserialize_payload={:.3}s"
            ),
            Self::duration_seconds(self.canonicalize_entries_parallel),
            Self::duration_seconds(self.canonicalize_entries_worker),
            Self::core_efficiency_percent(
                self.canonicalize_entries_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
            Self::duration_seconds(self.canonicalize_to_json_value_worker),
            Self::core_efficiency_percent(
                self.canonicalize_to_json_value_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
            Self::duration_seconds(self.canonicalize_json_value_worker),
            Self::core_efficiency_percent(
                self.canonicalize_json_value_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
            Self::duration_seconds(self.canonicalize_serialize_payload),
            Self::duration_seconds(self.canonicalize_deserialize_payload),
        ));

        output
    }

    fn format_parse_timing_breakdown(
        label: &str,
        timing: &ReplayEntryParseTiming,
        wall_time: Duration,
        worker_count: usize,
    ) -> String {
        let mut output = format!(
            concat!(
                "  {}: total={:.3}s ",
                "decode_replay={:.3}s capacity_eff={:.1}% ",
                "extract_fields={:.3}s capacity_eff={:.1}% ",
                "validate_filters={:.3}s capacity_eff={:.1}% ",
                "resolve_build={:.3}s capacity_eff={:.1}% ",
                "map_lookup={:.3}s capacity_eff={:.1}% ",
                "lobby_metadata={:.3}s capacity_eff={:.1}% ",
                "length_events={:.3}s capacity_eff={:.1}% ",
                "identify_mutators={:.3}s capacity_eff={:.1}% ",
                "messages={:.3}s capacity_eff={:.1}% ",
                "hash_file={:.3}s capacity_eff={:.1}% ",
                "file_date={:.3}s capacity_eff={:.1}% ",
                "event_filter={:.3}s capacity_eff={:.1}% ",
                "bundle_projection={:.3}s capacity_eff={:.1}% ",
                "candidate_filter={:.3}s capacity_eff={:.1}% ",
                "cache_entry_projection={:.3}s capacity_eff={:.1}%"
            ),
            label,
            Self::duration_seconds(timing.total),
            Self::duration_seconds(timing.base.decode_replay),
            Self::core_efficiency_percent(timing.base.decode_replay, wall_time, worker_count),
            Self::duration_seconds(timing.base.extract_fields),
            Self::core_efficiency_percent(timing.base.extract_fields, wall_time, worker_count),
            Self::duration_seconds(timing.base.validate_filters),
            Self::core_efficiency_percent(timing.base.validate_filters, wall_time, worker_count),
            Self::duration_seconds(timing.base.resolve_build),
            Self::core_efficiency_percent(timing.base.resolve_build, wall_time, worker_count),
            Self::duration_seconds(timing.base.map_lookup),
            Self::core_efficiency_percent(timing.base.map_lookup, wall_time, worker_count),
            Self::duration_seconds(timing.base.lobby_metadata),
            Self::core_efficiency_percent(timing.base.lobby_metadata, wall_time, worker_count),
            Self::duration_seconds(timing.base.length_events),
            Self::core_efficiency_percent(timing.base.length_events, wall_time, worker_count),
            Self::duration_seconds(timing.base.identify_mutators),
            Self::core_efficiency_percent(timing.base.identify_mutators, wall_time, worker_count),
            Self::duration_seconds(timing.base.collect_messages),
            Self::core_efficiency_percent(timing.base.collect_messages, wall_time, worker_count),
            Self::duration_seconds(timing.base.hash_file),
            Self::core_efficiency_percent(timing.base.hash_file, wall_time, worker_count),
            Self::duration_seconds(timing.base.file_date),
            Self::core_efficiency_percent(timing.base.file_date, wall_time, worker_count),
            Self::duration_seconds(timing.base.detailed_event_filter),
            Self::core_efficiency_percent(
                timing.base.detailed_event_filter,
                wall_time,
                worker_count,
            ),
            Self::duration_seconds(timing.bundle_projection),
            Self::core_efficiency_percent(timing.bundle_projection, wall_time, worker_count),
            Self::duration_seconds(timing.candidate_filter),
            Self::core_efficiency_percent(timing.candidate_filter, wall_time, worker_count),
            Self::duration_seconds(timing.cache_entry_projection),
            Self::core_efficiency_percent(timing.cache_entry_projection, wall_time, worker_count),
        );
        output.push('\n');
        output.push_str(&Self::format_decode_timing_breakdown(
            label,
            &timing.base.decode_replay_detail,
            wall_time,
            worker_count,
        ));
        output
    }

    fn format_decode_timing_breakdown(
        label: &str,
        timing: &ReplayParseTiming,
        wall_time: Duration,
        worker_count: usize,
    ) -> String {
        format!(
            concat!(
                "  {} decode: total={:.3}s mpq_bytes={:.1}MB ",
                "header_read={:.3}s capacity_eff={:.1}% ",
                "header_decode={:.3}s capacity_eff={:.1}% ",
                "protocol={:.3}s capacity_eff={:.1}% ",
                "archive_open={:.3}s capacity_eff={:.1}% ",
                "mpq_open_file={:.3}s capacity_eff={:.1}% ",
                "mpq_read_file={:.3}s capacity_eff={:.1}% ",
                "read_game={:.3}s capacity_eff={:.1}% ",
                "read_tracker={:.3}s capacity_eff={:.1}% ",
                "decode_ordered={:.3}s capacity_eff={:.1}% ",
                "read_details={:.3}s capacity_eff={:.1}% ",
                "decode_details={:.3}s capacity_eff={:.1}% ",
                "read_details_backup={:.3}s capacity_eff={:.1}% ",
                "decode_details_backup={:.3}s capacity_eff={:.1}% ",
                "read_init={:.3}s capacity_eff={:.1}% ",
                "decode_init={:.3}s capacity_eff={:.1}% ",
                "init_fallback={:.3}s capacity_eff={:.1}% ",
                "read_messages={:.3}s capacity_eff={:.1}% ",
                "decode_messages={:.3}s capacity_eff={:.1}% ",
                "read_metadata={:.3}s capacity_eff={:.1}% ",
                "decode_metadata_json={:.3}s capacity_eff={:.1}% ",
                "parse_metadata={:.3}s capacity_eff={:.1}% ",
                "read_attributes={:.3}s capacity_eff={:.1}% ",
                "decode_attributes={:.3}s capacity_eff={:.1}% ",
                "parse_attributes={:.3}s capacity_eff={:.1}%"
            ),
            label,
            Self::duration_seconds(timing.total()),
            timing.mpq_bytes_read() as f64 / (1024.0 * 1024.0),
            Self::duration_seconds(timing.read_header()),
            Self::core_efficiency_percent(timing.read_header(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_header()),
            Self::core_efficiency_percent(timing.decode_header(), wall_time, worker_count),
            Self::duration_seconds(timing.resolve_protocol()),
            Self::core_efficiency_percent(timing.resolve_protocol(), wall_time, worker_count),
            Self::duration_seconds(timing.open_archive()),
            Self::core_efficiency_percent(timing.open_archive(), wall_time, worker_count),
            Self::duration_seconds(timing.mpq_open_file()),
            Self::core_efficiency_percent(timing.mpq_open_file(), wall_time, worker_count),
            Self::duration_seconds(timing.mpq_read_file()),
            Self::core_efficiency_percent(timing.mpq_read_file(), wall_time, worker_count),
            Self::duration_seconds(timing.read_game_events()),
            Self::core_efficiency_percent(timing.read_game_events(), wall_time, worker_count),
            Self::duration_seconds(timing.read_tracker_events()),
            Self::core_efficiency_percent(timing.read_tracker_events(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_ordered_events()),
            Self::core_efficiency_percent(timing.decode_ordered_events(), wall_time, worker_count),
            Self::duration_seconds(timing.read_details()),
            Self::core_efficiency_percent(timing.read_details(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_details()),
            Self::core_efficiency_percent(timing.decode_details(), wall_time, worker_count),
            Self::duration_seconds(timing.read_details_backup()),
            Self::core_efficiency_percent(timing.read_details_backup(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_details_backup()),
            Self::core_efficiency_percent(timing.decode_details_backup(), wall_time, worker_count),
            Self::duration_seconds(timing.read_init_data()),
            Self::core_efficiency_percent(timing.read_init_data(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_init_data()),
            Self::core_efficiency_percent(timing.decode_init_data(), wall_time, worker_count),
            Self::duration_seconds(timing.init_data_fallback()),
            Self::core_efficiency_percent(timing.init_data_fallback(), wall_time, worker_count),
            Self::duration_seconds(timing.read_message_events()),
            Self::core_efficiency_percent(timing.read_message_events(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_message_events()),
            Self::core_efficiency_percent(timing.decode_message_events(), wall_time, worker_count),
            Self::duration_seconds(timing.read_metadata()),
            Self::core_efficiency_percent(timing.read_metadata(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_metadata_json()),
            Self::core_efficiency_percent(timing.decode_metadata_json(), wall_time, worker_count),
            Self::duration_seconds(timing.parse_metadata()),
            Self::core_efficiency_percent(timing.parse_metadata(), wall_time, worker_count),
            Self::duration_seconds(timing.read_attributes()),
            Self::core_efficiency_percent(timing.read_attributes(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_attributes()),
            Self::core_efficiency_percent(timing.decode_attributes(), wall_time, worker_count),
            Self::duration_seconds(timing.parse_attributes()),
            Self::core_efficiency_percent(timing.parse_attributes(), wall_time, worker_count),
        )
    }

    fn duration_fraction(part: Duration, total: Duration) -> f64 {
        let total_seconds = total.as_secs_f64();
        if total_seconds <= 0.0 {
            0.0
        } else {
            part.as_secs_f64() / total_seconds
        }
    }

    fn duration_seconds(duration: Duration) -> f64 {
        duration.as_secs_f64()
    }

    fn effective_cores(worker_time: Duration, wall_time: Duration) -> f64 {
        let wall_seconds = wall_time.as_secs_f64();
        if wall_seconds <= 0.0 {
            0.0
        } else {
            worker_time.as_secs_f64() / wall_seconds
        }
    }

    fn core_efficiency_percent(
        worker_time: Duration,
        wall_time: Duration,
        worker_count: usize,
    ) -> f64 {
        if worker_count == 0 {
            0.0
        } else {
            (Self::effective_cores(worker_time, wall_time) / worker_count as f64) * 100.0
        }
    }

    fn saturating_duration_sub_all(total: Duration, parts: &[Duration]) -> Duration {
        parts
            .iter()
            .fold(total, |remaining, part| remaining.saturating_sub(*part))
    }
}

impl GenerateCacheSummary {
    fn new(
        scanned_replays: usize,
        output_file: PathBuf,
        entries: Vec<CacheReplayEntry>,
        completed: bool,
        timing_report: GenerateCacheTimingReport,
    ) -> Self {
        Self {
            scanned_replays,
            output_file,
            entries,
            completed,
            timing_report,
        }
    }

    pub fn scanned_replays(&self) -> usize {
        self.scanned_replays
    }

    pub fn output_file(&self) -> &Path {
        &self.output_file
    }

    pub fn cache_entries(&self) -> &[CacheReplayEntry] {
        &self.entries
    }

    pub fn into_cache_entries(self) -> Vec<CacheReplayEntry> {
        self.entries
    }

    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn timing_report(&self) -> &GenerateCacheTimingReport {
        &self.timing_report
    }
}

struct GeneratedCacheOutput {
    entries: Vec<CacheReplayEntry>,
    completed: bool,
    timing_report: GenerateCacheTimingReport,
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
        if self.logger.is_none() {
            return;
        }

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
    hash: String,
    analysis_priority: ReplayAnalysisFilePriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CandidateReplayCollectionTiming {
    total: Duration,
    hash_lookup: Duration,
    priority: Duration,
}

impl CandidateReplayCollectionTiming {
    fn new(total: Duration, hash_lookup: Duration, priority: Duration) -> Self {
        Self {
            total,
            hash_lookup,
            priority,
        }
    }

    fn total(&self) -> Duration {
        self.total
    }

    fn hash_lookup(&self) -> Duration {
        self.hash_lookup
    }

    fn priority(&self) -> Duration {
        self.priority
    }
}

#[derive(Debug, Clone)]
struct CandidateReplayCollectionResult {
    candidate: CandidateReplay,
    timing: CandidateReplayCollectionTiming,
}

impl CandidateReplayCollectionResult {
    fn new(candidate: CandidateReplay, timing: CandidateReplayCollectionTiming) -> Self {
        Self { candidate, timing }
    }

    fn timing(&self) -> &CandidateReplayCollectionTiming {
        &self.timing
    }

    fn into_candidate(self) -> CandidateReplay {
        self.candidate
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CandidateReplayAnalysisTiming {
    total: Duration,
    parse_detailed: Duration,
    parse_detailed_breakdown: ReplayEntryParseTiming,
    parse_basic_fallback: Duration,
    parse_basic_fallback_breakdown: ReplayEntryParseTiming,
    detailed_report: Duration,
    temp_entry_write: Duration,
    progress_record: Duration,
}

impl CandidateReplayAnalysisTiming {
    fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    fn add_temp_entry_write(&mut self, duration: Duration) {
        self.temp_entry_write += duration;
    }

    fn add_progress_record(&mut self, duration: Duration) {
        self.progress_record += duration;
    }

    fn total(&self) -> Duration {
        self.total
    }

    fn parse_detailed(&self) -> Duration {
        self.parse_detailed
    }

    fn parse_detailed_breakdown(&self) -> &ReplayEntryParseTiming {
        &self.parse_detailed_breakdown
    }

    fn parse_basic_fallback(&self) -> Duration {
        self.parse_basic_fallback
    }

    fn parse_basic_fallback_breakdown(&self) -> &ReplayEntryParseTiming {
        &self.parse_basic_fallback_breakdown
    }

    fn detailed_report(&self) -> Duration {
        self.detailed_report
    }

    fn temp_entry_write(&self) -> Duration {
        self.temp_entry_write
    }

    fn progress_record(&self) -> Duration {
        self.progress_record
    }
}

#[derive(Debug, Clone)]
struct CandidateReplayAnalysisResult {
    entry: Option<CacheReplayEntry>,
    timing: CandidateReplayAnalysisTiming,
}

impl CandidateReplayAnalysisResult {
    fn new(entry: Option<CacheReplayEntry>, timing: CandidateReplayAnalysisTiming) -> Self {
        Self { entry, timing }
    }

    fn entry(&self) -> Option<&CacheReplayEntry> {
        self.entry.as_ref()
    }

    fn timing(&self) -> &CandidateReplayAnalysisTiming {
        &self.timing
    }

    fn timing_mut(&mut self) -> &mut CandidateReplayAnalysisTiming {
        &mut self.timing
    }

    fn into_parts(self) -> (Option<CacheReplayEntry>, CandidateReplayAnalysisTiming) {
        (self.entry, self.timing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct SimpleReplayAnalysisTiming {
    total: Duration,
    parse: Duration,
    parse_breakdown: ReplayEntryParseTiming,
}

impl SimpleReplayAnalysisTiming {
    fn new(total: Duration, parse: Duration, parse_breakdown: ReplayEntryParseTiming) -> Self {
        Self {
            total,
            parse,
            parse_breakdown,
        }
    }

    fn total(&self) -> Duration {
        self.total
    }

    fn parse(&self) -> Duration {
        self.parse
    }

    fn parse_breakdown(&self) -> &ReplayEntryParseTiming {
        &self.parse_breakdown
    }
}

#[derive(Debug, Clone)]
struct SimpleReplayAnalysisResult {
    entry: Option<CacheReplayEntry>,
    timing: SimpleReplayAnalysisTiming,
}

impl SimpleReplayAnalysisResult {
    fn new(entry: Option<CacheReplayEntry>, timing: SimpleReplayAnalysisTiming) -> Self {
        Self { entry, timing }
    }

    fn timing(&self) -> &SimpleReplayAnalysisTiming {
        &self.timing
    }

    fn into_entry(self) -> Option<CacheReplayEntry> {
        self.entry
    }
}

impl CandidateReplay {
    fn collect_for_cache_lookup_timed(replay_path: &Path) -> CandidateReplayCollectionResult {
        let total_start = Instant::now();
        let hash_lookup_start = Instant::now();
        let digest = DetailedReplayAnalyzer::calculate_replay_file_digest(replay_path);
        let hash_lookup = hash_lookup_start.elapsed();
        let priority_start = Instant::now();
        let analysis_priority =
            ReplayAnalysisFilePriority::from_size_and_path(digest.size_bytes, replay_path);
        let priority = priority_start.elapsed();
        let candidate = Self {
            path: replay_path.to_path_buf(),
            hash: digest.hash,
            analysis_priority,
        };
        CandidateReplayCollectionResult::new(
            candidate,
            CandidateReplayCollectionTiming::new(total_start.elapsed(), hash_lookup, priority),
        )
    }

    fn collect_without_cache_lookup_timed(replay_path: &Path) -> CandidateReplayCollectionResult {
        let total_start = Instant::now();
        let priority_start = Instant::now();
        let analysis_priority = ReplayAnalysisFilePriority::from_path(replay_path);
        let priority = priority_start.elapsed();
        let candidate = Self {
            path: replay_path.to_path_buf(),
            hash: String::new(),
            analysis_priority,
        };
        CandidateReplayCollectionResult::new(
            candidate,
            CandidateReplayCollectionTiming::new(total_start.elapsed(), Duration::ZERO, priority),
        )
    }

    fn analyze_timed(
        &self,
        main_handles: &HashSet<String>,
        resources: &ReplayAnalysisResources,
    ) -> CandidateReplayAnalysisResult {
        let total_start = Instant::now();
        let mut timing = CandidateReplayAnalysisTiming::default();
        let path = self.path.as_path();

        let parsed_detailed = CacheReplayEntry::parse_with_options_timed(
            path,
            resources,
            ReplayBaseParseOptions {
                include_events: true,
                filters: ReplayBaseParseFilters::saved_cache(),
            },
        );
        timing.parse_detailed = parsed_detailed.timing().total;
        timing
            .parse_detailed_breakdown
            .add(parsed_detailed.timing());
        let (parsed_detailed, _parse_timing) = parsed_detailed.into_parts();

        let Some((basic, parsed)) = parsed_detailed else {
            let parse_basic_fallback_start = Instant::now();
            let parsed_basic = CacheReplayEntry::parse_with_options_timed(
                path,
                resources,
                ReplayBaseParseOptions {
                    include_events: false,
                    filters: ReplayBaseParseFilters::saved_cache(),
                },
            );
            timing.parse_basic_fallback = parse_basic_fallback_start.elapsed();
            timing
                .parse_basic_fallback_breakdown
                .add(parsed_basic.timing());
            let entry = parsed_basic.into_parts().0.map(|(entry, _)| entry);
            return CandidateReplayAnalysisResult::new(entry, timing.finish(total_start.elapsed()));
        };

        let detailed_report_start = Instant::now();
        let detailed = DetailedReplayAnalyzer::analyze_parsed_replay_with_cache_entry(
            parsed,
            main_handles,
            resources.hidden_created_lost(),
            Some(&basic),
            resources,
        );
        timing.detailed_report = detailed_report_start.elapsed();

        if let Ok(result) = detailed {
            if result.report().has_non_empty_player_stats() {
                return CandidateReplayAnalysisResult::new(
                    Some(result.into_cache_entry()),
                    timing.finish(total_start.elapsed()),
                );
            }
        }

        CandidateReplayAnalysisResult::new(Some(basic), timing.finish(total_start.elapsed()))
    }

    fn partition_cached(
        candidates: Vec<Self>,
        existing_entries: &HashMap<String, CacheReplayEntry>,
    ) -> (HashMap<String, CacheReplayEntry>, Vec<(String, Self)>) {
        let mut reused_entries = HashMap::new();
        let mut pending_candidates = Vec::new();

        for candidate in candidates {
            let hash = candidate.hash.clone();
            if hash.is_empty() {
                pending_candidates.push((hash, candidate));
                continue;
            }

            if let Some(existing_entry) = existing_entries.get(&hash) {
                reused_entries.insert(
                    hash.clone(),
                    existing_entry.refreshed_for_candidate(candidate.path.as_path(), hash.as_str()),
                );
            } else {
                pending_candidates.push((hash, candidate));
            }
        }

        (reused_entries, pending_candidates)
    }

    fn sort_pending_by_analysis_priority(pending_candidates: &mut [(String, Self)]) {
        pending_candidates.sort_by(|left, right| {
            left.1
                .analysis_priority
                .compare_largest_first(&right.1.analysis_priority)
        });
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
        let mut timing_report = GenerateCacheTimingReport::default();
        let collect_replay_files_start = Instant::now();
        let replay_files = DetailedReplayAnalyzer::collect_cache_replay_files(
            &config.account_dir,
            config.recent_replay_count,
        );
        timing_report.collect_replay_files = collect_replay_files_start.elapsed();
        timing_report.total_replay_files = replay_files.len();

        if mode == FullAnalysisMode::Simple {
            return DetailedReplayAnalyzer::analyze_simple_replays_for_cache_output(
                replay_files,
                runtime,
                resources,
                timing_report,
            );
        }

        let resolve_main_handles_start = Instant::now();
        let main_handles = DetailedReplayAnalyzer::resolve_main_handles(&config.account_dir);
        timing_report.resolve_main_handles = resolve_main_handles_start.elapsed();

        let load_existing_cache_start = Instant::now();
        let existing_detailed_cache_entries =
            CacheReplayEntry::load_existing_detailed_cache_entries(
                config.output_file.as_path(),
                logger,
            );
        timing_report.load_existing_cache = load_existing_cache_start.elapsed();
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
            timing_report.worker_count = worker_count;
            let build_thread_pool_start = Instant::now();
            let thread_pool = ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .build()
                .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))?;
            timing_report.build_thread_pool = build_thread_pool_start.elapsed();
            let should_collect_cache_lookup_hashes = !existing_detailed_cache_entries.is_empty();
            let stop_requested_for_candidates = stop_requested.clone();
            let stop_controller_for_candidates = stop_controller.clone();
            let collect_candidates_start = Instant::now();
            let candidate_replay_results = thread_pool.install(|| {
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
                        Some(if should_collect_cache_lookup_hashes {
                            CandidateReplay::collect_for_cache_lookup_timed(path)
                        } else {
                            CandidateReplay::collect_without_cache_lookup_timed(path)
                        })
                    })
                    .collect::<Vec<CandidateReplayCollectionResult>>()
            });
            timing_report.collect_candidates_parallel = collect_candidates_start.elapsed();
            for candidate_result in &candidate_replay_results {
                timing_report.add_candidate_collection_timing(candidate_result.timing());
            }
            let candidate_replays = candidate_replay_results
                .into_iter()
                .map(CandidateReplayCollectionResult::into_candidate)
                .collect::<Vec<CandidateReplay>>();
            let total_candidates = candidate_replays.len();

            let partition_candidates_start = Instant::now();
            let (mut reused_entries, mut pending_candidates) = CandidateReplay::partition_cached(
                candidate_replays,
                &existing_detailed_cache_entries,
            );
            timing_report.partition_candidates = partition_candidates_start.elapsed();
            timing_report.candidate_count = total_candidates;
            timing_report.reused_candidate_count = reused_entries.len();
            timing_report.pending_candidate_count = pending_candidates.len();

            let sort_pending_candidates_start = Instant::now();
            CandidateReplay::sort_pending_by_analysis_priority(&mut pending_candidates);
            timing_report.sort_pending_candidates = sort_pending_candidates_start.elapsed();
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

                    let replay_analysis_start = Instant::now();
                    let analyzed_results = thread_pool.install(|| {
                        pending_candidates
                            .into_iter()
                            .map(|(_, candidate)| candidate)
                            .par_bridge()
                            .filter_map(|candidate| {
                                if stop_controller_for_workers
                                    .as_ref()
                                    .is_some_and(|controller| controller.stop_requested())
                                {
                                    stop_requested_for_workers.store(true, AtomicOrdering::Release);
                                    return None;
                                }
                                let mut result = candidate.analyze_timed(&main_handles, &resources);
                                if let Some(entry) = result.entry() {
                                    if entry.detailed_analysis {
                                        let temp_entry_write_start = Instant::now();
                                        progress_for_workers.add_temp_entry(entry.clone());
                                        result
                                            .timing_mut()
                                            .add_temp_entry_write(temp_entry_write_start.elapsed());
                                    }
                                }
                                let progress_record_start = Instant::now();
                                progress_for_workers.record_processed_file();
                                result
                                    .timing_mut()
                                    .add_progress_record(progress_record_start.elapsed());
                                Some(result)
                            })
                            .collect::<Vec<CandidateReplayAnalysisResult>>()
                    });
                    timing_report.replay_analysis_parallel = replay_analysis_start.elapsed();
                    for result in &analyzed_results {
                        timing_report.add_replay_analysis_timing(result.timing());
                    }

                    let collect_analyzed_entries_start = Instant::now();
                    let analyzed_entries = analyzed_results
                        .into_iter()
                        .filter_map(|result| {
                            let (entry, _timing) = result.into_parts();
                            entry.map(|entry| (entry.hash.clone(), entry))
                        })
                        .collect::<HashMap<_, _>>();
                    timing_report.collect_analyzed_entries =
                        collect_analyzed_entries_start.elapsed();
                    timing_report.analyzed_entry_count = analyzed_entries.len();
                    analyzed_entries
                };

                let merge_entries_start = Instant::now();
                reused_entries.extend(analyzed_entries);
                timing_report.merge_entries += merge_entries_start.elapsed();
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

        let merge_entries_start = Instant::now();
        let mut all_entries = if config.recent_replay_count.is_some() {
            HashMap::new()
        } else {
            existing_detailed_cache_entries
        };
        all_entries.extend(entries);
        timing_report.merge_entries += merge_entries_start.elapsed();

        let mut all_entries = all_entries.into_values().collect::<Vec<_>>();
        let sort_entries_start = Instant::now();
        all_entries.sort_by(|left, right| left.cmp_cache_order(right));
        timing_report.sort_entries = sort_entries_start.elapsed();

        let cleanup_temp_file_start = Instant::now();
        if temp_file_path.exists() {
            let _ = fs::remove_file(&temp_file_path);
        }
        timing_report.cleanup_temp_file = cleanup_temp_file_start.elapsed();

        Ok(GeneratedCacheOutput {
            entries: all_entries,
            completed: !stop_requested.load(AtomicOrdering::Acquire),
            timing_report,
        })
    }

    fn analyze_simple_replays_for_cache_output(
        replay_files: Vec<PathBuf>,
        runtime: &GenerateCacheRuntimeOptions,
        resources: &ReplayAnalysisResources,
        mut timing_report: GenerateCacheTimingReport,
    ) -> Result<GeneratedCacheOutput, GenerateCacheError> {
        if replay_files.is_empty() {
            return Ok(GeneratedCacheOutput {
                entries: Vec::new(),
                completed: true,
                timing_report,
            });
        }

        let worker_count = runtime.resolved_worker_count(replay_files.len());
        timing_report.worker_count = worker_count;
        let build_thread_pool_start = Instant::now();
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
            .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))?;
        timing_report.build_thread_pool = build_thread_pool_start.elapsed();
        let stop_controller = runtime.stop_controller.clone();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop_requested_for_workers = stop_requested.clone();
        let simple_analysis_start = Instant::now();
        let simple_results = thread_pool.install(|| {
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

                    let total_start = Instant::now();
                    let parse_start = Instant::now();
                    let parsed = CacheReplayEntry::parse_with_options_timed(
                        path,
                        resources,
                        ReplayBaseParseOptions {
                            include_events: false,
                            filters: ReplayBaseParseFilters::saved_cache(),
                        },
                    );
                    let parse = parse_start.elapsed();
                    let (entry, parse_breakdown) = parsed.into_parts();
                    Some(SimpleReplayAnalysisResult::new(
                        entry.map(|(entry, _)| entry),
                        SimpleReplayAnalysisTiming::new(
                            total_start.elapsed(),
                            parse,
                            parse_breakdown,
                        ),
                    ))
                })
                .collect::<Vec<SimpleReplayAnalysisResult>>()
        });
        timing_report.simple_analysis_parallel = simple_analysis_start.elapsed();
        for result in &simple_results {
            timing_report.add_simple_analysis_timing(result.timing());
        }
        let entries = simple_results
            .into_iter()
            .filter_map(SimpleReplayAnalysisResult::into_entry)
            .collect::<Vec<CacheReplayEntry>>();
        timing_report.analyzed_entry_count = entries.len();

        let mut entries = entries;
        let sort_entries_start = Instant::now();
        entries.sort_by(|left, right| left.cmp_cache_order(right));
        timing_report.sort_entries = sort_entries_start.elapsed();

        Ok(GeneratedCacheOutput {
            entries,
            completed: !stop_requested.load(AtomicOrdering::Acquire),
            timing_report,
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
            realtime_length: base.realtime_length,
            commander_found,
            enemy_race_present,
            cache_context,
            detailed: base.detailed,
        })
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
        Self::parse_with_options_timed(replay_path, resources, options)
            .into_parts()
            .0
    }

    fn parse_with_options_timed(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        options: ReplayBaseParseOptions,
    ) -> TimedReplayEntryParse {
        let total_start = Instant::now();
        let mut timing = ReplayEntryParseTiming::default();
        let (base_result, base_timing) = resources.parse_replay_base_timed(replay_path, options);
        timing.base = base_timing;

        let Some(base) = base_result.ok().flatten() else {
            return TimedReplayEntryParse::new(None, timing.finish(total_start.elapsed()));
        };

        let bundle_projection_start = Instant::now();
        let parsed =
            ReplayParsedInputBundle::from_base_parse(base, resources.cache_generation_data());
        timing.bundle_projection = bundle_projection_start.elapsed();
        let Ok(parsed) = parsed else {
            return TimedReplayEntryParse::new(None, timing.finish(total_start.elapsed()));
        };

        let candidate_filter_start = Instant::now();
        let is_cache_candidate = parsed.is_cache_candidate(options.filters);
        timing.candidate_filter = candidate_filter_start.elapsed();
        if !is_cache_candidate {
            return TimedReplayEntryParse::new(None, timing.finish(total_start.elapsed()));
        }

        let cache_entry_projection_start = Instant::now();
        let entry = parsed.cache_entry();
        timing.cache_entry_projection = cache_entry_projection_start.elapsed();
        TimedReplayEntryParse::new(Some((entry, parsed)), timing.finish(total_start.elapsed()))
    }

    fn refreshed_for_candidate(&self, path: &Path, hash: &str) -> Self {
        let mut reused_entry = self.clone();
        reused_entry.file = CacheOverallStatsFile::normalized_path_string(path);
        reused_entry.hash = hash.to_string();
        reused_entry
    }
}

impl DetailedReplayAnalyzer {
    fn map_name_has_amon_override(map_name: &str, candidate: &str) -> bool {
        map_name.contains(candidate)
            || (map_name.contains("[MM] Lnl") && candidate == "Lock & Load")
    }

    fn replay_unitid(index: Option<i64>, recycle_index: Option<i64>) -> Option<i64> {
        let index = index?;
        let recycle_index = recycle_index?;
        Some(recycle_index * 100_000 + index)
    }

    fn replay_event_unitid(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unitid(event.m_unit_tag_index, event.m_unit_tag_recycle)
    }

    fn replay_creator_unitid(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unitid(
            event.m_creator_unit_tag_index,
            event.m_creator_unit_tag_recycle,
        )
    }

    fn replay_killer_unitid(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unitid(
            event.m_killer_unit_tag_index,
            event.m_killer_unit_tag_recycle,
        )
    }

    fn clamp_nonnegative_to_u64(value: i64) -> u64 {
        if value <= 0 { 0 } else { value as u64 }
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
        StatsCounterDictionaries::new(
            dictionaries.unit_base_costs.clone(),
            dictionaries.royal_guards.clone(),
            dictionaries.horners_units.clone(),
            dictionaries.tychus_base_upgrades.clone(),
            dictionaries.tychus_ultimate_upgrades.clone(),
            dictionaries.outlaws.clone(),
        )
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
        analysis_sets: &ReplayAnalysisSets,
    ) -> (BTreeMap<String, UnitStats>, BTreeMap<String, u64>) {
        let mut icons = base_icons.clone();
        for (unit_name, values) in unit_counts {
            let created = values[0];
            if analysis_sets
                .locust_source_units
                .contains(unit_name.as_str())
            {
                DetailedReplayAnalyzer::increment_icon_count(&mut icons, "locust", created);
            } else if analysis_sets
                .broodling_source_units
                .contains(unit_name.as_str())
            {
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
            &analysis_sets.dont_include_units,
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

            if analysis_sets.icon_units.contains(&unit_name) {
                DetailedReplayAnalyzer::set_icon_count(&mut icons, &unit_name, created);
            }
        }

        let mut artifacts_collected = 0_i64;
        for (unit_name, values) in unit_counts {
            let created = values[0];
            let lost = values[1];

            if analysis_sets
                .zeratul_artifact_pickups
                .contains(unit_name.as_str())
            {
                artifacts_collected += lost;
            }
            if analysis_sets
                .zeratul_shade_projections
                .contains(unit_name.as_str())
            {
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
        amon_players: &ReplayPlayerIdSet,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        analysis_sets: &ReplayAnalysisSets,
    ) -> BTreeMap<String, UnitStats> {
        let rows = DetailedReplayAnalyzer::sorted_switch_name_entries(
            unit_counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            &analysis_sets.dont_include_units,
        );

        let mut total_amon_kills = amon_players
            .iter()
            .map(|player| DetailedReplayAnalyzer::count_for_pid(killcounts, player))
            .sum::<i64>();
        if total_amon_kills == 0 {
            total_amon_kills = 1;
        }

        let mut amon_units = BTreeMap::new();
        for (unit_name, created, lost, kills) in rows {
            if DetailedReplayAnalyzer::contains_skip_strings_text(
                &unit_name,
                &analysis_sets.skip_tokens,
            ) {
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
        let mut ai_order = unit_comp_dict.keys().collect::<Vec<&String>>();
        ai_order.sort();
        let mut scores = ai_order
            .iter()
            .map(|ai| (*ai, 0.0_f64))
            .collect::<Vec<(&String, f64)>>();

        for wave in identified_waves.values() {
            let types = wave.iter().map(String::as_str).collect::<HashSet<&str>>();
            if types.is_empty() {
                continue;
            }

            for (ai, score) in &mut scores {
                let Some(waves) = unit_comp_dict.get(ai.as_str()) else {
                    continue;
                };
                for wave_row in waves {
                    let wave_len = if wave_row.contains("Medivac") {
                        wave_row.len().saturating_sub(1)
                    } else {
                        wave_row.len()
                    };
                    let types_match_wave = types
                        .iter()
                        .all(|unit_type| *unit_type != "Medivac" && wave_row.contains(*unit_type));
                    if types_match_wave && types.len() == wave_len {
                        *score += wave_len as f64;
                    } else if types_match_wave && wave_len.saturating_sub(types.len()) == 1 {
                        *score += 0.25 * wave_len as f64;
                    }
                }
            }
        }

        let mut best_ai: Option<&String> = None;
        let mut best_score = 0.0_f64;
        for (ai, score) in scores {
            if score > best_score {
                best_score = score;
                best_ai = Some(ai);
            }
        }

        best_ai
            .cloned()
            .unwrap_or_else(|| "Unidentified AI".to_string())
    }

    fn apply_custom_kill_icons(
        main_icons: &mut BTreeMap<String, u64>,
        ally_icons: &mut BTreeMap<String, u64>,
        custom_kill_count: &replay_event_handlers::NestedPlayerCountMap,
        unit_type_dict_amon: &UnitTypeCountMap,
        map_flags: &ReplayMapAnalysisFlags,
        main_player: i64,
        ally_player: i64,
    ) {
        for key in CUSTOM_KILL_ICON_KEYS {
            let Some(player_counts) = custom_kill_count.get(key) else {
                continue;
            };
            if key == "deadofnight" && !map_flags.is_dead_of_night() {
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
        if ReplayAnalysisTimingCollector::enabled_from_env() {
            Self::analyze_replay_file_impl_with_timings::<ReplayAnalysisTimingCollector>(
                main_player_handles,
                parsed,
                dictionaries,
                analysis_sets,
                counter_dicts,
            )
        } else {
            Self::analyze_replay_file_impl_with_timings::<ReplayAnalysisNoopTimingCollector>(
                main_player_handles,
                parsed,
                dictionaries,
                analysis_sets,
                counter_dicts,
            )
        }
    }

    fn analyze_replay_file_impl_with_timings<Timing: ReplayAnalysisTiming>(
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
        let mut timings = Timing::new(parser.file.as_str());
        timings.add_count("count.events_input", events.len());
        let setup_started = timings.start();

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
        let dont_count_morphs_set = &analysis_sets.dont_count_morphs;
        let self_killing_units_set = &analysis_sets.self_killing_units;
        let aoe_units_set = &analysis_sets.aoe_units;
        let tychus_outlaws_set = &analysis_sets.tychus_outlaws;
        let units_killed_in_morph_set = &analysis_sets.units_killed_in_morph;
        let salvage_units_set = &analysis_sets.salvage_units;
        let unit_add_losses_to_set = &analysis_sets.unit_add_losses_to;
        let commander_no_units_values_set = &analysis_sets.commander_no_units_values;

        let mut amon_player_ids_set = ReplayPlayerIdSet::from_values([3_i64, 4_i64]);
        for (mission_name, player_ids) in dictionaries.amon_player_ids.iter() {
            if !DetailedReplayAnalyzer::map_name_has_amon_override(&parser.map_name, mission_name) {
                continue;
            }
            amon_player_ids_set.extend(player_ids.iter().copied());
            break;
        }
        let map_flags = ReplayMapAnalysisFlags::new(parser.map_name.as_str());
        let event_string_sets = &analysis_sets.event_string_sets;

        let mut unit_type_dict_main: UnitTypeCountMap = IndexMap::new();
        let mut unit_type_dict_ally: UnitTypeCountMap = IndexMap::new();
        let mut unit_type_dict_amon: UnitTypeCountMap = IndexMap::new();
        let mut unit_dict: UnitStateMap = HashMap::new();
        let mut dt_ht_ignore = vec![0_i64; 17];
        let mut killcounts = vec![0_i64; 18];
        let mut commander_by_player = HashMap::<i64, String>::new();
        let mut mastery_by_player = HashMap::from([(1_i64, [0_i64; 6]), (2_i64, [0_i64; 6])]);
        let mut prestige_by_player = HashMap::<i64, String>::new();
        let mut outlaw_order: Vec<String> = Vec::new();
        let mut outlaw_order_seen: HashSet<String> = HashSet::new();
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
        timings.finish("setup", setup_started);

        let end_gameloop = end_time * 16.0;
        let ally_leave_transfer_threshold = end_time * 0.5;
        let mut ally_kills_transfer_to_main = false;
        let event_loop_started = timings.start();
        for event in &events {
            let current_event_kind = ReplayEventKind::from_event(event);
            timings.increment_event_kind(current_event_kind);
            let event_gameloop = DetailedReplayAnalyzer::event_gameloop(event);

            if current_event_kind == ReplayEventKind::GameUserLeave {
                let handler_started = timings.start();
                let user_id = DetailedReplayAnalyzer::event_user_id(event).unwrap_or_default();
                let leaving_player = user_id + 1;
                let leave_time = event_gameloop as f64 / 16.0;
                ReplayEventHandlers::replay_handle_game_user_leave_event_fields(
                    user_id,
                    event_gameloop as f64,
                    &mut user_leave_times,
                );
                if leaving_player == ally_player && leave_time < ally_leave_transfer_threshold {
                    ally_kills_transfer_to_main = true;
                }
                timings.finish("event.game_user_leave", handler_started);
                continue;
            }

            if event_gameloop as f64 > end_gameloop {
                continue;
            }

            match current_event_kind {
                ReplayEventKind::GameCommand | ReplayEventKind::GameCommandUpdateTargetUnit => {
                    let ReplayEvent::Game(game_event) = event else {
                        continue;
                    };
                    let drone_event_kind = match current_event_kind {
                        ReplayEventKind::GameCommand => ReplayDroneCommandEventKind::Command,
                        ReplayEventKind::GameCommandUpdateTargetUnit => {
                            ReplayDroneCommandEventKind::CommandUpdateTargetUnit
                        }
                        _ => unreachable!(),
                    };
                    let handler_started = timings.start();
                    vespene_drone_identifier.event(drone_event_kind, game_event);
                    timings.finish("event.drone_command", handler_started);
                }
                ReplayEventKind::TrackerPlayerStats => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let handler_started = timings.start();
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
                            match update.target() {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.add_stats(
                                        &unit_type_dict_main,
                                        &vespene_drone_identifier,
                                        update.kills(),
                                        update.supply_used(),
                                        update.collection_rate(),
                                    );
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.add_stats(
                                        &unit_type_dict_ally,
                                        &vespene_drone_identifier,
                                        update.kills(),
                                        update.supply_used(),
                                        update.collection_rate(),
                                    );
                                }
                            }
                        }
                    }
                    timings.finish("event.player_stats", handler_started);
                }
                ReplayEventKind::TrackerUpgrade => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    if !matches!(event.m_player_id, Some(1 | 2)) {
                        continue;
                    }
                    let handler_started = timings.start();
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
                        &analysis_sets.mastery_upgrade_indices,
                        &analysis_sets.prestige_upgrade_names,
                    );

                    if let Some(target) = update.target() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.upgrade_event(upg_name.as_str())
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.upgrade_event(upg_name.as_str())
                            }
                        }
                    }

                    if let Some(commander_name) = update.commander_name() {
                        commander_by_player.insert(upg_pid, commander_name.to_string());
                        vespene_drone_identifier.update_commanders(upg_pid, commander_name);

                        if let Some(target) = update.target() {
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

                    if let Some(mastery_idx) = update.mastery_index() {
                        if let Some(row) = mastery_by_player.get_mut(&upg_pid) {
                            if let Ok(index) = usize::try_from(mastery_idx) {
                                if index < row.len() {
                                    row[index] = update.upgrade_count();
                                }
                            }
                        }

                        if let Some(target) = update.target() {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter
                                        .update_mastery(mastery_idx, update.upgrade_count());
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter
                                        .update_mastery(mastery_idx, update.upgrade_count());
                                }
                            }
                        }
                    }

                    if let Some(prestige_name) = update.prestige_name() {
                        prestige_by_player.insert(upg_pid, prestige_name.to_string());
                        if let Some(target) = update.target() {
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
                    timings.finish("event.upgrade", handler_started);
                }
                ReplayEventKind::TrackerUnitBorn | ReplayEventKind::TrackerUnitInit => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let handler_started = timings.start();
                    let event_fields = UnitBornOrInitEventFields::new(
                        event.m_unit_type_name.as_deref().unwrap_or_default(),
                        event.m_creator_ability_name.as_deref(),
                        DetailedReplayAnalyzer::replay_event_unitid(event).unwrap_or_default(),
                        DetailedReplayAnalyzer::replay_creator_unitid(event),
                        event.m_control_player_id.unwrap_or_default(),
                        event.game_loop,
                        event.m_x.unwrap_or_default(),
                        event.m_y.unwrap_or_default(),
                    );
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
                        &mut outlaw_order_seen,
                        &mut wave_units,
                        &mut identified_waves,
                        &mut abathur_kill_locusts,
                        last_biomass_position,
                        &dictionaries.replay_analysis_data.revival_types,
                        &dictionaries.replay_analysis_data.primal_combat_predecessors,
                        tychus_outlaws_set,
                        &dictionaries.units_in_waves,
                        event_string_sets,
                    );
                    unit_id = update.unit_id();
                    last_biomass_position = update.last_biomass_position();

                    if let Some((target, unit_type)) = update.created_event() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.unit_created_event(unit_type, event);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.unit_created_event(unit_type, event);
                            }
                        }
                    }
                    timings.finish("event.unit_born_or_init", handler_started);

                    if current_event_kind == ReplayEventKind::TrackerUnitInit {
                        let handler_started = timings.start();
                        if event.m_unit_type_name.as_deref() == Some("Archon") {
                            let control_pid = event.m_control_player_id.unwrap_or_default();
                            ReplayEventHandlers::replay_handle_archon_init_event_control_pid(
                                control_pid,
                                &mut dt_ht_ignore,
                            );
                        }
                        timings.finish("event.unit_init_archon", handler_started);
                    }
                }
                ReplayEventKind::TrackerUnitTypeChange => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_unit_id_started = timings.start();
                    let event_unit_id = DetailedReplayAnalyzer::replay_event_unitid(event);
                    let event_unit_in_dict = event_unit_id
                        .map(|value| unit_dict.contains_key(&value))
                        .unwrap_or(false);
                    timings.finish("event.unit_id_lookup", event_unit_id_started);
                    if !event_unit_in_dict {
                        continue;
                    }

                    let handler_started = timings.start();
                    let event_fields = UnitTypeChangeEventFields::new(
                        event_unit_id.unwrap_or_default(),
                        event.m_unit_type_name.as_deref().unwrap_or_default(),
                        event.game_loop,
                    );
                    let update = ReplayEventHandlers::replay_handle_unit_type_change_event_fields(
                        &event_fields,
                        &map_flags,
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
                        event_string_sets,
                    );
                    research_vessel_landed_timing = update.landed_timing();

                    if let Some((target, new_unit, old_unit)) = update.unit_change_event() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.unit_change_event(new_unit, old_unit);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.unit_change_event(new_unit, old_unit);
                            }
                        }
                    }
                    timings.finish("event.unit_type_change", handler_started);
                }
                ReplayEventKind::TrackerUnitOwnerChange => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_unit_id_started = timings.start();
                    let event_unit_id = DetailedReplayAnalyzer::replay_event_unitid(event);
                    let event_unit_in_dict = event_unit_id
                        .map(|value| unit_dict.contains_key(&value))
                        .unwrap_or(false);
                    timings.finish("event.unit_id_lookup", event_unit_id_started);
                    let Some(changed_unit_id) = event_unit_id.filter(|_| event_unit_in_dict) else {
                        continue;
                    };

                    let handler_started = timings.start();
                    let control_pid = event.m_control_player_id.unwrap_or_default();
                    let game_time = event.game_loop as f64 / 16.0 - start_time;
                    let update = ReplayEventHandlers::replay_handle_unit_owner_change_event_fields(
                        changed_unit_id,
                        &map_flags,
                        control_pid,
                        main_player,
                        ally_player,
                        &amon_player_ids_set,
                        &mut unit_dict,
                        game_time,
                        &mut bonus_timings,
                        &mut mw_bonus_initial_timing,
                    );

                    if let Some(mindcontrolled_unit_id) = update.mind_controlled_unit_id() {
                        mind_controlled_units.insert(mindcontrolled_unit_id);
                        match update.icon_target() {
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
                    timings.finish("event.unit_owner_change", handler_started);
                }
                ReplayEventKind::TrackerUnitDied => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_unit_id_started = timings.start();
                    let event_unit_id = DetailedReplayAnalyzer::replay_event_unitid(event);
                    let killed_snapshot = event_unit_id.and_then(|value| unit_dict.get(&value));
                    let event_unit_in_dict = killed_snapshot.is_some();
                    timings.finish("event.unit_id_lookup", event_unit_id_started);

                    let handler_started = timings.start();
                    if !event_unit_in_dict {
                        let killed_unit_type =
                            event.m_unit_type_name.as_deref().unwrap_or_default();
                        if !do_not_count_kills_set.contains(killed_unit_type) {
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
                            killed_snapshot,
                            event.m_killer_player_id,
                            event.game_loop,
                            main_player,
                            ally_player,
                            &amon_player_ids_set,
                            &mut killcounts,
                            ally_kills_transfer_to_main,
                            &mut last_aoe_unit_killed,
                            ally_kills_counted_toward_main,
                            do_not_count_kills_set,
                            aoe_units_set,
                        );
                    timings.finish("event.unit_died_kill_stats", handler_started);

                    let Some(detail_unit_id) = event_unit_id.filter(|_| event_unit_in_dict) else {
                        continue;
                    };
                    let Some(killed_snapshot) = killed_snapshot else {
                        continue;
                    };
                    let handler_started = timings.start();
                    let event_fields = UnitDiedEventFields::new(
                        detail_unit_id,
                        DetailedReplayAnalyzer::replay_killer_unitid(event),
                        event.m_killer_player_id,
                        event.game_loop,
                        event.m_x.unwrap_or_default(),
                        event.m_y.unwrap_or_default(),
                    );
                    let update = ReplayEventHandlers::replay_handle_unit_died_detail_event_fields(
                        &event_fields,
                        killed_snapshot,
                        &map_flags,
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
                        event_string_sets,
                    );
                    unit_id = update.current_unit_id();

                    if let Some((target, unit_name)) = update.salvaged_unit() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.append_salvaged_unit(unit_name);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.append_salvaged_unit(unit_name);
                            }
                        }
                    }

                    if let Some((target, unit_name)) = update.mindcontrolled_unit_died() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.mindcontrolled_unit_dies(unit_name);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.mindcontrolled_unit_dies(unit_name);
                            }
                        }
                    }
                    timings.finish("event.unit_died_detail", handler_started);
                }
                _ => {}
            }
        }
        timings.finish("events.total", event_loop_started);

        let overrides_started = timings.start();
        parser.apply_player_overrides(
            &commander_by_player,
            &mastery_by_player,
            &prestige_by_player,
        );
        parser.messages =
            ParsedReplayMessage::sorted_with_leave_events(&parser.messages, &user_leave_times);
        timings.finish("post.player_overrides_messages", overrides_started);

        let player_stats_started = timings.start();
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
        timings.finish("post.player_stats", player_stats_started);

        let bonus_comp_started = timings.start();
        let bonus = bonus_timings
            .iter()
            .map(|value| DetailedReplayAnalyzer::format_mm_ss(*value))
            .collect::<Vec<String>>();
        let comp = DetailedReplayAnalyzer::enemy_comp_from_identified_waves(
            &identified_waves,
            &dictionaries.unit_comp_dict,
        );
        timings.finish("post.bonus_comp", bonus_comp_started);

        let custom_icons_started = timings.start();
        DetailedReplayAnalyzer::apply_custom_kill_icons(
            &mut main_icons_base,
            &mut ally_icons_base,
            &custom_kill_count,
            &unit_type_dict_amon,
            &map_flags,
            main_player,
            ally_player,
        );
        timings.finish("post.custom_kill_icons", custom_icons_started);

        let main_units_started = timings.start();
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
            analysis_sets,
        );
        timings.finish("post.main_units_icons", main_units_started);
        let ally_units_started = timings.start();
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
            analysis_sets,
        );
        timings.finish("post.ally_units_icons", ally_units_started);

        let killbot_icons_started = timings.start();
        let main_killbot_feed = DetailedReplayAnalyzer::count_for_pid(&killbot_feed, main_player);
        if main_killbot_feed > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut main_icons, "killbots", main_killbot_feed);
        }
        let ally_killbot_feed = DetailedReplayAnalyzer::count_for_pid(&killbot_feed, ally_player);
        if ally_killbot_feed > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut ally_icons, "killbots", ally_killbot_feed);
        }
        timings.finish("post.killbot_icons", killbot_icons_started);

        let amon_units_started = timings.start();
        let amon_units = DetailedReplayAnalyzer::fill_amon_units(
            &unit_type_dict_amon,
            &killcounts,
            &amon_player_ids_set,
            &dictionaries.unit_name_dict,
            &dictionaries.unit_add_kills_to,
            &dictionaries.replay_analysis_data.unit_add_losses_to,
            analysis_sets,
        );
        timings.finish("post.amon_units", amon_units_started);

        let report_started = timings.start();
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

        let report = ReplayReport::from_detailed_input(
            &detailed_input.parser.file,
            &detailed_input,
            main_player_handles,
        );
        timings.finish("post.report_build", report_started);
        timings.print();
        Ok(report)
    }
}
