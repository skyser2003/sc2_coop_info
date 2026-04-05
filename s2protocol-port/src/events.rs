use crate::value::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerEventData {
    pub contains_selection_changed: bool,
    pub contains_none: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AbilityData {
    pub m_abilLink: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotPointValue {
    Int(i64),
    Float(f64),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapshotPoint {
    pub values: Vec<SnapshotPointValue>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TargetUnitData {
    pub m_snapshotPoint: Option<SnapshotPoint>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CmdEventData {
    pub TargetUnit: Option<TargetUnitData>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerStatsData {
    pub m_score_value_food_used: Option<f64>,
    pub m_score_value_minerals_collection_rate: Option<f64>,
    pub m_score_value_vespene_collection_rate: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameEvent {
    pub event: String,
    pub event_id: u32,
    pub game_loop: i64,
    pub user_id: Option<i64>,
    pub bits: i64,
    pub m_control_id: Option<i64>,
    pub m_event_type: Option<i64>,
    pub m_event_data: Option<TriggerEventData>,
    pub m_abil: Option<AbilityData>,
    pub m_data: Option<CmdEventData>,
    pub m_target: Option<TargetUnitData>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageEvent {
    pub event: String,
    pub event_id: u32,
    pub game_loop: i64,
    pub user_id: Option<i64>,
    pub bits: i64,
    pub m_string: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackerEvent {
    pub event: String,
    pub event_id: u32,
    pub game_loop: i64,
    pub bits: i64,
    pub m_player_id: Option<i64>,
    pub m_upgrade_type_name: Option<String>,
    pub m_count: Option<i64>,
    pub m_stats: Option<PlayerStatsData>,
    pub m_unit_type_name: Option<String>,
    pub m_creator_ability_name: Option<String>,
    pub m_control_player_id: Option<i64>,
    pub m_unit_tag_index: Option<i64>,
    pub m_unit_tag_recycle: Option<i64>,
    pub m_creator_unit_tag_index: Option<i64>,
    pub m_creator_unit_tag_recycle: Option<i64>,
    pub m_killer_unit_tag_index: Option<i64>,
    pub m_killer_unit_tag_recycle: Option<i64>,
    pub m_killer_player_id: Option<i64>,
    pub m_x: Option<i64>,
    pub m_y: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayEvent {
    Game(GameEvent),
    Tracker(TrackerEvent),
}

impl ReplayEvent {
    pub fn _event(&self) -> &str {
        match self {
            Self::Game(event) => &event.event,
            Self::Tracker(event) => &event.event,
        }
    }

    pub fn _gameloop(&self) -> i64 {
        match self {
            Self::Game(event) => event.game_loop,
            Self::Tracker(event) => event.game_loop,
        }
    }
}

impl GameEvent {
    pub(crate) fn from_value(value: Value) -> Self {
        let map = object_map(&value);
        Self {
            event: string_field(map, "_event").unwrap_or_default(),
            event_id: i64_field(map, "_eventid")
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default(),
            game_loop: i64_field(map, "_gameloop").unwrap_or_default(),
            user_id: nested_i64_field(map, &["_userid", "m_userId"]),
            bits: i64_field(map, "_bits").unwrap_or_default(),
            m_control_id: i64_field(map, "m_controlId"),
            m_event_type: i64_field(map, "m_eventType"),
            m_event_data: map
                .and_then(|fields| fields.get("m_eventData"))
                .map(parse_trigger_event_data),
            m_abil: map
                .and_then(|fields| fields.get("m_abil"))
                .and_then(parse_ability_data),
            m_data: map
                .and_then(|fields| fields.get("m_data"))
                .and_then(parse_cmd_event_data),
            m_target: map
                .and_then(|fields| fields.get("m_target"))
                .and_then(parse_target_unit_data),
        }
    }
}

impl MessageEvent {
    pub(crate) fn from_value(value: Value) -> Self {
        let map = object_map(&value);
        Self {
            event: string_field(map, "_event").unwrap_or_default(),
            event_id: i64_field(map, "_eventid")
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default(),
            game_loop: i64_field(map, "_gameloop").unwrap_or_default(),
            user_id: nested_i64_field(map, &["_userid", "m_userId"]),
            bits: i64_field(map, "_bits").unwrap_or_default(),
            m_string: string_field(map, "m_string"),
        }
    }
}

impl TrackerEvent {
    pub(crate) fn from_value(value: Value) -> Self {
        let map = object_map(&value);
        Self {
            event: string_field(map, "_event").unwrap_or_default(),
            event_id: i64_field(map, "_eventid")
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default(),
            game_loop: i64_field(map, "_gameloop").unwrap_or_default(),
            bits: i64_field(map, "_bits").unwrap_or_default(),
            m_player_id: i64_field(map, "m_playerId"),
            m_upgrade_type_name: string_field(map, "m_upgradeTypeName"),
            m_count: i64_field(map, "m_count"),
            m_stats: map
                .and_then(|fields| fields.get("m_stats"))
                .and_then(parse_player_stats),
            m_unit_type_name: string_field(map, "m_unitTypeName"),
            m_creator_ability_name: string_field(map, "m_creatorAbilityName"),
            m_control_player_id: i64_field(map, "m_controlPlayerId"),
            m_unit_tag_index: i64_field(map, "m_unitTagIndex"),
            m_unit_tag_recycle: i64_field(map, "m_unitTagRecycle"),
            m_creator_unit_tag_index: i64_field(map, "m_creatorUnitTagIndex"),
            m_creator_unit_tag_recycle: i64_field(map, "m_creatorUnitTagRecycle"),
            m_killer_unit_tag_index: i64_field(map, "m_killerUnitTagIndex"),
            m_killer_unit_tag_recycle: i64_field(map, "m_killerUnitTagRecycle"),
            m_killer_player_id: i64_field(map, "m_killerPlayerId"),
            m_x: i64_field(map, "m_x"),
            m_y: i64_field(map, "m_y"),
        }
    }
}

fn parse_trigger_event_data(value: &Value) -> TriggerEventData {
    TriggerEventData {
        contains_selection_changed: value_contains(value, "SelectionChanged"),
        contains_none: value_contains(value, "None"),
    }
}

fn parse_ability_data(value: &Value) -> Option<AbilityData> {
    let map = object_map(value)?;
    Some(AbilityData {
        m_abilLink: i64_field(Some(map), "m_abilLink")?,
    })
}

fn parse_cmd_event_data(value: &Value) -> Option<CmdEventData> {
    let map = object_map(value)?;
    Some(CmdEventData {
        TargetUnit: map.get("TargetUnit").and_then(parse_target_unit_data),
    })
}

fn parse_target_unit_data(value: &Value) -> Option<TargetUnitData> {
    let map = object_map(value)?;
    Some(TargetUnitData {
        m_snapshotPoint: map.get("m_snapshotPoint").and_then(parse_snapshot_point),
    })
}

fn parse_snapshot_point(value: &Value) -> Option<SnapshotPoint> {
    let map = object_map(value)?;
    let mut values = Vec::new();
    for entry in map.values() {
        if let Some(value) = value_as_i64(entry) {
            values.push(SnapshotPointValue::Int(value));
        } else if let Some(value) = value_as_f64(entry) {
            values.push(SnapshotPointValue::Float(value));
        }
    }
    if values.is_empty() {
        None
    } else {
        Some(SnapshotPoint { values })
    }
}

fn parse_player_stats(value: &Value) -> Option<PlayerStatsData> {
    let map = object_map(value)?;
    Some(PlayerStatsData {
        m_score_value_food_used: f64_field(Some(map), "m_scoreValueFoodUsed"),
        m_score_value_minerals_collection_rate: f64_field(
            Some(map),
            "m_scoreValueMineralsCollectionRate",
        ),
        m_score_value_vespene_collection_rate: f64_field(
            Some(map),
            "m_scoreValueVespeneCollectionRate",
        ),
    })
}

fn object_map(value: &Value) -> Option<&BTreeMap<String, Value>> {
    match value {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn nested_i64_field(map: Option<&BTreeMap<String, Value>>, path: &[&str]) -> Option<i64> {
    let mut current = map?.get(path.first().copied()?)?;
    for key in path.iter().skip(1) {
        current = current.get_key(key)?;
    }
    value_as_i64(current)
}

fn i64_field(map: Option<&BTreeMap<String, Value>>, key: &str) -> Option<i64> {
    map.and_then(|fields| fields.get(key))
        .and_then(value_as_i64)
}

fn f64_field(map: Option<&BTreeMap<String, Value>>, key: &str) -> Option<f64> {
    map.and_then(|fields| fields.get(key))
        .and_then(value_as_f64)
}

fn string_field(map: Option<&BTreeMap<String, Value>>, key: &str) -> Option<String> {
    map.and_then(|fields| fields.get(key))
        .and_then(value_as_string)
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i128()
        .and_then(|value| i64::try_from(value).ok())
        .or_else(|| match value {
            Value::Float(number) => Some(*number as i64),
            Value::String(text) => text.parse::<i64>().ok(),
            _ => None,
        })
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value_as_i64(value).map(|value| value as f64))
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::Int(number) => Some(number.to_string()),
        Value::Float(number) => Some(number.to_string()),
        _ => None,
    }
}

fn value_contains(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Bytes(bytes) => String::from_utf8_lossy(bytes).contains(needle),
        Value::Array(values) => values.iter().any(|value| value_contains(value, needle)),
        Value::Object(map) => {
            map.contains_key(needle) || map.values().any(|value| value_contains(value, needle))
        }
        _ => false,
    }
}
