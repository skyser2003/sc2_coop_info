use super::DetailedReplayAnalyzer;
use super::analysis_constants::{
    BROODLING_SOURCE_UNITS, LOCUST_SOURCE_UNITS, ZERATUL_ARTIFACT_PICKUPS,
    ZERATUL_SHADE_PROJECTIONS,
};
use super::replay_event_handlers::ReplayEventStringSets;
use super::timing::{DetailedReplayReportTiming, ReplayEntryParseTiming};
use crate::cache_overall_stats_generator::CacheReplayEntry;
use crate::dictionary_data::Sc2DictionaryData;
use crate::stats_counter_core::StatsCounterDictionaries;
use crate::tauri_replay_analysis_impl::{
    ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer, ReplayReport,
};
use s2protocol_port::{ProtocolStore, ReplayDetails, ReplayEvent, ReplayInitData, ReplayMetadata};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReplayFileDigest {
    pub(super) hash: String,
    pub(super) size_bytes: u64,
}

#[derive(Clone)]
pub struct ReplayAnalysisResources {
    pub(super) dictionary_data: Arc<Sc2DictionaryData>,
    pub(super) hidden_created_lost: HashSet<String>,
    pub(super) analysis_sets: ReplayAnalysisSets,
    pub(super) stats_counter_dictionaries: Arc<StatsCounterDictionaries>,
    pub(super) protocol_store: ProtocolStore,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayAnalysisSets {
    pub(super) do_not_count_kills: HashSet<String>,
    pub(super) duplicating_units: HashSet<String>,
    pub(super) skip_tokens: Vec<String>,
    pub(super) dont_count_morphs: HashSet<String>,
    pub(super) self_killing_units: HashSet<String>,
    pub(super) aoe_units: HashSet<String>,
    pub(super) tychus_outlaws: HashSet<String>,
    pub(super) units_killed_in_morph: HashSet<String>,
    pub(super) dont_include_units: HashSet<String>,
    pub(super) icon_units: HashSet<String>,
    pub(super) salvage_units: HashSet<String>,
    pub(super) unit_add_losses_to: HashSet<String>,
    pub(super) commander_no_units_values: HashSet<String>,
    pub(super) mastery_upgrade_indices: HashMap<String, i64>,
    pub(super) prestige_upgrade_names: HashMap<String, String>,
    pub(super) locust_source_units: HashSet<String>,
    pub(super) broodling_source_units: HashSet<String>,
    pub(super) zeratul_artifact_pickups: HashSet<String>,
    pub(super) zeratul_shade_projections: HashSet<String>,
    pub(super) event_string_sets: ReplayEventStringSets,
}

impl ReplayAnalysisSets {
    pub(super) fn new(data: &Sc2DictionaryData) -> Self {
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

pub struct ReplayFileIdentity;

impl ReplayFileIdentity {
    pub fn calculate_hash(path: &Path) -> String {
        DetailedReplayAnalyzer::calculate_replay_hash(path)
    }

    pub fn modified_seconds(path: &Path) -> Option<u64> {
        DetailedReplayAnalyzer::file_modified_seconds(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayCacheFileIdentity {
    hash: String,
    modified_seconds: u64,
}

impl ReplayCacheFileIdentity {
    pub fn new(hash: impl Into<String>, modified_seconds: u64) -> Self {
        Self {
            hash: hash.into(),
            modified_seconds,
        }
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn modified_seconds(&self) -> u64 {
        self.modified_seconds
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
pub(super) struct ReplayParsedContext {
    pub(super) details: ReplayDetails,
    pub(super) init_data: ReplayInitData,
    pub(super) metadata: ReplayMetadata,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayDetailedParseContext {
    pub(super) events: Vec<ReplayEvent>,
    pub(super) event_kinds: Vec<ReplayEventKind>,
    pub(super) start_time: f64,
    pub(super) end_time: f64,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayDetailedEventCollection {
    pub(super) events: Vec<ReplayEvent>,
    pub(super) event_kinds: Vec<ReplayEventKind>,
    pub(super) decoded_event_count: usize,
    pub(super) start_time: ReplayNumericValue,
    pub(super) last_deselect_event: Option<ReplayNumericValue>,
    pub(super) mm_mutator_keys: Vec<String>,
    pub(super) extension_actions: Vec<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayDetailedEventCollector {
    event_kinds: Vec<ReplayEventKind>,
    decoded_event_count: usize,
    start_time: Option<ReplayNumericValue>,
    last_deselect_event: Option<ReplayNumericValue>,
    mm_mutator_keys: Vec<String>,
    extension_actions: Vec<i64>,
    extension_offset: i64,
    extension_last_gameloop: Option<i64>,
    extension_actions_finished: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayBaseParse {
    pub(super) context: ReplayParsedContext,
    pub(super) build: ReplayBuildInfo,
    pub(super) file: String,
    pub(super) map_name: String,
    pub(super) extension: bool,
    pub(super) brutal_plus: u32,
    pub(super) result: String,
    pub(super) accurate_length: f64,
    pub(super) accurate_length_force_float: bool,
    pub(super) realtime_length: f64,
    pub(super) form_alength: String,
    pub(super) length: u64,
    pub(super) mutators: Vec<String>,
    pub(super) weekly: bool,
    pub(super) raw_messages: Vec<ParsedReplayMessage>,
    pub(super) hash: String,
    pub(super) date: String,
    pub(super) detailed: Option<ReplayDetailedParseContext>,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayParsedInputBundle {
    pub(super) parser: ParsedReplayInput,
    pub(super) all_players: Vec<ParsedReplayPlayer>,
    pub(super) accurate_length_force_float: bool,
    pub(super) realtime_length: f64,
    pub(super) commander_found: bool,
    pub(super) enemy_race_present: bool,
    pub(super) cache_context: ReplayCacheContext,
    pub(super) detailed: Option<ReplayDetailedParseContext>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ReplayCacheContext {
    pub(super) is_mm_replay: bool,
    pub(super) is_blizzard_map: bool,
    pub(super) recover_disabled: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ReplayMutatorParseContext {
    pub(super) cache_handles: Vec<String>,
    pub(super) brutal_plus_difficulty: i64,
    pub(super) retry_mutation_indexes: Vec<i64>,
}

impl ReplayMutatorParseContext {
    pub(super) fn from_init_data(init_data: &ReplayInitData) -> Self {
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
pub(super) struct ReplayBaseParseFilters {
    pub(super) only_blizzard: bool,
    pub(super) require_recover_disabled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ReplayBaseParseOptions {
    pub(super) include_events: bool,
    pub(super) filters: ReplayBaseParseFilters,
}

#[derive(Debug, Clone)]
pub(super) struct TimedReplayEntryParse {
    parsed: Option<(CacheReplayEntry, ReplayParsedInputBundle)>,
    timing: ReplayEntryParseTiming,
}

impl TimedReplayEntryParse {
    pub(super) fn new(
        parsed: Option<(CacheReplayEntry, ReplayParsedInputBundle)>,
        timing: ReplayEntryParseTiming,
    ) -> Self {
        Self { parsed, timing }
    }

    pub(super) fn timing(&self) -> &ReplayEntryParseTiming {
        &self.timing
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Option<(CacheReplayEntry, ReplayParsedInputBundle)>,
        ReplayEntryParseTiming,
    ) {
        (self.parsed, self.timing)
    }
}

impl ReplayBaseParseFilters {
    pub(super) fn saved_cache() -> Self {
        Self {
            only_blizzard: true,
            require_recover_disabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReplayBaseParseError {
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
    pub(super) fn into_detailed_analysis_error(self) -> DetailedReplayAnalysisError {
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

#[derive(Debug, Clone, Copy)]
pub(super) enum ReplayNumericValue {
    Int(i64),
    Float(f64),
}

impl ReplayNumericValue {
    pub(super) fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    pub(super) fn subtract(self, rhs: &Self) -> Self {
        match (self, *rhs) {
            (Self::Int(left), Self::Int(right)) => Self::Int(left - right),
            _ => Self::Float(self.as_f64() - rhs.as_f64()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReplayEventKind {
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

    pub(super) fn from_event(event: &ReplayEvent) -> Self {
        Self::from_name(DetailedReplayAnalyzer::event_name(event))
    }

    pub(super) fn needed_for_detailed_analysis_name(event_name: &str) -> bool {
        !matches!(Self::from_name(event_name), Self::Other)
    }

    pub(super) fn needed_for_replay_report_analysis(self) -> bool {
        matches!(
            self,
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

    pub(super) fn timing_count_index(self) -> usize {
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

impl ReplayDetailedEventCollector {
    pub(super) fn new() -> Self {
        Self {
            event_kinds: Vec::new(),
            decoded_event_count: 0,
            start_time: None,
            last_deselect_event: None,
            mm_mutator_keys: Vec::new(),
            extension_actions: Vec::new(),
            extension_offset: 0,
            extension_last_gameloop: None,
            extension_actions_finished: false,
        }
    }

    pub(super) fn observe_and_retain_for_report(&mut self, event: &ReplayEvent) -> bool {
        let kind = ReplayEventKind::from_event(event);
        self.decoded_event_count += 1;
        self.observe_length_event(event, kind);
        self.observe_mutator_event(event, kind);

        let retain = kind.needed_for_replay_report_analysis();
        if retain {
            self.event_kinds.push(kind);
        }
        retain
    }

    fn observe_length_event(&mut self, event: &ReplayEvent, kind: ReplayEventKind) {
        if kind == ReplayEventKind::GameSelectionDelta {
            self.last_deselect_event = Some(ReplayNumericValue::Float(
                DetailedReplayAnalyzer::event_gameloop(event) as f64 / 16.0 - 2.0,
            ));
            return;
        }

        if self.start_time.is_some() {
            return;
        }

        let ReplayEvent::Tracker(event) = event else {
            return;
        };

        match kind {
            ReplayEventKind::TrackerPlayerStats if event.m_player_id == Some(1) => {
                let minerals = event
                    .m_stats
                    .as_ref()
                    .and_then(|stats| stats.m_score_value_minerals_collection_rate)
                    .unwrap_or_default();
                if minerals > 0.0 {
                    self.start_time =
                        Some(ReplayNumericValue::Float(event.game_loop as f64 / 16.0));
                }
            }
            ReplayEventKind::TrackerUpgrade if matches!(event.m_player_id, Some(1 | 2)) => {
                let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                if upgrade_name.contains("Spray") {
                    self.start_time =
                        Some(ReplayNumericValue::Float(event.game_loop as f64 / 16.0));
                }
            }
            _ => {}
        }
    }

    fn observe_mutator_event(&mut self, event: &ReplayEvent, kind: ReplayEventKind) {
        if let ReplayEvent::Tracker(event) = event
            && kind == ReplayEventKind::TrackerUpgrade
        {
            if event.m_player_id == Some(0) {
                let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                if upgrade_name.contains("mutatorinfo") {
                    self.mm_mutator_keys
                        .push(upgrade_name.get(12..).unwrap_or_default().to_string());
                }
            }

            if matches!(event.m_player_id, Some(1 | 2)) {
                let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                if upgrade_name.contains("Spray") {
                    self.extension_actions_finished = true;
                }
            }
        }

        if self.extension_actions_finished || kind != ReplayEventKind::GameTriggerDialogControl {
            return;
        }

        let gameloop = DetailedReplayAnalyzer::event_gameloop(event);
        if gameloop == 0 && DetailedReplayAnalyzer::event_event_type(event) == Some(3) {
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
                    self.extension_offset = 129 - control_id;
                }
                return;
            }
        }

        if gameloop > 0
            && Some(gameloop) != self.extension_last_gameloop
            && DetailedReplayAnalyzer::event_user_id(event) == Some(0)
        {
            let contains_none = matches!(
                event,
                ReplayEvent::Game(event)
                    if event
                        .m_event_data
                        .as_ref()
                        .is_some_and(|data| data.contains_none)
            );
            if !contains_none
                && let Some(control_id) = DetailedReplayAnalyzer::event_control_id(event)
            {
                self.extension_actions
                    .push(control_id + self.extension_offset);
                self.extension_last_gameloop = Some(gameloop);
            }
        }
    }

    pub(super) fn finish(
        self,
        events: Vec<ReplayEvent>,
        ordered_events_decoded_count: usize,
    ) -> ReplayDetailedEventCollection {
        let decoded_event_count = ordered_events_decoded_count.max(self.decoded_event_count);
        debug_assert_eq!(events.len(), self.event_kinds.len());
        ReplayDetailedEventCollection {
            events,
            event_kinds: self.event_kinds,
            decoded_event_count,
            start_time: self.start_time.unwrap_or(ReplayNumericValue::Int(0)),
            last_deselect_event: self.last_deselect_event,
            mm_mutator_keys: self.mm_mutator_keys,
            extension_actions: self.extension_actions,
        }
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
    detailed_report_timing: DetailedReplayReportTiming,
    report_to_cache_entry: Duration,
}

impl DetailedReplayAnalysisResult {
    pub(super) fn new(
        report: ReplayReport,
        cache_entry: CacheReplayEntry,
        cache_persistable: bool,
        detailed_report_timing: DetailedReplayReportTiming,
        report_to_cache_entry: Duration,
    ) -> Self {
        Self {
            report,
            cache_entry,
            cache_persistable,
            detailed_report_timing,
            report_to_cache_entry,
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

    pub fn detailed_report_timing(&self) -> &DetailedReplayReportTiming {
        &self.detailed_report_timing
    }

    pub fn report_to_cache_entry(&self) -> Duration {
        self.report_to_cache_entry
    }
}

#[derive(Debug, Clone)]
pub(super) struct TimedDetailedReplayReport {
    report: ReplayReport,
    timing: DetailedReplayReportTiming,
}

impl TimedDetailedReplayReport {
    pub(super) fn new(report: ReplayReport, timing: DetailedReplayReportTiming) -> Self {
        Self { report, timing }
    }

    pub(super) fn into_parts(self) -> (ReplayReport, DetailedReplayReportTiming) {
        (self.report, self.timing)
    }
}
