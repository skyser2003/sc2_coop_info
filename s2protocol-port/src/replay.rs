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
use std::time::{Duration, Instant};

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
}

impl ParsedReplayWithEvents {
    fn new(replay: ParsedReplay, events: Vec<ReplayEvent>, timing: ReplayParseTiming) -> Self {
        Self {
            replay,
            events,
            timing,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplayParseTiming {
    total: Duration,
    read_header: Duration,
    decode_header: Duration,
    resolve_protocol: Duration,
    open_archive: Duration,
    mpq_open_file: Duration,
    mpq_read_file: Duration,
    mpq_bytes_read: u64,
    read_details: Duration,
    decode_details: Duration,
    parse_details: Duration,
    read_details_backup: Duration,
    decode_details_backup: Duration,
    parse_details_backup: Duration,
    read_init_data: Duration,
    decode_init_data: Duration,
    init_data_fallback: Duration,
    parse_init_data: Duration,
    read_message_events: Duration,
    decode_message_events: Duration,
    read_game_events: Duration,
    read_tracker_events: Duration,
    decode_game_events: Duration,
    decode_tracker_events: Duration,
    decode_ordered_events: Duration,
    read_metadata: Duration,
    decode_metadata_json: Duration,
    parse_metadata: Duration,
    read_attributes: Duration,
    decode_attributes: Duration,
    parse_attributes: Duration,
    build_result: Duration,
}

impl ReplayParseTiming {
    fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    fn add_mpq_read(&mut self, timing: &ReplayMpqFileTiming) {
        self.mpq_open_file += timing.open_file;
        self.mpq_read_file += timing.read_file;
        self.mpq_bytes_read = self.mpq_bytes_read.saturating_add(timing.bytes_read);
    }

    pub fn add(&mut self, other: &Self) {
        self.total += other.total;
        self.read_header += other.read_header;
        self.decode_header += other.decode_header;
        self.resolve_protocol += other.resolve_protocol;
        self.open_archive += other.open_archive;
        self.mpq_open_file += other.mpq_open_file;
        self.mpq_read_file += other.mpq_read_file;
        self.mpq_bytes_read = self.mpq_bytes_read.saturating_add(other.mpq_bytes_read);
        self.read_details += other.read_details;
        self.decode_details += other.decode_details;
        self.parse_details += other.parse_details;
        self.read_details_backup += other.read_details_backup;
        self.decode_details_backup += other.decode_details_backup;
        self.parse_details_backup += other.parse_details_backup;
        self.read_init_data += other.read_init_data;
        self.decode_init_data += other.decode_init_data;
        self.init_data_fallback += other.init_data_fallback;
        self.parse_init_data += other.parse_init_data;
        self.read_message_events += other.read_message_events;
        self.decode_message_events += other.decode_message_events;
        self.read_game_events += other.read_game_events;
        self.read_tracker_events += other.read_tracker_events;
        self.decode_game_events += other.decode_game_events;
        self.decode_tracker_events += other.decode_tracker_events;
        self.decode_ordered_events += other.decode_ordered_events;
        self.read_metadata += other.read_metadata;
        self.decode_metadata_json += other.decode_metadata_json;
        self.parse_metadata += other.parse_metadata;
        self.read_attributes += other.read_attributes;
        self.decode_attributes += other.decode_attributes;
        self.parse_attributes += other.parse_attributes;
        self.build_result += other.build_result;
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn read_header(&self) -> Duration {
        self.read_header
    }

    pub fn decode_header(&self) -> Duration {
        self.decode_header
    }

    pub fn resolve_protocol(&self) -> Duration {
        self.resolve_protocol
    }

    pub fn open_archive(&self) -> Duration {
        self.open_archive
    }

    pub fn mpq_open_file(&self) -> Duration {
        self.mpq_open_file
    }

    pub fn mpq_read_file(&self) -> Duration {
        self.mpq_read_file
    }

    pub fn mpq_bytes_read(&self) -> u64 {
        self.mpq_bytes_read
    }

    pub fn read_details(&self) -> Duration {
        self.read_details
    }

    pub fn decode_details(&self) -> Duration {
        self.decode_details
    }

    pub fn parse_details(&self) -> Duration {
        self.parse_details
    }

    pub fn read_details_backup(&self) -> Duration {
        self.read_details_backup
    }

    pub fn decode_details_backup(&self) -> Duration {
        self.decode_details_backup
    }

    pub fn parse_details_backup(&self) -> Duration {
        self.parse_details_backup
    }

    pub fn read_init_data(&self) -> Duration {
        self.read_init_data
    }

    pub fn decode_init_data(&self) -> Duration {
        self.decode_init_data
    }

    pub fn init_data_fallback(&self) -> Duration {
        self.init_data_fallback
    }

    pub fn parse_init_data(&self) -> Duration {
        self.parse_init_data
    }

    pub fn read_message_events(&self) -> Duration {
        self.read_message_events
    }

    pub fn decode_message_events(&self) -> Duration {
        self.decode_message_events
    }

    pub fn read_game_events(&self) -> Duration {
        self.read_game_events
    }

    pub fn read_tracker_events(&self) -> Duration {
        self.read_tracker_events
    }

    pub fn decode_game_events(&self) -> Duration {
        self.decode_game_events
    }

    pub fn decode_tracker_events(&self) -> Duration {
        self.decode_tracker_events
    }

    pub fn decode_ordered_events(&self) -> Duration {
        self.decode_ordered_events
    }

    pub fn read_metadata(&self) -> Duration {
        self.read_metadata
    }

    pub fn decode_metadata_json(&self) -> Duration {
        self.decode_metadata_json
    }

    pub fn parse_metadata(&self) -> Duration {
        self.parse_metadata
    }

    pub fn read_attributes(&self) -> Duration {
        self.read_attributes
    }

    pub fn decode_attributes(&self) -> Duration {
        self.decode_attributes
    }

    pub fn parse_attributes(&self) -> Duration {
        self.parse_attributes
    }

    pub fn build_result(&self) -> Duration {
        self.build_result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ReplayMpqFileTiming {
    open_file: Duration,
    read_file: Duration,
    bytes_read: u64,
}

impl ReplayMpqFileTiming {
    fn new(open_file: Duration, read_file: Duration, bytes_read: u64) -> Self {
        Self {
            open_file,
            read_file,
            bytes_read,
        }
    }
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
        Self::read_mpq_file_timed(archive, filename).map(|(data, _timing)| data)
    }

    fn read_mpq_file_timed(
        archive: &mut Archive,
        filename: &str,
    ) -> Result<(Option<Vec<u8>>, ReplayMpqFileTiming), DecodeError> {
        let open_file_start = Instant::now();
        let file = match archive.open_file(filename) {
            Ok(file) => file,
            Err(err) => {
                let timing = ReplayMpqFileTiming::new(open_file_start.elapsed(), Duration::ZERO, 0);
                if format!("{err}").contains("No such file")
                    || format!("{err}").contains("NotFound")
                {
                    return Ok((None, timing));
                }
                return Err(err.into());
            }
        };
        let open_file = open_file_start.elapsed();

        let size = file.size() as usize;
        let mut data = vec![0u8; size];
        let read_file_start = Instant::now();
        let read = file.read(archive, &mut data)?;
        let read_file = read_file_start.elapsed();
        data.truncate(read);
        Ok((
            Some(data),
            ReplayMpqFileTiming::new(open_file, read_file, read as u64),
        ))
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

    fn decode_replay_ordered_events_with_store_fallback_filtered(
        store: &crate::protocol::ProtocolStore,
        protocol: &crate::decoder::ProtocolDefinition,
        build: u32,
        game_raw: &[u8],
        tracker_raw: Option<&[u8]>,
        include_event: &dyn Fn(&str) -> bool,
    ) -> Result<Vec<ReplayEvent>, DecodeError> {
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
        Self::parse_file_with_store_timed(path, store, mode)
            .map(ParsedReplayWithEvents::take_replay)
    }

    pub fn parse_file_with_store_timed(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
        mode: ReplayParseMode,
    ) -> Result<ParsedReplayWithEvents, DecodeError> {
        let event_mode = match mode {
            ReplayParseMode::Simple => ReplayEventDecodeMode::None,
            ReplayParseMode::Detailed => ReplayEventDecodeMode::Split,
        };
        Self::parse_file_with_store_internal(path, store, event_mode, None)
    }

    pub fn parse_file_with_store_ordered_events(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
    ) -> Result<ParsedReplayWithEvents, DecodeError> {
        Self::parse_file_with_store_internal(path, store, ReplayEventDecodeMode::Ordered, None)
    }

    pub fn parse_file_with_store_ordered_events_filtered<F>(
        path: &Path,
        store: &crate::protocol::ProtocolStore,
        include_event: F,
    ) -> Result<ParsedReplayWithEvents, DecodeError>
    where
        F: Fn(&str) -> bool,
    {
        Self::parse_file_with_store_internal(
            path,
            store,
            ReplayEventDecodeMode::Ordered,
            Some(&include_event),
        )
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
        include_ordered_event: Option<&dyn Fn(&str) -> bool>,
    ) -> Result<ParsedReplayWithEvents, DecodeError> {
        let total_start = Instant::now();
        let mut timing = ReplayParseTiming::default();
        let parse_events = event_mode != ReplayEventDecodeMode::None;
        let read_header_start = Instant::now();
        let header_blob = Self::read_user_data_header_content(path)?;
        timing.read_header = read_header_start.elapsed();

        let decode_header_start = Instant::now();
        let header = ReplayHeader::from_value(store.latest()?.decode_replay_header(&header_blob)?)?;
        timing.decode_header = decode_header_start.elapsed();

        let resolve_protocol_start = Instant::now();
        let base_build = Self::extract_base_build(&header)?;
        let protocol = store.build(base_build).or_else(|_| {
            store
                .build_or_closest(base_build)
                .map_or_else(|| Err(DecodeError::ProtocolMissing(base_build)), Ok)
        })?;
        timing.resolve_protocol = resolve_protocol_start.elapsed();

        let open_archive_start = Instant::now();
        let mut archive = Archive::open(path)?;
        timing.open_archive = open_archive_start.elapsed();

        let details = {
            let (data, read_timing) = Self::read_mpq_file_timed(&mut archive, "replay.details")?;
            timing.add_mpq_read(&read_timing);
            timing.read_details = read_timing.open_file + read_timing.read_file;
            let data = data
                .ok_or_else(|| DecodeError::Corrupted("missing file replay.details".to_string()))?;
            let decode_details_start = Instant::now();
            let value = protocol
                .decode_replay_details(&data)
                .map_err(|err| DecodeError::Corrupted(format!("decode replay.details: {err}")))?;
            timing.decode_details = decode_details_start.elapsed();
            let parse_details_start = Instant::now();
            let parsed = ReplayDetails::from_value(value)?;
            timing.parse_details = parse_details_start.elapsed();
            Some(parsed)
        };

        let details_backup = {
            let (data, read_timing) =
                Self::read_mpq_file_timed(&mut archive, "replay.details.backup")?;
            timing.add_mpq_read(&read_timing);
            timing.read_details_backup = read_timing.open_file + read_timing.read_file;
            let data = data.ok_or_else(|| {
                DecodeError::Corrupted("missing file replay.details.backup".to_string())
            })?;
            let decode_details_backup_start = Instant::now();
            let value = protocol.decode_replay_details(&data).map_err(|err| {
                DecodeError::Corrupted(format!("decode replay.details.backup: {err}"))
            })?;
            timing.decode_details_backup = decode_details_backup_start.elapsed();
            let parse_details_backup_start = Instant::now();
            let parsed = ReplayDetails::from_value(value)?;
            timing.parse_details_backup = parse_details_backup_start.elapsed();
            Some(parsed)
        };

        let init_data = {
            let (data, read_timing) = Self::read_mpq_file_timed(&mut archive, "replay.initData")?;
            timing.add_mpq_read(&read_timing);
            timing.read_init_data = read_timing.open_file + read_timing.read_file;
            let data = data.ok_or_else(|| {
                DecodeError::Corrupted("missing file replay.initData".to_string())
            })?;
            let decode_init_data_start = Instant::now();
            match protocol.decode_replay_initdata(&data) {
                Ok(value) => {
                    timing.decode_init_data = decode_init_data_start.elapsed();
                    let parse_init_data_start = Instant::now();
                    let parsed = ReplayInitData::from_value(value)?;
                    timing.parse_init_data = parse_init_data_start.elapsed();
                    Some(parsed)
                }
                Err(err) if Self::is_decode_truncated(&err) => {
                    timing.decode_init_data = decode_init_data_start.elapsed();
                    if parse_events {
                        let fallback_start = Instant::now();
                        let parsed = Self::decode_replay_initdata_with_store_fallback(
                            store, base_build, &data,
                        );
                        timing.init_data_fallback = fallback_start.elapsed();
                        parsed
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
            let (data, read_timing) =
                Self::read_mpq_file_timed(&mut archive, "replay.message.events")?;
            timing.add_mpq_read(&read_timing);
            timing.read_message_events = read_timing.open_file + read_timing.read_file;
            let data = data.ok_or_else(|| {
                DecodeError::Corrupted("missing file replay.message.events".to_string())
            })?;
            let decode_message_events_start = Instant::now();
            protocol
                .decode_replay_message_events(&data)
                .map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.message.events: {err}"))
                })
                .inspect(|_| {
                    timing.decode_message_events = decode_message_events_start.elapsed();
                })?
        };

        let (game_events, tracker_events, ordered_events) = match event_mode {
            ReplayEventDecodeMode::None => (Vec::new(), Vec::new(), Vec::new()),
            ReplayEventDecodeMode::Split => {
                let game_events = {
                    let (data, read_timing) =
                        Self::read_mpq_file_timed(&mut archive, "replay.game.events")?;
                    timing.add_mpq_read(&read_timing);
                    timing.read_game_events = read_timing.open_file + read_timing.read_file;
                    let data = data.ok_or_else(|| {
                        DecodeError::Corrupted("missing file replay.game.events".to_string())
                    })?;
                    let decode_game_events_start = Instant::now();
                    let events = protocol.decode_replay_game_events(&data).map_err(|err| {
                        DecodeError::Corrupted(format!("decode replay.game.events: {err}"))
                    })?;
                    timing.decode_game_events = decode_game_events_start.elapsed();
                    events
                };

                let (tracker_data, read_timing) =
                    Self::read_mpq_file_timed(&mut archive, "replay.tracker.events")?;
                timing.add_mpq_read(&read_timing);
                timing.read_tracker_events = read_timing.open_file + read_timing.read_file;
                let tracker_events = match tracker_data {
                    Some(data) => {
                        let decode_tracker_events_start = Instant::now();
                        let events = match protocol.decode_replay_tracker_events(&data) {
                            Ok(events) => events,
                            Err(_) => Self::decode_replay_tracker_events_with_store_fallback(
                                store, base_build, &data,
                            )
                            .unwrap_or_default(),
                        };
                        timing.decode_tracker_events = decode_tracker_events_start.elapsed();
                        events
                    }
                    None => Vec::new(),
                };

                (game_events, tracker_events, Vec::new())
            }
            ReplayEventDecodeMode::Ordered => {
                let (data, read_timing) =
                    Self::read_mpq_file_timed(&mut archive, "replay.game.events")?;
                timing.add_mpq_read(&read_timing);
                timing.read_game_events = read_timing.open_file + read_timing.read_file;
                let data = data.ok_or_else(|| {
                    DecodeError::Corrupted("missing file replay.game.events".to_string())
                })?;
                let (tracker_data, read_timing) =
                    Self::read_mpq_file_timed(&mut archive, "replay.tracker.events")?;
                timing.add_mpq_read(&read_timing);
                timing.read_tracker_events = read_timing.open_file + read_timing.read_file;
                let decode_ordered_events_start = Instant::now();
                let events = match include_ordered_event {
                    Some(include_event) => {
                        Self::decode_replay_ordered_events_with_store_fallback_filtered(
                            store,
                            protocol,
                            base_build,
                            &data,
                            tracker_data.as_deref(),
                            include_event,
                        )
                    }
                    None => Self::decode_replay_ordered_events_with_store_fallback(
                        store,
                        protocol,
                        base_build,
                        &data,
                        tracker_data.as_deref(),
                    ),
                }
                .map_err(|err| DecodeError::Corrupted(format!("decode replay events: {err}")))?;
                timing.decode_ordered_events = decode_ordered_events_start.elapsed();
                (Vec::new(), Vec::new(), events)
            }
        };

        let (metadata_raw, read_timing) =
            Self::read_mpq_file_timed(&mut archive, "replay.gamemetadata.json")?;
        timing.add_mpq_read(&read_timing);
        timing.read_metadata = read_timing.open_file + read_timing.read_file;
        let metadata = match metadata_raw {
            Some(raw) => {
                let decode_metadata_json_start = Instant::now();
                let value = serde_json::from_slice(&raw).map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.gamemetadata.json: {err}"))
                })?;
                timing.decode_metadata_json = decode_metadata_json_start.elapsed();
                let parse_metadata_start = Instant::now();
                let parsed = ReplayMetadata::from_json_value(value)?;
                timing.parse_metadata = parse_metadata_start.elapsed();
                Some(parsed)
            }
            None => None,
        };

        let (attributes, attribute_scopes) = if let Some(raw) = {
            let (raw, read_timing) =
                Self::read_mpq_file_timed(&mut archive, "replay.attributes.events")?;
            timing.add_mpq_read(&read_timing);
            timing.read_attributes = read_timing.open_file + read_timing.read_file;
            raw
        } {
            let decode_attributes_start = Instant::now();
            let value = protocol
                .decode_replay_attributes_events(&raw)
                .map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.attributes.events: {err}"))
                })?;
            timing.decode_attributes = decode_attributes_start.elapsed();
            let parse_attributes_start = Instant::now();
            let attributes = ReplayAttributes::from_value(value)?;
            timing.parse_attributes = parse_attributes_start.elapsed();
            let scopes = attributes.scope_attributes();
            (Some(attributes), scopes)
        } else {
            (None, Vec::new())
        };

        let build_result_start = Instant::now();
        let replay = ParsedReplay::new(
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
        );
        timing.build_result = build_result_start.elapsed();
        let timing = timing.finish(total_start.elapsed());
        Ok(ParsedReplayWithEvents::new(replay, ordered_events, timing))
    }
}
