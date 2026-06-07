use super::{ParsedReplayParts, ReplayParseTiming};
use crate::{
    events::{GameEvent, MessageEvent, ReplayEvent, TrackerEvent},
    replay_data::{
        ReplayAttributeScope, ReplayAttributes, ReplayDetails, ReplayHeader, ReplayInitData,
        ReplayMetadata,
    },
};

#[derive(Debug, Clone)]
pub struct ParsedReplay {
    path: String,
    base_build: u32,
    header: ReplayHeader,
    details: Option<ReplayDetails>,
    details_backup: Option<ReplayDetails>,
    init_data: Option<ReplayInitData>,
    metadata: Option<ReplayMetadata>,
    game_events: Vec<GameEvent>,
    message_events: Vec<MessageEvent>,
    tracker_events: Vec<TrackerEvent>,
    attributes: Option<ReplayAttributes>,
    attribute_scopes: Vec<ReplayAttributeScope>,
}

#[derive(Debug, Clone)]
pub struct ParsedReplayWithEvents {
    replay: ParsedReplay,
    events: Vec<ReplayEvent>,
    timing: ReplayParseTiming,
    ordered_events_decoded_count: usize,
}

impl ParsedReplayWithEvents {
    pub(super) fn new_with_ordered_events_decoded_count(
        replay: ParsedReplay,
        events: Vec<ReplayEvent>,
        timing: ReplayParseTiming,
        ordered_events_decoded_count: usize,
    ) -> Self {
        Self {
            replay,
            events,
            timing,
            ordered_events_decoded_count,
        }
    }

    pub fn replay(&self) -> &ParsedReplay {
        &self.replay
    }

    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }

    pub fn timing(&self) -> &ReplayParseTiming {
        &self.timing
    }

    pub fn ordered_events_decoded_count(&self) -> usize {
        self.ordered_events_decoded_count
    }

    pub fn take_replay(self) -> ParsedReplay {
        self.replay
    }

    pub fn take_events(&mut self) -> Vec<ReplayEvent> {
        std::mem::take(&mut self.events)
    }
}

impl ParsedReplay {
    pub(super) fn new(parts: ParsedReplayParts) -> Self {
        Self {
            path: parts.path,
            base_build: parts.base_build,
            header: parts.header,
            details: parts.details,
            details_backup: parts.details_backup,
            init_data: parts.init_data,
            metadata: parts.metadata,
            game_events: parts.game_events,
            message_events: parts.message_events,
            tracker_events: parts.tracker_events,
            attributes: parts.attributes,
            attribute_scopes: parts.attribute_scopes,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn base_build(&self) -> u32 {
        self.base_build
    }

    pub fn header(&self) -> &ReplayHeader {
        &self.header
    }

    pub fn details(&self) -> Option<&ReplayDetails> {
        self.details.as_ref()
    }

    pub fn details_backup(&self) -> Option<&ReplayDetails> {
        self.details_backup.as_ref()
    }

    pub fn init_data(&self) -> Option<&ReplayInitData> {
        self.init_data.as_ref()
    }

    pub fn metadata(&self) -> Option<&ReplayMetadata> {
        self.metadata.as_ref()
    }

    pub fn game_events(&self) -> &[GameEvent] {
        &self.game_events
    }

    pub fn message_events(&self) -> &[MessageEvent] {
        &self.message_events
    }

    pub fn tracker_events(&self) -> &[TrackerEvent] {
        &self.tracker_events
    }

    pub fn attributes(&self) -> Option<&ReplayAttributes> {
        self.attributes.as_ref()
    }

    pub fn attribute_scopes(&self) -> &[ReplayAttributeScope] {
        &self.attribute_scopes
    }

    pub fn take_details(&mut self) -> Option<ReplayDetails> {
        self.details.take()
    }

    pub fn take_init_data(&mut self) -> Option<ReplayInitData> {
        self.init_data.take()
    }

    pub fn take_metadata(&mut self) -> Option<ReplayMetadata> {
        self.metadata.take()
    }

    pub fn take_game_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.game_events)
    }

    pub fn take_message_events(&mut self) -> Vec<MessageEvent> {
        std::mem::take(&mut self.message_events)
    }

    pub fn take_tracker_events(&mut self) -> Vec<TrackerEvent> {
        std::mem::take(&mut self.tracker_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayParseMode {
    Simple,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayParseOptions {
    decode_attributes: bool,
}

impl Default for ReplayParseOptions {
    fn default() -> Self {
        Self {
            decode_attributes: true,
        }
    }
}

impl ReplayParseOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_decode_attributes(mut self, decode_attributes: bool) -> Self {
        self.decode_attributes = decode_attributes;
        self
    }

    pub(super) fn decode_attributes(self) -> bool {
        self.decode_attributes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayEventDecodeMode {
    None,
    Split,
    Ordered,
}
