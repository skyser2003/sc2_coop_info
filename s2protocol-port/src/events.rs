use crate::decoder::TypeDecoder;
use crate::{DecodeError, Value};
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

pub(crate) trait DirectEventDecode: Sized {
    fn new_decoded(event: &str, event_id: u32, game_loop: i128, user_id: Option<i64>) -> Self;
    fn set_decoded_bits(&mut self, bits: i128);
    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        key: &str,
        typeid: usize,
    ) -> Result<(), DecodeError>;
    fn apply_fallback_value(&mut self, key: &str, value: Value);
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

impl DirectEventDecode for GameEvent {
    fn new_decoded(event: &str, event_id: u32, game_loop: i128, user_id: Option<i64>) -> Self {
        Self {
            event: event.to_owned(),
            event_id,
            game_loop: i64::try_from(game_loop).unwrap_or_default(),
            user_id,
            bits: 0,
            m_control_id: None,
            m_event_type: None,
            m_event_data: None,
            m_abil: None,
            m_data: None,
            m_target: None,
        }
    }

    fn set_decoded_bits(&mut self, bits: i128) {
        self.bits = i64::try_from(bits).unwrap_or_default();
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        key: &str,
        typeid: usize,
    ) -> Result<(), DecodeError> {
        match key {
            "m_controlId" => {
                self.m_control_id = decoder.i64_from_typeid(typeid)?;
            }
            "m_eventType" => {
                self.m_event_type = decoder.i64_from_typeid(typeid)?;
            }
            "m_eventData" => {
                let value = decoder.instance(typeid)?;
                self.m_event_data = Some(parse_trigger_event_data(&value));
            }
            "m_abil" => {
                self.m_abil = decode_ability_data(decoder, typeid)?;
            }
            "m_data" => {
                let value = decoder.instance(typeid)?;
                self.m_data = parse_cmd_event_data(&value);
            }
            "m_target" => {
                let value = decoder.instance(typeid)?;
                self.m_target = parse_target_unit_data(&value);
            }
            _ => {
                decoder.skip_from_typeid(typeid)?;
            }
        }
        Ok(())
    }

    fn apply_fallback_value(&mut self, key: &str, value: Value) {
        match key {
            "m_controlId" => {
                self.m_control_id = value_as_i64(&value);
            }
            "m_eventType" => {
                self.m_event_type = value_as_i64(&value);
            }
            "m_eventData" => {
                self.m_event_data = Some(parse_trigger_event_data(&value));
            }
            "m_abil" => {
                self.m_abil = parse_ability_data(&value);
            }
            "m_data" => {
                self.m_data = parse_cmd_event_data(&value);
            }
            "m_target" => {
                self.m_target = parse_target_unit_data(&value);
            }
            _ => {}
        }
    }
}

impl DirectEventDecode for MessageEvent {
    fn new_decoded(event: &str, event_id: u32, game_loop: i128, user_id: Option<i64>) -> Self {
        Self {
            event: event.to_owned(),
            event_id,
            game_loop: i64::try_from(game_loop).unwrap_or_default(),
            user_id,
            bits: 0,
            m_string: None,
        }
    }

    fn set_decoded_bits(&mut self, bits: i128) {
        self.bits = i64::try_from(bits).unwrap_or_default();
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        key: &str,
        typeid: usize,
    ) -> Result<(), DecodeError> {
        if key == "m_string" {
            self.m_string = decoder.string_from_typeid(typeid)?;
        } else {
            decoder.skip_from_typeid(typeid)?;
        }
        Ok(())
    }

    fn apply_fallback_value(&mut self, key: &str, value: Value) {
        if key == "m_string" {
            self.m_string = value_as_string(&value);
        }
    }
}

impl DirectEventDecode for TrackerEvent {
    fn new_decoded(event: &str, event_id: u32, game_loop: i128, _user_id: Option<i64>) -> Self {
        Self {
            event: event.to_owned(),
            event_id,
            game_loop: i64::try_from(game_loop).unwrap_or_default(),
            bits: 0,
            m_player_id: None,
            m_upgrade_type_name: None,
            m_count: None,
            m_stats: None,
            m_unit_type_name: None,
            m_creator_ability_name: None,
            m_control_player_id: None,
            m_unit_tag_index: None,
            m_unit_tag_recycle: None,
            m_creator_unit_tag_index: None,
            m_creator_unit_tag_recycle: None,
            m_killer_unit_tag_index: None,
            m_killer_unit_tag_recycle: None,
            m_killer_player_id: None,
            m_x: None,
            m_y: None,
        }
    }

    fn set_decoded_bits(&mut self, bits: i128) {
        self.bits = i64::try_from(bits).unwrap_or_default();
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        key: &str,
        typeid: usize,
    ) -> Result<(), DecodeError> {
        match key {
            "m_playerId" => {
                self.m_player_id = decoder.i64_from_typeid(typeid)?;
            }
            "m_upgradeTypeName" => {
                self.m_upgrade_type_name = decoder.string_from_typeid(typeid)?;
            }
            "m_count" => {
                self.m_count = decoder.i64_from_typeid(typeid)?;
            }
            "m_stats" => {
                self.m_stats = decode_player_stats(decoder, typeid)?;
            }
            "m_unitTypeName" => {
                self.m_unit_type_name = decoder.string_from_typeid(typeid)?;
            }
            "m_creatorAbilityName" => {
                self.m_creator_ability_name = decoder.string_from_typeid(typeid)?;
            }
            "m_controlPlayerId" => {
                self.m_control_player_id = decoder.i64_from_typeid(typeid)?;
            }
            "m_unitTagIndex" => {
                self.m_unit_tag_index = decoder.i64_from_typeid(typeid)?;
            }
            "m_unitTagRecycle" => {
                self.m_unit_tag_recycle = decoder.i64_from_typeid(typeid)?;
            }
            "m_creatorUnitTagIndex" => {
                self.m_creator_unit_tag_index = decoder.i64_from_typeid(typeid)?;
            }
            "m_creatorUnitTagRecycle" => {
                self.m_creator_unit_tag_recycle = decoder.i64_from_typeid(typeid)?;
            }
            "m_killerUnitTagIndex" => {
                self.m_killer_unit_tag_index = decoder.i64_from_typeid(typeid)?;
            }
            "m_killerUnitTagRecycle" => {
                self.m_killer_unit_tag_recycle = decoder.i64_from_typeid(typeid)?;
            }
            "m_killerPlayerId" => {
                self.m_killer_player_id = decoder.i64_from_typeid(typeid)?;
            }
            "m_x" => {
                self.m_x = decoder.i64_from_typeid(typeid)?;
            }
            "m_y" => {
                self.m_y = decoder.i64_from_typeid(typeid)?;
            }
            _ => {
                decoder.skip_from_typeid(typeid)?;
            }
        }
        Ok(())
    }

    fn apply_fallback_value(&mut self, key: &str, value: Value) {
        match key {
            "m_playerId" => {
                self.m_player_id = value_as_i64(&value);
            }
            "m_upgradeTypeName" => {
                self.m_upgrade_type_name = value_as_string(&value);
            }
            "m_count" => {
                self.m_count = value_as_i64(&value);
            }
            "m_stats" => {
                self.m_stats = parse_player_stats(&value);
            }
            "m_unitTypeName" => {
                self.m_unit_type_name = value_as_string(&value);
            }
            "m_creatorAbilityName" => {
                self.m_creator_ability_name = value_as_string(&value);
            }
            "m_controlPlayerId" => {
                self.m_control_player_id = value_as_i64(&value);
            }
            "m_unitTagIndex" => {
                self.m_unit_tag_index = value_as_i64(&value);
            }
            "m_unitTagRecycle" => {
                self.m_unit_tag_recycle = value_as_i64(&value);
            }
            "m_creatorUnitTagIndex" => {
                self.m_creator_unit_tag_index = value_as_i64(&value);
            }
            "m_creatorUnitTagRecycle" => {
                self.m_creator_unit_tag_recycle = value_as_i64(&value);
            }
            "m_killerUnitTagIndex" => {
                self.m_killer_unit_tag_index = value_as_i64(&value);
            }
            "m_killerUnitTagRecycle" => {
                self.m_killer_unit_tag_recycle = value_as_i64(&value);
            }
            "m_killerPlayerId" => {
                self.m_killer_player_id = value_as_i64(&value);
            }
            "m_x" => {
                self.m_x = value_as_i64(&value);
            }
            "m_y" => {
                self.m_y = value_as_i64(&value);
            }
            _ => {}
        }
    }
}

fn parse_trigger_event_data(value: &Value) -> TriggerEventData {
    TriggerEventData {
        contains_selection_changed: value_contains(value, "SelectionChanged"),
        contains_none: value_contains(value, "None"),
    }
}

pub(crate) fn decode_user_id<D: TypeDecoder>(
    decoder: &mut D,
    typeid: usize,
) -> Result<Option<i64>, DecodeError> {
    let mut user_id = None;
    match decoder.visit_struct_fields_from_typeid(typeid, &mut |decoder, key, field_typeid| {
        if key == "m_userId" {
            user_id = decoder.i64_from_typeid(field_typeid)?;
        } else {
            decoder.skip_from_typeid(field_typeid)?;
        }
        Ok(())
    }) {
        Ok(()) => Ok(user_id),
        Err(DecodeError::UnexpectedType(_)) => decoder.i64_from_typeid(typeid),
        Err(error) => Err(error),
    }
}

fn decode_ability_data<D: TypeDecoder>(
    decoder: &mut D,
    typeid: usize,
) -> Result<Option<AbilityData>, DecodeError> {
    let mut ability = AbilityData::default();
    let mut found = false;
    match decoder.visit_struct_fields_from_typeid(typeid, &mut |decoder, key, field_typeid| {
        if key == "m_abilLink" {
            ability.m_abilLink = decoder.i64_from_typeid(field_typeid)?.unwrap_or_default();
            found = true;
        } else {
            decoder.skip_from_typeid(field_typeid)?;
        }
        Ok(())
    }) {
        Ok(()) => Ok(found.then_some(ability)),
        Err(DecodeError::UnexpectedType(_)) => {
            let value = decoder.instance(typeid)?;
            Ok(parse_ability_data(&value))
        }
        Err(error) => Err(error),
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

fn decode_player_stats<D: TypeDecoder>(
    decoder: &mut D,
    typeid: usize,
) -> Result<Option<PlayerStatsData>, DecodeError> {
    let mut stats = PlayerStatsData::default();
    let mut found = false;
    match decoder.visit_struct_fields_from_typeid(typeid, &mut |decoder, key, field_typeid| {
        match key {
            "m_scoreValueFoodUsed" => {
                stats.m_score_value_food_used = decoder.f64_from_typeid(field_typeid)?;
                found = true;
            }
            "m_scoreValueMineralsCollectionRate" => {
                stats.m_score_value_minerals_collection_rate =
                    decoder.f64_from_typeid(field_typeid)?;
                found = true;
            }
            "m_scoreValueVespeneCollectionRate" => {
                stats.m_score_value_vespene_collection_rate =
                    decoder.f64_from_typeid(field_typeid)?;
                found = true;
            }
            _ => {
                decoder.skip_from_typeid(field_typeid)?;
            }
        }
        Ok(())
    }) {
        Ok(()) => Ok(found.then_some(stats)),
        Err(DecodeError::UnexpectedType(_)) => {
            let value = decoder.instance(typeid)?;
            Ok(parse_player_stats(&value))
        }
        Err(error) => Err(error),
    }
}

fn object_map(value: &Value) -> Option<&BTreeMap<String, Value>> {
    match value {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn i64_field(map: Option<&BTreeMap<String, Value>>, key: &str) -> Option<i64> {
    map.and_then(|fields| fields.get(key))
        .and_then(value_as_i64)
}

fn f64_field(map: Option<&BTreeMap<String, Value>>, key: &str) -> Option<f64> {
    map.and_then(|fields| fields.get(key))
        .and_then(value_as_f64)
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
