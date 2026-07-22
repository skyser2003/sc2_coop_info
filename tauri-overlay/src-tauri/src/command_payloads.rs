use crate::replay_info::{GamesRowPayload, ReplayChatPayload};
use crate::replay_visual::ReplayVisualPayload;
use crate::{
    AppSettings, LocalizedLabels, MonitorOption, OverlayRandomizerCatalog, PlayerRowPayload,
    RandomizerResult, ReplayScanProgressPayload, StatsState, WeeklyRowPayload,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
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
    pub randomizer: Option<RandomizerResult>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigPayload {
    pub status: &'static str,
    pub settings: AppSettings,
    pub active_settings: AppSettings,
    pub randomizer_catalog: OverlayRandomizerCatalog,
    pub monitor_catalog: Vec<MonitorOption>,
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
    pub players: Vec<PlayerRowPayload>,
    #[ts(type = "number")]
    pub total_players: usize,
    pub loading: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigWeekliesPayload {
    pub status: &'static str,
    pub weeklies: Vec<WeeklyRowPayload>,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigChatPayload {
    pub status: &'static str,
    pub chat: ReplayChatPayload,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigReplayVisualPayload {
    pub status: &'static str,
    pub visual: ReplayVisualPayload,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsFastestMapPlayer {
    pub name: String,
    pub handle: String,
    pub commander: String,
    #[ts(type = "number")]
    pub apm: u64,
    #[serde(rename = "mastery_level")]
    #[ts(type = "number")]
    pub mastery_level: u64,
    #[ts(type = "Array<number>")]
    pub masteries: Vec<u64>,
    #[ts(type = "number")]
    pub prestige: u64,
    #[serde(rename = "prestige_name")]
    pub prestige_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsFastestMapDetails {
    pub length: f64,
    pub file: String,
    #[ts(type = "number")]
    pub date: u64,
    pub difficulty: String,
    pub players: Vec<StatsFastestMapPlayer>,
    #[serde(rename = "enemy_race")]
    pub enemy_race: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsMapDataRow {
    pub id: String,
    pub average_victory_time: f64,
    pub frequency: f64,
    #[serde(rename = "Victory")]
    #[ts(type = "number")]
    pub victory: u64,
    #[serde(rename = "Defeat")]
    #[ts(type = "number")]
    pub defeat: u64,
    #[serde(rename = "Winrate")]
    pub winrate: f64,
    pub bonus: f64,
    #[serde(rename = "detailedCount")]
    #[ts(type = "number")]
    pub detailed_count: u64,
    #[serde(rename = "Fastest")]
    pub fastest: StatsFastestMapDetails,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsCommanderDataRow {
    #[serde(rename = "Frequency")]
    pub frequency: f64,
    #[serde(rename = "Victory")]
    #[ts(type = "number")]
    pub victory: u64,
    #[serde(rename = "Defeat")]
    #[ts(type = "number")]
    pub defeat: u64,
    #[serde(rename = "Winrate")]
    pub winrate: f64,
    #[serde(rename = "MedianAPM")]
    pub median_apm: f64,
    #[serde(rename = "KillFraction")]
    pub kill_fraction: f64,
    #[serde(rename = "Mastery")]
    #[ts(type = "Record<string, number>")]
    pub mastery: BTreeMap<String, f64>,
    #[serde(rename = "MasteryDistribution")]
    #[ts(type = "Record<string, Record<string, number>>")]
    pub mastery_distribution: BTreeMap<String, BTreeMap<String, f64>>,
    #[serde(rename = "MasteryDistributionByPrestige")]
    #[ts(type = "Record<string, Record<string, Record<string, number>>>")]
    pub mastery_distribution_by_prestige: BTreeMap<String, BTreeMap<String, BTreeMap<String, f64>>>,
    #[serde(rename = "Prestige")]
    #[ts(type = "Record<string, number>")]
    pub prestige: BTreeMap<String, f64>,
    #[serde(rename = "MasteryByPrestige")]
    #[ts(type = "Record<string, Record<string, number>>")]
    pub mastery_by_prestige: BTreeMap<String, BTreeMap<String, f64>>,
    #[serde(rename = "detailedCount")]
    #[ts(type = "number")]
    pub detailed_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsDifficultyDataRow {
    #[serde(rename = "Victory")]
    #[ts(type = "number")]
    pub victory: u64,
    #[serde(rename = "Defeat")]
    #[ts(type = "number")]
    pub defeat: u64,
    #[serde(rename = "Winrate")]
    pub winrate: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsRegionDataRow {
    pub frequency: f64,
    #[serde(rename = "Victory")]
    #[ts(type = "number")]
    pub victory: u64,
    #[serde(rename = "Defeat")]
    #[ts(type = "number")]
    pub defeat: u64,
    pub winrate: f64,
    #[ts(type = "number")]
    pub max_asc: u64,
    #[ts(type = "Record<string, number>")]
    pub prestiges: BTreeMap<String, u64>,
    pub max_com: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsPlayerDataRow {
    #[ts(type = "number")]
    pub wins: u64,
    #[ts(type = "number")]
    pub losses: u64,
    pub winrate: f64,
    pub kills: f64,
    pub apm: f64,
    pub frequency: f64,
    #[ts(type = "number")]
    pub last_seen: u64,
    pub commander: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StatsUnitCountValue {
    Count(i64),
    Hidden(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StatsUnitRatioValue {
    Ratio(f64),
    Hidden(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsCommanderUnitRow {
    #[ts(type = "number | string")]
    pub created: StatsUnitCountValue,
    pub made: f64,
    #[ts(type = "number | string")]
    pub lost: StatsUnitCountValue,
    pub lost_percent: Option<f64>,
    #[ts(type = "number")]
    pub kills: i64,
    #[serde(rename = "KD")]
    pub kd: Option<f64>,
    pub kill_percentage: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StatsCommanderUnitRows {
    #[serde(default)]
    pub count: u64,
    #[serde(flatten)]
    pub units: BTreeMap<String, StatsCommanderUnitRow>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsAmonUnitRow {
    #[ts(type = "number")]
    pub created: i64,
    #[ts(type = "number")]
    pub lost: i64,
    #[ts(type = "number")]
    pub kills: i64,
    #[serde(rename = "KD")]
    #[ts(type = "number | string")]
    pub kd: StatsUnitRatioValue,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsUnitDataPayload {
    #[ts(
        type = "Record<string, ({ count: number } & Record<string, StatsCommanderUnitRow | number>) | null>"
    )]
    pub main: BTreeMap<String, Option<StatsCommanderUnitRows>>,
    #[ts(
        type = "Record<string, ({ count: number } & Record<string, StatsCommanderUnitRow | number>) | null>"
    )]
    pub ally: BTreeMap<String, Option<StatsCommanderUnitRows>>,
    pub amon: BTreeMap<String, StatsAmonUnitRow>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsAnalysisPayload {
    #[serde(rename = "MapData")]
    pub map_data: BTreeMap<String, StatsMapDataRow>,
    #[serde(rename = "CommanderData")]
    pub commander_data: BTreeMap<String, StatsCommanderDataRow>,
    #[serde(rename = "AllyCommanderData")]
    pub ally_commander_data: BTreeMap<String, StatsCommanderDataRow>,
    #[serde(rename = "DifficultyData")]
    pub difficulty_data: BTreeMap<String, StatsDifficultyDataRow>,
    #[serde(rename = "RegionData")]
    pub region_data: BTreeMap<String, StatsRegionDataRow>,
    #[serde(rename = "PlayerData")]
    pub player_data: BTreeMap<String, StatsPlayerDataRow>,
    #[serde(rename = "AmonData")]
    pub amon_data: BTreeMap<String, StatsAmonUnitRow>,
    #[serde(rename = "UnitData")]
    pub unit_data: Option<StatsUnitDataPayload>,
    #[serde(
        rename = "MapDataReady",
        default,
        skip_serializing_if = "Self::map_data_ready_is_false"
    )]
    pub map_data_ready: bool,
}

impl StatsAnalysisPayload {
    fn map_data_ready_is_false(value: &bool) -> bool {
        !*value
    }

    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn from_optional_value(value: Option<Value>) -> Result<Option<Self>, serde_json::Error> {
        match value {
            Some(Value::Null) | None => Ok(None),
            Some(value) => Self::from_value(value).map(Some),
        }
    }
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
    #[ts(optional)]
    pub analysis: Option<StatsAnalysisPayload>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub query: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct AnalysisStatusPayload {
    status: &'static str,
    ready: bool,
    analysis_running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    analysis_running_mode: Option<String>,
    current_status: String,
    simple_analysis_status: String,
    detailed_analysis_status: String,
    #[ts(type = "number")]
    detailed_parsed_count: u64,
    #[ts(type = "number")]
    total_valid_files: u64,
    scan_progress: ReplayScanProgressPayload,
}

impl AnalysisStatusPayload {
    pub fn new(stats: &StatsState, scan_progress: ReplayScanProgressPayload) -> Self {
        Self {
            status: "ok",
            ready: stats.ready(),
            analysis_running: stats.analysis_running(),
            analysis_running_mode: stats
                .analysis_running_mode()
                .map(|mode| mode.key().to_string()),
            current_status: stats.current_analysis_status().to_string(),
            simple_analysis_status: stats.simple_analysis_status().to_string(),
            detailed_analysis_status: stats.detailed_analysis_status().to_string(),
            detailed_parsed_count: stats.detailed_parsed_count(),
            total_valid_files: stats.total_valid_files(),
            scan_progress,
        }
    }
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
