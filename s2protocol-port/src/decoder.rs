use crate::bitstream::BitPackedBuffer;
use crate::{
    error::DecodeError,
    events::{GameEvent, MessageEvent, TrackerEvent},
    value::Value,
};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeOp {
    Array,
    BitArray,
    Blob,
    Bool,
    Choice,
    Fourcc,
    Int,
    Null,
    Optional,
    Real32,
    Real64,
    Struct,
}

impl TypeOp {
    fn parse(op_name: &str) -> Result<Self, DecodeError> {
        match op_name {
            "_array" => Ok(Self::Array),
            "_bitarray" => Ok(Self::BitArray),
            "_blob" => Ok(Self::Blob),
            "_bool" => Ok(Self::Bool),
            "_choice" => Ok(Self::Choice),
            "_fourcc" => Ok(Self::Fourcc),
            "_int" => Ok(Self::Int),
            "_null" => Ok(Self::Null),
            "_optional" => Ok(Self::Optional),
            "_real32" => Ok(Self::Real32),
            "_real64" => Ok(Self::Real64),
            "_struct" => Ok(Self::Struct),
            other => Err(DecodeError::Json(format!(
                "unsupported typeinfo opcode {other}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Array => "_array",
            Self::BitArray => "_bitarray",
            Self::Blob => "_blob",
            Self::Bool => "_bool",
            Self::Choice => "_choice",
            Self::Fourcc => "_fourcc",
            Self::Int => "_int",
            Self::Null => "_null",
            Self::Optional => "_optional",
            Self::Real32 => "_real32",
            Self::Real64 => "_real64",
            Self::Struct => "_struct",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TypeInfo {
    op: TypeOp,
    args: Arc<[JsonValue]>,
    choice_fields: Option<Arc<BTreeMap<i128, ChoiceField>>>,
    struct_fields: Option<Arc<[StructField]>>,
    struct_fields_by_tag: Option<Arc<BTreeMap<i128, StructField>>>,
}

impl TypeInfo {
    pub(crate) fn new(op_name: &str, args: Vec<JsonValue>) -> Result<Self, DecodeError> {
        let op = TypeOp::parse(op_name)?;
        let choice_fields = if op == TypeOp::Choice {
            Some(Arc::new(parse_choice_fields(&args)?))
        } else {
            None
        };
        let struct_fields = if op == TypeOp::Struct {
            Some(Arc::from(parse_struct_fields(&args)?))
        } else {
            None
        };
        let struct_fields_by_tag = struct_fields
            .as_deref()
            .map(build_struct_field_tag_lookup)
            .transpose()?
            .map(Arc::new);

        Ok(Self {
            op,
            args: Arc::from(args),
            choice_fields,
            struct_fields,
            struct_fields_by_tag,
        })
    }

    fn op(&self) -> TypeOp {
        self.op
    }

    fn op_name(&self) -> &'static str {
        self.op.as_str()
    }

    fn args(&self) -> &[JsonValue] {
        &self.args
    }

    fn choice_fields(&self) -> Result<&BTreeMap<i128, ChoiceField>, DecodeError> {
        self.choice_fields
            .as_deref()
            .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))
    }

    fn struct_fields(&self) -> Result<&[StructField], DecodeError> {
        self.struct_fields
            .as_deref()
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))
    }

    fn struct_fields_by_tag(&self) -> Result<&BTreeMap<i128, StructField>, DecodeError> {
        self.struct_fields_by_tag
            .as_deref()
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))
    }
}

#[derive(Debug, Clone)]
struct ChoiceField {
    name: Arc<str>,
    typeid: usize,
}

impl ChoiceField {
    fn new(name: String, typeid: usize) -> Self {
        Self {
            name: Arc::<str>::from(name),
            typeid,
        }
    }

    fn name(&self) -> &str {
        self.name.as_ref()
    }

    fn typeid(&self) -> usize {
        self.typeid
    }
}

#[derive(Debug, Clone)]
struct StructField {
    name: Arc<str>,
    typeid: usize,
    tag: Option<i128>,
    is_parent: bool,
}

impl StructField {
    fn new(name: String, typeid: usize, tag: Option<i128>) -> Self {
        let is_parent = name == "__parent";
        Self {
            name: Arc::<str>::from(name),
            typeid,
            tag,
            is_parent,
        }
    }

    fn name(&self) -> &str {
        self.name.as_ref()
    }

    fn typeid(&self) -> usize {
        self.typeid
    }

    fn is_parent(&self) -> bool {
        self.is_parent
    }
}

fn parse_choice_fields(args: &[JsonValue]) -> Result<BTreeMap<i128, ChoiceField>, DecodeError> {
    let map = args
        .get(1)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))?;

    map.iter()
        .map(|(tag, field)| {
            let parsed_tag = tag
                .parse::<i128>()
                .map_err(|_| DecodeError::Corrupted("_choice key".into()))?;
            let choice_field = parse_choice_field(field)?;
            Ok((parsed_tag, choice_field))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
}

fn parse_choice_field(value: &JsonValue) -> Result<ChoiceField, DecodeError> {
    let field = value
        .as_array()
        .ok_or_else(|| DecodeError::Corrupted("_choice value".into()))?;

    let field_name = field
        .first()
        .and_then(JsonValue::as_str)
        .ok_or_else(|| DecodeError::Corrupted("_choice field name".into()))?;
    let typeid = field
        .get(1)
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| DecodeError::Corrupted("_choice field typeid".into()))?
        as usize;

    Ok(ChoiceField::new(field_name.to_string(), typeid))
}

fn parse_struct_fields(args: &[JsonValue]) -> Result<Vec<StructField>, DecodeError> {
    let fields = args
        .first()
        .and_then(JsonValue::as_array)
        .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))?;

    fields
        .iter()
        .map(parse_struct_field)
        .collect::<Result<Vec<_>, _>>()
}

fn build_struct_field_tag_lookup(
    fields: &[StructField],
) -> Result<BTreeMap<i128, StructField>, DecodeError> {
    let mut field_map = BTreeMap::new();

    for field in fields {
        let Some(tag) = field.tag else {
            continue;
        };

        if field_map.insert(tag, field.clone()).is_some() {
            return Err(DecodeError::Corrupted(format!(
                "duplicate _struct tag {tag}"
            )));
        }
    }

    Ok(field_map)
}

fn parse_struct_field(value: &JsonValue) -> Result<StructField, DecodeError> {
    let field = value
        .as_array()
        .ok_or_else(|| DecodeError::Corrupted("_struct field".into()))?;

    if field.len() < 2 {
        return Err(DecodeError::Corrupted("_struct field len".into()));
    }

    let field_name = field
        .first()
        .and_then(JsonValue::as_str)
        .ok_or_else(|| DecodeError::Corrupted("_struct field name".into()))?;
    let typeid = field
        .get(1)
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| DecodeError::Corrupted("_struct field typeid".into()))?
        as usize;
    let tag = field.get(2).map(json_to_i128).transpose()?;

    Ok(StructField::new(field_name.to_string(), typeid, tag))
}

fn json_to_i128(value: &JsonValue) -> Result<i128, DecodeError> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
        .ok_or_else(|| DecodeError::Corrupted("expected integer json value".into()))
}

#[derive(Debug, Clone)]
pub(crate) struct EventTypeInfo {
    typeid: usize,
    typeinfo: TypeInfo,
    name: Arc<str>,
}

impl EventTypeInfo {
    pub(crate) fn new(typeid: usize, typeinfo: TypeInfo, name: String) -> Self {
        Self {
            typeid,
            typeinfo,
            name: Arc::<str>::from(name),
        }
    }

    fn typeid(&self) -> usize {
        self.typeid
    }

    fn typeinfo(&self) -> &TypeInfo {
        &self.typeinfo
    }

    fn name(&self) -> &str {
        self.name.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct ProtocolDefinition {
    pub build: u32,
    pub(crate) typeinfos: Arc<[TypeInfo]>,
    pub game_event_types: Vec<(u32, u32, String)>,
    pub message_event_types: Vec<(u32, u32, String)>,
    pub tracker_event_types: Vec<(u32, u32, String)>,
    pub(crate) game_event_typeinfos: Arc<[Option<EventTypeInfo>]>,
    pub(crate) message_event_typeinfos: Arc<[Option<EventTypeInfo>]>,
    pub(crate) tracker_event_typeinfos: Arc<[Option<EventTypeInfo>]>,
    pub game_eventid_typeid: usize,
    pub message_eventid_typeid: usize,
    pub tracker_eventid_typeid: Option<usize>,
    pub svaruint32_typeid: usize,
    pub replay_userid_typeid: Option<usize>,
    pub replay_header_typeid: usize,
    pub game_details_typeid: usize,
    pub replay_initdata_typeid: usize,
}

impl ProtocolDefinition {
    pub fn decode_replay_game_events(
        &self,
        contents: &[u8],
    ) -> Result<Vec<GameEvent>, DecodeError> {
        let mut decoder = BitPackedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decode_event_stream(
            &mut decoder,
            &self.game_event_typeinfos,
            Some(self.game_eventid_typeid),
            true,
            self.replay_userid_typeid,
            self.svaruint32_typeid,
            false,
        )
        .map(|events| events.into_iter().map(GameEvent::from_value).collect())
    }

    pub fn decode_replay_message_events(
        &self,
        contents: &[u8],
    ) -> Result<Vec<MessageEvent>, DecodeError> {
        let mut decoder = BitPackedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decode_event_stream(
            &mut decoder,
            &self.message_event_typeinfos,
            Some(self.message_eventid_typeid),
            true,
            self.replay_userid_typeid,
            self.svaruint32_typeid,
            false,
        )
        .map(|events| events.into_iter().map(MessageEvent::from_value).collect())
    }

    pub fn decode_replay_tracker_events(
        &self,
        contents: &[u8],
    ) -> Result<Vec<TrackerEvent>, DecodeError> {
        let Some(eventid_typeid) = self.tracker_eventid_typeid else {
            return Ok(Vec::new());
        };

        let mut decoder = VersionedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decode_event_stream_tolerant(
            &mut decoder,
            &self.tracker_event_typeinfos,
            Some(eventid_typeid),
            false,
            self.replay_userid_typeid,
            self.svaruint32_typeid,
        )
        .map(|events| events.into_iter().map(TrackerEvent::from_value).collect())
    }

    pub fn decode_replay_header(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut decoder = VersionedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decoder.instance(self.replay_header_typeid)
    }

    pub fn decode_replay_details(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut decoder = VersionedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decoder.instance(self.game_details_typeid)
    }

    pub fn decode_replay_initdata(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut decoder = BitPackedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decoder.instance(self.replay_initdata_typeid)
    }

    pub fn decode_replay_attributes_events(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut buffer = BitPackedBuffer::new(contents, false);
        let mut object = BTreeMap::new();

        if buffer.done() {
            return Ok(Value::Object(object));
        }

        object.insert(
            "source".to_string(),
            Value::Int(buffer.read_bits(8)? as i128),
        );
        object.insert(
            "mapNamespace".to_string(),
            Value::Int(buffer.read_bits(32)? as i128),
        );
        object.insert(
            "count".to_string(),
            Value::Int(buffer.read_bits(32)? as i128),
        );

        let mut scopes: BTreeMap<String, Value> = BTreeMap::new();
        while !buffer.done() {
            let namespace = buffer.read_bits(32)?;
            let attrid = buffer.read_bits(32)?;
            let scope = buffer.read_bits(8)?;
            let raw = buffer.read_aligned_bytes(4)?;

            let mut value_bytes = raw.into_iter().rev().collect::<Vec<u8>>();
            while let Some(0) = value_bytes.last().copied() {
                value_bytes.pop();
            }

            let scope_key = scope.to_string();
            let attr_key = attrid.to_string();
            let scope_entry = scopes
                .entry(scope_key)
                .or_insert_with(|| Value::Object(BTreeMap::new()));

            let scope_map = match scope_entry {
                Value::Object(map) => map,
                _ => {
                    return Err(DecodeError::Corrupted("invalid attributes scope".into()));
                }
            };

            let list = scope_map
                .entry(attr_key)
                .or_insert_with(|| Value::Array(Vec::new()));
            let list = match list {
                Value::Array(values) => values,
                _ => {
                    return Err(DecodeError::Corrupted("invalid attributes payload".into()));
                }
            };

            let mut item = BTreeMap::new();
            item.insert("namespace".to_string(), Value::Int(namespace as i128));
            item.insert("attrid".to_string(), Value::Int(attrid as i128));
            item.insert("scope".to_string(), Value::Int(scope as i128));
            item.insert("value".to_string(), Value::Bytes(value_bytes));
            list.push(Value::Object(item));
        }

        object.insert("scopes".to_string(), Value::Object(scopes));
        Ok(Value::Object(object))
    }
}

pub trait TypeDecoder {
    fn done(&self) -> bool;
    fn used_bits(&self) -> usize;
    fn byte_align(&mut self);
    fn typeinfos(&self) -> Arc<[TypeInfo]>;
    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError>;
    fn instance_from_typeinfo(
        &mut self,
        typeid: usize,
        typeinfo: &TypeInfo,
    ) -> Result<Value, DecodeError>;
}

pub struct BitPackedDecoder {
    buffer: BitPackedBuffer,
    typeinfos: Arc<[TypeInfo]>,
}

fn lookup_typeinfo(typeinfos: &[TypeInfo], typeid: usize) -> Result<&TypeInfo, DecodeError> {
    typeinfos
        .get(typeid)
        .ok_or_else(|| DecodeError::Corrupted(format!("typeid {typeid} out of range")))
}

impl BitPackedDecoder {
    pub fn new(contents: &[u8], typeinfos: Arc<[TypeInfo]>) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos,
        }
    }

    fn int(&mut self, bounds: &JsonValue) -> Result<i128, DecodeError> {
        let bounds = bounds
            .as_array()
            .ok_or_else(|| DecodeError::Corrupted("_int bounds".into()))?;
        if bounds.len() != 2 {
            return Err(DecodeError::Corrupted("_int bounds len".into()));
        }

        let min = bounds[0]
            .as_i64()
            .or_else(|| bounds[0].as_u64().map(|v| v as i64))
            .ok_or_else(|| DecodeError::Corrupted("_int bounds min".into()))?;
        let bits = bounds[1]
            .as_u64()
            .ok_or_else(|| DecodeError::Corrupted("_int bounds bits".into()))?;

        let raw = self.buffer.read_bits(bits as usize)? as i128;
        Ok(min as i128 + raw)
    }

    fn bool_value(&mut self, bounds: &JsonValue) -> Result<bool, DecodeError> {
        Ok(self.int(bounds)? != 0)
    }

    fn array(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        if args.len() != 2 {
            return Err(DecodeError::Corrupted("_array args".into()));
        }
        let length = self.int(&args[0])? as usize;
        let typeid = args[1]
            .as_u64()
            .ok_or_else(|| DecodeError::Corrupted("_array typeid".into()))?
            as usize;

        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(self.instance(typeid)?);
        }
        Ok(Value::Array(values))
    }

    fn bitarray(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        let length = self.int(&args[0])? as usize;
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

    fn blob(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        let length = self.int(&args[0])? as usize;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(length)?))
    }

    fn choice(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let args = typeinfo.args();
        let bounds = args
            .first()
            .ok_or_else(|| DecodeError::Corrupted("_choice bounds".into()))?;
        let tag = self.int(bounds)?;
        let field = typeinfo
            .choice_fields()?
            .get(&tag)
            .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;

        let value = self.instance(field.typeid())?;
        let mut object = BTreeMap::new();
        object.insert(field.name().to_string(), value);
        Ok(Value::Object(object))
    }

    fn fourcc(&mut self) -> Result<Value, DecodeError> {
        let bytes = self.buffer.read_aligned_bytes(4)?;
        Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
    }

    fn optional(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        let exists = self.bool_value(&JsonValue::Array(vec![
            JsonValue::from(0),
            JsonValue::from(1),
        ]))?;
        if exists {
            let typeid = args
                .first()
                .and_then(|v| v.as_u64())
                .ok_or_else(|| DecodeError::Corrupted("_optional typeid".into()))?
                as usize;
            self.instance(typeid)
        } else {
            Ok(Value::Null)
        }
    }

    fn real32(&mut self) -> Result<Value, DecodeError> {
        let bytes = self.buffer.read_unaligned_bytes(4)?;
        let bits = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Ok(Value::Float(f32::from_bits(bits) as f64))
    }

    fn real64(&mut self) -> Result<Value, DecodeError> {
        let bytes = self.buffer.read_unaligned_bytes(8)?;
        let bits = u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(Value::Float(f64::from_bits(bits)))
    }

    fn object(&mut self, fields: &[StructField]) -> Result<Value, DecodeError> {
        let mut map = BTreeMap::new();
        for field in fields {
            if field.is_parent() {
                let parent = self.instance(field.typeid())?;
                match parent {
                    Value::Object(parent_map) => {
                        if fields.len() == 1 {
                            return Ok(Value::Object(parent_map));
                        }
                        for (k, v) in parent_map {
                            map.insert(k, v);
                        }
                    }
                    _ => {
                        if fields.len() == 1 {
                            return Ok(parent);
                        }
                        map.insert("__parent".to_string(), parent);
                    }
                }
            } else {
                map.insert(field.name().to_string(), self.instance(field.typeid())?);
            }
        }

        Ok(Value::Object(map))
    }

    fn dispatch(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let args = typeinfo.args();
        match typeinfo.op() {
            TypeOp::Array => self.array(args),
            TypeOp::BitArray => self.bitarray(args),
            TypeOp::Blob => self.blob(args),
            TypeOp::Bool => Ok(Value::Bool(self.buffer.read_bits(1)? != 0)),
            TypeOp::Choice => self.choice(typeinfo),
            TypeOp::Fourcc => self.fourcc(),
            TypeOp::Int => Ok(Value::Int(self.int(&args[0])?)),
            TypeOp::Null => Ok(Value::Null),
            TypeOp::Optional => self.optional(args),
            TypeOp::Real32 => self.real32(),
            TypeOp::Real64 => self.real64(),
            TypeOp::Struct => self.object(typeinfo.struct_fields()?),
        }
    }
}

impl TypeDecoder for BitPackedDecoder {
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

    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError> {
        let typeinfos = Arc::clone(&self.typeinfos);
        let typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
        self.instance_from_typeinfo(typeid, typeinfo)
    }

    fn instance_from_typeinfo(
        &mut self,
        typeid: usize,
        typeinfo: &TypeInfo,
    ) -> Result<Value, DecodeError> {
        if std::env::var("S2_DEBUG_DECODER").is_ok() {
            eprintln!(
                "[bitpacked] typeid={typeid} op={} used_bits={}",
                typeinfo.op_name(),
                self.used_bits()
            );
        }

        if typeinfo.op() == TypeOp::Int {
            let args = typeinfo.args();
            if args.is_empty() {
                return Err(DecodeError::Corrupted("_int args".into()));
            }
            return Ok(Value::Int(self.int(&args[0])?));
        }

        self.dispatch(typeinfo)
    }
}

pub struct VersionedDecoder {
    buffer: BitPackedBuffer,
    typeinfos: Arc<[TypeInfo]>,
}

impl VersionedDecoder {
    pub fn new(contents: &[u8], typeinfos: Arc<[TypeInfo]>) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos,
        }
    }

    fn expect_skip(&mut self, expected: u8) -> Result<(), DecodeError> {
        let marker = self.buffer.read_bits(8)? as u8;
        if marker != expected {
            Err(DecodeError::Corrupted(format!(
                "unexpected versioned skip marker expected {expected} got {marker}"
            )))
        } else {
            Ok(())
        }
    }

    fn vint(&mut self) -> Result<i128, DecodeError> {
        let mut b = self.buffer.read_bits(8)? as u8;
        let negative = (b & 1) != 0;
        let mut value: i128 = ((u16::from(b) >> 1) & 0x3f) as i128;
        let mut shift = 6;

        while (b & 0x80) != 0 {
            b = self.buffer.read_bits(8)? as u8;
            value |= ((u16::from(b) & 0x7f) as i128) << shift;
            shift += 7;
        }

        if negative {
            Ok(-value)
        } else {
            Ok(value)
        }
    }

    fn skip_instance(&mut self) -> Result<(), DecodeError> {
        let skip = self.buffer.read_bits(8)? as u8;
        match skip {
            0 => {
                let length = self.vint()? as usize;
                for _ in 0..length {
                    self.skip_instance()?;
                }
            }
            1 => {
                let bits = self.vint()? as usize;
                let bytes = (bits + 7) / 8;
                self.buffer.read_aligned_bytes(bytes)?;
            }
            2 => {
                let bytes = self.vint()? as usize;
                self.buffer.read_aligned_bytes(bytes)?;
            }
            3 => {
                let _ = self.vint()?;
                self.skip_instance()?;
            }
            4 => {
                let exists = self.buffer.read_bits(8)? != 0;
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
                self.buffer.read_aligned_bytes(1)?;
            }
            7 => {
                self.buffer.read_aligned_bytes(4)?;
            }
            8 => {
                self.buffer.read_aligned_bytes(8)?;
            }
            9 => {
                let _ = self.vint()?;
            }
            _ => {
                return Err(DecodeError::Corrupted(format!(
                    "invalid skip marker {skip}"
                )))
            }
        }
        Ok(())
    }

    fn array(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        self.expect_skip(0)?;
        if args.len() != 2 {
            return Err(DecodeError::Corrupted("_array args".into()));
        }
        let length = self.vint()? as usize;
        let typeid = args[1]
            .as_u64()
            .ok_or_else(|| DecodeError::Corrupted("_array typeid".into()))?
            as usize;

        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(self.instance(typeid)?);
        }
        Ok(Value::Array(values))
    }

    fn bitarray(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        self.expect_skip(1)?;
        let _ = args;
        let length = self.vint()? as usize;
        let bytes = (length + 7) / 8;
        let raw = self.buffer.read_aligned_bytes(bytes)?;

        if length > 127 {
            return Ok(Value::Array(vec![
                Value::Int(length as i128),
                Value::Bytes(raw),
            ]));
        }

        let mut value: i128 = 0;
        for b in raw {
            value = (value << 8) | b as i128;
        }
        Ok(Value::Array(vec![
            Value::Int(length as i128),
            Value::Int(value),
        ]))
    }

    fn blob(&mut self, _args: &[JsonValue]) -> Result<Value, DecodeError> {
        self.expect_skip(2)?;
        let length = self.vint()? as usize;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(length)?))
    }

    fn bool(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(6)?;
        Ok(Value::Bool(self.buffer.read_bits(8)? != 0))
    }

    fn choice(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(3)?;
        let tag = self.vint()?;
        if let Some(field) = typeinfo.choice_fields()?.get(&tag) {
            let value = self.instance(field.typeid())?;
            let mut object = BTreeMap::new();
            object.insert(field.name().to_string(), value);
            return Ok(Value::Object(object));
        }

        self.skip_instance()?;
        Ok(Value::Object(BTreeMap::new()))
    }

    fn fourcc(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(7)?;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(4)?))
    }

    fn int(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(9)?;
        Ok(Value::Int(self.vint()?))
    }

    fn optional(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        self.expect_skip(4)?;
        let exists = self.buffer.read_bits(8)? != 0;
        if !exists {
            return Ok(Value::Null);
        }

        let typeid = args
            .first()
            .and_then(|v| v.as_u64())
            .ok_or_else(|| DecodeError::Corrupted("_optional typeid".into()))?
            as usize;
        self.instance(typeid)
    }

    fn real32(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(7)?;
        let bytes = self.buffer.read_aligned_bytes(4)?;
        let bits = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Ok(Value::Float(f32::from_bits(bits) as f64))
    }

    fn real64(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(8)?;
        let bytes = self.buffer.read_aligned_bytes(8)?;
        let bits = u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(Value::Float(f64::from_bits(bits)))
    }

    fn object(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(5)?;
        let fields = typeinfo.struct_fields()?;
        let field_map = typeinfo.struct_fields_by_tag()?;
        let field_count = self.vint()? as usize;
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
                let parent = self.instance(field.typeid())?;
                match parent {
                    Value::Object(parent_map) => {
                        if fields.len() == 1 {
                            return Ok(Value::Object(parent_map));
                        }
                        for (k, v) in parent_map {
                            result.insert(k, v);
                        }
                    }
                    _ => {
                        if fields.len() == 1 {
                            return Ok(parent);
                        }
                        result.insert("__parent".to_string(), parent);
                    }
                }
            } else {
                result.insert(field.name().to_string(), self.instance(field.typeid())?);
            }
        }

        Ok(Value::Object(result))
    }

    fn dispatch(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let args = typeinfo.args();
        match typeinfo.op() {
            TypeOp::Array => self.array(args),
            TypeOp::BitArray => self.bitarray(args),
            TypeOp::Blob => self.blob(args),
            TypeOp::Bool => self.bool(),
            TypeOp::Choice => self.choice(typeinfo),
            TypeOp::Fourcc => self.fourcc(),
            TypeOp::Int => self.int(),
            TypeOp::Null => Ok(Value::Null),
            TypeOp::Optional => self.optional(args),
            TypeOp::Real32 => self.real32(),
            TypeOp::Real64 => self.real64(),
            TypeOp::Struct => self.object(typeinfo),
        }
    }
}

impl BitPackedDecoder {
    fn read_bits_as_bytes(&mut self, bits: usize) -> Result<Vec<u8>, DecodeError> {
        let mut remaining = bits;
        let mut out = Vec::with_capacity((bits + 7) / 8);
        let mut current = 0u8;
        let mut current_bits = 0u8;

        while remaining > 0 {
            let bit = self.buffer.read_bits(1)? as u8;
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

impl TypeDecoder for VersionedDecoder {
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

    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError> {
        let typeinfos = Arc::clone(&self.typeinfos);
        let typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
        self.instance_from_typeinfo(typeid, typeinfo)
    }

    fn instance_from_typeinfo(
        &mut self,
        typeid: usize,
        typeinfo: &TypeInfo,
    ) -> Result<Value, DecodeError> {
        if std::env::var("S2_DEBUG_DECODER").is_ok() {
            eprintln!(
                "[versioned] typeid={typeid} op={} used_bits={}",
                typeinfo.op_name(),
                self.used_bits()
            );
        }

        let used_bits = self.used_bits();
        self.dispatch(typeinfo).map_err(|error| match error {
            DecodeError::Corrupted(message) => DecodeError::Corrupted(format!(
                "typeid={typeid} op={} used_bits={used_bits}: {message}",
                typeinfo.op_name()
            )),
            DecodeError::Truncated => DecodeError::Corrupted(format!(
                "typeid={typeid} op={} used_bits={used_bits}: buffer truncated",
                typeinfo.op_name()
            )),
            other => other,
        })
    }
}

pub fn decode_varuint_value(value: &Value) -> Result<i128, DecodeError> {
    match value {
        Value::Int(v) => Ok(*v),
        Value::Object(map) => map
            .values()
            .next()
            .and_then(|v| v.as_i128())
            .ok_or_else(|| DecodeError::Corrupted("invalid svaruint object".into())),
        _ => Err(DecodeError::Corrupted("invalid svaruint type".into())),
    }
}

fn decode_event_stream<D: TypeDecoder>(
    decoder: &mut D,
    event_typeinfos: &[Option<EventTypeInfo>],
    eventid_typeid: Option<usize>,
    decode_user_id: bool,
    replay_userid_typeid: Option<usize>,
    svaruint32_typeid: usize,
    tolerant: bool,
) -> Result<Vec<Value>, DecodeError> {
    let Some(eventid_typeid) = eventid_typeid else {
        return Ok(Vec::new());
    };

    let typeinfos = decoder.typeinfos();
    let svaruint32_typeinfo = lookup_typeinfo(typeinfos.as_ref(), svaruint32_typeid)?;
    let replay_userid_typeinfo = replay_userid_typeid
        .map(|typeid| {
            lookup_typeinfo(typeinfos.as_ref(), typeid).map(|typeinfo| (typeid, typeinfo))
        })
        .transpose()?;
    let eventid_typeinfo = lookup_typeinfo(typeinfos.as_ref(), eventid_typeid)?;

    let mut events = Vec::new();
    let mut gameloop = 0i128;

    while !decoder.done() {
        let start_bits = decoder.used_bits();

        let event_result = (|| -> Result<Value, DecodeError> {
            let delta = decode_varuint_value(
                &decoder.instance_from_typeinfo(svaruint32_typeid, svaruint32_typeinfo)?,
            )?;
            gameloop += delta;

            let userid = if decode_user_id {
                replay_userid_typeinfo
                    .as_ref()
                    .map(|(typeid, typeinfo)| decoder.instance_from_typeinfo(*typeid, typeinfo))
                    .transpose()?
            } else {
                None
            };

            let eventid_raw = decoder.instance_from_typeinfo(eventid_typeid, eventid_typeinfo)?;
            let eventid = eventid_raw
                .as_i128()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| DecodeError::Corrupted("invalid event id".into()))?;

            let event_typeinfo = usize::try_from(eventid)
                .ok()
                .and_then(|index| event_typeinfos.get(index))
                .and_then(|value| value.as_ref())
                .ok_or_else(|| DecodeError::Corrupted(format!("eventid({eventid}) unknown")))?;

            let mut event = decoder
                .instance_from_typeinfo(event_typeinfo.typeid(), event_typeinfo.typeinfo())?;
            match &mut event {
                Value::Object(map) => {
                    map.insert(
                        "_event".to_string(),
                        Value::String(event_typeinfo.name().to_string()),
                    );
                    map.insert("_eventid".to_string(), Value::Int(eventid as i128));
                    map.insert("_gameloop".to_string(), Value::Int(gameloop));
                    if let Some(userid) = userid {
                        map.insert("_userid".to_string(), userid);
                    }
                    map.insert(
                        "_bits".to_string(),
                        Value::Int((decoder.used_bits() - start_bits) as i128),
                    );
                }
                _ => {
                    let mut map = BTreeMap::new();
                    map.insert(
                        "_event".to_string(),
                        Value::String(event_typeinfo.name().to_string()),
                    );
                    map.insert("_eventid".to_string(), Value::Int(eventid as i128));
                    map.insert("_gameloop".to_string(), Value::Int(gameloop));
                    if let Some(userid) = userid {
                        map.insert("_userid".to_string(), userid);
                    }
                    map.insert(
                        "_bits".to_string(),
                        Value::Int((decoder.used_bits() - start_bits) as i128),
                    );
                    map.insert("event".to_string(), event);
                    event = Value::Object(map);
                }
            }

            Ok(event)
        })();

        let event = match event_result {
            Ok(event) => event,
            Err(error) => {
                if tolerant && !events.is_empty() {
                    return Ok(events);
                } else {
                    return Err(error);
                }
            }
        };

        decoder.byte_align();
        events.push(event);
    }

    Ok(events)
}

fn decode_event_stream_tolerant<D: TypeDecoder>(
    decoder: &mut D,
    event_typeinfos: &[Option<EventTypeInfo>],
    eventid_typeid: Option<usize>,
    decode_user_id: bool,
    replay_userid_typeid: Option<usize>,
    svaruint32_typeid: usize,
) -> Result<Vec<Value>, DecodeError> {
    decode_event_stream(
        decoder,
        event_typeinfos,
        eventid_typeid,
        decode_user_id,
        replay_userid_typeid,
        svaruint32_typeid,
        true,
    )
}
