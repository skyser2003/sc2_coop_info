use crate::cache_overall_stats_generator::{PlayerStatsSeries, ReplayBuildInfo};
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

fn find_main_player_pid(replay: &ParsedReplayInput, main_player_handles: &HashSet<String>) -> u8 {
    if main_player_handles.is_empty() {
        return 1;
    }

    replay
        .players
        .iter()
        .filter(|player| player.pid == 1 || player.pid == 2)
        .find(|player| main_player_handles.contains(player.handle.as_str()))
        .map(|player| player.pid)
        .unwrap_or(1)
}

fn find_player_or_unknown(replay: &ParsedReplayInput, pid: u8) -> ParsedReplayPlayer {
    if let Some(found) = replay.players.iter().find(|player| player.pid == pid) {
        return found.clone();
    }

    ParsedReplayPlayer {
        pid,
        name: "Unknown".to_string(),
        handle: String::new(),
        race: String::new(),
        observer: false,
        result: String::new(),
        commander: "Unknown".to_string(),
        commander_level: 0,
        commander_mastery_level: 0,
        prestige: 0,
        prestige_name: String::new(),
        apm: 0,
        masteries: [0, 0, 0, 0, 0, 0],
    }
}

fn normalized_commander_name(raw: &str) -> String {
    if raw.trim().is_empty() {
        "Unknown".to_string()
    } else {
        raw.to_string()
    }
}

fn empty_player_stats_series(name: String) -> PlayerStatsSeries {
    PlayerStatsSeries {
        name,
        supply: Vec::new(),
        mining: Vec::new(),
        army: Vec::new(),
        killed: Vec::new(),
        army_force_float_indices: Default::default(),
    }
}

fn resolve_main_player_pid(
    detailed_input: &ReplayReportDetailedInput,
    replay: &ParsedReplayInput,
    main_player_handles: &HashSet<String>,
) -> u8 {
    if let Some(positions) = detailed_input.positions.as_ref() {
        if matches!(positions.main, 1 | 2) {
            return positions.main;
        }
    }

    if let Some(main_position) = detailed_input.main_position {
        if matches!(main_position, 1 | 2) {
            return main_position;
        }
    }

    find_main_player_pid(replay, main_player_handles)
}

fn player_stats_with_names(
    incoming: Option<BTreeMap<u8, PlayerStatsSeries>>,
    main_name: &str,
    ally_name: &str,
) -> BTreeMap<u8, PlayerStatsSeries> {
    let mut player_stats = incoming.unwrap_or_default();
    player_stats
        .entry(1)
        .or_insert_with(|| empty_player_stats_series(main_name.to_string()))
        .name = main_name.to_string();
    player_stats
        .entry(2)
        .or_insert_with(|| empty_player_stats_series(ally_name.to_string()))
        .name = ally_name.to_string();
    player_stats
}

pub fn build_replay_report_detailed(
    replay_file: &str,
    detailed_input: &ReplayReportDetailedInput,
    main_player_handles: &HashSet<String>,
) -> ReplayReport {
    let replay = &detailed_input.parser;
    let main_pid = resolve_main_player_pid(detailed_input, replay, main_player_handles);
    let ally_pid = if main_pid == 1 { 2 } else { 1 };
    let main_player = find_player_or_unknown(replay, main_pid);
    let ally_player = find_player_or_unknown(replay, ally_pid);
    let player_stats = player_stats_with_names(
        detailed_input.player_stats.clone(),
        &main_player.name,
        &ally_player.name,
    );

    let report_length = detailed_input.length.unwrap_or(replay.accurate_length);
    let parser_accurate_length = detailed_input
        .length
        .map(|value| value * 1.4)
        .unwrap_or(replay.accurate_length);
    let parser_hash = replay
        .hash
        .clone()
        .or_else(|| detailed_input.replay_hash.clone());
    let mut parser = replay.clone();
    parser.accurate_length = parser_accurate_length;
    parser.hash = parser_hash;

    ReplayReport {
        file: replay_file.to_string(),
        replaydata: true,
        map_name: replay.map_name.clone(),
        extension: replay.extension,
        brutal_plus: replay.brutal_plus,
        result: replay.result.clone(),
        main: main_player.name.clone(),
        ally: ally_player.name.clone(),
        main_apm: main_player.apm,
        ally_apm: ally_player.apm,
        positions: PlayerPositions {
            main: main_pid,
            ally: ally_pid,
        },
        difficulty: replay.difficulty.1.clone(),
        main_icons: detailed_input.main_icons.clone().unwrap_or_default(),
        ally_icons: detailed_input.ally_icons.clone().unwrap_or_default(),
        player_stats,
        bonus: detailed_input.bonus.clone().unwrap_or_default(),
        comp: detailed_input.comp.clone().unwrap_or_default(),
        length: report_length,
        parser,
        mutators: replay.mutators.clone(),
        weekly: replay.weekly,
        main_commander: normalized_commander_name(main_player.commander.as_str()),
        main_commander_level: main_player.commander_level,
        main_masteries: main_player.masteries,
        main_kills: detailed_input.main_kills.unwrap_or(0),
        main_prestige: main_player.prestige_name,
        ally_commander: normalized_commander_name(ally_player.commander.as_str()),
        ally_commander_level: ally_player.commander_level,
        ally_masteries: ally_player.masteries,
        ally_kills: detailed_input.ally_kills.unwrap_or(0),
        ally_prestige: ally_player.prestige_name,
        main_units: detailed_input.main_units.clone().unwrap_or_default(),
        ally_units: detailed_input.ally_units.clone().unwrap_or_default(),
        amon_units: detailed_input.amon_units.clone().unwrap_or_default(),
        outlaw_order: detailed_input.outlaw_order.clone(),
    }
}

pub fn build_replay_report(
    replay_file: &str,
    replay: &ParsedReplayInput,
    main_player_handles: &HashSet<String>,
) -> ReplayReport {
    let input = ReplayReportDetailedInput::from_parser(replay.clone());
    build_replay_report_detailed(replay_file, &input, main_player_handles)
}
