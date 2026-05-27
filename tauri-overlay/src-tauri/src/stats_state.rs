use crate::shared_types::{LocalizedLabels, ReplayScanProgressPayload};
use crate::{AppSettings, ReplayInfo, StatsAnalysisPayload, StatsStatePayload, TauriOverlayOps};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::Serialize;
use serde_json::{Map, Value};
use std::time::Duration;

type StatsStateParts = (
    bool,
    u64,
    Vec<String>,
    Vec<String>,
    Value,
    std::collections::BTreeMap<String, LocalizedLabels>,
    String,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupAnalysisTrigger {
    Setup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupAnalysisRequestOutcome {
    include_detailed: bool,
    started: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisMode {
    Simple,
    Detailed,
}

impl StartupAnalysisTrigger {
    pub fn label(self) -> &'static str {
        match self {
            Self::Setup => "setup",
        }
    }
}

impl StartupAnalysisRequestOutcome {
    pub fn new(include_detailed: bool, started: bool) -> Self {
        Self {
            include_detailed,
            started,
        }
    }

    pub fn include_detailed(&self) -> bool {
        self.include_detailed
    }

    pub fn started(&self) -> bool {
        self.started
    }
}

impl AnalysisMode {
    pub fn from_include_detailed(include_detailed: bool) -> Self {
        if include_detailed {
            Self::Detailed
        } else {
            Self::Simple
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Detailed => "detailed",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            Self::Simple => "Simple analysis",
            Self::Detailed => "Detailed analysis",
        }
    }

    pub fn peer_display(self) -> &'static str {
        match self {
            Self::Simple => "Detailed analysis",
            Self::Detailed => "Simple analysis",
        }
    }

    pub fn key(self) -> &'static str {
        self.slug()
    }
}

impl TauriOverlayOps {
    pub fn analysis_mode(include_detailed: bool) -> AnalysisMode {
        AnalysisMode::from_include_detailed(include_detailed)
    }

    pub fn analysis_status_text(mode: AnalysisMode, phase: &str) -> String {
        format!("{}: {phase}.", mode.display())
    }

    pub fn analysis_started_message(mode: AnalysisMode) -> String {
        TauriOverlayOps::analysis_status_text(mode, "started in background")
    }

    pub fn analysis_already_running_message(mode: AnalysisMode) -> String {
        TauriOverlayOps::analysis_status_text(mode, "already running")
    }

    pub fn analysis_blocked_by_other_mode_message(mode: AnalysisMode) -> String {
        format!(
            "{} cannot start while {} is running.",
            mode.display(),
            mode.peer_display()
        )
    }

    pub fn analysis_at_start_message(enabled: bool) -> String {
        if enabled {
            "Detailed analysis at startup enabled.".to_string()
        } else {
            "Detailed analysis at startup disabled.".to_string()
        }
    }

    pub fn analysis_error_status_text(mode: AnalysisMode, message: &str) -> String {
        format!("{}: {message}", mode.display())
    }

    fn analysis_elapsed_suffix(elapsed: Duration) -> String {
        format!("Time consumed: {:.2} s.", elapsed.as_secs_f64())
    }

    pub fn analysis_completed_message(
        mode: AnalysisMode,
        replay_count: u64,
        elapsed: Duration,
    ) -> String {
        let summary = if replay_count == 0 {
            "No replay files found.".to_string()
        } else {
            format!(
                "{} completed with {replay_count} replay file(s).",
                mode.display()
            )
        };
        format!(
            "{summary} {}",
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }

    pub fn cache_generation_completed_message(mode: AnalysisMode, elapsed: Duration) -> String {
        format!(
            "{} cache generation completed. {}",
            mode.display(),
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }

    pub fn analysis_stopped_message(mode: AnalysisMode, detail: &str, elapsed: Duration) -> String {
        format!(
            "{} stopped. {} {}",
            mode.display(),
            detail,
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }

    pub fn analysis_failed_message(mode: AnalysisMode, message: &str, elapsed: Duration) -> String {
        format!(
            "{} failed: {message} {}",
            mode.display(),
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }

    pub fn normalize_detailed_analysis_logger_message(message: &str) -> String {
        let normalized = message.replace('\n', " | ");
        if normalized == "Starting detailed analysis!" {
            return TauriOverlayOps::analysis_status_text(
                AnalysisMode::Detailed,
                "generating cache",
            );
        }
        if normalized.starts_with("Running... ")
            || normalized.starts_with("Estimated remaining time:")
        {
            return format!(
                "{}: cache generation progress | {normalized}",
                AnalysisMode::Detailed.display()
            );
        }
        if normalized.starts_with("Detailed analysis completed! ") {
            return TauriOverlayOps::analysis_status_text(
                AnalysisMode::Detailed,
                "cache generation completed",
            );
        }
        if normalized.starts_with("Detailed analysis completed in ") {
            return format!("{}: {}", AnalysisMode::Detailed.display(), normalized);
        }
        normalized
    }

    fn parse_progress_fraction(value: &str) -> Option<(u64, u64)> {
        let (completed, remainder) = value.trim().split_once('/')?;
        let completed = completed.trim().parse::<u64>().ok()?;
        let total_text = remainder.trim();
        let total_end = total_text
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(total_text.len());
        let total = total_text.get(..total_end)?.trim().parse::<u64>().ok()?;
        Some((completed, total))
    }

    pub fn parse_detailed_analysis_progress_counts(message: &str) -> Option<(u64, u64)> {
        for line in message.lines().map(str::trim) {
            if let Some(progress) = line.strip_prefix("Running... ") {
                return TauriOverlayOps::parse_progress_fraction(progress);
            }
            if let Some(progress) = line.strip_prefix("Detailed analysis completed! ") {
                return TauriOverlayOps::parse_progress_fraction(progress);
            }
        }
        None
    }

    pub fn empty_stats_payload() -> Value {
        #[derive(Serialize)]
        struct EmptyStatsPayload {
            #[serde(rename = "MapData")]
            map_data: Map<String, Value>,
            #[serde(rename = "CommanderData")]
            commander_data: Map<String, Value>,
            #[serde(rename = "AllyCommanderData")]
            ally_commander_data: Map<String, Value>,
            #[serde(rename = "DifficultyData")]
            difficulty_data: Map<String, Value>,
            #[serde(rename = "RegionData")]
            region_data: Map<String, Value>,
            #[serde(rename = "UnitData")]
            unit_data: Value,
            #[serde(rename = "AmonData")]
            amon_data: Map<String, Value>,
            #[serde(rename = "PlayerData")]
            player_data: Map<String, Value>,
        }

        TauriOverlayOps::to_json_value(EmptyStatsPayload {
            map_data: Map::new(),
            commander_data: Map::new(),
            ally_commander_data: Map::new(),
            difficulty_data: Map::new(),
            region_data: Map::new(),
            unit_data: Value::Null,
            amon_data: Map::new(),
            player_data: Map::new(),
        })
    }

    pub fn apply_rebuild_snapshot(
        stats: &mut StatsState,
        snapshot: StatsSnapshot,
        mode: AnalysisMode,
    ) {
        let (ready, games, main_players, main_handles, analysis, prestige_names, message) =
            snapshot.into_parts();
        stats.set_ready(ready);
        stats.set_games(games);
        stats.set_main_players(main_players);
        stats.set_main_handles(main_handles);
        stats.set_analysis(Some(analysis));
        stats.set_prestige_names(prestige_names);
        stats.set_message(message);

        stats.set_analysis_terminal_status(mode, "completed");
    }
}

pub struct AnalysisOutcome {
    reported_replay_count: usize,
    replays: Vec<ReplayInfo>,
    analysis_completed: bool,
    snapshot: Option<StatsSnapshot>,
}

impl AnalysisOutcome {
    pub fn new(
        reported_replay_count: usize,
        replays: Vec<ReplayInfo>,
        analysis_completed: bool,
    ) -> Self {
        Self {
            reported_replay_count,
            replays,
            analysis_completed,
            snapshot: None,
        }
    }

    pub fn with_snapshot(
        reported_replay_count: usize,
        snapshot: StatsSnapshot,
        analysis_completed: bool,
    ) -> Self {
        Self {
            reported_replay_count,
            replays: Vec::new(),
            analysis_completed,
            snapshot: Some(snapshot),
        }
    }

    pub fn into_parts(self) -> (usize, Vec<ReplayInfo>, bool, Option<StatsSnapshot>) {
        (
            self.reported_replay_count,
            self.replays,
            self.analysis_completed,
            self.snapshot,
        )
    }

    pub fn reported_replay_count(&self) -> usize {
        self.reported_replay_count
    }

    pub fn analysis_completed(&self) -> bool {
        self.analysis_completed
    }
}

#[derive(Debug)]
pub struct StatsState {
    ready: bool,
    analysis: Option<Value>,
    games: u64,
    main_players: Vec<String>,
    main_handles: Vec<String>,
    startup_analysis_requested: bool,
    analysis_running: bool,
    analysis_running_mode: Option<AnalysisMode>,
    simple_analysis_status: String,
    detailed_analysis_status: String,
    detailed_analysis_atstart: bool,
    prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    message: String,
}

impl Default for StatsState {
    fn default() -> Self {
        Self {
            ready: false,
            analysis: Some(TauriOverlayOps::empty_stats_payload()),
            games: 0,
            main_players: vec![],
            main_handles: vec![],
            startup_analysis_requested: false,
            analysis_running: false,
            analysis_running_mode: None,
            simple_analysis_status: TauriOverlayOps::analysis_status_text(
                AnalysisMode::Simple,
                "waiting for startup",
            ),
            detailed_analysis_status: TauriOverlayOps::analysis_status_text(
                AnalysisMode::Detailed,
                "not started",
            ),
            detailed_analysis_atstart: false,
            prestige_names: Default::default(),
            message: "No parsed statistics available yet.".to_string(),
        }
    }
}

impl StatsState {
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            detailed_analysis_atstart: settings.detailed_analysis_atstart(),
            ..Self::default()
        }
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn analysis(&self) -> Option<&Value> {
        self.analysis.as_ref()
    }

    pub fn games(&self) -> u64 {
        self.games
    }

    pub fn main_players(&self) -> &[String] {
        &self.main_players
    }

    pub fn main_handles(&self) -> &[String] {
        &self.main_handles
    }

    pub fn startup_analysis_requested(&self) -> bool {
        self.startup_analysis_requested
    }

    pub fn analysis_running(&self) -> bool {
        self.analysis_running
    }

    pub fn analysis_running_mode(&self) -> Option<AnalysisMode> {
        self.analysis_running_mode
    }

    pub fn should_start_lazy_statistics_analysis(&self) -> bool {
        !self.ready && !self.analysis_running
    }

    pub fn detailed_analysis_status(&self) -> &str {
        &self.detailed_analysis_status
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn set_analysis_running(&mut self, value: bool) {
        self.analysis_running = value;
        if !value {
            self.analysis_running_mode = None;
        }
    }

    pub fn start_analysis(&mut self, mode: AnalysisMode) {
        self.analysis_running = true;
        self.analysis_running_mode = Some(mode);
    }

    pub fn set_startup_analysis_requested(&mut self, value: bool) {
        self.startup_analysis_requested = value;
    }

    pub fn set_ready(&mut self, value: bool) {
        self.ready = value;
    }

    pub fn set_analysis(&mut self, value: Option<Value>) {
        self.analysis = value;
    }

    pub fn set_games(&mut self, value: u64) {
        self.games = value;
    }

    pub fn set_message(&mut self, value: impl Into<String>) {
        self.message = value.into();
    }

    pub fn set_main_players(&mut self, value: Vec<String>) {
        self.main_players = value;
    }

    pub fn set_main_handles(&mut self, value: Vec<String>) {
        self.main_handles = value;
    }

    pub fn clear_main_identities(&mut self) {
        self.main_players = Vec::new();
        self.main_handles = Vec::new();
    }

    pub fn set_prestige_names(
        &mut self,
        value: std::collections::BTreeMap<String, LocalizedLabels>,
    ) {
        self.prestige_names = value;
    }

    pub fn clear_prestige_names(&mut self) {
        self.prestige_names = Default::default();
    }

    pub fn set_detailed_analysis_status(&mut self, value: impl Into<String>) {
        self.detailed_analysis_status = value.into();
    }

    pub fn set_detailed_analysis_atstart(&mut self, value: bool) {
        self.detailed_analysis_atstart = value;
    }

    pub fn with_detailed_analysis_atstart(mut self, value: bool) -> Self {
        self.set_detailed_analysis_atstart(value);
        self
    }

    pub fn detailed_analysis_atstart(&self) -> bool {
        self.detailed_analysis_atstart
    }

    pub fn set_analysis_running_status(&mut self, mode: AnalysisMode, phase: &str) {
        let status = TauriOverlayOps::analysis_status_text(mode, phase);
        match mode {
            AnalysisMode::Simple => self.simple_analysis_status = status,
            AnalysisMode::Detailed => self.detailed_analysis_status = status,
        }
    }

    pub fn set_analysis_terminal_status(&mut self, mode: AnalysisMode, phase: &str) {
        self.analysis_running = false;
        self.analysis_running_mode = None;
        match mode {
            AnalysisMode::Simple => {
                self.simple_analysis_status = TauriOverlayOps::analysis_status_text(mode, phase);
            }
            AnalysisMode::Detailed => {
                self.detailed_analysis_status = TauriOverlayOps::analysis_status_text(mode, phase);
            }
        }
    }

    pub fn as_payload(&self, scan_progress: ReplayScanProgressPayload) -> Value {
        TauriOverlayOps::to_json_value(self.as_payload_typed(scan_progress))
    }

    pub fn as_payload_typed(&self, scan_progress: ReplayScanProgressPayload) -> StatsStatePayload {
        let (analysis, main_players, main_handles, prestige_names, games, message) = if self.ready {
            (
                self.analysis.clone(),
                self.main_players.clone(),
                self.main_handles.clone(),
                self.prestige_names.clone(),
                self.games,
                self.message.clone(),
            )
        } else {
            (
                Some(TauriOverlayOps::empty_stats_payload()),
                Vec::new(),
                Vec::new(),
                Default::default(),
                0,
                if self.message.is_empty() {
                    "Statistics are updating. This may take a while.".to_string()
                } else {
                    self.message.clone()
                },
            )
        };

        let analysis = StatsAnalysisPayload::from_optional_value(analysis)
            .unwrap_or_else(|error| panic!("Failed to convert stats analysis payload: {error}"));

        StatsStatePayload {
            ready: self.ready,
            games,
            detailed_parsed_count: 0,
            total_valid_files: 0,
            analysis,
            main_players,
            main_handles,
            analysis_running: self.analysis_running,
            analysis_running_mode: self
                .analysis_running_mode
                .map(|mode| mode.key().to_string()),
            simple_analysis_status: self.simple_analysis_status.clone(),
            detailed_analysis_status: self.detailed_analysis_status.clone(),
            detailed_analysis_atstart: self.detailed_analysis_atstart,
            prestige_names,
            message,
            scan_progress,
            query: None,
        }
    }

    pub fn sync_detailed_analysis_status_from_replays(&mut self, replays: &[ReplayInfo]) {
        let total_valid_files = replays
            .iter()
            .filter(|replay| {
                replay.result() != "Unparsed" && replay.map().trim().starts_with("AC_")
            })
            .count();
        let detailed_parsed_count = replays
            .iter()
            .filter(|replay| {
                replay.result() != "Unparsed"
                    && replay.map().trim().starts_with("AC_")
                    && replay.has_detailed_analysis_cache()
            })
            .count();

        self.set_analysis_running(false);
        self.set_detailed_analysis_status(if detailed_parsed_count == 0 {
            TauriOverlayOps::analysis_status_text(AnalysisMode::Detailed, "not started")
        } else {
            format!(
                "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
            )
        });
    }

    pub fn sync_detailed_analysis_status_from_replays_with_dictionary(
        &mut self,
        replays: &[ReplayInfo],
        dictionary: &Sc2DictionaryData,
    ) {
        let total_valid_files = replays
            .iter()
            .filter(|replay| {
                replay.result() != "Unparsed"
                    && dictionary.canonicalize_coop_map_id(replay.map()).is_some()
            })
            .count();
        let detailed_parsed_count = replays
            .iter()
            .filter(|replay| {
                replay.result() != "Unparsed"
                    && dictionary.canonicalize_coop_map_id(replay.map()).is_some()
                    && replay.has_detailed_analysis_cache()
            })
            .count();

        self.set_analysis_running(false);
        self.set_detailed_analysis_status(if detailed_parsed_count == 0 {
            TauriOverlayOps::analysis_status_text(AnalysisMode::Detailed, "not started")
        } else {
            format!(
                "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
            )
        });
    }
}

#[derive(Debug, Default)]
pub struct StatsSnapshot {
    ready: bool,
    games: u64,
    main_players: Vec<String>,
    main_handles: Vec<String>,
    analysis: Value,
    prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    message: String,
}

impl StatsSnapshot {
    pub fn new(
        ready: bool,
        games: u64,
        main_players: Vec<String>,
        main_handles: Vec<String>,
        analysis: Value,
        prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ready,
            games,
            main_players,
            main_handles,
            analysis,
            prestige_names,
            message: message.into(),
        }
    }

    fn into_parts(self) -> StatsStateParts {
        (
            self.ready,
            self.games,
            self.main_players,
            self.main_handles,
            self.analysis,
            self.prestige_names,
            self.message,
        )
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn games(&self) -> u64 {
        self.games
    }

    pub fn main_players(&self) -> &[String] {
        &self.main_players
    }

    pub fn main_handles(&self) -> &[String] {
        &self.main_handles
    }

    pub fn analysis(&self) -> &Value {
        &self.analysis
    }

    pub fn prestige_names(&self) -> &std::collections::BTreeMap<String, LocalizedLabels> {
        &self.prestige_names
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}
