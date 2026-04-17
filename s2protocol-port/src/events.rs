use crate::decoder::{TypeDecoder, TypeInfo};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameEventField {
    ControlId,
    EventType,
    EventData,
    Abil,
    Data,
    Target,
}

impl GameEventField {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "m_controlId" => Some(Self::ControlId),
            "m_eventType" => Some(Self::EventType),
            "m_eventData" => Some(Self::EventData),
            "m_abil" => Some(Self::Abil),
            "m_data" => Some(Self::Data),
            "m_target" => Some(Self::Target),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageEventField {
    String,
}

impl MessageEventField {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "m_string" => Some(Self::String),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackerEventField {
    PlayerId,
    UpgradeTypeName,
    Count,
    Stats,
    UnitTypeName,
    CreatorAbilityName,
    ControlPlayerId,
    UnitTagIndex,
    UnitTagRecycle,
    CreatorUnitTagIndex,
    CreatorUnitTagRecycle,
    KillerUnitTagIndex,
    KillerUnitTagRecycle,
    KillerPlayerId,
    X,
    Y,
}

impl TrackerEventField {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "m_playerId" => Some(Self::PlayerId),
            "m_upgradeTypeName" => Some(Self::UpgradeTypeName),
            "m_count" => Some(Self::Count),
            "m_stats" => Some(Self::Stats),
            "m_unitTypeName" => Some(Self::UnitTypeName),
            "m_creatorAbilityName" => Some(Self::CreatorAbilityName),
            "m_controlPlayerId" => Some(Self::ControlPlayerId),
            "m_unitTagIndex" => Some(Self::UnitTagIndex),
            "m_unitTagRecycle" => Some(Self::UnitTagRecycle),
            "m_creatorUnitTagIndex" => Some(Self::CreatorUnitTagIndex),
            "m_creatorUnitTagRecycle" => Some(Self::CreatorUnitTagRecycle),
            "m_killerUnitTagIndex" => Some(Self::KillerUnitTagIndex),
            "m_killerUnitTagRecycle" => Some(Self::KillerUnitTagRecycle),
            "m_killerPlayerId" => Some(Self::KillerPlayerId),
            "m_x" => Some(Self::X),
            "m_y" => Some(Self::Y),
            _ => None,
        }
    }
}

pub(crate) trait DirectEventDecode: Sized {
    type Field: Copy;

    fn new_decoded(event: &str, event_id: u32, game_loop: i128, user_id: Option<i64>) -> Self;
    fn set_decoded_bits(&mut self, bits: i128);
    fn field_from_key(key: &str) -> Option<Self::Field>;
    fn decode_fields_from_typeinfo<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        decoder.visit_struct_fields_from_typeinfo(
            typeinfo,
            &mut |key| Self::field_from_key(key),
            &mut |decoder, field, field_typeinfo| self.decode_field(decoder, field, field_typeinfo),
        )
    }
    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError>;
    fn apply_fallback_field(&mut self, field: Self::Field, value: Value);
    fn apply_fallback_value(&mut self, key: &str, value: Value) {
        if let Some(field) = Self::field_from_key(key) {
            self.apply_fallback_field(field, value);
        }
    }
}

trait GameEventFieldSource {
    fn read_i64(self) -> Result<Option<i64>, DecodeError>;
    fn read_trigger_event_data(self) -> Result<TriggerEventData, DecodeError>;
    fn read_ability_data(self) -> Result<Option<AbilityData>, DecodeError>;
    fn read_cmd_event_data(self) -> Result<Option<CmdEventData>, DecodeError>;
    fn read_target_unit_data(self) -> Result<Option<TargetUnitData>, DecodeError>;
}

struct DecodedGameEventFieldSource<'a, D: TypeDecoder> {
    decoder: &'a mut D,
    field_typeinfo: &'a TypeInfo,
}

impl<D: TypeDecoder> GameEventFieldSource for DecodedGameEventFieldSource<'_, D> {
    fn read_i64(self) -> Result<Option<i64>, DecodeError> {
        self.decoder.i64_from_typeinfo(self.field_typeinfo)
    }

    fn read_trigger_event_data(self) -> Result<TriggerEventData, DecodeError> {
        let value = self.decoder.instance_from_typeinfo(self.field_typeinfo)?;
        Ok(parse_trigger_event_data(&value))
    }

    fn read_ability_data(self) -> Result<Option<AbilityData>, DecodeError> {
        decode_ability_data(self.decoder, self.field_typeinfo)
    }

    fn read_cmd_event_data(self) -> Result<Option<CmdEventData>, DecodeError> {
        let value = self.decoder.instance_from_typeinfo(self.field_typeinfo)?;
        Ok(parse_cmd_event_data(&value))
    }

    fn read_target_unit_data(self) -> Result<Option<TargetUnitData>, DecodeError> {
        let value = self.decoder.instance_from_typeinfo(self.field_typeinfo)?;
        Ok(parse_target_unit_data(&value))
    }
}

struct FallbackGameEventFieldSource<'a> {
    value: &'a Value,
}

impl GameEventFieldSource for FallbackGameEventFieldSource<'_> {
    fn read_i64(self) -> Result<Option<i64>, DecodeError> {
        Ok(value_as_i64(self.value))
    }

    fn read_trigger_event_data(self) -> Result<TriggerEventData, DecodeError> {
        Ok(parse_trigger_event_data(self.value))
    }

    fn read_ability_data(self) -> Result<Option<AbilityData>, DecodeError> {
        Ok(parse_ability_data(self.value))
    }

    fn read_cmd_event_data(self) -> Result<Option<CmdEventData>, DecodeError> {
        Ok(parse_cmd_event_data(self.value))
    }

    fn read_target_unit_data(self) -> Result<Option<TargetUnitData>, DecodeError> {
        Ok(parse_target_unit_data(self.value))
    }
}

trait MessageEventFieldSource {
    fn read_string(self) -> Result<Option<String>, DecodeError>;
}

struct DecodedMessageEventFieldSource<'a, D: TypeDecoder> {
    decoder: &'a mut D,
    field_typeinfo: &'a TypeInfo,
}

impl<D: TypeDecoder> MessageEventFieldSource for DecodedMessageEventFieldSource<'_, D> {
    fn read_string(self) -> Result<Option<String>, DecodeError> {
        self.decoder.string_from_typeinfo(self.field_typeinfo)
    }
}

struct FallbackMessageEventFieldSource<'a> {
    value: &'a Value,
}

impl MessageEventFieldSource for FallbackMessageEventFieldSource<'_> {
    fn read_string(self) -> Result<Option<String>, DecodeError> {
        Ok(value_as_string(self.value))
    }
}

trait TrackerEventFieldSource {
    fn read_i64(self) -> Result<Option<i64>, DecodeError>;
    fn read_string(self) -> Result<Option<String>, DecodeError>;
    fn read_player_stats(self) -> Result<Option<PlayerStatsData>, DecodeError>;
}

struct DecodedTrackerEventFieldSource<'a, D: TypeDecoder> {
    decoder: &'a mut D,
    field_typeinfo: &'a TypeInfo,
}

impl<D: TypeDecoder> TrackerEventFieldSource for DecodedTrackerEventFieldSource<'_, D> {
    fn read_i64(self) -> Result<Option<i64>, DecodeError> {
        self.decoder.i64_from_typeinfo(self.field_typeinfo)
    }

    fn read_string(self) -> Result<Option<String>, DecodeError> {
        self.decoder.string_from_typeinfo(self.field_typeinfo)
    }

    fn read_player_stats(self) -> Result<Option<PlayerStatsData>, DecodeError> {
        decode_player_stats(self.decoder, self.field_typeinfo)
    }
}

struct FallbackTrackerEventFieldSource<'a> {
    value: &'a Value,
}

impl TrackerEventFieldSource for FallbackTrackerEventFieldSource<'_> {
    fn read_i64(self) -> Result<Option<i64>, DecodeError> {
        Ok(value_as_i64(self.value))
    }

    fn read_string(self) -> Result<Option<String>, DecodeError> {
        Ok(value_as_string(self.value))
    }

    fn read_player_stats(self) -> Result<Option<PlayerStatsData>, DecodeError> {
        Ok(parse_player_stats(self.value))
    }
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
    type Field = GameEventField;

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

    fn field_from_key(key: &str) -> Option<Self::Field> {
        GameEventField::from_key(key)
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        self.apply_field(
            field,
            DecodedGameEventFieldSource {
                decoder,
                field_typeinfo,
            },
        )
    }

    fn apply_fallback_field(&mut self, field: Self::Field, value: Value) {
        if let Err(error) = self.apply_field(field, FallbackGameEventFieldSource { value: &value })
        {
            unreachable!("fallback field handling cannot fail: {error}");
        }
    }
}

impl GameEvent {
    fn apply_field<S: GameEventFieldSource>(
        &mut self,
        field: GameEventField,
        source: S,
    ) -> Result<(), DecodeError> {
        match field {
            GameEventField::ControlId => {
                self.m_control_id = source.read_i64()?;
            }
            GameEventField::EventType => {
                self.m_event_type = source.read_i64()?;
            }
            GameEventField::EventData => {
                self.m_event_data = Some(source.read_trigger_event_data()?);
            }
            GameEventField::Abil => {
                self.m_abil = source.read_ability_data()?;
            }
            GameEventField::Data => {
                self.m_data = source.read_cmd_event_data()?;
            }
            GameEventField::Target => {
                self.m_target = source.read_target_unit_data()?;
            }
        }
        Ok(())
    }
}

impl DirectEventDecode for MessageEvent {
    type Field = MessageEventField;

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

    fn field_from_key(key: &str) -> Option<Self::Field> {
        MessageEventField::from_key(key)
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        self.apply_field(
            field,
            DecodedMessageEventFieldSource {
                decoder,
                field_typeinfo,
            },
        )
    }

    fn apply_fallback_field(&mut self, field: Self::Field, value: Value) {
        if let Err(error) =
            self.apply_field(field, FallbackMessageEventFieldSource { value: &value })
        {
            unreachable!("fallback field handling cannot fail: {error}");
        }
    }
}

impl MessageEvent {
    fn apply_field<S: MessageEventFieldSource>(
        &mut self,
        field: MessageEventField,
        source: S,
    ) -> Result<(), DecodeError> {
        match field {
            MessageEventField::String => {
                self.m_string = source.read_string()?;
            }
        }
        Ok(())
    }
}

impl DirectEventDecode for TrackerEvent {
    type Field = TrackerEventField;

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

    fn field_from_key(key: &str) -> Option<Self::Field> {
        TrackerEventField::from_key(key)
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        self.apply_field(
            field,
            DecodedTrackerEventFieldSource {
                decoder,
                field_typeinfo,
            },
        )
    }

    fn apply_fallback_field(&mut self, field: Self::Field, value: Value) {
        if let Err(error) =
            self.apply_field(field, FallbackTrackerEventFieldSource { value: &value })
        {
            unreachable!("fallback field handling cannot fail: {error}");
        }
    }
}

impl TrackerEvent {
    fn apply_field<S: TrackerEventFieldSource>(
        &mut self,
        field: TrackerEventField,
        source: S,
    ) -> Result<(), DecodeError> {
        match field {
            TrackerEventField::PlayerId => {
                self.m_player_id = source.read_i64()?;
            }
            TrackerEventField::UpgradeTypeName => {
                self.m_upgrade_type_name = source.read_string()?;
            }
            TrackerEventField::Count => {
                self.m_count = source.read_i64()?;
            }
            TrackerEventField::Stats => {
                self.m_stats = source.read_player_stats()?;
            }
            TrackerEventField::UnitTypeName => {
                self.m_unit_type_name = source.read_string()?;
            }
            TrackerEventField::CreatorAbilityName => {
                self.m_creator_ability_name = source.read_string()?;
            }
            TrackerEventField::ControlPlayerId => {
                self.m_control_player_id = source.read_i64()?;
            }
            TrackerEventField::UnitTagIndex => {
                self.m_unit_tag_index = source.read_i64()?;
            }
            TrackerEventField::UnitTagRecycle => {
                self.m_unit_tag_recycle = source.read_i64()?;
            }
            TrackerEventField::CreatorUnitTagIndex => {
                self.m_creator_unit_tag_index = source.read_i64()?;
            }
            TrackerEventField::CreatorUnitTagRecycle => {
                self.m_creator_unit_tag_recycle = source.read_i64()?;
            }
            TrackerEventField::KillerUnitTagIndex => {
                self.m_killer_unit_tag_index = source.read_i64()?;
            }
            TrackerEventField::KillerUnitTagRecycle => {
                self.m_killer_unit_tag_recycle = source.read_i64()?;
            }
            TrackerEventField::KillerPlayerId => {
                self.m_killer_player_id = source.read_i64()?;
            }
            TrackerEventField::X => {
                self.m_x = source.read_i64()?;
            }
            TrackerEventField::Y => {
                self.m_y = source.read_i64()?;
            }
        }
        Ok(())
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
    typeinfo: &TypeInfo,
) -> Result<Option<i64>, DecodeError> {
    let mut user_id = None;
    match decoder.visit_struct_fields_from_typeinfo(
        typeinfo,
        &mut |key| if key == "m_userId" { Some(()) } else { None },
        &mut |decoder, (), field_typeinfo| {
            user_id = decoder.i64_from_typeinfo(field_typeinfo)?;
            Ok(())
        },
    ) {
        Ok(()) => Ok(user_id),
        Err(DecodeError::UnexpectedType(_)) => decoder.i64_from_typeinfo(typeinfo),
        Err(error) => Err(error),
    }
}

fn decode_ability_data<D: TypeDecoder>(
    decoder: &mut D,
    typeinfo: &TypeInfo,
) -> Result<Option<AbilityData>, DecodeError> {
    let mut ability = AbilityData::default();
    let mut found = false;
    match decoder.visit_struct_fields_from_typeinfo(
        typeinfo,
        &mut |key| if key == "m_abilLink" { Some(()) } else { None },
        &mut |decoder, (), field_typeinfo| {
            ability.m_abilLink = decoder
                .i64_from_typeinfo(field_typeinfo)?
                .unwrap_or_default();
            found = true;
            Ok(())
        },
    ) {
        Ok(()) => Ok(found.then_some(ability)),
        Err(DecodeError::UnexpectedType(_)) => {
            let value = decoder.instance_from_typeinfo(typeinfo)?;
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
    typeinfo: &TypeInfo,
) -> Result<Option<PlayerStatsData>, DecodeError> {
    let mut stats = PlayerStatsData::default();
    let mut found = false;
    match decoder.visit_struct_fields_from_typeinfo(
        typeinfo,
        &mut |key| match key {
            "m_scoreValueFoodUsed" => Some(0u8),
            "m_scoreValueMineralsCollectionRate" => Some(1u8),
            "m_scoreValueVespeneCollectionRate" => Some(2u8),
            _ => None,
        },
        &mut |decoder, field, field_typeinfo| {
            match field {
                0 => {
                    stats.m_score_value_food_used = decoder.f64_from_typeinfo(field_typeinfo)?;
                }
                1 => {
                    stats.m_score_value_minerals_collection_rate =
                        decoder.f64_from_typeinfo(field_typeinfo)?;
                }
                2 => {
                    stats.m_score_value_vespene_collection_rate =
                        decoder.f64_from_typeinfo(field_typeinfo)?;
                }
                _ => unreachable!("invalid selected player stats field"),
            }
            found = true;
            Ok(())
        },
    ) {
        Ok(()) => Ok(found.then_some(stats)),
        Err(DecodeError::UnexpectedType(_)) => {
            let value = decoder.instance_from_typeinfo(typeinfo)?;
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
