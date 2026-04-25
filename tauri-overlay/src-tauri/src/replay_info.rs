use crate::shared_types;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use ts_rs::TS;

#[derive(Clone, Serialize, Default, PartialEq, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayChatMessage {
    pub player: u8,
    pub text: String,
    pub time: f64,
}

#[derive(Clone, Serialize, Default, PartialEq, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayChatPayload {
    pub file: String,
    #[ts(type = "number")]
    pub date: u64,
    pub map: String,
    pub result: String,
    pub slot1_name: String,
    pub slot2_name: String,
    pub messages: Vec<ReplayChatMessage>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct GamesRowPayload {
    pub file: String,
    #[ts(type = "number")]
    pub date: u64,
    pub map: String,
    pub result: String,
    pub difficulty: String,
    pub p1: String,
    pub p2: String,
    pub slot1_commander: String,
    pub slot2_commander: String,
    pub enemy: String,
    pub main_commander: String,
    pub ally_commander: String,
    #[ts(type = "number")]
    pub length: u64,
    #[ts(type = "number")]
    pub main_apm: u64,
    #[ts(type = "number")]
    pub ally_apm: u64,
    #[ts(type = "number")]
    pub main_kills: u64,
    #[ts(type = "number")]
    pub ally_kills: u64,
    pub extension: bool,
    #[ts(type = "number")]
    pub brutal_plus: u64,
    pub weekly: bool,
    #[ts(optional)]
    pub weekly_name: Option<String>,
    pub mutators: Vec<shared_types::UiMutatorRow>,
    pub is_mutation: bool,
}

#[derive(Clone, Default)]
pub struct ReplayInfo {
    pub(crate) file: String,
    pub(crate) date: u64,
    pub(crate) map: String,
    pub(crate) result: String,
    pub(crate) difficulty: String,
    pub(crate) enemy: String,
    pub(crate) length: u64,
    pub(crate) accurate_length: f64,
    pub(crate) slot1: ReplayPlayerInfo,
    pub(crate) slot2: ReplayPlayerInfo,
    pub(crate) main_slot: usize,
    pub(crate) amon_units: Value,
    pub(crate) player_stats: Value,
    pub(crate) extension: bool,
    pub(crate) brutal_plus: u64,
    pub(crate) weekly: bool,
    pub(crate) weekly_name: Option<String>,
    pub(crate) mutators: Vec<String>,
    pub(crate) comp: String,
    pub(crate) bonus: Vec<u64>,
    pub(crate) bonus_total: Option<u64>,
    pub(crate) messages: Vec<ReplayChatMessage>,
    pub(crate) is_detailed: bool,
}

#[derive(Clone, Default)]
pub struct ReplayPlayerInfo {
    pub(crate) name: String,
    pub(crate) handle: String,
    pub(crate) apm: u64,
    pub(crate) kills: u64,
    pub(crate) commander: String,
    pub(crate) commander_level: u64,
    pub(crate) mastery_level: u64,
    pub(crate) prestige: u64,
    pub(crate) masteries: Vec<u64>,
    pub(crate) units: Value,
    pub(crate) icons: Value,
}

#[derive(Default)]
pub(crate) struct Aggregate {
    pub(crate) wins: u64,
    pub(crate) losses: u64,
}

#[derive(Default)]
pub(crate) struct RegionAggregate {
    pub(crate) wins: u64,
    pub(crate) losses: u64,
    pub(crate) max_asc: u64,
    pub(crate) max_com: HashSet<String>,
    pub(crate) prestiges: HashMap<String, u64>,
}

#[derive(Default)]
pub(crate) struct CommanderAggregate {
    pub(crate) wins: u64,
    pub(crate) losses: u64,
    pub(crate) apm_values: Vec<u64>,
    pub(crate) kill_fractions: Vec<f64>,
    pub(crate) mastery_counts: [f64; 6],
    pub(crate) mastery_by_prestige_counts: [[f64; 6]; 4],
    pub(crate) prestige_counts: [u64; 4],
    pub(crate) detailed_count: u64,
}

#[derive(Default)]
pub(crate) struct PlayerAggregate {
    pub(crate) wins: u64,
    pub(crate) losses: u64,
    pub(crate) apm_values: Vec<u64>,
    pub(crate) kill_fractions: Vec<f64>,
    pub(crate) last_seen: u64,
    pub(crate) handles: BTreeSet<String>,
    pub(crate) names: HashMap<String, u64>,
    pub(crate) commander: String,
    pub(crate) commander_counts: HashMap<String, u64>,
}

#[derive(Default)]
pub(crate) struct MapAggregate {
    pub(crate) wins: u64,
    pub(crate) losses: u64,
    pub(crate) victory_length_sum: f64,
    pub(crate) victory_games: u64,
    pub(crate) bonus_fraction_sum: f64,
    pub(crate) bonus_games: u64,
    pub(crate) fastest_length: f64,
    pub(crate) fastest_file: String,
    pub(crate) fastest_p1: String,
    pub(crate) fastest_p2: String,
    pub(crate) fastest_p1_handle: String,
    pub(crate) fastest_p2_handle: String,
    pub(crate) fastest_p1_commander: String,
    pub(crate) fastest_p2_commander: String,
    pub(crate) fastest_p1_apm: u64,
    pub(crate) fastest_p2_apm: u64,
    pub(crate) fastest_p1_mastery_level: u64,
    pub(crate) fastest_p2_mastery_level: u64,
    pub(crate) fastest_p1_masteries: Vec<u64>,
    pub(crate) fastest_p2_masteries: Vec<u64>,
    pub(crate) fastest_p1_prestige: u64,
    pub(crate) fastest_p2_prestige: u64,
    pub(crate) fastest_date: u64,
    pub(crate) fastest_difficulty: String,
    pub(crate) fastest_enemy_race: String,
    pub(crate) detailed_count: u64,
}

#[derive(Default, Clone)]
pub struct UnitStatsRollup {
    pub created: i64,
    pub created_hidden: bool,
    pub made: u64,
    pub lost: i64,
    pub lost_hidden: bool,
    pub kills: i64,
    pub kill_percentages: Vec<f64>,
}

#[derive(Default)]
pub struct CommanderUnitRollup {
    pub count: u64,
    pub units: HashMap<String, UnitStatsRollup>,
}
