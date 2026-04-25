use crate::bitstream::BitPackedBuffer;
use crate::{
    error::DecodeError,
    events::{
        decode_user_id as decode_event_user_id, AbilityData, CmdEventData, DirectEventDecode,
        GameEvent, GameEventField, MessageEvent, MessageEventField, PlayerStatsData, ReplayEvent,
        SnapshotPoint, SnapshotPointValue, TargetUnitData, TrackerEvent, TrackerEventField,
        TriggerEventData,
    },
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

#[derive(Debug, Clone, Copy)]
struct IntBounds {
    min: i64,
    bits: usize,
}

#[derive(Debug, Clone)]
enum TagLookup<T> {
    Dense {
        min_tag: i128,
        entries: Arc<[Option<T>]>,
    },
    Sparse(Arc<BTreeMap<i128, T>>),
}

impl<T> TagLookup<T> {
    fn get(&self, tag: &i128) -> Option<&T> {
        match self {
            Self::Dense { min_tag, entries } => {
                let offset = tag.checked_sub(*min_tag)?;
                let index = usize::try_from(offset).ok()?;
                entries.get(index).and_then(Option::as_ref)
            }
            Self::Sparse(map) => map.get(tag),
        }
    }

    fn visit<F>(&self, mut visitor: F) -> Result<(), DecodeError>
    where
        F: FnMut(i128, &T) -> Result<(), DecodeError>,
    {
        match self {
            Self::Dense { min_tag, entries } => {
                for (index, entry) in entries.iter().enumerate() {
                    if let Some(value) = entry {
                        let tag = min_tag
                            .checked_add(index as i128)
                            .ok_or_else(|| DecodeError::Corrupted("tag out of range".into()))?;
                        visitor(tag, value)?;
                    }
                }
            }
            Self::Sparse(map) => {
                for (tag, value) in map.iter() {
                    visitor(*tag, value)?;
                }
            }
        }

        Ok(())
    }
}

impl<T: Clone> TagLookup<T> {
    fn new(entries: Vec<(i128, T)>, duplicate_context: &str) -> Result<Option<Self>, DecodeError> {
        if entries.is_empty() {
            return Ok(None);
        }

        let mut min_tag = i128::MAX;
        let mut max_tag = i128::MIN;
        let mut sparse = BTreeMap::new();
        for (tag, value) in &entries {
            if sparse.insert(*tag, value.clone()).is_some() {
                return Err(DecodeError::Corrupted(format!("{duplicate_context} {tag}")));
            }
            min_tag = min_tag.min(*tag);
            max_tag = max_tag.max(*tag);
        }

        let dense_len = max_tag
            .checked_sub(min_tag)
            .and_then(|width| width.checked_add(1))
            .and_then(|width| usize::try_from(width).ok());
        let dense_threshold = entries.len().saturating_mul(4).max(16);
        if min_tag >= 0 {
            if let Some(dense_len) = dense_len {
                if dense_len <= dense_threshold {
                    let mut dense_entries = vec![None; dense_len];
                    for (tag, value) in entries {
                        let index = usize::try_from(tag - min_tag)
                            .map_err(|_| DecodeError::Corrupted("tag index out of range".into()))?;
                        dense_entries[index] = Some(value);
                    }
                    return Ok(Some(Self::Dense {
                        min_tag,
                        entries: Arc::from(dense_entries),
                    }));
                }
            }
        }

        Ok(Some(Self::Sparse(Arc::new(sparse))))
    }
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
    typeid: usize,
    op: TypeOp,
    int_bounds: Option<IntBounds>,
    length_bounds: Option<IntBounds>,
    child_typeid: Option<usize>,
    choice_tag_bounds: Option<IntBounds>,
    choice_fields: Option<TagLookup<ChoiceField>>,
    struct_fields: Option<Arc<[StructField]>>,
    struct_fields_by_tag: Option<TagLookup<StructField>>,
}

impl TypeInfo {
    pub(crate) fn new(
        typeid: usize,
        op_name: &str,
        args: Vec<JsonValue>,
    ) -> Result<Self, DecodeError> {
        let op = TypeOp::parse(op_name)?;
        let int_bounds = if op == TypeOp::Int {
            Some(parse_int_bounds(args.first(), "_int bounds")?)
        } else {
            None
        };
        let length_bounds = match op {
            TypeOp::Array | TypeOp::BitArray | TypeOp::Blob => {
                Some(parse_int_bounds(args.first(), "length bounds")?)
            }
            _ => None,
        };
        let child_typeid = match op {
            TypeOp::Array => Some(parse_typeid_arg(args.get(1), "_array typeid")?),
            TypeOp::Optional => Some(parse_typeid_arg(args.first(), "_optional typeid")?),
            _ => None,
        };
        let choice_tag_bounds = if op == TypeOp::Choice {
            Some(parse_int_bounds(args.first(), "_choice bounds")?)
        } else {
            None
        };
        let choice_fields = if op == TypeOp::Choice {
            parse_choice_fields(&args)?
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
            .flatten();

        Ok(Self {
            typeid,
            op,
            int_bounds,
            length_bounds,
            child_typeid,
            choice_tag_bounds,
            choice_fields,
            struct_fields,
            struct_fields_by_tag,
        })
    }

    fn typeid(&self) -> usize {
        self.typeid
    }

    fn op(&self) -> TypeOp {
        self.op
    }

    fn op_name(&self) -> &'static str {
        self.op.as_str()
    }

    fn int_bounds(&self) -> Result<IntBounds, DecodeError> {
        self.int_bounds
            .ok_or_else(|| DecodeError::Corrupted("_int bounds".into()))
    }

    fn length_bounds(&self) -> Result<IntBounds, DecodeError> {
        self.length_bounds
            .ok_or_else(|| DecodeError::Corrupted("length bounds".into()))
    }

    fn child_typeid(&self) -> Result<usize, DecodeError> {
        self.child_typeid
            .ok_or_else(|| DecodeError::Corrupted("child typeid".into()))
    }

    fn choice_tag_bounds(&self) -> Result<IntBounds, DecodeError> {
        self.choice_tag_bounds
            .ok_or_else(|| DecodeError::Corrupted("_choice bounds".into()))
    }

    fn choice_fields(&self) -> Result<&TagLookup<ChoiceField>, DecodeError> {
        self.choice_fields
            .as_ref()
            .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))
    }

    fn struct_fields(&self) -> Result<&[StructField], DecodeError> {
        self.struct_fields
            .as_deref()
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))
    }

    fn struct_fields_by_tag(&self) -> Result<&TagLookup<StructField>, DecodeError> {
        self.struct_fields_by_tag
            .as_ref()
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

    fn tag(&self) -> Option<i128> {
        self.tag
    }
}

fn parse_choice_fields(args: &[JsonValue]) -> Result<Option<TagLookup<ChoiceField>>, DecodeError> {
    let map = args
        .get(1)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))?;

    let entries = map
        .iter()
        .map(|(tag, field)| -> Result<(i128, ChoiceField), DecodeError> {
            let parsed_tag = tag
                .parse::<i128>()
                .map_err(|_| DecodeError::Corrupted("_choice key".into()))?;
            let choice_field = parse_choice_field(field)?;
            Ok((parsed_tag, choice_field))
        })
        .collect::<Result<Vec<_>, _>>()?;

    TagLookup::new(entries, "duplicate _choice tag")
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
) -> Result<Option<TagLookup<StructField>>, DecodeError> {
    let entries = fields
        .iter()
        .filter_map(|field| field.tag().map(|tag| (tag, field.clone())))
        .collect::<Vec<_>>();

    TagLookup::new(entries, "duplicate _struct tag")
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

fn parse_int_bounds(value: Option<&JsonValue>, context: &str) -> Result<IntBounds, DecodeError> {
    let bounds = value
        .and_then(JsonValue::as_array)
        .ok_or_else(|| DecodeError::Corrupted(context.into()))?;
    if bounds.len() != 2 {
        return Err(DecodeError::Corrupted(format!("{context} len")));
    }

    let min = bounds[0]
        .as_i64()
        .or_else(|| bounds[0].as_u64().map(|value| value as i64))
        .ok_or_else(|| DecodeError::Corrupted(format!("{context} min")))?;
    let bits = bounds[1]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| DecodeError::Corrupted(format!("{context} bits")))?;

    Ok(IntBounds { min, bits })
}

fn parse_typeid_arg(value: Option<&JsonValue>, context: &str) -> Result<usize, DecodeError> {
    value
        .and_then(JsonValue::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| DecodeError::Corrupted(context.into()))
}

#[derive(Debug, Clone)]
pub(crate) enum EventDecodePlan<F> {
    Ordered(Arc<[OrderedEventFieldPlan<F>]>),
    Tagged(Arc<TaggedEventDecodePlan<F>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventPlanKind {
    Ordered,
    Tagged,
}

#[derive(Debug, Clone)]
pub(crate) enum OrderedEventFieldPlan<F> {
    Decode { field: F, typeinfo: TypeInfo },
    Skip { typeinfo: TypeInfo },
    Nested(Arc<[OrderedEventFieldPlan<F>]>),
}

#[derive(Debug, Clone)]
pub(crate) struct TaggedEventDecodePlan<F> {
    fields_by_tag: TagLookup<TaggedEventFieldPlan<F>>,
}

#[derive(Debug, Clone)]
pub(crate) enum TaggedEventFieldPlan<F> {
    Decode { field: F, typeinfo: TypeInfo },
    Skip,
    Nested(Arc<TaggedEventDecodePlan<F>>),
}

#[derive(Debug, Clone)]
pub(crate) struct EventTypeInfo<F> {
    name: Arc<str>,
    decode_plan: Option<Arc<EventDecodePlan<F>>>,
}

impl<F> EventTypeInfo<F> {
    pub(crate) fn new(name: String, decode_plan: Option<EventDecodePlan<F>>) -> Self {
        Self {
            name: Arc::<str>::from(name),
            decode_plan: decode_plan.map(Arc::new),
        }
    }

    fn name(&self) -> &str {
        self.name.as_ref()
    }

    fn decode_plan(&self) -> Option<&EventDecodePlan<F>> {
        self.decode_plan.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct ProtocolDefinition {
    build: u32,
    typeinfos: Arc<[TypeInfo]>,
    game_event_typeinfos: Arc<[Option<EventTypeInfo<GameEventField>>]>,
    message_event_typeinfos: Arc<[Option<EventTypeInfo<MessageEventField>>]>,
    tracker_event_typeinfos: Arc<[Option<EventTypeInfo<TrackerEventField>>]>,
    game_event_header: EventHeaderDecodePlan,
    message_event_header: EventHeaderDecodePlan,
    tracker_event_header: Option<EventHeaderDecodePlan>,
    replay_header_typeid: usize,
    game_details_typeid: usize,
    replay_initdata_typeid: usize,
}

#[derive(Debug, Clone)]
struct EventHeaderDecodePlan {
    eventid: IntegerDecodePlan,
    gameloop_delta: IntegerDecodePlan,
    replay_userid_typeinfo: Option<TypeInfo>,
    decode_user_id: bool,
    tolerant: bool,
}

#[derive(Debug, Clone)]
enum IntegerDecodePlan {
    Int {
        bitpacked_bounds: IntBounds,
    },
    Choice {
        bitpacked_tag_bounds: IntBounds,
        fields: TagLookup<IntegerDecodePlan>,
    },
}

impl IntegerDecodePlan {
    fn compile(typeinfo: &TypeInfo, typeinfos: &[TypeInfo]) -> Result<Self, DecodeError> {
        match typeinfo.op() {
            TypeOp::Int => Ok(Self::Int {
                bitpacked_bounds: typeinfo.int_bounds()?,
            }),
            TypeOp::Choice => {
                let mut fields = Vec::new();
                typeinfo.choice_fields()?.visit(|tag, field| {
                    let child_typeinfo = lookup_typeinfo(typeinfos, field.typeid())?;
                    let child_plan = Self::compile(child_typeinfo, typeinfos)?;
                    fields.push((tag, child_plan));
                    Ok(())
                })?;
                let fields = TagLookup::new(fields, "integer choice duplicate tag")?
                    .ok_or_else(|| DecodeError::Corrupted("integer choice has no fields".into()))?;

                Ok(Self::Choice {
                    bitpacked_tag_bounds: typeinfo.choice_tag_bounds()?,
                    fields,
                })
            }
            _ => Err(DecodeError::Corrupted(format!(
                "typeid={} op={} does not decode to integer",
                typeinfo.typeid(),
                typeinfo.op_name()
            ))),
        }
    }
}

impl EventHeaderDecodePlan {
    fn new(
        typeinfos: &[TypeInfo],
        eventid_typeid: usize,
        svaruint32_typeid: usize,
        replay_userid_typeid: Option<usize>,
        decode_user_id: bool,
        tolerant: bool,
    ) -> Result<Self, DecodeError> {
        let eventid_typeinfo = lookup_typeinfo(typeinfos, eventid_typeid)?;
        let svaruint32_typeinfo = lookup_typeinfo(typeinfos, svaruint32_typeid)?;
        let eventid = IntegerDecodePlan::compile(eventid_typeinfo, typeinfos)?;
        let gameloop_delta = IntegerDecodePlan::compile(svaruint32_typeinfo, typeinfos)?;
        let replay_userid_typeinfo = if decode_user_id {
            replay_userid_typeid
                .map(|typeid| lookup_typeinfo(typeinfos, typeid))
                .transpose()?
                .cloned()
        } else {
            None
        };

        Ok(Self {
            eventid,
            gameloop_delta,
            replay_userid_typeinfo,
            decode_user_id,
            tolerant,
        })
    }
}

impl ProtocolDefinition {
    pub(crate) fn new(
        build: u32,
        typeinfos: Arc<[TypeInfo]>,
        game_event_typeinfos: Arc<[Option<EventTypeInfo<GameEventField>>]>,
        message_event_typeinfos: Arc<[Option<EventTypeInfo<MessageEventField>>]>,
        tracker_event_typeinfos: Arc<[Option<EventTypeInfo<TrackerEventField>>]>,
        game_eventid_typeid: usize,
        message_eventid_typeid: usize,
        tracker_eventid_typeid: Option<usize>,
        svaruint32_typeid: usize,
        replay_userid_typeid: Option<usize>,
        replay_header_typeid: usize,
        game_details_typeid: usize,
        replay_initdata_typeid: usize,
    ) -> Result<Self, DecodeError> {
        let game_event_header = EventHeaderDecodePlan::new(
            typeinfos.as_ref(),
            game_eventid_typeid,
            svaruint32_typeid,
            replay_userid_typeid,
            true,
            false,
        )?;
        let message_event_header = EventHeaderDecodePlan::new(
            typeinfos.as_ref(),
            message_eventid_typeid,
            svaruint32_typeid,
            replay_userid_typeid,
            true,
            false,
        )?;
        let tracker_event_header = tracker_eventid_typeid
            .map(|eventid_typeid| {
                EventHeaderDecodePlan::new(
                    typeinfos.as_ref(),
                    eventid_typeid,
                    svaruint32_typeid,
                    replay_userid_typeid,
                    false,
                    true,
                )
            })
            .transpose()?;

        Ok(Self {
            build,
            typeinfos,
            game_event_typeinfos,
            message_event_typeinfos,
            tracker_event_typeinfos,
            game_event_header,
            message_event_header,
            tracker_event_header,
            replay_header_typeid,
            game_details_typeid,
            replay_initdata_typeid,
        })
    }

    pub fn build(&self) -> u32 {
        self.build
    }

    pub fn decode_replay_game_events(
        &self,
        contents: &[u8],
    ) -> Result<Vec<GameEvent>, DecodeError> {
        let decoder = BitPackedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decode_event_stream::<_, GameEvent>(
            decoder,
            &self.game_event_typeinfos,
            &self.game_event_header,
        )
    }

    pub fn decode_replay_message_events(
        &self,
        contents: &[u8],
    ) -> Result<Vec<MessageEvent>, DecodeError> {
        let decoder = BitPackedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decode_event_stream::<_, MessageEvent>(
            decoder,
            &self.message_event_typeinfos,
            &self.message_event_header,
        )
    }

    pub fn decode_replay_tracker_events(
        &self,
        contents: &[u8],
    ) -> Result<Vec<TrackerEvent>, DecodeError> {
        let Some(tracker_event_header) = self.tracker_event_header.as_ref() else {
            return Ok(Vec::new());
        };

        let decoder = VersionedDecoder::new(contents, Arc::clone(&self.typeinfos));
        decode_event_stream::<_, TrackerEvent>(
            decoder,
            &self.tracker_event_typeinfos,
            tracker_event_header,
        )
    }

    pub fn decode_replay_ordered_events(
        &self,
        game_contents: &[u8],
        tracker_contents: Option<&[u8]>,
    ) -> Result<Vec<ReplayEvent>, DecodeError> {
        self.decode_replay_ordered_events_filtered(game_contents, tracker_contents, |_| true)
    }

    pub fn decode_replay_ordered_events_filtered<F>(
        &self,
        game_contents: &[u8],
        tracker_contents: Option<&[u8]>,
        include_event: F,
    ) -> Result<Vec<ReplayEvent>, DecodeError>
    where
        F: Fn(&str) -> bool,
    {
        let mut game_reader = EventStreamReader::<_, GameEvent>::new(
            BitPackedDecoder::new(game_contents, Arc::clone(&self.typeinfos)),
            &self.game_event_typeinfos,
            &self.game_event_header,
        );
        let mut tracker_reader = match (self.tracker_event_header.as_ref(), tracker_contents) {
            (Some(tracker_event_header), Some(contents)) => {
                Some(EventStreamReader::<_, TrackerEvent>::new(
                    VersionedDecoder::new(contents, Arc::clone(&self.typeinfos)),
                    &self.tracker_event_typeinfos,
                    tracker_event_header,
                ))
            }
            _ => None,
        };

        let mut next_game = game_reader.next_matching_event(&include_event)?;
        let mut next_tracker = tracker_reader
            .as_mut()
            .map(|reader| reader.next_matching_event(&include_event))
            .transpose()?
            .flatten();
        let mut events = Vec::new();

        while next_game.is_some() || next_tracker.is_some() {
            let take_game = match (&next_game, &next_tracker) {
                (Some(game_event), Some(tracker_event)) => {
                    game_event.game_loop <= tracker_event.game_loop
                }
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => false,
            };

            if take_game {
                if let Some(event) = next_game.take() {
                    events.push(ReplayEvent::Game(event));
                }
                next_game = game_reader.next_matching_event(&include_event)?;
            } else {
                if let Some(event) = next_tracker.take() {
                    events.push(ReplayEvent::Tracker(event));
                }
                next_tracker = tracker_reader
                    .as_mut()
                    .map(|reader| reader.next_matching_event(&include_event))
                    .transpose()?
                    .flatten();
            }
        }

        Ok(events)
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
            let raw = buffer.read_aligned_array::<4>()?;

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
    fn instance(&mut self, typeid: usize) -> Result<Value, DecodeError> {
        let typeinfos = self.typeinfos();
        let typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
        self.instance_from_typeinfo(typeinfo)
    }
    fn instance_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError>;
    fn integer_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<i128, DecodeError>;
    fn i64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<i64>, DecodeError>;
    fn f64_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<f64>, DecodeError>;
    fn string_from_typeinfo(&mut self, typeinfo: &TypeInfo) -> Result<Option<String>, DecodeError>;
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

trait HeaderIntegerDecoder {
    fn integer_from_plan(&mut self, plan: &IntegerDecodePlan) -> Result<i128, DecodeError>;
}

pub struct BitPackedDecoder<'a> {
    buffer: BitPackedBuffer<'a>,
    typeinfos: Arc<[TypeInfo]>,
}

fn lookup_typeinfo(typeinfos: &[TypeInfo], typeid: usize) -> Result<&TypeInfo, DecodeError> {
    typeinfos
        .get(typeid)
        .ok_or_else(|| DecodeError::Corrupted(format!("typeid {typeid} out of range")))
}

pub(crate) fn compile_event_decode_plan<F, S>(
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
        EventPlanKind::Ordered => {
            compile_ordered_event_decode_plan(typeinfo, typeinfos, select_field)
                .map(|plan| plan.map(EventDecodePlan::Ordered))
        }
        EventPlanKind::Tagged => {
            compile_tagged_event_decode_plan(typeinfo, typeinfos, select_field)
                .map(|plan| plan.map(EventDecodePlan::Tagged))
        }
    }
}

fn compile_ordered_event_decode_plan<F, S>(
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
        let child_typeinfo = lookup_typeinfo(typeinfos, field.typeid())?.clone();
        if field.is_parent() {
            if child_typeinfo.op() == TypeOp::Struct {
                let Some(parent_plan) =
                    compile_ordered_event_decode_plan(&child_typeinfo, typeinfos, select_field)?
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

fn compile_tagged_event_decode_plan<F, S>(
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

        let child_typeinfo = lookup_typeinfo(typeinfos, field.typeid())?.clone();
        let plan = if field.is_parent() {
            if child_typeinfo.op() == TypeOp::Struct {
                match compile_tagged_event_decode_plan(&child_typeinfo, typeinfos, select_field)? {
                    Some(parent_plan) => TaggedEventFieldPlan::Nested(parent_plan),
                    None if fields.len() == 1 => return Ok(None),
                    None => TaggedEventFieldPlan::Skip,
                }
            } else if fields.len() == 1 {
                return Ok(None);
            } else if let Some(selected_field) = select_field("__parent") {
                TaggedEventFieldPlan::Decode {
                    field: selected_field,
                    typeinfo: child_typeinfo,
                }
            } else {
                TaggedEventFieldPlan::Skip
            }
        } else if let Some(selected_field) = select_field(field.name()) {
            TaggedEventFieldPlan::Decode {
                field: selected_field,
                typeinfo: child_typeinfo,
            }
        } else {
            TaggedEventFieldPlan::Skip
        };

        entries.push((tag, plan));
    }

    let Some(fields_by_tag) = TagLookup::new(entries, "duplicate event plan tag")? else {
        return Ok(None);
    };
    Ok(Some(Arc::new(TaggedEventDecodePlan { fields_by_tag })))
}

fn mark_trigger_text(result: &mut TriggerEventData, text: &str) {
    if text.contains("SelectionChanged") {
        result.contains_selection_changed = true;
    }
    if text.contains("None") {
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
                    scan_trigger_event_data(decoder, child_typeinfo, result)
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
                scan_trigger_event_data(decoder, child_typeinfo, result)
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
                scan_trigger_event_data(decoder, child_typeinfo, result)
            },
        ),
        TypeOp::Array => decoder
            .visit_array_elements_from_typeinfo(typeinfo, &mut |decoder, child_typeinfo| {
                scan_trigger_event_data(decoder, child_typeinfo, result)
            }),
        TypeOp::Blob | TypeOp::Fourcc => {
            if let Some(text) = decoder.string_from_typeinfo(typeinfo)? {
                mark_trigger_text(result, &text);
            }
            Ok(())
        }
        _ => decoder.skip_from_typeinfo(typeinfo),
    }
}

pub(crate) fn decode_trigger_event_data_from_typeinfo<D: TypeDecoder>(
    decoder: &mut D,
    typeinfo: &TypeInfo,
) -> Result<TriggerEventData, DecodeError> {
    let mut result = TriggerEventData::default();
    scan_trigger_event_data(decoder, typeinfo, &mut result)?;
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
                    decode_ability_data_inner(decoder, child_typeinfo, ability, found)
                },
            )?;
            Ok(())
        }
        TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
            typeinfo,
            &mut |field_name| (field_name == "m_abilLink").then_some(()),
            &mut |decoder, (), field_typeinfo| {
                ability.m_abilLink = decoder
                    .i64_from_typeinfo(field_typeinfo)?
                    .unwrap_or_default();
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
    decode_ability_data_inner(decoder, typeinfo, &mut ability, &mut found)?;
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
                    collect_snapshot_point_values(decoder, child_typeinfo, values)
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
                collect_snapshot_point_values(decoder, field_typeinfo, values)
            },
        ),
        TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
            typeinfo,
            &mut |_| Some(()),
            &mut |decoder, (), field_typeinfo| {
                collect_snapshot_point_values(decoder, field_typeinfo, values)
            },
        ),
        TypeOp::Array => {
            decoder.visit_array_elements_from_typeinfo(typeinfo, &mut |decoder, child_typeinfo| {
                collect_snapshot_point_values(decoder, child_typeinfo, values)
            })
        }
        _ => decoder.skip_from_typeinfo(typeinfo),
    }
}

pub(crate) fn decode_snapshot_point_from_typeinfo<D: TypeDecoder>(
    decoder: &mut D,
    typeinfo: &TypeInfo,
) -> Result<Option<SnapshotPoint>, DecodeError> {
    let mut values = Vec::new();
    collect_snapshot_point_values(decoder, typeinfo, &mut values)?;
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
                    decode_target_unit_data_inner(decoder, child_typeinfo, data, found)
                },
            )?;
            Ok(())
        }
        TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
            typeinfo,
            &mut |field_name| (field_name == "m_snapshotPoint").then_some(()),
            &mut |decoder, (), field_typeinfo| {
                data.m_snapshotPoint =
                    decode_snapshot_point_from_typeinfo(decoder, field_typeinfo)?;
                *found = true;
                Ok(())
            },
        ),
        TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
            typeinfo,
            &mut |field_name| (field_name == "m_snapshotPoint").then_some(()),
            &mut |decoder, (), field_typeinfo| {
                data.m_snapshotPoint =
                    decode_snapshot_point_from_typeinfo(decoder, field_typeinfo)?;
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
    decode_target_unit_data_inner(decoder, typeinfo, &mut data, &mut found)?;
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
                    decode_cmd_event_data_inner(decoder, child_typeinfo, data, found)
                },
            )?;
            Ok(())
        }
        TypeOp::Struct => decoder.visit_struct_fields_from_typeinfo(
            typeinfo,
            &mut |field_name| (field_name == "TargetUnit").then_some(()),
            &mut |decoder, (), field_typeinfo| {
                data.TargetUnit = decode_target_unit_data_from_typeinfo(decoder, field_typeinfo)?;
                *found = true;
                Ok(())
            },
        ),
        TypeOp::Choice => decoder.visit_choice_field_from_typeinfo(
            typeinfo,
            &mut |field_name| (field_name == "TargetUnit").then_some(()),
            &mut |decoder, (), field_typeinfo| {
                data.TargetUnit = decode_target_unit_data_from_typeinfo(decoder, field_typeinfo)?;
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
    decode_cmd_event_data_inner(decoder, typeinfo, &mut data, &mut found)?;
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
                    decode_player_stats_inner(decoder, child_typeinfo, stats, found)
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
    decode_player_stats_inner(decoder, typeinfo, &mut stats, &mut found)?;
    Ok(found.then_some(stats))
}

impl<'a> BitPackedDecoder<'a> {
    pub fn new(contents: &'a [u8], typeinfos: Arc<[TypeInfo]>) -> Self {
        Self {
            buffer: BitPackedBuffer::new(contents, true),
            typeinfos,
        }
    }

    fn int(&mut self, bounds: IntBounds) -> Result<i128, DecodeError> {
        let raw = self.buffer.read_bits(bounds.bits)? as i128;
        Ok(bounds.min as i128 + raw)
    }

    fn optional_exists(&mut self) -> Result<bool, DecodeError> {
        Ok(self.buffer.read_bits(1)? != 0)
    }

    fn array(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        let length = self.int(typeinfo.length_bounds()?)? as usize;
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;

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
            let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
            } else {
                if let Some(selected_field) = select_field(field.name()) {
                    on_field(self, selected_field, child_typeinfo)?;
                } else {
                    self.skip_from_typeinfo(child_typeinfo)?;
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

        let tag = self.int(typeinfo.choice_tag_bounds()?)?;
        let field = typeinfo
            .choice_fields()?
            .get(&tag)
            .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
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
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                self.buffer.read_bits(1)?;
                Ok(())
            }
            TypeOp::Choice => {
                let tag = self.int(typeinfo.choice_tag_bounds()?)?;
                let field = typeinfo
                    .choice_fields()?
                    .get(&tag)
                    .ok_or_else(|| DecodeError::Corrupted(format!("invalid choice tag {tag}")))?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                    let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
            let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
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
                let parent_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
            TypeOp::Bool => Ok(Value::Bool(self.buffer.read_bits(1)? != 0)),
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
            eprintln!(
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
            TypeOp::Bool => Ok(Some((self.buffer.read_bits(1)? != 0).to_string())),
            TypeOp::Real32 => Ok(self.real32()?.as_f64().map(|value| value.to_string())),
            TypeOp::Real64 => Ok(self.real64()?.as_f64().map(|value| value.to_string())),
            _ => Ok(Some(self.integer_from_typeinfo(typeinfo)?.to_string())),
        }
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

pub struct VersionedDecoder<'a> {
    buffer: BitPackedBuffer<'a>,
    typeinfos: Arc<[TypeInfo]>,
}

impl<'a> VersionedDecoder<'a> {
    pub fn new(contents: &'a [u8], typeinfos: Arc<[TypeInfo]>) -> Self {
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
                )))
            }
        }
        Ok(())
    }

    fn array(&mut self, typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(0)?;
        let length = self.vint()? as usize;
        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;

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
        let bytes = (length + 7) / 8;

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

    fn blob(&mut self, _typeinfo: &TypeInfo) -> Result<Value, DecodeError> {
        self.expect_skip(2)?;
        let length = self.vint()? as usize;
        Ok(Value::Bytes(self.buffer.read_aligned_bytes(length)?))
    }

    fn bool(&mut self) -> Result<Value, DecodeError> {
        self.expect_skip(6)?;
        Ok(Value::Bool(self.buffer.read_bits(8)? != 0))
    }

    fn choice(&mut self, typeinfo: &TypeInfo) -> Result<BTreeMap<String, Value>, DecodeError> {
        self.expect_skip(3)?;
        let tag = self.vint()?;
        if let Some(field) = typeinfo.choice_fields()?.get(&tag) {
            let typeinfos = Arc::clone(&self.typeinfos);
            let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
        let exists = self.buffer.read_bits(8)? != 0;
        if !exists {
            return Ok(Value::Null);
        }

        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
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
                let parent_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
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
        let exists = self.buffer.read_bits(8)? != 0;
        if !exists {
            return Ok(false);
        }

        let typeinfos = Arc::clone(&self.typeinfos);
        let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeinfo.child_typeid()?)?;
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
            let Some(step) = plan.fields_by_tag.get(&tag) else {
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
                let parent_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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

impl BitPackedDecoder<'_> {
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
            eprintln!(
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
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), field.typeid())?;
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
                let exists = self.buffer.read_bits(8)? != 0;
                if !exists {
                    return Ok(None);
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                let exists = self.buffer.read_bits(8)? != 0;
                if !exists {
                    return Ok(None);
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                let exists = self.buffer.read_bits(8)? != 0;
                if !exists {
                    return Ok(None);
                }

                let typeid = typeinfo.child_typeid()?;
                let typeinfos = Arc::clone(&self.typeinfos);
                let child_typeinfo = lookup_typeinfo(typeinfos.as_ref(), typeid)?;
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
                Ok(Some((self.buffer.read_bits(8)? != 0).to_string()))
            }
            TypeOp::Real32 => Ok(self.real32()?.as_f64().map(|value| value.to_string())),
            TypeOp::Real64 => Ok(self.real64()?.as_f64().map(|value| value.to_string())),
            _ => Ok(Some(self.integer_from_typeinfo(typeinfo)?.to_string())),
        }
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

fn decode_event_stream<'a, D, T>(
    decoder: D,
    event_typeinfos: &'a [Option<EventTypeInfo<T::Field>>],
    header: &'a EventHeaderDecodePlan,
) -> Result<Vec<T>, DecodeError>
where
    D: TypeDecoder + HeaderIntegerDecoder,
    T: DirectEventDecode,
{
    let mut reader = EventStreamReader::<_, T>::new(decoder, event_typeinfos, header);
    let mut events = Vec::new();
    while let Some(event) = reader.next_event()? {
        events.push(event);
    }

    Ok(events)
}

struct EventStreamReader<'a, D, T>
where
    D: TypeDecoder + HeaderIntegerDecoder,
    T: DirectEventDecode,
{
    decoder: D,
    event_typeinfos: &'a [Option<EventTypeInfo<T::Field>>],
    header: &'a EventHeaderDecodePlan,
    gameloop: i128,
    produced_any: bool,
    finished: bool,
}

impl<'a, D, T> EventStreamReader<'a, D, T>
where
    D: TypeDecoder + HeaderIntegerDecoder,
    T: DirectEventDecode,
{
    fn new(
        decoder: D,
        event_typeinfos: &'a [Option<EventTypeInfo<T::Field>>],
        header: &'a EventHeaderDecodePlan,
    ) -> Self {
        Self {
            decoder,
            event_typeinfos,
            header,
            gameloop: 0,
            produced_any: false,
            finished: false,
        }
    }

    fn next_event(&mut self) -> Result<Option<T>, DecodeError> {
        self.next_matching_event(&|_| true)
    }

    fn next_matching_event<F>(&mut self, include_event: &F) -> Result<Option<T>, DecodeError>
    where
        F: Fn(&str) -> bool,
    {
        loop {
            if self.finished || self.decoder.done() {
                self.finished = true;
                return Ok(None);
            }

            let start_bits = self.decoder.used_bits();

            let event_result = (|| -> Result<Option<T>, DecodeError> {
                let delta = self
                    .decoder
                    .integer_from_plan(&self.header.gameloop_delta)?;
                self.gameloop += delta;

                let userid = if self.header.decode_user_id {
                    self.header
                        .replay_userid_typeinfo
                        .as_ref()
                        .map(|typeinfo| decode_event_user_id(&mut self.decoder, typeinfo))
                        .transpose()?
                        .flatten()
                } else {
                    None
                };

                let eventid = u32::try_from(self.decoder.integer_from_plan(&self.header.eventid)?)
                    .map_err(|_| DecodeError::Corrupted("invalid event id".into()))?;

                let event_typeinfo = usize::try_from(eventid)
                    .ok()
                    .and_then(|index| self.event_typeinfos.get(index))
                    .and_then(|value| value.as_ref())
                    .ok_or_else(|| DecodeError::Corrupted(format!("eventid({eventid}) unknown")))?;
                let plan = event_typeinfo.decode_plan().ok_or_else(|| {
                    DecodeError::Corrupted(format!("eventid({eventid}) missing decode plan"))
                })?;

                if !include_event(event_typeinfo.name()) {
                    self.decoder.skip_event_fields_from_plan(plan)?;
                    return Ok(None);
                }

                let mut event =
                    T::new_decoded(event_typeinfo.name(), eventid, self.gameloop, userid);
                event.decode_fields_from_plan(&mut self.decoder, plan)?;

                event.set_decoded_bits((self.decoder.used_bits() - start_bits) as i128);
                Ok(Some(event))
            })();

            let event = match event_result {
                Ok(event) => event,
                Err(error) => {
                    if self.header.tolerant && self.produced_any {
                        self.finished = true;
                        return Ok(None);
                    } else {
                        return Err(error);
                    }
                }
            };

            self.decoder.byte_align();
            self.produced_any = true;
            if event.is_some() {
                return Ok(event);
            }
        }
    }
}
