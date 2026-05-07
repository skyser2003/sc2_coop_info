use crate::shared_types::{LocalizedLabels, ReplayScanProgressPayload};
use crate::{AppSettings, ReplayInfo, StatsStatePayload, TauriOverlayOps};
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use serde_json::Value;

pub(crate) type StatsStateParts = (
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
    FrontendReady,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupAnalysisRequestOutcome {
    pub include_detailed: bool,
    pub started: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisMode {
    Simple,
    Detailed,
}

pub(crate) struct AnalysisOutcome {
    reported_replay_count: usize,
    replays: Vec<ReplayInfo>,
    final_cache_entries: Vec<CacheReplayEntry>,
    analysis_completed: bool,
}

impl AnalysisOutcome {
    pub(crate) fn new(
        reported_replay_count: usize,
        replays: Vec<ReplayInfo>,
        final_cache_entries: Vec<CacheReplayEntry>,
        analysis_completed: bool,
    ) -> Self {
        Self {
            reported_replay_count,
            replays,
            final_cache_entries,
            analysis_completed,
        }
    }

    pub(crate) fn into_parts(self) -> (usize, Vec<ReplayInfo>, Vec<CacheReplayEntry>, bool) {
        (
            self.reported_replay_count,
            self.replays,
            self.final_cache_entries,
            self.analysis_completed,
        )
    }

    pub(crate) fn reported_replay_count(&self) -> usize {
        self.reported_replay_count
    }

    pub(crate) fn analysis_completed(&self) -> bool {
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
    pub(crate) fn from_settings(settings: &AppSettings) -> Self {
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

    pub fn analysis_cloned(&self) -> Option<Value> {
        self.analysis.clone()
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

    pub(crate) fn analysis_running_mode(&self) -> Option<AnalysisMode> {
        self.analysis_running_mode
    }

    pub fn detailed_analysis_status(&self) -> &str {
        &self.detailed_analysis_status
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn set_analysis_running(&mut self, value: bool) {
        self.analysis_running = value;
        if !value {
            self.analysis_running_mode = None;
        }
    }

    pub(crate) fn start_analysis(&mut self, mode: AnalysisMode) {
        self.analysis_running = true;
        self.analysis_running_mode = Some(mode);
    }

    pub(crate) fn set_startup_analysis_requested(&mut self, value: bool) {
        self.startup_analysis_requested = value;
    }

    pub fn set_ready(&mut self, value: bool) {
        self.ready = value;
    }

    pub fn set_analysis(&mut self, value: Option<Value>) {
        self.analysis = value;
    }

    pub(crate) fn set_games(&mut self, value: u64) {
        self.games = value;
    }

    pub fn set_message(&mut self, value: impl Into<String>) {
        self.message = value.into();
    }

    pub(crate) fn set_main_players(&mut self, value: Vec<String>) {
        self.main_players = value;
    }

    pub(crate) fn set_main_handles(&mut self, value: Vec<String>) {
        self.main_handles = value;
    }

    pub(crate) fn clear_main_identities(&mut self) {
        self.main_players = Vec::new();
        self.main_handles = Vec::new();
    }

    pub(crate) fn set_prestige_names(
        &mut self,
        value: std::collections::BTreeMap<String, LocalizedLabels>,
    ) {
        self.prestige_names = value;
    }

    pub(crate) fn clear_prestige_names(&mut self) {
        self.prestige_names = Default::default();
    }

    pub(crate) fn set_detailed_analysis_status(&mut self, value: impl Into<String>) {
        self.detailed_analysis_status = value.into();
    }

    pub fn set_detailed_analysis_atstart(&mut self, value: bool) {
        self.detailed_analysis_atstart = value;
    }

    pub fn with_detailed_analysis_atstart(mut self, value: bool) -> Self {
        self.set_detailed_analysis_atstart(value);
        self
    }

    pub(crate) fn detailed_analysis_atstart(&self) -> bool {
        self.detailed_analysis_atstart
    }

    pub(crate) fn set_analysis_running_status(&mut self, mode: AnalysisMode, phase: &str) {
        let status = TauriOverlayOps::analysis_status_text(mode, phase);
        match mode {
            AnalysisMode::Simple => self.simple_analysis_status = status,
            AnalysisMode::Detailed => self.detailed_analysis_status = status,
        }
    }

    pub(crate) fn set_analysis_terminal_status(&mut self, mode: AnalysisMode, phase: &str) {
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

    pub(crate) fn include_detailed_stats_for_cache(&self, replays: &[ReplayInfo]) -> bool {
        self.analysis
            .as_ref()
            .and_then(|analysis| analysis.get("UnitData"))
            .is_some_and(|value| !value.is_null())
            || replays.iter().any(ReplayInfo::has_detailed_unit_stats)
    }

    pub(crate) fn as_payload(&self, scan_progress: ReplayScanProgressPayload) -> Value {
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

        TauriOverlayOps::to_json_value(StatsStatePayload {
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
        })
    }

    pub(crate) fn as_payload_typed(
        &self,
        scan_progress: ReplayScanProgressPayload,
    ) -> StatsStatePayload {
        serde_json::from_value(self.as_payload(scan_progress))
            .unwrap_or_else(|error| panic!("Failed to convert stats payload: {error}"))
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
    pub(crate) fn new(
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

    pub(crate) fn into_parts(self) -> StatsStateParts {
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
