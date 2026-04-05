use crate::{
    error::DecodeError,
    events::{GameEvent, MessageEvent, TrackerEvent},
    replay_data::{
        process_scope_attributes, ReplayAttributeScope, ReplayAttributes, ReplayDetails,
        ReplayHeader, ReplayInitData, ReplayMetadata,
    },
};
use mpq::Archive;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct ParseResult {
    pub path: String,
    pub base_build: u32,
    pub header: ReplayHeader,
}

#[derive(Debug, Clone)]
pub struct ParsedReplay {
    pub path: String,
    pub base_build: u32,
    pub header: ReplayHeader,
    pub details: Option<ReplayDetails>,
    pub details_backup: Option<ReplayDetails>,
    pub init_data: Option<ReplayInitData>,
    pub metadata: Option<ReplayMetadata>,
    pub game_events: Vec<GameEvent>,
    pub message_events: Vec<MessageEvent>,
    pub tracker_events: Vec<TrackerEvent>,
    pub attributes: Option<ReplayAttributes>,
    pub attribute_scopes: Vec<ReplayAttributeScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayParseMode {
    Simple,
    Detailed,
}

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

fn read_mpq_file(archive: &mut Archive, filename: &str) -> Result<Option<Vec<u8>>, DecodeError> {
    let file = match archive.open_file(filename) {
        Ok(file) => file,
        Err(err) => {
            if format!("{err}").contains("No such file") || format!("{err}").contains("NotFound") {
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
    Ok(header.m_version.m_baseBuild)
}

pub fn parse_file_with_store(
    path: &Path,
    store: &crate::protocol::ProtocolStore,
    mode: ReplayParseMode,
) -> Result<ParsedReplay, DecodeError> {
    parse_file_with_store_internal(path, store, mode)
}

fn parse_file_with_store_internal(
    path: &Path,
    store: &crate::protocol::ProtocolStore,
    mode: ReplayParseMode,
) -> Result<ParsedReplay, DecodeError> {
    let parse_events = matches!(mode, ReplayParseMode::Detailed);
    let header_blob = read_user_data_header_content(path)?;
    let header = ReplayHeader::from_value(store.latest()?.decode_replay_header(&header_blob)?)?;

    let base_build = extract_base_build(&header)?;
    let protocol = store.build(base_build).or_else(|_| {
        store
            .build_or_closest(base_build)
            .map_or_else(|| Err(DecodeError::ProtocolMissing(base_build)), Ok)
    })?;

    let mut archive = Archive::open(path)?;

    let details = {
        let data = read_mpq_file(&mut archive, "replay.details")?
            .ok_or_else(|| DecodeError::Corrupted("missing file replay.details".to_string()))?;
        Some(ReplayDetails::from_value(
            protocol
                .decode_replay_details(&data)
                .map_err(|err| DecodeError::Corrupted(format!("decode replay.details: {err}")))?,
        )?)
    };

    let details_backup = {
        let data = read_mpq_file(&mut archive, "replay.details.backup")?.ok_or_else(|| {
            DecodeError::Corrupted("missing file replay.details.backup".to_string())
        })?;
        Some(ReplayDetails::from_value(
            protocol.decode_replay_details(&data).map_err(|err| {
                DecodeError::Corrupted(format!("decode replay.details.backup: {err}"))
            })?,
        )?)
    };

    let init_data = {
        let data = read_mpq_file(&mut archive, "replay.initData")?
            .ok_or_else(|| DecodeError::Corrupted("missing file replay.initData".to_string()))?;
        match protocol.decode_replay_initdata(&data) {
            Ok(value) => Some(ReplayInitData::from_value(value)?),
            Err(err) if is_decode_truncated(&err) => {
                if parse_events {
                    decode_replay_initdata_with_store_fallback(store, base_build, &data)
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
        let data = read_mpq_file(&mut archive, "replay.message.events")?.ok_or_else(|| {
            DecodeError::Corrupted("missing file replay.message.events".to_string())
        })?;
        protocol
            .decode_replay_message_events(&data)
            .map_err(|err| DecodeError::Corrupted(format!("decode replay.message.events: {err}")))?
    };

    let (game_events, tracker_events) = if parse_events {
        let game_events = {
            let data = read_mpq_file(&mut archive, "replay.game.events")?.ok_or_else(|| {
                DecodeError::Corrupted("missing file replay.game.events".to_string())
            })?;
            protocol.decode_replay_game_events(&data).map_err(|err| {
                DecodeError::Corrupted(format!("decode replay.game.events: {err}"))
            })?
        };

        let tracker_events = {
            match read_mpq_file(&mut archive, "replay.tracker.events")? {
                Some(data) => match protocol.decode_replay_tracker_events(&data) {
                    Ok(events) => events,
                    Err(_) => {
                        if parse_events {
                            decode_replay_tracker_events_with_store_fallback(
                                store, base_build, &data,
                            )
                            .unwrap_or_default()
                        } else {
                            Vec::new()
                        }
                    }
                },
                None => Vec::new(),
            }
        };

        (game_events, tracker_events)
    } else {
        (Vec::new(), Vec::new())
    };

    let metadata = read_mpq_file(&mut archive, "replay.gamemetadata.json")?
        .map(|raw| serde_json::from_slice(&raw))
        .transpose()
        .map_err(|err| DecodeError::Corrupted(format!("decode replay.gamemetadata.json: {err}")))?
        .map(ReplayMetadata::from_json_value)
        .transpose()?;

    let (attributes, attribute_scopes) =
        if let Some(raw) = read_mpq_file(&mut archive, "replay.attributes.events")? {
            let value = protocol
                .decode_replay_attributes_events(&raw)
                .map_err(|err| {
                    DecodeError::Corrupted(format!("decode replay.attributes.events: {err}"))
                })?;
            let attributes = ReplayAttributes::from_value(value)?;
            let scopes = process_scope_attributes(&attributes);
            (Some(attributes), scopes)
        } else {
            (None, Vec::new())
        };

    Ok(ParsedReplay {
        path: path.display().to_string(),
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
    })
}

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

    let purpose = convert_fourcc(&handle[0..4]);
    let region = convert_fourcc(&handle[4..8]);
    let hash = bytes_to_hex(&handle[8..]);
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

/// Convert a unit index/recycle pair to a unit tag value.
pub fn unit_tag(unit_tag_index: i128, unit_tag_recycle: i128) -> i128 {
    (unit_tag_index << 18) + unit_tag_recycle
}

/// Extract the unit index from a unit tag.
pub fn unit_tag_index(unit_tag: i128) -> i128 {
    (unit_tag >> 18) & 0x00003fff
}

/// Extract the unit recycle value from a unit tag.
pub fn unit_tag_recycle(unit_tag: i128) -> i128 {
    unit_tag & 0x0003ffff
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
