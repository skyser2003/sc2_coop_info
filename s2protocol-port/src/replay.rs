use crate::{
    error::DecodeError,
    events::{GameEvent, MessageEvent, ReplayEvent, TrackerEvent},
    replay_data::{
        ReplayAttributeScope, ReplayAttributes, ReplayDetails, ReplayHeader, ReplayInitData,
        ReplayMetadata,
    },
};
use mpq::Archive;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

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
}

impl ParsedReplayWithEvents {
    fn new(replay: ParsedReplay, events: Vec<ReplayEvent>) -> Self {
        Self { replay, events }
    }

    pub fn replay(&self) -> &ParsedReplay {
        &self.replay
    }

    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }

    pub fn take_replay(self) -> ParsedReplay {
        self.replay
    }

    pub fn take_events(&mut self) -> Vec<ReplayEvent> {
        std::mem::take(&mut self.events)
    }
}

impl ParsedReplay {
    fn new(
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
    ) -> Self {
        Self {
            path,
            base_build,
            header,
            details,
            details_backup,
            init_data,
            metadata,
            game_events,
            message_events,
            tracker_events,
            attributes,
            attribute_scopes,
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
enum ReplayEventDecodeMode {
    None,
    Split,
    Ordered,
}

pub struct ReplayParser;

impl ReplayParser {
    fn is_decode_truncated(err: &DecodeError) -> bool {
        match err {
            DecodeError::Truncated => true,
            DecodeError::Corrupted(message) => message.contains("buffer truncated"),
            _ => false,
        }
    }

    fn decode_replay_initdata_with_store_fallback(
        store: &crate::protocol::ProtocolStore,
        build: u32,
        raw: &[u8],
    ) -> Option<ReplayInitData> {
        let mut builds = store.known_builds();
        builds.sort_by_key(|candidate| candidate.abs_diff(build));

        for candidate in builds {
            let protocol = match store.build(candidate) {
                Ok(protocol) => protocol,
                Err(_) => continue,
            };

            if let Ok(value) = protocol.decode_replay_initdata(raw) {
                if let Ok(parsed) = ReplayInitData::from_value(value) {
                    return Some(parsed);
                }
            }
        }

        None
    }

    fn decode_replay_tracker_events_with_store_fallback(
        store: &crate::protocol::ProtocolStore,
        build: u32,
        raw: &[u8],
    ) -> Option<Vec<TrackerEvent>> {
        let mut builds = store.known_builds();
        builds.sort_by_key(|candidate| candidate.abs_diff(build));

        let mut empty_decode = None;
        for candidate in builds {
            let protocol = match store.build(candidate) {
                Ok(protocol) => protocol,
                Err(_) => continue,
            };

            match protocol.decode_replay_tracker_events(raw) {
                Ok(events) if !events.is_empty() => return Some(events),
                Ok(events) if empty_decode.is_none() => empty_decode = Some(events),
                Ok(_) | Err(_) => {}
            }
        }

        empty_decode
    }

    fn read_mpq_file(
        archive: &mut Archive,
        filename: &str,
    ) -> Result<Option<Vec<u8>>, DecodeError> {
        let file = match archive.open_file(filename) {
            Ok(file) => file,
            Err(err) => {
                if format!("{err}").contains("No such file")
                    || format!("{err}").contains("NotFound")
                {
                    return Ok(None);
                }
                return Err(err.into());
            }
        };

        let size = file.size() as usize;
        let mut data = vec![0u8; size];
        let read = file.read(archive, &mut data)?;
        data.truncate(read);
        Ok(Some(data))
    }

    fn read_user_data_header_content(path: &Path) -> Result<Vec<u8>, DecodeError> {
        let mut file = File::open(path)?;

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != b"MPQ\x1B" {
            return Err(DecodeError::Corrupted("not a SC2Replay MPQ\"".into()));
        }

        let mut header = [0u8; 12];
        file.read_exact(&mut header)?;
        // user_data_size = header[0..4] ignored for compatibility.
        let user_data_header_size =
            u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;

        let mut content = vec![0u8; user_data_header_size];
        file.seek(SeekFrom::Current(0))?;
        file.read_exact(&mut content)?;

        Ok(content)
    }

    fn extract_base_build(header: &ReplayHeader) -> Result<u32, DecodeError> {
        Ok(header.base_build())
    }

    fn decode_replay_ordered_events_with_store_fallback(
        store: &crate::protocol::ProtocolStore,
        protocol: &crate::decoder::ProtocolDefinition,
        build: u32,
        game_raw: &[u8],
        tracker_raw: Option<&[u8]>,
    ) -> Result<Vec<ReplayEvent>, DecodeError> {
        match protocol.decode_replay_ordered_events(game_raw, tracker_raw) {
            Ok(events) => Ok(events),
            Err(error) => {
                let Some(tracker_raw) = tracker_raw else {
                    return Err(error);
                };

                let game_events = match protocol.decode_replay_game_events(game_raw) {
                    Ok(events) => events,
                    Err(_) => return Err(error),
                };
                let tracker_events = Self::decode_replay_tracker_events_with_store_fallback(
                    store,
                    build,
                    tracker_raw,
                )
                .unwrap_or_default();

                let mut events = Vec::with_capacity(game_events.len() + tracker_events.len());
                events.extend(game_events.into_iter().map(ReplayEvent::Game));
                events.extend(tracker_events.into_iter().map(ReplayEvent::Tracker));
                events.sort_by_key(ReplayEvent::_gameloop);
                Ok(events)
            }
        }
    }

    fn decode_replay_ordered_events_with_store_fallback_filtered<F>(
        store: &crate::protocol::ProtocolStore,
        protocol: &crate::decoder::ProtocolDefinition,
        build: u32,
        game_raw: &[u8],
        tracker_raw: Option<&[u8]>,
        include_event: &F,
    ) -> Result<Vec<ReplayEvent>, DecodeError>
    where
        F: Fn(&str) -> bool,
    {
        match protocol.decode_replay_ordered_events_filtered(game_raw, tracker_raw, include_event) {
            Ok(events) => Ok(events),
            Err(error) => {
                let Some(tracker_raw) = tracker_raw else {
                    return Err(error);
                };

                let game_events = match protocol.decode_replay_game_events(game_raw) {
                    Ok(events) => events,
                    Err(_) => return Err(error),
                };
                let tracker_events = Self::decode_replay_tracker_events_with_store_fallback(
                    store,
                    build,
                    tracker_raw,
                )
                .unwrap_or_default();

                let mut events = Vec::with_capacity(game_events.len() + tracker_events.len());
                events.extend(
                    game_events
                        .into_iter()
                        .filter(|event| include_event(&event.event))
                        .map(ReplayEvent::Game),
                );
                events.extend(
                    tracker_events
                        .into_iter()
                        .filter(|event| include_event(&event.event))
                        .map(ReplayEvent::Tracker),
                );
                events.sort_by_key(ReplayEvent::_gameloop);
                Ok(events)
            }
        }
    }

    pub fn parse_file_with_store(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
        mode: ReplayParseMode,
    ) -> Result<ParsedReplay, DecodeError> {
        let event_mode = match mode {
            ReplayParseMode::Simple => ReplayEventDecodeMode::None,
            ReplayParseMode::Detailed => ReplayEventDecodeMode::Split,
        };
        Self::parse_file_with_store_internal(path, store, event_mode)
            .map(ParsedReplayWithEvents::take_replay)
    }

    pub fn parse_file_with_store_ordered_events(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
    ) -> Result<ParsedReplayWithEvents, DecodeError> {
        Self::parse_file_with_store_internal(path, store, ReplayEventDecodeMode::Ordered)
    }

    pub fn parse_ordered_events_with_store(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
    ) -> Result<Vec<ReplayEvent>, DecodeError> {
        Self::parse_ordered_events_with_store_filtered(path, store, |_| true)
    }

    pub fn parse_ordered_events_with_store_filtered<F>(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
        include_event: F,
    ) -> Result<Vec<ReplayEvent>, DecodeError>
    where
        F: Fn(&str) -> bool,
    {
        let header_blob = Self::read_user_data_header_content(path)?;
        let header = ReplayHeader::from_value(store.latest()?.decode_replay_header(&header_blob)?)?;

        let base_build = Self::extract_base_build(&header)?;
        let protocol = store.build(base_build).or_else(|_| {
            store
                .build_or_closest(base_build)
                .map_or_else(|| Err(DecodeError::ProtocolMissing(base_build)), Ok)
        })?;

        let mut archive = Archive::open(path)?;
        let game_data = Self::read_mpq_file(&mut archive, "replay.game.events")?
            .ok_or_else(|| DecodeError::Corrupted("missing file replay.game.events".to_string()))?;
        let tracker_data = Self::read_mpq_file(&mut archive, "replay.tracker.events")?;

        Self::decode_replay_ordered_events_with_store_fallback_filtered(
            store,
            protocol,
            base_build,
            &game_data,
            tracker_data.as_deref(),
            &include_event,
        )
        .map_err(|err| DecodeError::Corrupted(format!("decode replay events: {err}")))
    }

    fn parse_file_with_store_internal(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
        event_mode: ReplayEventDecodeMode,
    ) -> Result<ParsedReplayWithEvents, DecodeError> {
        let parse_events = event_mode != ReplayEventDecodeMode::None;
        let header_blob = Self::read_user_data_header_content(path)?;
        let header = ReplayHeader::from_value(store.latest()?.decode_replay_header(&header_blob)?)?;

        let base_build = Self::extract_base_build(&header)?;
        let protocol = store.build(base_build).or_else(|_| {
            store
                .build_or_closest(base_build)
                .map_or_else(|| Err(DecodeError::ProtocolMissing(base_build)), Ok)
        })?;

        let mut archive = Archive::open(path)?;

        let details = {
            let data = Self::read_mpq_file(&mut archive, "replay.details")?
                .ok_or_else(|| DecodeError::Corrupted("missing file replay.details".to_string()))?;
            Some(ReplayDetails::from_value(
                protocol.decode_replay_details(&data).map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.details: {err}"))
                })?,
            )?)
        };

        let details_backup = {
            let data =
                Self::read_mpq_file(&mut archive, "replay.details.backup")?.ok_or_else(|| {
                    DecodeError::Corrupted("missing file replay.details.backup".to_string())
                })?;
            Some(ReplayDetails::from_value(
                protocol.decode_replay_details(&data).map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.details.backup: {err}"))
                })?,
            )?)
        };

        let init_data = {
            let data = Self::read_mpq_file(&mut archive, "replay.initData")?.ok_or_else(|| {
                DecodeError::Corrupted("missing file replay.initData".to_string())
            })?;
            match protocol.decode_replay_initdata(&data) {
                Ok(value) => Some(ReplayInitData::from_value(value)?),
                Err(err) if Self::is_decode_truncated(&err) => {
                    if parse_events {
                        Self::decode_replay_initdata_with_store_fallback(store, base_build, &data)
                    } else {
                        None
                    }
                }
                Err(err) => {
                    return Err(DecodeError::Corrupted(format!(
                        "decode replay.initData: {err}"
                    )));
                }
            }
        };

        let message_events = {
            let data =
                Self::read_mpq_file(&mut archive, "replay.message.events")?.ok_or_else(|| {
                    DecodeError::Corrupted("missing file replay.message.events".to_string())
                })?;
            protocol
                .decode_replay_message_events(&data)
                .map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.message.events: {err}"))
                })?
        };

        let (game_events, tracker_events, ordered_events) = match event_mode {
            ReplayEventDecodeMode::None => (Vec::new(), Vec::new(), Vec::new()),
            ReplayEventDecodeMode::Split => {
                let game_events = {
                    let data = Self::read_mpq_file(&mut archive, "replay.game.events")?
                        .ok_or_else(|| {
                            DecodeError::Corrupted("missing file replay.game.events".to_string())
                        })?;
                    protocol.decode_replay_game_events(&data).map_err(|err| {
                        DecodeError::Corrupted(format!("decode replay.game.events: {err}"))
                    })?
                };

                let tracker_events =
                    match Self::read_mpq_file(&mut archive, "replay.tracker.events")? {
                        Some(data) => match protocol.decode_replay_tracker_events(&data) {
                            Ok(events) => events,
                            Err(_) => Self::decode_replay_tracker_events_with_store_fallback(
                                store, base_build, &data,
                            )
                            .unwrap_or_default(),
                        },
                        None => Vec::new(),
                    };

                (game_events, tracker_events, Vec::new())
            }
            ReplayEventDecodeMode::Ordered => {
                let data =
                    Self::read_mpq_file(&mut archive, "replay.game.events")?.ok_or_else(|| {
                        DecodeError::Corrupted("missing file replay.game.events".to_string())
                    })?;
                let tracker_data = Self::read_mpq_file(&mut archive, "replay.tracker.events")?;
                let events = Self::decode_replay_ordered_events_with_store_fallback(
                    store,
                    protocol,
                    base_build,
                    &data,
                    tracker_data.as_deref(),
                )
                .map_err(|err| DecodeError::Corrupted(format!("decode replay events: {err}")))?;
                (Vec::new(), Vec::new(), events)
            }
        };

        let metadata = Self::read_mpq_file(&mut archive, "replay.gamemetadata.json")?
            .map(|raw| serde_json::from_slice(&raw))
            .transpose()
            .map_err(|err| {
                DecodeError::Corrupted(format!("decode replay.gamemetadata.json: {err}"))
            })?
            .map(ReplayMetadata::from_json_value)
            .transpose()?;

        let (attributes, attribute_scopes) =
            if let Some(raw) = Self::read_mpq_file(&mut archive, "replay.attributes.events")? {
                let value = protocol
                    .decode_replay_attributes_events(&raw)
                    .map_err(|err| {
                        DecodeError::Corrupted(format!("decode replay.attributes.events: {err}"))
                    })?;
                let attributes = ReplayAttributes::from_value(value)?;
                let scopes = attributes.scope_attributes();
                (Some(attributes), scopes)
            } else {
                (None, Vec::new())
            };

        Ok(ParsedReplayWithEvents::new(
            ParsedReplay::new(
                path.display().to_string(),
                base_build,
                header,
                details,
                details_backup,
                init_data,
                metadata,
                game_events,
                message_events,
                tracker_events,
                attributes,
                attribute_scopes,
            ),
            ordered_events,
        ))
    }
}

pub struct ReplayFormat;

impl ReplayFormat {
    pub fn convert_fourcc(bytes: &[u8]) -> String {
        let mut s = String::new();
        for byte in bytes {
            if *byte != 0 {
                s.push(*byte as char);
            }
        }
        s
    }

    pub fn cache_handle_uri(handle: &[u8]) -> Option<String> {
        if handle.len() < 8 {
            return None;
        }

        let purpose = Self::convert_fourcc(&handle[0..4]);
        let region = Self::convert_fourcc(&handle[4..8]);
        let hash = Self::bytes_to_hex(&handle[8..]);
        if purpose.is_empty() || region.is_empty() {
            return None;
        }

        Some(format!(
            "http://{}.depot.battle.net:1119/{}.{}",
            region.to_ascii_lowercase(),
            hash.to_ascii_lowercase(),
            purpose.to_ascii_lowercase()
        ))
    }

    fn bytes_to_hex(value: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(value.len() * 2);
        for byte in value {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }
}

pub struct UnitTag;

impl UnitTag {
    /// Convert a unit index/recycle pair to a unit tag value.
    pub fn from_parts(unit_tag_index: i128, unit_tag_recycle: i128) -> i128 {
        (unit_tag_index << 18) + unit_tag_recycle
    }

    /// Extract the unit index from a unit tag.
    pub fn index(unit_tag: i128) -> i128 {
        (unit_tag >> 18) & 0x00003fff
    }

    /// Extract the unit recycle value from a unit tag.
    pub fn recycle(unit_tag: i128) -> i128 {
        unit_tag & 0x0003ffff
    }
}
