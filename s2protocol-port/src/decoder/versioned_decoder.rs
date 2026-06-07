use super::{
    EventDecodePlan, EventPlanCompiler, IntegerDecodePlan, TaggedEventDecodePlan,
    TaggedEventFieldPlan, TypeDecoder, TypeInfo, TypeOp,
};
use crate::bitstream::BitPackedBuffer;
use crate::{error::DecodeError, value::Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) trait HeaderIntegerDecoder {
    fn integer_from_plan(&mut self, plan: &IntegerDecodePlan) -> Result<i128, DecodeError>;
}

pub(super) struct VersionedDecoder<'a> {
    buffer: BitPackedBuffer<'a>,
    typeinfos: Arc<[TypeInfo]>,
}

impl<'a> VersionedDecoder<'a> {
    pub(super) fn new(contents: &'a [u8], typeinfos: Arc<[TypeInfo]>) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos,
        }
    }

    fn expect_skip(&mut self, expected: u8) -> Result<(), DecodeError> {
        let marker = self.buffer.read_u8()?;
        if marker != expected {
            Err(DecodeError::Corrupted(format!(
                "unexpected versioned skip marker expected {expected} got {marker}"
            )))
        } else {
            Ok(())
        }
    }

    fn vint(&mut self) -> Result<i128, DecodeError> {
        let mut b = self.buffer.read_u8()?;
        let negative = (b & 1) != 0;
        let mut value: i128 = ((u16::from(b) >> 1) & 0x3f) as i128;
        let mut shift = 6;

        while (b & 0x80) != 0 {
            b = self.buffer.read_u8()?;
            value |= ((u16::from(b) & 0x7f) as i128) << shift;
            shift += 7;
        }

        if negative { Ok(-value) } else { Ok(value) }
    }

    fn skip_instance(&mut self) -> Result<(), DecodeError> {
        let skip = self.buffer.read_u8()?;
        match skip {
            0 => {
                let length = self.vint()? as usize;
                for _ in 0..length {
                    self.skip_instance()?;
                }
            }
            1 => {
                let bits = self.vint()? as usize;
                let bytes = bits.div_ceil(8);
                self.buffer.skip_aligned_bytes(bytes)?;
            }
            2 => {
                let bytes = self.vint()? as usize;
                self.buffer.skip_aligned_bytes(bytes)?;
            }
            3 => {
                let _ = self.vint()?;
                self.skip_instance()?;
            }
            4 => {
                let exists = self.buffer.read_u8()? != 0;
                if exists {
                    self.skip_instance()?;
                }
            }
            5 => {
                let length = self.vint()? as usize;
                for _ in 0..length {
                    let _ = self.vint()?;
                    self.skip_instance()?;
                }
            }
            6 => {
                self.buffer.skip_aligned_bytes(1)?;
            }
            7 => {
                self.buffer.skip_aligned_bytes(4)?;
            }
            8 => {
                self.buffer.skip_aligned_bytes(8)?;
            }
            9 => {
                let _ = self.vint()?;
            }
            _ => {
                return Err(DecodeError::Corrupted(format!(
                    "invalid skip marker {skip}"
                )));
            }
        }
        Ok(())
    }

    fn array(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(0)?;
        let length = self.vint()? as usize;
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;

        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(self.instance_from_typeinfo(child_typeinfo)?);
        }
        Ok(Value::Array(values))
    }

    fn bitarray(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(1)?;
        let _ = typeinfo;
        let length = self.vint()? as usize;
        let bytes = length.div_ceil(8);

        if length > 127 {
            let raw = self.buffer.read_aligned_bytes(bytes)?;
            return Ok(Value::Array(vec![
                Value::Int(length as i128),
                Value::Bytes(raw),
            ]));
        }

        let raw = self.buffer.read_aligned_slice(bytes)?;
        let mut value: i128 = 0;
        for byte in raw {
            value = (value << 8) | i128::from(*byte);
        }
        Ok(Value::Array(vec![
            Value::Int(length as i128),
            Value::Int(value),
        ]))
    }

    fn bitarray_bools(&mut self, typeinfo: &TypeInfo) -> Result<Vec<bool>, DecodeError> {
        if typeinfo.op() != TypeOp::BitArray {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to bitarray",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        self.expect_skip(1)?;
        let length = self.vint()? as usize;
        let bytes = length.div_ceil(8);
        let raw = self.buffer.read_aligned_slice(bytes)?;

        if length > 127 {
            let mut bits = Vec::with_capacity(length);
            for index in 0..length {
                let byte = raw.get(index / 8).copied().unwrap_or_default();
                let bit_offset = 7 - (index % 8);
                bits.push(((byte >> bit_offset) & 1) != 0);
            }
            return Ok(bits);
        }

        let mut value: i128 = 0;
        for byte in raw {
            value = (value << 8) | i128::from(*byte);
        }
        let bits = u128::try_from(value).map_err(|_| {
            DecodeError::Corrupted("selection remove mask bitarray is negative".into())
        })?;
        Ok((0..length)
            .map(|index| ((bits >> index) & 1) != 0)
            .collect())
    }

    fn blob(&mut self, _typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(2)?;
        let length = self.vint()? as usize;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(length)?))
    }

    fn bool(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(6)?;
        Ok(Value::Bool(self.buffer.read_u8()? != 0))
    }

    fn choice(&mut self, typeinfo: &TypeInfo) -> Result<BTreeMap<String, Value>, DecodeError> {
        self.expect_skip(3)?;
        let tag = self.vint()?;
        if let Some(field) = typeinfo.choice_fields()?.get(&tag) {
            let typeinfos = Arc::clone(&self.typeinfos);
            let child_typeinfo =
                EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
            let value = self.instance_from_typeinfo(child_typeinfo)?;
            let mut object = BTreeMap::new();
            object.insert(field.name().to_string(), value);
            return Ok(object);
        }

        self.skip_instance()?;
        Ok(BTreeMap::new())
    }

    fn fourcc(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(7)?;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(4)?))
    }

    fn int(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(9)?;
        Ok(Value::Int(self.vint()?))
    }

    fn optional(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(4)?;
        let exists = self.buffer.read_u8()? != 0;
        if !exists {
            return Ok(Value::Null);
        }

        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
        self.instance_from_typeinfo(child_typeinfo)
    }

    fn real32(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(7)?;
        let bits = u32::from_be_bytes(self.buffer.read_aligned_array::<4>()?);
        Ok(Value::Float(f32::from_bits(bits) as f64))
    }

    fn real64(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(8)?;
        let bits = u64::from_be_bytes(self.buffer.read_aligned_array::<8>()?);
        Ok(Value::Float(f64::from_bits(bits)))
    }

    fn visit_struct_fields<K, S, F>(
        &mut self,
        typeinfo: &TypeInfo,
        select_field: &mut S,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        K: Copy,
        S: FnMut(&str) -> Option<K>,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        if typeinfo.op() != TypeOp::Struct {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to struct",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        self.expect_skip(5)?;
        let fields = typeinfo.struct_fields()?;
        let field_map = typeinfo.struct_fields_by_tag()?;
        let field_count = self.vint()? as usize;
        let typeinfos = Arc::clone(&self.typeinfos);

        for _ in 0..field_count {
            let tag = self.vint()?;
            let field = match field_map.get(&tag) {
                Some(value) => value,
                None => {
                    self.skip_instance()?;
                    continue;
                }
            };

            if field.is_parent() {
                let parent_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                if parent_typeinfo.op() == TypeOp::Struct {
                    self.visit_struct_fields(parent_typeinfo, select_field, on_field)?;
                    continue;
                }

                if fields.len() == 1 {
                    return Err(DecodeError::UnexpectedType(format!(
                        "typeid={} op={} does not decode to struct",
                        typeinfo.typeid(),
                        typeinfo.op_name()
                    )));
                }

                if let Some(selected_field) = select_field("__parent") {
                    on_field(self, selected_field, parent_typeinfo)?;
                } else {
                    self.skip_instance()?;
                }
            } else {
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                if let Some(selected_field) = select_field(field.name()) {
                    on_field(self, selected_field, child_typeinfo)?;
                } else {
                    self.skip_instance()?;
                }
            }
        }

        Ok(())
    }

    fn visit_choice_field<K, S, F>(
        &mut self,
        typeinfo: &TypeInfo,
        select_field: &mut S,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        K: Copy,
        S: FnMut(&str) -> Option<K>,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        if typeinfo.op() != TypeOp::Choice {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to choice",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        self.expect_skip(3)?;
        let tag = self.vint()?;
        let Some(field) = typeinfo.choice_fields()?.get(&tag) else {
            self.skip_instance()?;
            return Ok(());
        };
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
        if let Some(selected_field) = select_field(field.name()) {
            on_field(self, selected_field, child_typeinfo)?;
        } else {
            self.skip_instance()?;
        }
        Ok(())
    }

    fn visit_array_elements<F>(
        &mut self,
        typeinfo: &TypeInfo,
        on_element: &mut F,
    ) -> Result<(), DecodeError>
    where
        F: FnMut(&mut Self, &TypeInfo) -> Result<(), DecodeError>,
    {
        if typeinfo.op() != TypeOp::Array {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to array",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        self.expect_skip(0)?;
        let length = self.vint()? as usize;
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
        for _ in 0..length {
            on_element(self, child_typeinfo)?;
        }
        Ok(())
    }

    fn visit_optional_child<F>(
        &mut self,
        typeinfo: &TypeInfo,
        on_child: &mut F,
    ) -> Result<bool, DecodeError>
    where
        F: FnMut(&mut Self, &TypeInfo) -> Result<(), DecodeError>,
    {
        if typeinfo.op() != TypeOp::Optional {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to optional",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        self.expect_skip(4)?;
        let exists = self.buffer.read_u8()? != 0;
        if !exists {
            return Ok(false);
        }

        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
        on_child(self, child_typeinfo)?;
        Ok(true)
    }

    fn decode_tagged_event_fields<K, F>(
        &mut self,
        plan: &TaggedEventDecodePlan<K>,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        K: Copy,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        self.expect_skip(5)?;
        let field_count = self.vint()? as usize;

        for _ in 0..field_count {
            let tag = self.vint()?;
            let Some(step) = plan.field_for_tag(&tag) else {
                self.skip_instance()?;
                continue;
            };

            match step {
                TaggedEventFieldPlan::Decode { field, typeinfo } => {
                    on_field(self, *field, typeinfo)?;
                }
                TaggedEventFieldPlan::Skip => {
                    self.skip_instance()?;
                }
                TaggedEventFieldPlan::Nested(nested) => {
                    self.decode_tagged_event_fields(nested, on_field)?;
                }
            }
        }

        Ok(())
    }

    fn object(&mut self, typeinfo: &TypeInfo) -> Result<BTreeMap<String, Value>, DecodeError> {
        if typeinfo.op() != TypeOp::Struct {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to struct",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        self.expect_skip(5)?;
        let fields = typeinfo.struct_fields()?;
        let field_map = typeinfo.struct_fields_by_tag()?;
        let field_count = self.vint()? as usize;
        let typeinfos = Arc::clone(&self.typeinfos);
        let mut result = BTreeMap::new();

        for _ in 0..field_count {
            let tag = self.vint()?;
            let field = match field_map.get(&tag) {
                Some(v) => v,
                None => {
                    self.skip_instance()?;
                    continue;
                }
            };

            if field.is_parent() {
                let parent_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                if parent_typeinfo.op() == TypeOp::Struct {
                    let parent_map = self.object(parent_typeinfo)?;
                    if fields.len() == 1 {
                        return Ok(parent_map);
                    }
                    for (k, v) in parent_map {
                        result.insert(k, v);
                    }
                    continue;
                }

                let parent = self.instance_from_typeinfo(parent_typeinfo)?;
                match parent {
                    Value::Object(parent_map) => {
                        if fields.len() == 1 {
                            return Ok(parent_map);
                        }
                        for (k, v) in parent_map {
                            result.insert(k, v);
                        }
                    }
                    other => {
                        if fields.len() == 1 {
                            return Err(DecodeError::UnexpectedType(format!(
                                "typeid={} op={} does not decode to struct",
                                typeinfo.typeid(),
                                typeinfo.op_name()
                            )));
                        }
                        result.insert("__parent".to_string(), other);
                    }
                }
            } else {
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                result.insert(
                    field.name().to_string(),
                    self.instance_from_typeinfo(child_typeinfo)?,
                );
            }
        }

        Ok(result)
    }

    fn dispatch(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        match typeinfo.op() {
            TypeOp::Array => self.array(typeinfo),
            TypeOp::BitArray => self.bitarray(typeinfo),
            TypeOp::Blob => self.blob(typeinfo),
            TypeOp::Bool => self.bool(),
            TypeOp::Choice => Ok(Value::Object(self.choice(typeinfo)?)),
            TypeOp::Fourcc => self.fourcc(),
            TypeOp::Int => self.int(),
            TypeOp::Null => Ok(Value::Null),
            TypeOp::Optional => self.optional(typeinfo),
            TypeOp::Real32 => self.real32(),
            TypeOp::Real64 => self.real64(),
            TypeOp::Struct => Ok(Value::Object(self.object(typeinfo)?)),
        }
    }
}

impl TypeDecoder for VersionedDecoder<'_> {
    fn done(&self) -> bool {
        self.buffer.done()
    }

    fn used_bits(&self) -> usize {
        self.buffer.used_bits()
    }

    fn byte_align(&mut self) {
        self.buffer.byte_align();
    }

    fn typeinfos(&self) -> Arc<[TypeInfo]> {
        Arc::clone(&self.typeinfos)
    }

    fn instance_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        if std::env::var("S2_DEBUG_DECODER").is_ok() {
            log::trace!(
                "[versioned] typeid={typeid} op={} used_bits={}",
                typeinfo.op_name(),
                self.used_bits(),
                typeid = typeinfo.typeid()
            );
        }

        let used_bits = self.used_bits();
        self.dispatch(typeinfo).map_err(|error| match error {
            DecodeError::Corrupted(message) => DecodeError::Corrupted(format!(
                "typeid={typeid} op={} used_bits={used_bits}: {message}",
                typeinfo.op_name(),
                typeid = typeinfo.typeid()
            )),
            DecodeError::Truncated => DecodeError::Corrupted(format!(
                "typeid={typeid} op={} used_bits={used_bits}: buffer truncated",
                typeinfo.op_name(),
                typeid = typeinfo.typeid()
            )),
            other => other,
        })
    }

    fn integer_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<i128, DecodeError> {
        match typeinfo.op() {
            TypeOp::Int => {
                self.expect_skip(9)?;
                self.vint()
            }
            TypeOp::Choice => {
                self.expect_skip(3)?;
                let tag = self.vint()?;
                let field = typeinfo
                    .choice_fields()?
                    .get(&tag)
                    .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                self.integer_from_typeinfo(child_typeinfo)
            }
            _ => Err(DecodeError::Corrupted(format!(
                "typeid={} op={} does not decode to integer",
                typeinfo.typeid(),
                typeinfo.op_name()
            ))),
        }
    }

    fn i64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<i64>, DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => Ok(None),
            TypeOp::Optional => {
                self.expect_skip(4)?;
                let exists = self.buffer.read_u8()? != 0;
                if !exists {
                    return Ok(None);
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
                self.i64_from_typeinfo(child_typeinfo)
            }
            TypeOp::Real32 => Ok(self.real32()?.as_f64().map(|value| value as i64)),
            TypeOp::Real64 => Ok(self.real64()?.as_f64().map(|value| value as i64)),
            _ => Ok(i64::try_from(self.integer_from_typeinfo(typeinfo)?).ok()),
        }
    }

    fn f64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<f64>, DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => Ok(None),
            TypeOp::Optional => {
                self.expect_skip(4)?;
                let exists = self.buffer.read_u8()? != 0;
                if !exists {
                    return Ok(None);
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
                self.f64_from_typeinfo(child_typeinfo)
            }
            TypeOp::Real32 => Ok(self.real32()?.as_f64()),
            TypeOp::Real64 => Ok(self.real64()?.as_f64()),
            _ => Ok(Some(self.integer_from_typeinfo(typeinfo)? as f64)),
        }
    }

    fn string_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<String>, DecodeError> {
        match typeinfo.op() {
            TypeOp::Null => Ok(None),
            TypeOp::Optional => {
                self.expect_skip(4)?;
                let exists = self.buffer.read_u8()? != 0;
                if !exists {
                    return Ok(None);
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
                self.string_from_typeinfo(child_typeinfo)
            }
            TypeOp::Blob => {
                self.expect_skip(2)?;
                let length = self.vint()? as usize;
                let bytes = self.buffer.read_aligned_slice(length)?;
                Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
            }
            TypeOp::Fourcc => {
                self.expect_skip(7)?;
                let bytes = self.buffer.read_aligned_slice(4)?;
                Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
            }
            TypeOp::Bool => {
                self.expect_skip(6)?;
                Ok(Some((self.buffer.read_u8()? != 0).to_string()))
            }
            TypeOp::Real32 => Ok(self.real32()?.as_f64().map(|value| value.to_string())),
            TypeOp::Real64 => Ok(self.real64()?.as_f64().map(|value| value.to_string())),
            _ => Ok(Some(self.integer_from_typeinfo(typeinfo)?.to_string())),
        }
    }

    #[inline(always)]
    fn optional_exists_from_typeinfo(&mut self, _typeinfo: &TypeInfo) -> Result<bool, DecodeError> {
        self.expect_skip(4)?;
        Ok(self.buffer.read_u8()? != 0)
    }

    #[inline(always)]
    fn array_length_from_typeinfo(&mut self, _typeinfo: &TypeInfo) -> Result<usize, DecodeError> {
        self.expect_skip(0)?;
        Ok(self.vint()? as usize)
    }

    #[inline(always)]
    fn int_i64_from_typeinfo(&mut self, _typeinfo: &TypeInfo) -> Result<Option<i64>, DecodeError> {
        self.expect_skip(9)?;
        Ok(i64::try_from(self.vint()?).ok())
    }

    #[inline(always)]
    fn blob_bytes_from_typeinfo<'data>(
        &'data mut self,
        _typeinfo: &TypeInfo,
    ) -> Result<&'data [u8], DecodeError> {
        self.expect_skip(2)?;
        let length = self.vint()? as usize;
        self.buffer.read_aligned_slice(length)
    }

    #[inline(always)]
    fn fourcc_bytes_from_typeinfo<'data>(
        &'data mut self,
        _typeinfo: &TypeInfo,
    ) -> Result<&'data [u8], DecodeError> {
        self.expect_skip(7)?;
        self.buffer.read_aligned_slice(4)
    }

    fn bools_from_bitarray_typeinfo(
        &mut self,
        typeinfo: &TypeInfo,
    ) -> Result<Vec<bool>, DecodeError> {
        self.bitarray_bools(typeinfo)
    }

    fn skip_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<(), DecodeError> {
        if typeinfo.op() == TypeOp::Null {
            return Ok(());
        }
        self.skip_instance()
    }

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
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        self.visit_struct_fields(typeinfo, select_field, on_field)
    }

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
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        self.visit_choice_field(typeinfo, select_field, on_field)
    }

    fn visit_array_elements_from_typeinfo<F>(
        &mut self,
        typeinfo: &TypeInfo,
        on_element: &mut F,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
        F: FnMut(&mut Self, &TypeInfo) -> Result<(), DecodeError>,
    {
        self.visit_array_elements(typeinfo, on_element)
    }

    fn visit_optional_child_from_typeinfo<F>(
        &mut self,
        typeinfo: &TypeInfo,
        on_child: &mut F,
    ) -> Result<bool, DecodeError>
    where
        Self: Sized,
        F: FnMut(&mut Self, &TypeInfo) -> Result<(), DecodeError>,
    {
        self.visit_optional_child(typeinfo, on_child)
    }

    fn decode_event_fields_from_plan<K, F>(
        &mut self,
        plan: &EventDecodePlan<K>,
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
        K: Copy,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        match plan {
            EventDecodePlan::Ordered(_) => Err(DecodeError::UnexpectedType(
                "versioned event plan expects tagged struct fields".into(),
            )),
            EventDecodePlan::Tagged(plan) => self.decode_tagged_event_fields(plan, on_field),
        }
    }

    fn skip_event_fields_from_plan<K>(
        &mut self,
        plan: &EventDecodePlan<K>,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        match plan {
            EventDecodePlan::Ordered(_) => Err(DecodeError::UnexpectedType(
                "versioned event plan expects tagged struct fields".into(),
            )),
            EventDecodePlan::Tagged(_) => self.skip_instance(),
        }
    }
}

impl HeaderIntegerDecoder for VersionedDecoder<'_> {
    fn integer_from_plan(&mut self, plan: &IntegerDecodePlan) -> Result<i128, DecodeError> {
        match plan {
            IntegerDecodePlan::Int { .. } => {
                self.expect_skip(9)?;
                self.vint()
            }
            IntegerDecodePlan::Choice { fields, .. } => {
                self.expect_skip(3)?;
                let tag = self.vint()?;
                let child_plan = fields
                    .get(&tag)
                    .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
                self.integer_from_plan(child_plan)
            }
        }
    }
}
