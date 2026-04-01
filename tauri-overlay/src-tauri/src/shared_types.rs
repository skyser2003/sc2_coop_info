use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

fn as_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct LocalizedLabels {
    pub ko: Vec<String>,
    pub en: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct LocalizedText {
    pub ko: String,
    pub en: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct UiMutatorRow {
    pub id: String,
    pub name: LocalizedText,
    #[serde(rename = "iconName")]
    pub icon_name: String,
    pub description: LocalizedText,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayRandomizerRange {
    pub min: u64,
    pub max: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayRandomizerMutator {
    pub id: String,
    pub name: LocalizedText,
    #[serde(rename = "iconName")]
    pub icon_name: String,
    pub description: LocalizedText,
    pub points: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayRandomizerBrutalPlus {
    pub brutal_plus: u8,
    pub mutator_points: OverlayRandomizerRange,
    pub mutator_count: OverlayRandomizerRange,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct EmptyPayload {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct MonitorOption {
    pub index: usize,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayRandomizerCatalog {
    pub prestige_names: BTreeMap<String, LocalizedLabels>,
    pub mutators: Vec<OverlayRandomizerMutator>,
    pub brutal_plus: Vec<OverlayRandomizerBrutalPlus>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayColorPreviewPayload {
    #[ts(optional)]
    pub color_player1: Option<String>,
    #[ts(optional)]
    pub color_player2: Option<String>,
    #[ts(optional)]
    pub color_amon: Option<String>,
    #[ts(optional)]
    pub color_mastery: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayLanguagePreviewPayload {
    pub language: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayInitColorsDurationPayload {
    pub colors: [Option<String>; 4],
    pub duration: u32,
    pub show_charts: bool,
    pub show_session: bool,
    pub session_victory: u32,
    pub session_defeat: u32,
    pub language: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayScreenshotRequestPayload {
    pub path: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayScreenshotResultPayload {
    pub ok: bool,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct PerformanceVisibilityPayload {
    pub visible: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayPlayerSeries {
    pub name: String,
    pub army: Vec<f64>,
    pub supply: Vec<f64>,
    pub killed: Vec<f64>,
    pub mining: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(untagged)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
#[ts(untagged)]
pub enum OverlayIconValue {
    Count(u32),
    Names(Vec<String>),
}

pub type UnitStatsTuple = [f64; 4];
pub type UnitStatsMap = BTreeMap<String, UnitStatsTuple>;
pub type OverlayIconPayload = BTreeMap<String, OverlayIconValue>;
pub type ReplayDataRecord = BTreeMap<String, ReplayPlayerSeries>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayReplayPayload {
    pub file: String,
    pub map_name: String,
    pub main: String,
    pub ally: String,
    #[serde(rename = "mainCommander")]
    pub main_commander: String,
    #[serde(rename = "allyCommander")]
    pub ally_commander: String,
    #[serde(rename = "mainAPM")]
    pub main_apm: u32,
    #[serde(rename = "allyAPM")]
    pub ally_apm: u32,
    pub mainkills: u32,
    pub allykills: u32,
    pub result: String,
    pub difficulty: String,
    pub length: u32,
    #[serde(rename = "B+")]
    pub brutal_plus: u32,
    pub weekly: bool,
    #[ts(optional)]
    pub weekly_name: Option<String>,
    pub extension: bool,
    #[serde(rename = "mainCommanderLevel")]
    pub main_commander_level: u32,
    #[serde(rename = "allyCommanderLevel")]
    pub ally_commander_level: u32,
    #[serde(rename = "mainMasteries")]
    pub main_masteries: Vec<u32>,
    #[serde(rename = "allyMasteries")]
    pub ally_masteries: Vec<u32>,
    #[serde(rename = "mainUnits")]
    pub main_units: UnitStatsMap,
    #[serde(rename = "allyUnits")]
    pub ally_units: UnitStatsMap,
    pub amon_units: UnitStatsMap,
    #[serde(rename = "mainIcons")]
    pub main_icons: OverlayIconPayload,
    #[serde(rename = "allyIcons")]
    pub ally_icons: OverlayIconPayload,
    pub mutators: Vec<String>,
    pub bonus: Vec<u32>,
    #[ts(optional)]
    pub bonus_total: Option<u32>,
    #[serde(rename = "player_stats")]
    #[ts(optional)]
    pub player_stats: Option<ReplayDataRecord>,
    #[serde(rename = "mainPrestige")]
    pub main_prestige: String,
    #[serde(rename = "allyPrestige")]
    pub ally_prestige: String,
    #[serde(rename = "Victory")]
    #[ts(optional)]
    pub victory: Option<u32>,
    #[serde(rename = "Defeat")]
    #[ts(optional)]
    pub defeat: Option<u32>,
    #[serde(rename = "Commander")]
    #[ts(optional)]
    pub commander: Option<String>,
    #[serde(rename = "Prestige")]
    #[ts(optional)]
    pub prestige: Option<String>,
    #[serde(rename = "newReplay")]
    #[ts(optional)]
    pub new_replay: Option<bool>,
    #[ts(optional)]
    pub fastest: Option<bool>,
    pub comp: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
#[ts(tag = "kind", rename_all = "snake_case")]
pub enum OverlayPlayerInfoRow {
    NoGames {
        #[ts(optional)]
        note: Option<String>,
    },
    Stats {
        wins: u32,
        losses: u32,
        apm: u32,
        commander: String,
        frequency: f64,
        kills: f64,
        last_seen_relative: String,
        #[ts(optional)]
        note: Option<String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayPlayerInfoPayload {
    pub data: BTreeMap<String, OverlayPlayerInfoRow>,
}

pub fn unit_stats_map_from_value(value: &Value) -> UnitStatsMap {
    let mut output = UnitStatsMap::new();
    if let Value::Object(raw) = value {
        for (key, raw_entry) in raw {
            if key.is_empty() {
                continue;
            }
            let Some(arr) = raw_entry.as_array() else {
                continue;
            };
            let mut values = [0.0_f64; 4];
            for (idx, item) in arr.iter().take(4).enumerate() {
                let Some(number) = item.as_f64() else {
                    continue;
                };
                values[idx] = if idx < 3 {
                    if number.is_finite() {
                        number.round().max(0.0)
                    } else {
                        0.0
                    }
                } else if number.is_finite() {
                    number.max(0.0)
                } else {
                    0.0
                };
            }
            output.insert(key.clone(), values);
        }
    }
    output
}

pub fn overlay_icon_payload_from_value(value: &Value) -> OverlayIconPayload {
    let mut output = OverlayIconPayload::new();
    if let Value::Object(raw) = value {
        for (key, raw_value) in raw {
            if key.is_empty() {
                continue;
            }
            if key == "outlaws" {
                if let Some(items) = raw_value.as_array() {
                    let outlaws = items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    if !outlaws.is_empty() {
                        output.insert(key.clone(), OverlayIconValue::Names(outlaws));
                    }
                }
                continue;
            }
            if let Some(count) = raw_value.as_u64() {
                output.insert(key.clone(), OverlayIconValue::Count(as_u32(count)));
            }
        }
    }
    output
}

pub fn replay_data_record_from_value(value: &Value) -> ReplayDataRecord {
    let mut output = ReplayDataRecord::new();
    if let Value::Object(players) = value {
        for (key, raw_player) in players {
            let Some(raw_player) = raw_player.as_object() else {
                continue;
            };
            let sanitize_array = |entry: Option<&Vec<Value>>| -> Vec<f64> {
                entry
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(Value::as_f64)
                            .map(|value| if value.is_finite() { value } else { 0.0 })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };

            output.insert(
                key.clone(),
                ReplayPlayerSeries {
                    name: raw_player
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    killed: sanitize_array(raw_player.get("killed").and_then(Value::as_array)),
                    army: sanitize_array(raw_player.get("army").and_then(Value::as_array)),
                    supply: sanitize_array(raw_player.get("supply").and_then(Value::as_array)),
                    mining: sanitize_array(raw_player.get("mining").and_then(Value::as_array)),
                },
            );
        }
    }
    output
}

pub fn swap_replay_data_record_sides(value: &mut Option<ReplayDataRecord>) {
    let Some(record) = value.as_mut() else {
        return;
    };

    let left = record.remove("1");
    let right = record.remove("2");

    if let Some(entry) = left {
        record.insert("2".to_string(), entry);
    }
    if let Some(entry) = right {
        record.insert("1".to_string(), entry);
    }
}
