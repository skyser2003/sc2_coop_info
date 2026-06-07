use super::{TypeDecoder, TypeInfo, TypeOp};
use crate::{
    error::DecodeError,
    events::{
        AbilityData, CmdEventData, PlayerStatsData, SelectionDeltaData, SelectionRemoveMask,
        SnapshotPoint, SnapshotPointValue, TargetUnitData, TriggerEventData,
    },
};

pub(crate) struct EventSpecialDataDecoder;

impl EventSpecialDataDecoder {
    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    pub(super) fn mark_trigger_text_bytes(result: &mut TriggerEventData, bytes: &[u8]) {
        if !result.contains_selection_changed && Self::contains_subslice(bytes, b"SelectionChanged")
        {
            result.contains_selection_changed = true;
        }
        if !result.contains_none && Self::contains_subslice(bytes, b"None") {
            result.contains_none = true;
        }
    }

    fn scan_trigger_event_data<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        result: &mut TriggerEventData,
    ) -> Result<(), DecodeError> {
        #[derive(Clone, Copy)]
        enum TriggerFieldMarker {
            Other,
            SelectionChanged,
            None,
        }

        if result.contains_selection_changed && result.contains_none {
            decoder.skip_from_typeinfo(typeinfo)?;
            return Ok(());
        }

        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::scan_trigger_event_data(decoder, child_typeinfo, result)
                    },
                )?;
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |field_name| {
                    Some(match field_name {
                        "SelectionChanged" => TriggerFieldMarker::SelectionChanged,
                        "None" => TriggerFieldMarker::None,
                        _ => TriggerFieldMarker::Other,
                    })
                },
                &mut |decoder, marker, child_typeinfo| {
                    match marker {
                        TriggerFieldMarker::SelectionChanged => {
                            result.contains_selection_changed = true;
                        }
                        TriggerFieldMarker::None => {
                            result.contains_none = true;
                        }
                        TriggerFieldMarker::Other => {}
                    }
                    Self::scan_trigger_event_data(decoder, child_typeinfo, result)
                },
            ),
            TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
                typeinfo,
                &mut |field_name| {
                    Some(match field_name {
                        "SelectionChanged" => TriggerFieldMarker::SelectionChanged,
                        "None" => TriggerFieldMarker::None,
                        _ => TriggerFieldMarker::Other,
                    })
                },
                &mut |decoder, marker, child_typeinfo| {
                    match marker {
                        TriggerFieldMarker::SelectionChanged => {
                            result.contains_selection_changed = true;
                        }
                        TriggerFieldMarker::None => {
                            result.contains_none = true;
                        }
                        TriggerFieldMarker::Other => {}
                    }
                    Self::scan_trigger_event_data(decoder, child_typeinfo, result)
                },
            ),
            TypeOp::Array => decoder.visit_array_elements_from_typeinfo(
                typeinfo,
                &mut |decoder, child_typeinfo| {
                    Self::scan_trigger_event_data(decoder, child_typeinfo, result)
                },
            ),
            TypeOp::Blob | TypeOp::Fourcc => {
                decoder.mark_trigger_text_from_typeinfo(typeinfo, result)
            }
            _ => decoder.skip_from_typeinfo(typeinfo),
        }
    }

    pub(crate) fn decode_trigger_event_data_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<TriggerEventData, DecodeError> {
        let mut result = TriggerEventData::default();
        Self::scan_trigger_event_data(decoder, typeinfo, &mut result)?;
        Ok(result)
    }

    fn decode_ability_data_inner<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        ability: &mut AbilityData,
        found: &mut bool,
    ) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::decode_ability_data_inner(decoder, child_typeinfo, ability, found)
                    },
                )?;
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |field_name| match field_name {
                    "m_abilLink" => Some(0_u8),
                    "m_abilCmdIndex" => Some(1_u8),
                    "m_abilCmdData" => Some(2_u8),
                    _ => None,
                },
                &mut |decoder, field, field_typeinfo| {
                    match field {
                        0 => {
                            ability.m_abilLink = decoder
                                .i64_from_typeinfo(field_typeinfo)?
                                .unwrap_or_default();
                        }
                        1 => {
                            ability.m_abilCmdIndex = decoder.i64_from_typeinfo(field_typeinfo)?;
                        }
                        2 => {
                            ability.m_abilCmdData = decoder.i64_from_typeinfo(field_typeinfo)?;
                        }
                        _ => unreachable!("invalid selected ability data field"),
                    }
                    *found = true;
                    Ok(())
                },
            ),
            _ => {
                if let Some(value) = decoder.i64_from_typeinfo(typeinfo)? {
                    ability.m_abilLink = value;
                    *found = true;
                }
                Ok(())
            }
        }
    }

    pub(crate) fn decode_ability_data_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Option<AbilityData>, DecodeError> {
        let mut ability = AbilityData::default();
        let mut found = false;
        Self::decode_ability_data_inner(decoder, typeinfo, &mut ability, &mut found)?;
        Ok(found.then_some(ability))
    }

    fn collect_snapshot_point_values<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        values: &mut Vec<SnapshotPointValue>,
    ) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::collect_snapshot_point_values(decoder, child_typeinfo, values)
                    },
                )?;
                Ok(())
            }
            TypeOp::Int => {
                if let Some(value) = decoder.i64_from_typeinfo(typeinfo)? {
                    values.push(SnapshotPointValue::Int(value));
                }
                Ok(())
            }
            TypeOp::Real32 | TypeOp::Real64 => {
                if let Some(value) = decoder.f64_from_typeinfo(typeinfo)? {
                    values.push(SnapshotPointValue::Float(value));
                }
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |_| Some(()),
                &mut |decoder, (), field_typeinfo| {
                    Self::collect_snapshot_point_values(decoder, field_typeinfo, values)
                },
            ),
            TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
                typeinfo,
                &mut |_| Some(()),
                &mut |decoder, (), field_typeinfo| {
                    Self::collect_snapshot_point_values(decoder, field_typeinfo, values)
                },
            ),
            TypeOp::Array => decoder.visit_array_elements_from_typeinfo(
                typeinfo,
                &mut |decoder, child_typeinfo| {
                    Self::collect_snapshot_point_values(decoder, child_typeinfo, values)
                },
            ),
            _ => decoder.skip_from_typeinfo(typeinfo),
        }
    }

    pub(crate) fn decode_snapshot_point_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Option<SnapshotPoint>, DecodeError> {
        let mut values = Vec::new();
        Self::collect_snapshot_point_values(decoder, typeinfo, &mut values)?;
        if values.is_empty() {
            Ok(None)
        } else {
            Ok(Some(SnapshotPoint { values }))
        }
    }

    fn decode_target_unit_data_inner<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        data: &mut TargetUnitData,
        found: &mut bool,
    ) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::decode_target_unit_data_inner(decoder, child_typeinfo, data, found)
                    },
                )?;
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |field_name| (field_name == "m_snapshotPoint").then_some(()),
                &mut |decoder, (), field_typeinfo| {
                    data.m_snapshotPoint =
                        Self::decode_snapshot_point_from_typeinfo(decoder, field_typeinfo)?;
                    *found = true;
                    Ok(())
                },
            ),
            TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
                typeinfo,
                &mut |field_name| (field_name == "m_snapshotPoint").then_some(()),
                &mut |decoder, (), field_typeinfo| {
                    data.m_snapshotPoint =
                        Self::decode_snapshot_point_from_typeinfo(decoder, field_typeinfo)?;
                    *found = true;
                    Ok(())
                },
            ),
            _ => decoder.skip_from_typeinfo(typeinfo),
        }
    }

    pub(crate) fn decode_target_unit_data_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Option<TargetUnitData>, DecodeError> {
        let mut data = TargetUnitData::default();
        let mut found = false;
        Self::decode_target_unit_data_inner(decoder, typeinfo, &mut data, &mut found)?;
        Ok(found.then_some(data))
    }

    fn decode_cmd_event_data_inner<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        data: &mut CmdEventData,
        found: &mut bool,
    ) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::decode_cmd_event_data_inner(decoder, child_typeinfo, data, found)
                    },
                )?;
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |field_name| match field_name {
                    "TargetPoint" => Some(0u8),
                    "TargetUnit" => Some(1u8),
                    _ => None,
                },
                &mut |decoder, field, field_typeinfo| {
                    match field {
                        0 => {
                            data.TargetPoint =
                                Self::decode_snapshot_point_from_typeinfo(decoder, field_typeinfo)?;
                        }
                        _ => {
                            data.TargetUnit = Self::decode_target_unit_data_from_typeinfo(
                                decoder,
                                field_typeinfo,
                            )?;
                        }
                    }
                    *found = true;
                    Ok(())
                },
            ),
            TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
                typeinfo,
                &mut |field_name| match field_name {
                    "TargetPoint" => Some(0u8),
                    "TargetUnit" => Some(1u8),
                    _ => None,
                },
                &mut |decoder, field, field_typeinfo| {
                    match field {
                        0 => {
                            data.TargetPoint =
                                Self::decode_snapshot_point_from_typeinfo(decoder, field_typeinfo)?;
                        }
                        _ => {
                            data.TargetUnit = Self::decode_target_unit_data_from_typeinfo(
                                decoder,
                                field_typeinfo,
                            )?;
                        }
                    }
                    *found = true;
                    Ok(())
                },
            ),
            _ => decoder.skip_from_typeinfo(typeinfo),
        }
    }

    pub(crate) fn decode_cmd_event_data_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Option<CmdEventData>, DecodeError> {
        let mut data = CmdEventData::default();
        let mut found = false;
        Self::decode_cmd_event_data_inner(decoder, typeinfo, &mut data, &mut found)?;
        Ok(found.then_some(data))
    }

    fn collect_i64_values<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        values: &mut Vec<i64>,
    ) -> Result<(), DecodeError> {
        decoder.append_i64_values_from_typeinfo(typeinfo, values)
    }

    pub(crate) fn decode_i64_values_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Vec<i64>, DecodeError> {
        let mut values = Vec::new();
        Self::collect_i64_values(decoder, typeinfo, &mut values)?;
        Ok(values)
    }

    fn decode_bitarray_bools<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Vec<bool>, DecodeError> {
        decoder.bools_from_bitarray_typeinfo(typeinfo)
    }

    fn decode_selection_remove_mask<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<SelectionRemoveMask, DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(SelectionRemoveMask::None)
            }
            TypeOp::Optional => {
                let mut mask = SelectionRemoveMask::None;
                let exists = decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        mask = Self::decode_selection_remove_mask(decoder, child_typeinfo)?;
                        Ok(())
                    },
                )?;
                Ok(if exists {
                    mask
                } else {
                    SelectionRemoveMask::None
                })
            }
            TypeOp::Choice => {
                let mut mask = SelectionRemoveMask::None;
                decoder.visit_choice_field_from_typeinfo(
                    typeinfo,
                    &mut |field_name| match field_name {
                        "None" => Some(0u8),
                        "Mask" => Some(1u8),
                        "OneIndices" => Some(2u8),
                        "ZeroIndices" => Some(3u8),
                        _ => None,
                    },
                    &mut |decoder, field, field_typeinfo| {
                        match field {
                            0 => {
                                decoder.skip_from_typeinfo(field_typeinfo)?;
                                mask = SelectionRemoveMask::None;
                            }
                            1 => {
                                mask = SelectionRemoveMask::Mask(Self::decode_bitarray_bools(
                                    decoder,
                                    field_typeinfo,
                                )?);
                            }
                            2 => {
                                let mut indices = Vec::new();
                                Self::collect_i64_values(decoder, field_typeinfo, &mut indices)?;
                                mask = SelectionRemoveMask::OneIndices(indices);
                            }
                            3 => {
                                let mut indices = Vec::new();
                                Self::collect_i64_values(decoder, field_typeinfo, &mut indices)?;
                                mask = SelectionRemoveMask::ZeroIndices(indices);
                            }
                            _ => unreachable!("invalid selected selection remove mask field"),
                        }
                        Ok(())
                    },
                )?;
                Ok(mask)
            }
            _ => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(SelectionRemoveMask::None)
            }
        }
    }

    fn decode_selection_delta_inner<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        data: &mut SelectionDeltaData,
        found: &mut bool,
    ) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::decode_selection_delta_inner(decoder, child_typeinfo, data, found)
                    },
                )?;
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |field_name| match field_name {
                    "m_subgroupIndex" => Some(0u8),
                    "m_removeMask" => Some(1u8),
                    "m_addUnitTags" => Some(2u8),
                    _ => None,
                },
                &mut |decoder, field, field_typeinfo| {
                    match field {
                        0 => {
                            data.m_subgroup_index = decoder.i64_from_typeinfo(field_typeinfo)?;
                        }
                        1 => {
                            data.m_remove_mask =
                                Self::decode_selection_remove_mask(decoder, field_typeinfo)?;
                        }
                        2 => {
                            Self::collect_i64_values(
                                decoder,
                                field_typeinfo,
                                &mut data.m_add_unit_tags,
                            )?;
                        }
                        _ => unreachable!("invalid selected selection delta field"),
                    }
                    *found = true;
                    Ok(())
                },
            ),
            _ => decoder.skip_from_typeinfo(typeinfo),
        }
    }

    pub(crate) fn decode_selection_delta_data_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Option<SelectionDeltaData>, DecodeError> {
        let mut data = SelectionDeltaData::default();
        let mut found = false;
        Self::decode_selection_delta_inner(decoder, typeinfo, &mut data, &mut found)?;
        Ok(found.then_some(data))
    }

    fn decode_player_stats_inner<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
        stats: &mut PlayerStatsData,
        found: &mut bool,
    ) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => {
                decoder.skip_from_typeinfo(typeinfo)?;
                Ok(())
            }
            TypeOp::Optional => {
                decoder.visit_optional_child_from_typeinfo(
                    typeinfo,
                    &mut |decoder, child_typeinfo| {
                        Self::decode_player_stats_inner(decoder, child_typeinfo, stats, found)
                    },
                )?;
                Ok(())
            }
            TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
                typeinfo,
                &mut |field_name| match field_name {
                    "m_scoreValueFoodUsed" => Some(0u8),
                    "m_scoreValueMineralsCollectionRate" => Some(1u8),
                    "m_scoreValueVespeneCollectionRate" => Some(2u8),
                    _ => None,
                },
                &mut |decoder, field, field_typeinfo| {
                    match field {
                        0 => {
                            stats.m_score_value_food_used =
                                decoder.f64_from_typeinfo(field_typeinfo)?;
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
                    *found = true;
                    Ok(())
                },
            ),
            TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
                typeinfo,
                &mut |field_name| match field_name {
                    "m_scoreValueFoodUsed" => Some(0u8),
                    "m_scoreValueMineralsCollectionRate" => Some(1u8),
                    "m_scoreValueVespeneCollectionRate" => Some(2u8),
                    _ => None,
                },
                &mut |decoder, field, field_typeinfo| {
                    match field {
                        0 => {
                            stats.m_score_value_food_used =
                                decoder.f64_from_typeinfo(field_typeinfo)?;
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
                    *found = true;
                    Ok(())
                },
            ),
            _ => decoder.skip_from_typeinfo(typeinfo),
        }
    }

    pub(crate) fn decode_player_stats_from_typeinfo<D: TypeDecoder>(
        decoder: &mut D,
        typeinfo: &TypeInfo,
    ) -> Result<Option<PlayerStatsData>, DecodeError> {
        let mut stats = PlayerStatsData::default();
        let mut found = false;
        Self::decode_player_stats_inner(decoder, typeinfo, &mut stats, &mut found)?;
        Ok(found.then_some(stats))
    }
}
