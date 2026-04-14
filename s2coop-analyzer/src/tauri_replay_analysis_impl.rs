use crate::cache_overall_stats_generator::{PlayerStatsSeries, ReplayBuildInfo};
use crate::detailed_replay_analysis::{
    build_replay_report_from_detailed_input, build_replay_report_from_parser,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

type UnitStats = (i64, i64, i64, f64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedReplayPlayer {
    pub pid: u8,
    pub name: String,
    pub handle: String,
    pub race: String,
    pub observer: bool,
    pub result: String,
    pub commander: String,
    pub commander_level: u32,
    pub commander_mastery_level: u32,
    pub prestige: u32,
    pub prestige_name: String,
    pub apm: u32,
    pub masteries: [u32; 6],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedReplayMessage {
    pub text: String,
    pub player: u8,
    pub time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedReplayInput {
    pub file: String,
    pub map_name: String,
    pub extension: bool,
    pub brutal_plus: u32,
    pub result: String,
    pub players: Vec<ParsedReplayPlayer>,
    pub difficulty: (String, String),
    pub accurate_length: f64,
    pub form_alength: String,
    pub length: u64,
    pub mutators: Vec<String>,
    pub weekly: bool,
    pub messages: Vec<ParsedReplayMessage>,
    pub hash: Option<String>,
    pub build: ReplayBuildInfo,
    pub date: String,
    pub enemy_race: String,
    pub ext_difficulty: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerPositions {
    pub main: u8,
    pub ally: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReport {
    pub file: String,
    pub replaydata: bool,
    pub map_name: String,
    pub extension: bool,
    #[serde(rename = "B+")]
    pub brutal_plus: u32,
    pub result: String,
    pub main: String,
    pub ally: String,
    #[serde(rename = "mainAPM")]
    pub main_apm: u32,
    #[serde(rename = "allyAPM")]
    pub ally_apm: u32,
    pub positions: PlayerPositions,
    pub difficulty: String,
    #[serde(rename = "mainIcons")]
    pub main_icons: BTreeMap<String, u64>,
    #[serde(rename = "allyIcons")]
    pub ally_icons: BTreeMap<String, u64>,
    pub player_stats: BTreeMap<u8, PlayerStatsSeries>,
    pub bonus: Vec<String>,
    pub comp: String,
    pub length: f64,
    pub parser: ParsedReplayInput,
    pub mutators: Vec<String>,
    pub weekly: bool,
    #[serde(rename = "mainCommander")]
    pub main_commander: String,
    #[serde(rename = "mainCommanderLevel")]
    pub main_commander_level: u32,
    #[serde(rename = "mainMasteries")]
    pub main_masteries: [u32; 6],
    #[serde(rename = "mainkills")]
    pub main_kills: u64,
    #[serde(rename = "mainPrestige")]
    pub main_prestige: String,
    #[serde(rename = "allyCommander")]
    pub ally_commander: String,
    #[serde(rename = "allyCommanderLevel")]
    pub ally_commander_level: u32,
    #[serde(rename = "allyMasteries")]
    pub ally_masteries: [u32; 6],
    #[serde(rename = "allykills")]
    pub ally_kills: u64,
    #[serde(rename = "allyPrestige")]
    pub ally_prestige: String,
    #[serde(rename = "mainUnits")]
    pub main_units: BTreeMap<String, UnitStats>,
    #[serde(rename = "allyUnits")]
    pub ally_units: BTreeMap<String, UnitStats>,
    pub amon_units: BTreeMap<String, UnitStats>,
    #[serde(skip)]
    pub outlaw_order: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReportDetailedInput {
    pub parser: ParsedReplayInput,
    pub positions: Option<PlayerPositions>,
    pub main_position: Option<u8>,
    pub length: Option<f64>,
    pub bonus: Option<Vec<String>>,
    pub comp: Option<String>,
    pub replay_hash: Option<String>,
    pub main_kills: Option<u64>,
    pub ally_kills: Option<u64>,
    pub main_icons: Option<BTreeMap<String, u64>>,
    pub ally_icons: Option<BTreeMap<String, u64>>,
    pub main_units: Option<BTreeMap<String, UnitStats>>,
    pub ally_units: Option<BTreeMap<String, UnitStats>>,
    pub amon_units: Option<BTreeMap<String, UnitStats>>,
    pub player_stats: Option<BTreeMap<u8, PlayerStatsSeries>>,
    #[serde(skip)]
    pub outlaw_order: Option<Vec<String>>,
}

impl ReplayReportDetailedInput {
    pub fn from_parser(parser: ParsedReplayInput) -> Self {
        Self {
            parser,
            positions: None,
            main_position: None,
            length: None,
            bonus: None,
            comp: None,
            replay_hash: None,
            main_kills: None,
            ally_kills: None,
            main_icons: None,
            ally_icons: None,
            main_units: None,
            ally_units: None,
            amon_units: None,
            player_stats: None,
            outlaw_order: None,
        }
    }
}

pub fn build_replay_report_detailed(
    replay_file: &str,
    detailed_input: &ReplayReportDetailedInput,
    main_player_handles: &HashSet<String>,
) -> ReplayReport {
    build_replay_report_from_detailed_input(replay_file, detailed_input, main_player_handles)
}

pub fn build_replay_report(
    replay_file: &str,
    replay: &ParsedReplayInput,
    main_player_handles: &HashSet<String>,
) -> ReplayReport {
    build_replay_report_from_parser(replay_file, replay, main_player_handles)
}
