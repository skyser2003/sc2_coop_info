use super::{EventDecodePlan, EventPlanCompiler, EventSpecialDataDecoder, TypeInfo, TypeOp};
use crate::{error::DecodeError, events::TriggerEventData, value::Value};
use std::sync::Arc;

pub(crate) trait TypeDecoder {
    fn done(&self) -> bool;
    fn used_bits(&self) -> usize;
    fn byte_align(&mut self);
    fn typeinfos(&self) -> Arc<[TypeInfo]>;
    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError> {
        let typeinfos = self.typeinfos();
        let typeinfo = EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
        self.instance_from_typeinfo(typeinfo)
    }
    fn instance_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError>;
    fn integer_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<i128, DecodeError>;
    fn i64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<i64>, DecodeError>;
    fn f64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<f64>, DecodeError>;
    fn string_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<String>, DecodeError>;
    fn optional_exists_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<bool, DecodeError>;
    fn array_length_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<usize, DecodeError>;
    fn int_i64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<i64>, DecodeError>;
    fn blob_bytes_from_typeinfo<'data>(
        &'data mut self,
        typeinfo: &TypeInfo,
    ) -> Result<&'data [u8], DecodeError>;
    fn fourcc_bytes_from_typeinfo<'data>(
        &'data mut self,
        typeinfo: &TypeInfo,
    ) -> Result<&'data [u8], DecodeError>;
    #[inline(always)]
    fn mark_trigger_text_from_typeinfo(
        &mut self,
        typeinfo: &TypeInfo,
        result: &mut TriggerEventData,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        mark_trigger_text_from_typeinfo(self, typeinfo, result)
    }
    #[inline(always)]
    fn append_i64_values_from_typeinfo(
        &mut self,
        typeinfo: &TypeInfo,
        values: &mut Vec<i64>,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        append_i64_values_from_typeinfo(self, typeinfo, values)
    }
    fn bools_from_bitarray_typeinfo(
        &mut self,
        typeinfo: &TypeInfo,
    ) -> Result<Vec<bool>, DecodeError>;
    fn skip_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<(), DecodeError>;
    fn visit_struct_fields_from_typeinfo<K, S, F>(
        &mut self,
        typeinfo: &TypeInfo,
        select_field: &mut S,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
        K: Copy,
        S: FnMut(&str) -> Option<K>,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>;
    fn visit_choice_field_from_typeinfo<K, S, F>(
        &mut self,
        typeinfo: &TypeInfo,
        select_field: &mut S,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
        K: Copy,
        S: FnMut(&str) -> Option<K>,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>;
    fn visit_array_elements_from_typeinfo<F>(
        &mut self,
        typeinfo: &TypeInfo,
        on_element: &mut F,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
        F: FnMut(&mut Self, &TypeInfo) -> Result<(), DecodeError>;
    fn visit_optional_child_from_typeinfo<F>(
        &mut self,
        typeinfo: &TypeInfo,
        on_child: &mut F,
    ) -> Result<bool, DecodeError>
    where
        Self: Sized,
        F: FnMut(&mut Self, &TypeInfo) -> Result<(), DecodeError>;
    fn decode_event_fields_from_plan<K, F>(
        &mut self,
        plan: &EventDecodePlan<K>,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
        K: Copy,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>;
    fn skip_event_fields_from_plan<K>(
        &mut self,
        plan: &EventDecodePlan<K>,
    ) -> Result<(), DecodeError>
    where
        Self: Sized;
}

#[inline(always)]
fn append_i64_values_from_typeinfo<D: TypeDecoder + ?Sized>(
    decoder: &mut D,
    typeinfo: &TypeInfo,
    values: &mut Vec<i64>,
) -> Result<(), DecodeError> {
    match typeinfo.op() {
        TypeOp::Null => decoder.skip_from_typeinfo(typeinfo),
        TypeOp::Optional => {
            if !decoder.optional_exists_from_typeinfo(typeinfo)? {
                return Ok(());
            }

            let typeid = typeinfo.child_typeid()?;
            let typeinfos = decoder.typeinfos();
            let child_typeinfo = EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
            append_i64_values_from_typeinfo(decoder, child_typeinfo, values)
        }
        TypeOp::Int => {
            if let Some(value) = decoder.int_i64_from_typeinfo(typeinfo)? {
                values.push(value);
            }
            Ok(())
        }
        TypeOp::Array => {
            let length = decoder.array_length_from_typeinfo(typeinfo)?;
            values.reserve(length);
            let typeid = typeinfo.child_typeid()?;
            let typeinfos = decoder.typeinfos();
            let child_typeinfo = EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
            for _ in 0..length {
                append_i64_values_from_typeinfo(decoder, child_typeinfo, values)?;
            }
            Ok(())
        }
        _ => decoder.skip_from_typeinfo(typeinfo),
    }
}

#[inline(always)]
fn mark_trigger_text_from_typeinfo<D: TypeDecoder + ?Sized>(
    decoder: &mut D,
    typeinfo: &TypeInfo,
    result: &mut TriggerEventData,
) -> Result<(), DecodeError> {
    match typeinfo.op() {
        TypeOp::Blob => {
            let bytes = decoder.blob_bytes_from_typeinfo(typeinfo)?;
            EventSpecialDataDecoder::mark_trigger_text_bytes(result, bytes);
            Ok(())
        }
        TypeOp::Fourcc => {
            let bytes = decoder.fourcc_bytes_from_typeinfo(typeinfo)?;
            EventSpecialDataDecoder::mark_trigger_text_bytes(result, bytes);
            Ok(())
        }
        _ => decoder.skip_from_typeinfo(typeinfo),
    }
}
