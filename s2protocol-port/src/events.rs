use crate::decoder::{EventDecodePlan, EventSpecialDataDecoder, TypeDecoder, TypeInfo};
use crate::DecodeError;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerEventData {
    pub contains_selection_changed: bool,
    pub contains_none: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AbilityData {
    pub m_abilLink: i64,
    pub m_abilCmdIndex: Option<i64>,
    pub m_abilCmdData: Option<i64>,
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
    pub TargetPoint: Option<SnapshotPoint>,
    pub TargetUnit: Option<TargetUnitData>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SelectionRemoveMask {
    #[default]
    None,
    Mask(Vec<bool>),
    OneIndices(Vec<i64>),
    ZeroIndices(Vec<i64>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionDeltaData {
    pub m_subgroup_index: Option<i64>,
    pub m_remove_mask: SelectionRemoveMask,
    pub m_add_unit_tags: Vec<i64>,
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
    pub m_cmd_flags: Option<i64>,
    pub m_abil: Option<AbilityData>,
    pub m_data: Option<CmdEventData>,
    pub m_sequence: Option<i64>,
    pub m_other_unit: Option<i64>,
    pub m_unit_group: Option<i64>,
    pub m_target: Option<TargetUnitData>,
    pub m_delta: Option<SelectionDeltaData>,
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
    pub m_first_unit_index: Option<i64>,
    pub m_position_items: Vec<i64>,
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
    CmdFlags,
    Sequence,
    OtherUnit,
    UnitGroup,
    Target,
    Delta,
}

impl GameEventField {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "m_controlId" => Some(Self::ControlId),
            "m_eventType" => Some(Self::EventType),
            "m_eventData" => Some(Self::EventData),
            "m_cmdFlags" => Some(Self::CmdFlags),
            "m_abil" => Some(Self::Abil),
            "m_data" => Some(Self::Data),
            "m_sequence" => Some(Self::Sequence),
            "m_otherUnit" => Some(Self::OtherUnit),
            "m_unitGroup" => Some(Self::UnitGroup),
            "m_target" => Some(Self::Target),
            "m_delta" => Some(Self::Delta),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageEventField {
    String,
}

impl MessageEventField {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
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
    FirstUnitIndex,
    PositionItems,
}

impl TrackerEventField {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
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
            "m_firstUnitIndex" => Some(Self::FirstUnitIndex),
            "m_items" => Some(Self::PositionItems),
            _ => None,
        }
    }
}

pub(crate) trait DirectEventDecode: Sized {
    type Field: Copy;

    fn new_decoded(event: &str, event_id: u32, game_loop: i128, user_id: Option<i64>) -> Self;
    fn set_decoded_bits(&mut self, bits: i128);
    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError>;

    fn decode_fields_from_plan<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        plan: &EventDecodePlan<Self::Field>,
    ) -> Result<(), DecodeError> {
        decoder.decode_event_fields_from_plan(plan, &mut |decoder, field, field_typeinfo| {
            self.decode_field(decoder, field, field_typeinfo)
        })
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
            m_cmd_flags: None,
            m_abil: None,
            m_data: None,
            m_sequence: None,
            m_other_unit: None,
            m_unit_group: None,
            m_target: None,
            m_delta: None,
        }
    }

    fn set_decoded_bits(&mut self, bits: i128) {
        self.bits = i64::try_from(bits).unwrap_or_default();
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        match field {
            GameEventField::ControlId => {
                self.m_control_id = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            GameEventField::EventType => {
                self.m_event_type = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            GameEventField::EventData => {
                self.m_event_data = Some(
                    EventSpecialDataDecoder::decode_trigger_event_data_from_typeinfo(
                        decoder,
                        field_typeinfo,
                    )?,
                );
            }
            GameEventField::CmdFlags => {
                self.m_cmd_flags = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            GameEventField::Abil => {
                self.m_abil = EventSpecialDataDecoder::decode_ability_data_from_typeinfo(
                    decoder,
                    field_typeinfo,
                )?;
            }
            GameEventField::Data => {
                self.m_data = EventSpecialDataDecoder::decode_cmd_event_data_from_typeinfo(
                    decoder,
                    field_typeinfo,
                )?;
            }
            GameEventField::Sequence => {
                self.m_sequence = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            GameEventField::OtherUnit => {
                self.m_other_unit = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            GameEventField::UnitGroup => {
                self.m_unit_group = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            GameEventField::Target => {
                self.m_target = EventSpecialDataDecoder::decode_target_unit_data_from_typeinfo(
                    decoder,
                    field_typeinfo,
                )?;
            }
            GameEventField::Delta => {
                self.m_delta = EventSpecialDataDecoder::decode_selection_delta_data_from_typeinfo(
                    decoder,
                    field_typeinfo,
                )?;
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

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        match field {
            MessageEventField::String => {
                self.m_string = decoder.string_from_typeinfo(field_typeinfo)?;
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
            m_first_unit_index: None,
            m_position_items: Vec::new(),
        }
    }

    fn set_decoded_bits(&mut self, bits: i128) {
        self.bits = i64::try_from(bits).unwrap_or_default();
    }

    fn decode_field<D: TypeDecoder>(
        &mut self,
        decoder: &mut D,
        field: Self::Field,
        field_typeinfo: &TypeInfo,
    ) -> Result<(), DecodeError> {
        match field {
            TrackerEventField::PlayerId => {
                self.m_player_id = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::UpgradeTypeName => {
                self.m_upgrade_type_name = decoder.string_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::Count => {
                self.m_count = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::Stats => {
                self.m_stats = EventSpecialDataDecoder::decode_player_stats_from_typeinfo(
                    decoder,
                    field_typeinfo,
                )?;
            }
            TrackerEventField::UnitTypeName => {
                self.m_unit_type_name = decoder.string_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::CreatorAbilityName => {
                self.m_creator_ability_name = decoder.string_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::ControlPlayerId => {
                self.m_control_player_id = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::UnitTagIndex => {
                self.m_unit_tag_index = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::UnitTagRecycle => {
                self.m_unit_tag_recycle = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::CreatorUnitTagIndex => {
                self.m_creator_unit_tag_index = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::CreatorUnitTagRecycle => {
                self.m_creator_unit_tag_recycle = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::KillerUnitTagIndex => {
                self.m_killer_unit_tag_index = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::KillerUnitTagRecycle => {
                self.m_killer_unit_tag_recycle = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::KillerPlayerId => {
                self.m_killer_player_id = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::X => {
                self.m_x = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::Y => {
                self.m_y = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::FirstUnitIndex => {
                self.m_first_unit_index = decoder.i64_from_typeinfo(field_typeinfo)?;
            }
            TrackerEventField::PositionItems => {
                decoder.visit_array_elements_from_typeinfo(
                    field_typeinfo,
                    &mut |decoder, child_typeinfo| {
                        if let Some(item) = decoder.i64_from_typeinfo(child_typeinfo)? {
                            self.m_position_items.push(item);
                        }
                        Ok(())
                    },
                )?;
            }
        }
        Ok(())
    }
}

pub(crate) struct EventUserIdDecoder;

impl EventUserIdDecoder {
    pub(crate) fn decode<D: TypeDecoder>(
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
}
