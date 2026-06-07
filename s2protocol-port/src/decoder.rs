use crate::bitstream::BitPackedBuffer;
use crate::{
    error::DecodeError,
    events::{
        GameEvent, GameEventField, MessageEvent, MessageEventField, ReplayEvent, TrackerEvent,
        TrackerEventField,
    },
    value::Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

mod bit_packed_decoder;
mod event_special_data;
mod event_stream;
mod type_decoder;
mod type_info;
mod versioned_decoder;

use bit_packed_decoder::BitPackedDecoder;
pub(crate) use bit_packed_decoder::EventPlanCompiler;
pub(crate) use event_special_data::EventSpecialDataDecoder;
use event_stream::{EventStreamDecoder, EventStreamReader, EventTypeInfoFilter};
pub(crate) use type_decoder::TypeDecoder;
pub(crate) use type_info::{EventDecodePlan, EventPlanKind, EventTypeInfo, TypeInfo};
use type_info::{
    IntBounds, OrderedEventFieldPlan, TagLookup, TaggedEventDecodePlan, TaggedEventFieldPlan,
    TypeOp,
};
use versioned_decoder::HeaderIntegerDecoder;
use versioned_decoder::VersionedDecoder;

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
pub(crate) struct ProtocolEventTypeInfos {
    game: Arc<[Option<EventTypeInfo<GameEventField>>]>,
    message: Arc<[Option<EventTypeInfo<MessageEventField>>]>,
    tracker: Arc<[Option<EventTypeInfo<TrackerEventField>>]>,
}

impl ProtocolEventTypeInfos {
    pub(crate) fn new(
        game: Arc<[Option<EventTypeInfo<GameEventField>>]>,
        message: Arc<[Option<EventTypeInfo<MessageEventField>>]>,
        tracker: Arc<[Option<EventTypeInfo<TrackerEventField>>]>,
    ) -> Self {
        Self {
            game,
            message,
            tracker,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProtocolEventTypeIds {
    game_eventid: usize,
    message_eventid: usize,
    tracker_eventid: Option<usize>,
    svaruint32: usize,
    replay_userid: Option<usize>,
}

impl ProtocolEventTypeIds {
    pub(crate) fn new(
        game_eventid: usize,
        message_eventid: usize,
        tracker_eventid: Option<usize>,
        svaruint32: usize,
        replay_userid: Option<usize>,
    ) -> Self {
        Self {
            game_eventid,
            message_eventid,
            tracker_eventid,
            svaruint32,
            replay_userid,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReplayTypeIds {
    header: usize,
    details: usize,
    initdata: usize,
}

impl ReplayTypeIds {
    pub(crate) fn new(header: usize, details: usize, initdata: usize) -> Self {
        Self {
            header,
            details,
            initdata,
        }
    }
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
                    let child_typeinfo =
                        EventPlanCompiler::lookup_typeinfo(typeinfos, field.typeid())?;
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
        let eventid_typeinfo = EventPlanCompiler::lookup_typeinfo(typeinfos, eventid_typeid)?;
        let svaruint32_typeinfo = EventPlanCompiler::lookup_typeinfo(typeinfos, svaruint32_typeid)?;
        let eventid = IntegerDecodePlan::compile(eventid_typeinfo, typeinfos)?;
        let gameloop_delta = IntegerDecodePlan::compile(svaruint32_typeinfo, typeinfos)?;
        let replay_userid_typeinfo = if decode_user_id {
            replay_userid_typeid
                .map(|typeid| EventPlanCompiler::lookup_typeinfo(typeinfos, typeid))
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
        event_typeinfos: ProtocolEventTypeInfos,
        event_typeids: ProtocolEventTypeIds,
        replay_typeids: ReplayTypeIds,
    ) -> Result<Self, DecodeError> {
        let game_event_header = EventHeaderDecodePlan::new(
            typeinfos.as_ref(),
            event_typeids.game_eventid,
            event_typeids.svaruint32,
            event_typeids.replay_userid,
            true,
            false,
        )?;
        let message_event_header = EventHeaderDecodePlan::new(
            typeinfos.as_ref(),
            event_typeids.message_eventid,
            event_typeids.svaruint32,
            event_typeids.replay_userid,
            true,
            false,
        )?;
        let tracker_event_header = event_typeids
            .tracker_eventid
            .map(|eventid_typeid| {
                EventHeaderDecodePlan::new(
                    typeinfos.as_ref(),
                    eventid_typeid,
                    event_typeids.svaruint32,
                    event_typeids.replay_userid,
                    false,
                    true,
                )
            })
            .transpose()?;

        Ok(Self {
            build,
            typeinfos,
            game_event_typeinfos: event_typeinfos.game,
            message_event_typeinfos: event_typeinfos.message,
            tracker_event_typeinfos: event_typeinfos.tracker,
            game_event_header,
            message_event_header,
            tracker_event_header,
            replay_header_typeid: replay_typeids.header,
            game_details_typeid: replay_typeids.details,
            replay_initdata_typeid: replay_typeids.initdata,
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
        EventStreamDecoder::decode::<_, GameEvent>(
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
        EventStreamDecoder::decode::<_, MessageEvent>(
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
        EventStreamDecoder::decode::<_, TrackerEvent>(
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
        let mut retain_all = |_: &ReplayEvent| true;
        self.decode_replay_ordered_events_filtered_retained_internal(
            game_contents,
            tracker_contents,
            &include_event,
            &mut retain_all,
            false,
        )
        .map(|(events, _decoded_count)| events)
    }

    pub fn decode_replay_ordered_events_filtered_retained<F, R>(
        &self,
        game_contents: &[u8],
        tracker_contents: Option<&[u8]>,
        include_event: F,
        mut retain_event: R,
    ) -> Result<(Vec<ReplayEvent>, usize), DecodeError>
    where
        F: Fn(&str) -> bool,
        R: FnMut(&ReplayEvent) -> bool,
    {
        self.decode_replay_ordered_events_filtered_retained_internal(
            game_contents,
            tracker_contents,
            &include_event,
            &mut retain_event,
            true,
        )
    }

    fn decode_replay_ordered_events_filtered_retained_internal(
        &self,
        game_contents: &[u8],
        tracker_contents: Option<&[u8]>,
        include_event: &dyn Fn(&str) -> bool,
        retain_event: &mut dyn FnMut(&ReplayEvent) -> bool,
        use_retained_capacity_hint: bool,
    ) -> Result<(Vec<ReplayEvent>, usize), DecodeError> {
        let game_event_filter =
            EventTypeInfoFilter::from_typeinfos(&self.game_event_typeinfos, include_event);
        let tracker_event_filter =
            EventTypeInfoFilter::from_typeinfos(&self.tracker_event_typeinfos, include_event);

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

        let mut next_game = game_reader.next_matching_event(&game_event_filter)?;
        let mut next_tracker = tracker_reader
            .as_mut()
            .map(|reader| reader.next_matching_event(&tracker_event_filter))
            .transpose()?
            .flatten();
        let capacity = if use_retained_capacity_hint {
            Self::ordered_retained_event_capacity_hint(game_contents, tracker_contents)
        } else {
            Self::ordered_event_capacity_hint(game_contents, tracker_contents)
        };
        let mut events = Vec::with_capacity(capacity);
        let mut decoded_count = 0_usize;

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
                    let event = ReplayEvent::Game(event);
                    decoded_count += 1;
                    if retain_event(&event) {
                        events.push(event);
                    }
                }
                next_game = game_reader.next_matching_event(&game_event_filter)?;
            } else {
                if let Some(event) = next_tracker.take() {
                    let event = ReplayEvent::Tracker(event);
                    decoded_count += 1;
                    if retain_event(&event) {
                        events.push(event);
                    }
                }
                next_tracker = tracker_reader
                    .as_mut()
                    .map(|reader| reader.next_matching_event(&tracker_event_filter))
                    .transpose()?
                    .flatten();
            }
        }

        Ok((events, decoded_count))
    }

    fn ordered_event_capacity_hint(game_contents: &[u8], tracker_contents: Option<&[u8]>) -> usize {
        let bytes = game_contents
            .len()
            .saturating_add(tracker_contents.map_or(0, <[u8]>::len));
        (bytes / 32).max(128)
    }

    fn ordered_retained_event_capacity_hint(
        game_contents: &[u8],
        tracker_contents: Option<&[u8]>,
    ) -> usize {
        let bytes = game_contents
            .len()
            .saturating_add(tracker_contents.map_or(0, <[u8]>::len));
        (bytes / 48).max(128)
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

        object.insert("source".to_string(), Value::Int(buffer.read_u8()? as i128));
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
            let scope = u64::from(buffer.read_u8()?);
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
