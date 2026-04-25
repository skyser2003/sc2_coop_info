use crate::UiMutatorRow;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
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
    pub mutators: Vec<UiMutatorRow>,
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
