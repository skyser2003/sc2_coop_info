use super::{EventHeaderDecodePlan, EventTypeInfo, HeaderIntegerDecoder, TypeDecoder};
use crate::{
    error::DecodeError,
    events::{DirectEventDecode, EventUserIdDecoder},
};

pub(super) struct EventStreamDecoder;

impl EventStreamDecoder {
    pub(super) fn decode<'a, D, T>(
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
}

pub(super) struct EventStreamReader<'a, D, T>
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

pub(super) enum EventTypeInfoFilter {
    All,
    Included(Vec<bool>),
}

impl EventTypeInfoFilter {
    pub(super) fn from_typeinfos<F, E>(
        event_typeinfos: &[Option<EventTypeInfo<E>>],
        include_event: &F,
    ) -> Self
    where
        F: Fn(&str) -> bool + ?Sized,
    {
        Self::Included(
            event_typeinfos
                .iter()
                .map(|event_typeinfo| {
                    event_typeinfo
                        .as_ref()
                        .is_some_and(|event_typeinfo| include_event(event_typeinfo.name()))
                })
                .collect(),
        )
    }

    fn includes(&self, event_id: u32) -> bool {
        match self {
            Self::All => true,
            Self::Included(included) => usize::try_from(event_id)
                .ok()
                .and_then(|index| included.get(index))
                .copied()
                .unwrap_or(false),
        }
    }
}

impl<'a, D, T> EventStreamReader<'a, D, T>
where
    D: TypeDecoder + HeaderIntegerDecoder,
    T: DirectEventDecode,
{
    pub(super) fn new(
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
        self.next_matching_event(&EventTypeInfoFilter::All)
    }

    pub(super) fn next_matching_event(
        &mut self,
        include_event: &EventTypeInfoFilter,
    ) -> Result<Option<T>, DecodeError> {
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
                        .map(|typeinfo| EventUserIdDecoder::decode(&mut self.decoder, typeinfo))
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

                if !include_event.includes(eventid) {
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
