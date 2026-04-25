use crate::replay_info::{GamesRowPayload, ReplayChatPayload};
use crate::shared_types::{LocalizedLabels, ReplayScanProgressPayload};
use crate::{randomizer, replay_analysis, AppSettings};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayActionResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayActionResponse {
    pub status: &'static str,
    pub result: OverlayActionResult,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub randomizer: Option<randomizer::RandomizerResult>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigPayload {
    pub status: &'static str,
    pub settings: AppSettings,
    pub active_settings: AppSettings,
    pub randomizer_catalog: crate::shared_types::OverlayRandomizerCatalog,
    pub monitor_catalog: Vec<crate::shared_types::MonitorOption>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigReplaysPayload {
    pub status: &'static str,
    pub replays: Vec<GamesRowPayload>,
    #[ts(type = "number")]
    pub total_replays: usize,
    pub selected_replay_file: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigPlayersPayload {
    pub status: &'static str,
    pub players: Vec<replay_analysis::PlayerRowPayload>,
    pub loading: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigWeekliesPayload {
    pub status: &'static str,
    pub weeklies: Vec<replay_analysis::WeeklyRowPayload>,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigChatPayload {
    pub status: &'static str,
    pub chat: ReplayChatPayload,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsStatePayload {
    pub ready: bool,
    #[ts(type = "number")]
    pub games: u64,
    #[ts(type = "number")]
    pub detailed_parsed_count: u64,
    #[ts(type = "number")]
    pub total_valid_files: u64,
    #[ts(type = "Record<string, any> | null")]
    #[ts(optional)]
    pub analysis: Option<Value>,
    pub main_players: Vec<String>,
    pub main_handles: Vec<String>,
    pub analysis_running: bool,
    #[ts(optional)]
    pub analysis_running_mode: Option<String>,
    pub simple_analysis_status: String,
    pub detailed_analysis_status: String,
    pub detailed_analysis_atstart: bool,
    pub prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    pub message: String,
    pub scan_progress: ReplayScanProgressPayload,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsActionPayload {
    pub status: &'static str,
    pub result: OverlayActionResult,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<StatsStatePayload>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct AnalysisCompletedPayload {
    pub mode: String,
    pub message: String,
}
