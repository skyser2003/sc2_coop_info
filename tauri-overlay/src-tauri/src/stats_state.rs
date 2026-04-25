use crate::shared_types::LocalizedLabels;
use crate::ReplayInfo;
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use serde_json::Value;

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
    pub(crate) reported_replay_count: usize,
    pub(crate) replays: Vec<ReplayInfo>,
    pub(crate) final_cache_entries: Vec<CacheReplayEntry>,
    pub(crate) analysis_completed: bool,
}

#[derive(Debug)]
pub struct StatsState {
    pub(crate) ready: bool,
    pub(crate) analysis: Option<Value>,
    pub(crate) games: u64,
    pub(crate) main_players: Vec<String>,
    pub(crate) main_handles: Vec<String>,
    pub(crate) startup_analysis_requested: bool,
    pub(crate) analysis_running: bool,
    pub(crate) analysis_running_mode: Option<AnalysisMode>,
    pub(crate) simple_analysis_status: String,
    pub(crate) detailed_analysis_status: String,
    pub(crate) detailed_analysis_atstart: bool,
    pub(crate) prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    pub(crate) message: String,
}

#[derive(Debug, Default)]
pub struct StatsSnapshot {
    pub(crate) ready: bool,
    pub(crate) games: u64,
    pub(crate) main_players: Vec<String>,
    pub(crate) main_handles: Vec<String>,
    pub(crate) analysis: Value,
    pub(crate) prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    pub(crate) message: String,
}
