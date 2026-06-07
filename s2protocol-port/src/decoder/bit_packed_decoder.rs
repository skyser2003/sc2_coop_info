use std::collections::BTreeMap;
use std::sync::Arc;

use crate::bitstream::BitPackedBuffer;
use crate::{error::DecodeError, value::Value};

use super::{
    EventDecodePlan, EventPlanKind, HeaderIntegerDecoder, IntBounds, IntegerDecodePlan,
    OrderedEventFieldPlan, TagLookup, TaggedEventDecodePlan, TaggedEventFieldPlan, TypeDecoder,
    TypeInfo, TypeOp,
};

pub(super) struct BitPackedDecoder<'a> {
    buffer: BitPackedBuffer<'a>,
    typeinfos: Arc<[TypeInfo]>,
}

pub(crate) struct EventPlanCompiler;

impl EventPlanCompiler {
    pub(super) fn lookup_typeinfo(
        typeinfos: &[TypeInfo],
        typeid: usize,
    ) -> Result<&TypeInfo, DecodeError> {
        typeinfos
            .get(typeid)
            .ok_or_else(|| DecodeError::Corrupted(format!("typeid {typeid} out of range")))
    }

    pub(crate) fn compile<F, S>(
        typeinfo: &TypeInfo,
        typeinfos: &[TypeInfo],
        plan_kind: EventPlanKind,
        select_field: &mut S,
    ) -> Result<Option<EventDecodePlan<F>>, DecodeError>
    where
        F: Copy,
        S: FnMut(&str) -> Option<F>,
    {
        if typeinfo.op() != TypeOp::Struct {
            return Ok(None);
        }

        match plan_kind {
            EventPlanKind::Ordered => Self::compile_ordered(typeinfo, typeinfos, select_field)
                .map(|plan| plan.map(EventDecodePlan::Ordered)),
            EventPlanKind::Tagged => Self::compile_tagged(typeinfo, typeinfos, select_field)
                .map(|plan| plan.map(EventDecodePlan::Tagged)),
        }
    }

    fn compile_ordered<F, S>(
        typeinfo: &TypeInfo,
        typeinfos: &[TypeInfo],
        select_field: &mut S,
    ) -> Result<Option<Arc<[OrderedEventFieldPlan<F>]>>, DecodeError>
    where
        F: Copy,
        S: FnMut(&str) -> Option<F>,
    {
        if typeinfo.op() != TypeOp::Struct {
            return Ok(None);
        }

        let fields = typeinfo.struct_fields()?;
        let mut plans = Vec::with_capacity(fields.len());
        for field in fields {
            let child_typeinfo = Self::lookup_typeinfo(typeinfos, field.typeid())?.clone();
            if field.is_parent() {
                if child_typeinfo.op() == TypeOp::Struct {
                    let Some(parent_plan) =
                        Self::compile_ordered(&child_typeinfo, typeinfos, select_field)?
                    else {
                        if fields.len() == 1 {
                            return Ok(None);
                        }
                        plans.push(OrderedEventFieldPlan::Skip {
                            typeinfo: child_typeinfo,
                        });
                        continue;
                    };
                    plans.push(OrderedEventFieldPlan::Nested(parent_plan));
                    continue;
                }

                if fields.len() == 1 {
                    return Ok(None);
                }

                if let Some(selected_field) = select_field("__parent") {
                    plans.push(OrderedEventFieldPlan::Decode {
                        field: selected_field,
                        typeinfo: child_typeinfo,
                    });
                } else {
                    plans.push(OrderedEventFieldPlan::Skip {
                        typeinfo: child_typeinfo,
                    });
                }
                continue;
            }

            if let Some(selected_field) = select_field(field.name()) {
                plans.push(OrderedEventFieldPlan::Decode {
                    field: selected_field,
                    typeinfo: child_typeinfo,
                });
            } else {
                plans.push(OrderedEventFieldPlan::Skip {
                    typeinfo: child_typeinfo,
                });
            }
        }

        Ok(Some(Arc::from(plans)))
    }

    fn compile_tagged<F, S>(
        typeinfo: &TypeInfo,
        typeinfos: &[TypeInfo],
        select_field: &mut S,
    ) -> Result<Option<Arc<TaggedEventDecodePlan<F>>>, DecodeError>
    where
        F: Copy,
        S: FnMut(&str) -> Option<F>,
    {
        if typeinfo.op() != TypeOp::Struct {
            return Ok(None);
        }

        let fields = typeinfo.struct_fields()?;
        let mut entries = Vec::new();
        for field in fields {
            let Some(tag) = field.tag() else {
                continue;
            };

            let child_typeinfo = Self::lookup_typeinfo(typeinfos, field.typeid())?.clone();
            let plan = if field.is_parent() {
                if child_typeinfo.op() == TypeOp::Struct {
                    match Self::compile_tagged(&child_typeinfo, typeinfos, select_field)? {
                        Some(parent_plan) => TaggedEventFieldPlan::Nested(parent_plan),
                        None if fields.len() == 1 => return Ok(None),
                        None => TaggedEventFieldPlan::Skip,
                    }
                } else if fields.len() == 1 {
                    return Ok(None);
                } else if let Some(selected_field) = select_field("__parent") {
                    TaggedEventFieldPlan::Decode {
                        field: selected_field,
                        typeinfo: Box::new(child_typeinfo),
                    }
                } else {
                    TaggedEventFieldPlan::Skip
                }
            } else if let Some(selected_field) = select_field(field.name()) {
                TaggedEventFieldPlan::Decode {
                    field: selected_field,
                    typeinfo: Box::new(child_typeinfo),
                }
            } else {
                TaggedEventFieldPlan::Skip
            };

            entries.push((tag, plan));
        }

        let Some(fields_by_tag) = TagLookup::new(entries, "duplicate event plan tag")? else {
            return Ok(None);
        };
        Ok(Some(Arc::new(TaggedEventDecodePlan::new(fields_by_tag))))
    }
}

impl<'a> BitPackedDecoder<'a> {
    pub(super) fn new(contents: &'a [u8], typeinfos: Arc<[TypeInfo]>) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos,
        }
    }

    fn int(&mut self, bounds: IntBounds) -> Result<i128, DecodeError> {
        let raw = self.buffer.read_bits(bounds.bits())? as i128;
        Ok(bounds.min() as i128 + raw)
    }

    fn optional_exists(&mut self) -> Result<bool, DecodeError> {
        self.buffer.read_bool()
    }

    fn array(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let length = self.int(typeinfo.length_bounds()?)? as usize;
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
        let length = self.int(typeinfo.length_bounds()?)? as usize;
        if length <= 64 {
            let value = self.buffer.read_bits(length)?;
            return Ok(Value::Array(vec![
                Value::Int(length as i128),
                Value::Int(value as i128),
            ]));
        }

        let bytes = self.read_bits_as_bytes(length)?;
        Ok(Value::Array(vec![
            Value::Int(length as i128),
            Value::Bytes(bytes),
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

        let length = self.int(typeinfo.length_bounds()?)? as usize;
        if length <= 64 {
            let bits = self.buffer.read_bits(length)?;
            return Ok((0..length)
                .map(|index| ((bits >> index) & 1) != 0)
                .collect());
        }

        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(self.buffer.read_bool()?);
        }
        Ok(values)
    }

    fn blob(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let length = self.int(typeinfo.length_bounds()?)? as usize;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(length)?))
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

        let fields = typeinfo.struct_fields()?;
        let typeinfos = Arc::clone(&self.typeinfos);
        for field in fields {
            let child_typeinfo =
                EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
            if field.is_parent() {
                let parent_typeinfo = child_typeinfo;
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
                    self.skip_from_typeinfo(parent_typeinfo)?;
                }
            } else if let Some(selected_field) = select_field(field.name()) {
                on_field(self, selected_field, child_typeinfo)?;
            } else {
                self.skip_from_typeinfo(child_typeinfo)?;
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

        let tag = self.int(typeinfo.choice_tag_bounds()?)?;
        let field = typeinfo
            .choice_fields()?
            .get(&tag)
            .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
        if let Some(selected_field) = select_field(field.name()) {
            on_field(self, selected_field, child_typeinfo)?;
        } else {
            self.skip_from_typeinfo(child_typeinfo)?;
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

        let length = self.int(typeinfo.length_bounds()?)? as usize;
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

        let exists = self.optional_exists()?;
        if !exists {
            return Ok(false);
        }

        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
        on_child(self, child_typeinfo)?;
        Ok(true)
    }

    fn decode_ordered_event_fields<K, F>(
        &mut self,
        plan: &[OrderedEventFieldPlan<K>],
        on_field: &mut F,
    ) -> Result<(), DecodeError>
    where
        K: Copy,
        F: FnMut(&mut Self, K, &TypeInfo) -> Result<(), DecodeError>,
    {
        for step in plan {
            match step {
                OrderedEventFieldPlan::Decode { field, typeinfo } => {
                    on_field(self, *field, typeinfo)?;
                }
                OrderedEventFieldPlan::Skip { typeinfo } => {
                    self.skip_from_typeinfo(typeinfo)?;
                }
                OrderedEventFieldPlan::Nested(nested) => {
                    self.decode_ordered_event_fields(nested.as_ref(), on_field)?;
                }
            }
        }

        Ok(())
    }

    fn skip_ordered_event_fields<K>(
        &mut self,
        plan: &[OrderedEventFieldPlan<K>],
    ) -> Result<(), DecodeError> {
        for step in plan {
            match step {
                OrderedEventFieldPlan::Decode { typeinfo, .. }
                | OrderedEventFieldPlan::Skip { typeinfo } => {
                    self.skip_from_typeinfo(typeinfo)?;
                }
                OrderedEventFieldPlan::Nested(nested) => {
                    self.skip_ordered_event_fields(nested.as_ref())?;
                }
            }
        }

        Ok(())
    }

    fn skip_value(&mut self, typeinfo: &TypeInfo) -> Result<(), DecodeError> {
        match typeinfo.op() {
            TypeOp::Array => {
                let length = self.int(typeinfo.length_bounds()?)? as usize;
                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
                for _ in 0..length {
                    self.skip_from_typeinfo(child_typeinfo)?;
                }
                Ok(())
            }
            TypeOp::BitArray => {
                let length = self.int(typeinfo.length_bounds()?)? as usize;
                self.buffer.skip_bits(length)
            }
            TypeOp::Blob => {
                let length = self.int(typeinfo.length_bounds()?)? as usize;
                self.buffer.skip_aligned_bytes(length)?;
                Ok(())
            }
            TypeOp::Bool => {
                self.buffer.read_bool()?;
                Ok(())
            }
            TypeOp::Choice => {
                let tag = self.int(typeinfo.choice_tag_bounds()?)?;
                let field = typeinfo
                    .choice_fields()?
                    .get(&tag)
                    .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                self.skip_from_typeinfo(child_typeinfo)
            }
            TypeOp::Fourcc => {
                self.buffer.skip_aligned_bytes(4)?;
                Ok(())
            }
            TypeOp::Int => {
                let _ = self.int(typeinfo.int_bounds()?)?;
                Ok(())
            }
            TypeOp::Null => Ok(()),
            TypeOp::Optional => {
                let exists = self.optional_exists()?;
                if !exists {
                    return Ok(());
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeid)?;
                self.skip_from_typeinfo(child_typeinfo)
            }
            TypeOp::Real32 => {
                self.buffer.skip_unaligned_bytes(4)?;
                Ok(())
            }
            TypeOp::Real64 => {
                self.buffer.skip_unaligned_bytes(8)?;
                Ok(())
            }
            TypeOp::Struct => {
                let fields = typeinfo.struct_fields()?;
                for field in fields {
                    let typeinfos = Arc::clone(&self.typeinfos);
                    let child_typeinfo =
                        EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                    self.skip_from_typeinfo(child_typeinfo)?;
                }
                Ok(())
            }
        }
    }

    fn choice(&mut self, typeinfo: &TypeInfo) -> Result<BTreeMap<String, Value>, DecodeError> {
        let tag = self.int(typeinfo.choice_tag_bounds()?)?;
        let field = typeinfo
            .choice_fields()?
            .get(&tag)
            .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;

        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo =
            EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
        let value = self.instance_from_typeinfo(child_typeinfo)?;
        let mut object = BTreeMap::new();
        object.insert(field.name().to_string(), value);
        Ok(object)
    }

    fn fourcc(&mut self) -> Result<Value, DecodeError> {
        let bytes = self.buffer.read_aligned_slice(4)?;
        Ok(Value::String(String::from_utf8_lossy(bytes).to_string()))
    }

    fn optional(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let exists = self.optional_exists()?;
        if exists {
            let typeinfos = Arc::clone(&self.typeinfos);
            let child_typeinfo =
                EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
            self.instance_from_typeinfo(child_typeinfo)
        } else {
            Ok(Value::Null)
        }
    }

    fn real32(&mut self) -> Result<Value, DecodeError> {
        let bits = u32::from_be_bytes(self.buffer.read_unaligned_array::<4>()?);
        Ok(Value::Float(f32::from_bits(bits) as f64))
    }

    fn real64(&mut self) -> Result<Value, DecodeError> {
        let bits = u64::from_be_bytes(self.buffer.read_unaligned_array::<8>()?);
        Ok(Value::Float(f64::from_bits(bits)))
    }

    fn object(&mut self, typeinfo: &TypeInfo) -> Result<BTreeMap<String, Value>, DecodeError> {
        if typeinfo.op() != TypeOp::Struct {
            return Err(DecodeError::UnexpectedType(format!(
                "typeid={} op={} does not decode to struct",
                typeinfo.typeid(),
                typeinfo.op_name()
            )));
        }

        let fields = typeinfo.struct_fields()?;
        let typeinfos = Arc::clone(&self.typeinfos);
        let mut map = BTreeMap::new();
        for field in fields {
            if field.is_parent() {
                let parent_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                if parent_typeinfo.op() == TypeOp::Struct {
                    let parent_map = self.object(parent_typeinfo)?;
                    if fields.len() == 1 {
                        return Ok(parent_map);
                    }
                    for (k, v) in parent_map {
                        map.insert(k, v);
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
                            map.insert(k, v);
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
                        map.insert("__parent".to_string(), other);
                    }
                }
            } else {
                let child_typeinfo =
                    EventPlanCompiler::lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
                map.insert(
                    field.name().to_string(),
                    self.instance_from_typeinfo(child_typeinfo)?,
                );
            }
        }

        Ok(map)
    }

    fn dispatch(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        match typeinfo.op() {
            TypeOp::Array => self.array(typeinfo),
            TypeOp::BitArray => self.bitarray(typeinfo),
            TypeOp::Blob => self.blob(typeinfo),
            TypeOp::Bool => Ok(Value::Bool(self.buffer.read_bool()?)),
            TypeOp::Choice => Ok(Value::Object(self.choice(typeinfo)?)),
            TypeOp::Fourcc => self.fourcc(),
            TypeOp::Int => Ok(Value::Int(self.int(typeinfo.int_bounds()?)?)),
            TypeOp::Null => Ok(Value::Null),
            TypeOp::Optional => self.optional(typeinfo),
            TypeOp::Real32 => self.real32(),
            TypeOp::Real64 => self.real64(),
            TypeOp::Struct => Ok(Value::Object(self.object(typeinfo)?)),
        }
    }
}

impl TypeDecoder for BitPackedDecoder<'_> {
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
                "[bitpacked] typeid={typeid} op={} used_bits={}",
                typeinfo.op_name(),
                self.used_bits(),
                typeid = typeinfo.typeid()
            );
        }

        if typeinfo.op() == TypeOp::Int {
            return Ok(Value::Int(self.int(typeinfo.int_bounds()?)?));
        }

        self.dispatch(typeinfo)
    }

    fn integer_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<i128, DecodeError> {
        match typeinfo.op() {
            TypeOp::Int => self.int(typeinfo.int_bounds()?),
            TypeOp::Choice => {
                let tag = self.int(typeinfo.choice_tag_bounds()?)?;
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
                let exists = self.optional_exists()?;
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
                let exists = self.optional_exists()?;
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
                let exists = self.optional_exists()?;
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
                let length = self.int(typeinfo.length_bounds()?)? as usize;
                let bytes = self.buffer.read_aligned_slice(length)?;
                Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
            }
            TypeOp::Fourcc => {
                let bytes = self.buffer.read_aligned_slice(4)?;
                Ok(Some(String::from_utf8_lossy(bytes).into_owned()))
            }
            TypeOp::Bool => Ok(Some(self.buffer.read_bool()?.to_string())),
            TypeOp::Real32 => Ok(self.real32()?.as_f64().map(|value| value.to_string())),
            TypeOp::Real64 => Ok(self.real64()?.as_f64().map(|value| value.to_string())),
            _ => Ok(Some(self.integer_from_typeinfo(typeinfo)?.to_string())),
        }
    }

    #[inline(always)]
    fn optional_exists_from_typeinfo(&mut self, _typeinfo: &TypeInfo) -> Result<bool, DecodeError> {
        self.optional_exists()
    }

    #[inline(always)]
    fn array_length_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<usize, DecodeError> {
        Ok(self.int(typeinfo.length_bounds()?)? as usize)
    }

    #[inline(always)]
    fn int_i64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<i64>, DecodeError> {
        Ok(i64::try_from(self.int(typeinfo.int_bounds()?)?).ok())
    }

    #[inline(always)]
    fn blob_bytes_from_typeinfo<'data>(
        &'data mut self,
        typeinfo: &TypeInfo,
    ) -> Result<&'data [u8], DecodeError> {
        let length = self.int(typeinfo.length_bounds()?)? as usize;
        self.buffer.read_aligned_slice(length)
    }

    #[inline(always)]
    fn fourcc_bytes_from_typeinfo<'data>(
        &'data mut self,
        _typeinfo: &TypeInfo,
    ) -> Result<&'data [u8], DecodeError> {
        self.buffer.read_aligned_slice(4)
    }

    fn bools_from_bitarray_typeinfo(
        &mut self,
        typeinfo: &TypeInfo,
    ) -> Result<Vec<bool>, DecodeError> {
        self.bitarray_bools(typeinfo)
    }

    fn skip_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<(), DecodeError> {
        self.skip_value(typeinfo)
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
            EventDecodePlan::Ordered(steps) => {
                self.decode_ordered_event_fields(steps.as_ref(), on_field)
            }
            EventDecodePlan::Tagged(_) => Err(DecodeError::UnexpectedType(
                "bitpacked event plan expects ordered struct fields".into(),
            )),
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
            EventDecodePlan::Ordered(steps) => self.skip_ordered_event_fields(steps.as_ref()),
            EventDecodePlan::Tagged(_) => Err(DecodeError::UnexpectedType(
                "bitpacked event plan expects ordered struct fields".into(),
            )),
        }
    }
}

impl HeaderIntegerDecoder for BitPackedDecoder<'_> {
    fn integer_from_plan(&mut self, plan: &IntegerDecodePlan) -> Result<i128, DecodeError> {
        match plan {
            IntegerDecodePlan::Int { bitpacked_bounds } => self.int(*bitpacked_bounds),
            IntegerDecodePlan::Choice {
                bitpacked_tag_bounds,
                fields,
            } => {
                let tag = self.int(*bitpacked_tag_bounds)?;
                let child_plan = fields
                    .get(&tag)
                    .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
                self.integer_from_plan(child_plan)
            }
        }
    }
}

impl BitPackedDecoder<'_> {
    fn read_bits_as_bytes(&mut self, bits: usize) -> Result<Vec<u8>, DecodeError> {
        let mut remaining = bits;
        let mut out = Vec::with_capacity(bits.div_ceil(8));
        let mut current = 0u8;
        let mut current_bits = 0u8;

        while remaining > 0 {
            let bit = u8::from(self.buffer.read_bool()?);
            current = (current << 1) | (bit & 1);
            current_bits += 1;
            remaining -= 1;

            if current_bits == 8 {
                out.push(current);
                current = 0;
                current_bits = 0;
            }
        }

        if current_bits > 0 {
            current <<= 8 - current_bits;
            out.push(current);
        }

        Ok(out)
    }
}
