use crate::bitstream::BitPackedBuffer;
use crate::{
    error::DecodeError,
    events::{GameEvent, MessageEvent, TrackerEvent},
    value::Value,
};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ProtocolDefinition {
    pub build: u32,
    pub typeinfos: Vec<(String, Vec<JsonValue>)>,
    pub game_event_types: Vec<(u32, u32, String)>,
    pub message_event_types: Vec<(u32, u32, String)>,
    pub tracker_event_types: Vec<(u32, u32, String)>,
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
        let mut decoder = BitPackedDecoder::new(contents, &self.typeinfos);
        decode_event_stream(
            &mut decoder,
            Some(self.game_eventid_typeid),
            &self.game_event_types,
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
        let mut decoder = BitPackedDecoder::new(contents, &self.typeinfos);
        decode_event_stream(
            &mut decoder,
            Some(self.message_eventid_typeid),
            &self.message_event_types,
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

        let mut decoder = VersionedDecoder::new(contents, &self.typeinfos);
        decode_event_stream_tolerant(
            &mut decoder,
            Some(eventid_typeid),
            &self.tracker_event_types,
            false,
            self.replay_userid_typeid,
            self.svaruint32_typeid,
        )
        .map(|events| events.into_iter().map(TrackerEvent::from_value).collect())
    }

    pub fn decode_replay_header(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut decoder = VersionedDecoder::new(contents, &self.typeinfos);
        decoder.instance(self.replay_header_typeid)
    }

    pub fn decode_replay_details(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut decoder = VersionedDecoder::new(contents, &self.typeinfos);
        decoder.instance(self.game_details_typeid)
    }

    pub fn decode_replay_initdata(&self, contents: &[u8]) -> Result<Value, DecodeError> {
        let mut decoder = BitPackedDecoder::new(contents, &self.typeinfos);
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
    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError>;
}

pub struct BitPackedDecoder {
    buffer: BitPackedBuffer,
    typeinfos: Vec<(String, Vec<JsonValue>)>,
}

impl BitPackedDecoder {
    pub fn new(contents: &[u8], typeinfos: &[(String, Vec<JsonValue>)]) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos: typeinfos.to_vec(),
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

    fn choice(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        let tag = self.int(&args[0])?;
        let map = args
            .get(1)
            .and_then(|v| v.as_object())
            .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))?;

        for (encoded_tag, value) in map {
            let expected = encoded_tag
                .parse::<i128>()
                .map_err(|_| DecodeError::Corrupted("_choice key".into()))?;
            if expected == tag {
                let field = value
                    .as_array()
                    .ok_or_else(|| DecodeError::Corrupted("_choice value".into()))?;
                let field_name = field
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| DecodeError::Corrupted("_choice field name".into()))?;
                let field_typeid = field
                    .get(1)
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| DecodeError::Corrupted("_choice field typeid".into()))?
                    as usize;

                let value = self.instance(field_typeid)?;
                let mut object = BTreeMap::new();
                object.insert(field_name.to_string(), value);
                return Ok(Value::Object(object));
            }
        }

        Err(DecodeError::Corrupted(format!("invalid choice tag {tag}")))
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

    fn object(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        let fields = args
            .first()
            .and_then(|v| v.as_array())
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))?;

        let mut map = BTreeMap::new();
        for field in fields {
            let field = field
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
                .and_then(|v| v.as_u64())
                .ok_or_else(|| DecodeError::Corrupted("_struct field typeid".into()))?
                as usize;

            if field_name == "__parent" {
                let parent = self.instance(typeid)?;
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
                map.insert(field_name.to_string(), self.instance(typeid)?);
            }
        }

        Ok(Value::Object(map))
    }

    fn dispatch(&mut self, op: &str, args: &[JsonValue]) -> Result<Value, DecodeError> {
        match op {
            "_array" => self.array(args),
            "_bitarray" => self.bitarray(args),
            "_blob" => self.blob(args),
            "_bool" => Ok(Value::Bool(self.buffer.read_bits(1)? != 0)),
            "_choice" => self.choice(args),
            "_fourcc" => self.fourcc(),
            "_int" => Ok(Value::Int(self.int(&args[0])?)),
            "_null" => Ok(Value::Null),
            "_optional" => self.optional(args),
            "_real32" => self.real32(),
            "_real64" => self.real64(),
            "_struct" => self.object(args),
            other => Err(DecodeError::Corrupted(format!(
                "unsupported bitpacked opcode {other}"
            ))),
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

    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError> {
        let (op, args) = self
            .typeinfos
            .get(typeid)
            .cloned()
            .ok_or_else(|| DecodeError::Corrupted(format!("typeid {typeid} out of range")))?;
        if std::env::var("S2_DEBUG_DECODER").is_ok() {
            eprintln!(
                "[bitpacked] typeid={typeid} op={op} used_bits={}",
                self.used_bits()
            );
        }

        if op == "_int" {
            if args.is_empty() {
                return Err(DecodeError::Corrupted("_int args".into()));
            }
            return Ok(Value::Int(self.int(&args[0])?));
        }

        self.dispatch(op.as_str(), &args)
    }
}

pub struct VersionedDecoder {
    buffer: BitPackedBuffer,
    typeinfos: Vec<(String, Vec<JsonValue>)>,
}

impl VersionedDecoder {
    pub fn new(contents: &[u8], typeinfos: &[(String, Vec<JsonValue>)]) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos: typeinfos.to_vec(),
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

    fn choice(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        self.expect_skip(3)?;
        let tag = self.vint()?;
        let map = args
            .get(1)
            .and_then(|v| v.as_object())
            .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))?;

        for (encoded_tag, value) in map {
            let expected = encoded_tag
                .parse::<i128>()
                .map_err(|_| DecodeError::Corrupted("_choice tag".into()))?;
            if expected == tag {
                let tuple = value
                    .as_array()
                    .ok_or_else(|| DecodeError::Corrupted("_choice value".into()))?;
                let field_name = tuple
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| DecodeError::Corrupted("_choice field name".into()))?;
                let field_typeid = tuple
                    .get(1)
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| DecodeError::Corrupted("_choice typeid".into()))?
                    as usize;

                let value = self.instance(field_typeid)?;
                let mut object = BTreeMap::new();
                object.insert(field_name.to_string(), value);
                return Ok(Value::Object(object));
            }
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

    fn object(&mut self, args: &[JsonValue]) -> Result<Value, DecodeError> {
        self.expect_skip(5)?;
        let fields = args
            .first()
            .and_then(|v| v.as_array())
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))?;

        let field_count = self.vint()? as usize;
        let mut result = BTreeMap::new();

        for _ in 0..field_count {
            let tag = self.vint()?;
            let mut parsed = None;

            for field in fields {
                let tuple = field
                    .as_array()
                    .ok_or_else(|| DecodeError::Corrupted("_struct field".into()))?;
                if tuple.len() < 3 {
                    return Err(DecodeError::Corrupted("_struct field tuple".into()));
                }

                let field_tag = tuple[2]
                    .as_i64()
                    .or_else(|| tuple[2].as_u64().map(|v| v as i64))
                    .ok_or_else(|| DecodeError::Corrupted("_struct field tag".into()))?;

                if field_tag as i128 == tag {
                    let field_name = tuple
                        .first()
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| DecodeError::Corrupted("_struct field name".into()))?;
                    let typeid = tuple
                        .get(1)
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| DecodeError::Corrupted("_struct field typeid".into()))?
                        as usize;

                    parsed = Some((field_name.to_string(), typeid));
                    break;
                }
            }

            let (field_name, typeid) = match parsed {
                Some(v) => v,
                None => {
                    self.skip_instance()?;
                    continue;
                }
            };

            if field_name == "__parent" {
                let parent = self.instance(typeid)?;
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
                result.insert(field_name, self.instance(typeid)?);
            }
        }

        Ok(Value::Object(result))
    }

    fn dispatch(&mut self, op: &str, args: &[JsonValue]) -> Result<Value, DecodeError> {
        match op {
            "_array" => self.array(args),
            "_bitarray" => self.bitarray(args),
            "_blob" => self.blob(args),
            "_bool" => self.bool(),
            "_choice" => self.choice(args),
            "_fourcc" => self.fourcc(),
            "_int" => self.int(),
            "_null" => Ok(Value::Null),
            "_optional" => self.optional(args),
            "_real32" => self.real32(),
            "_real64" => self.real64(),
            "_struct" => self.object(args),
            other => Err(DecodeError::Corrupted(format!(
                "unsupported versioned opcode {other}"
            ))),
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

    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError> {
        let (op, args) = self
            .typeinfos
            .get(typeid)
            .cloned()
            .ok_or_else(|| DecodeError::Corrupted(format!("typeid {typeid} out of range")))?;
        if std::env::var("S2_DEBUG_DECODER").is_ok() {
            eprintln!(
                "[versioned] typeid={typeid} op={op} used_bits={}",
                self.used_bits()
            );
        }

        let used_bits = self.used_bits();
        self.dispatch(op.as_str(), &args)
            .map_err(|error| match error {
                DecodeError::Corrupted(message) => DecodeError::Corrupted(format!(
                    "typeid={typeid} op={op} used_bits={used_bits}: {message}"
                )),
                DecodeError::Truncated => DecodeError::Corrupted(format!(
                    "typeid={typeid} op={op} used_bits={used_bits}: buffer truncated"
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
    eventid_typeid: Option<usize>,
    event_types: &[(u32, u32, String)],
    decode_user_id: bool,
    replay_userid_typeid: Option<usize>,
    svaruint32_typeid: usize,
    tolerant: bool,
) -> Result<Vec<Value>, DecodeError> {
    let Some(eventid_typeid) = eventid_typeid else {
        return Ok(Vec::new());
    };

    let mut events = Vec::new();
    let mut gameloop = 0i128;

    while !decoder.done() {
        let start_bits = decoder.used_bits();

        let event_result = (|| -> Result<Value, DecodeError> {
            let delta = decode_varuint_value(&decoder.instance(svaruint32_typeid)?)?;
            gameloop += delta;

            let userid = if decode_user_id {
                replay_userid_typeid
                    .map(|typeid| decoder.instance(typeid))
                    .transpose()?
            } else {
                None
            };

            let eventid_raw = decoder.instance(eventid_typeid)?;
            let eventid = eventid_raw
                .as_i128()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| DecodeError::Corrupted("invalid event id".into()))?;

            let (typeid, name) = event_types
                .iter()
                .find(|(id, _, _)| *id == eventid)
                .map(|(_, typeid, name)| (*typeid, name.clone()))
                .ok_or_else(|| DecodeError::Corrupted(format!("eventid({eventid}) unknown")))?;

            let mut event = decoder.instance(typeid as usize)?;
            match &mut event {
                Value::Object(map) => {
                    map.insert("_event".to_string(), Value::String(name));
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
                    map.insert("_event".to_string(), Value::String(name));
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
    eventid_typeid: Option<usize>,
    event_types: &[(u32, u32, String)],
    decode_user_id: bool,
    replay_userid_typeid: Option<usize>,
    svaruint32_typeid: usize,
) -> Result<Vec<Value>, DecodeError> {
    decode_event_stream(
        decoder,
        eventid_typeid,
        event_types,
        decode_user_id,
        replay_userid_typeid,
        svaruint32_typeid,
        true,
    )
}
