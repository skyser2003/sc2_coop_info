use crate::cache_overall_stats_generator::{
    PlayerStatsSeries, ProtocolBuildValue, ReplayBuildInfo,
};
use crate::dictionary_data::{
    self, CacheGenerationData, CachedMutatorsJson, DictionaryDataError, MutatorIdsJson,
    UnitAddKillsToJson, UnitNamesJson,
};
use crate::tauri_replay_analysis_impl::{
    build_replay_report_detailed, ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer,
    PlayerPositions, ReplayReport, ReplayReportDetailedInput,
};
use chrono::{DateTime, Local};
use indexmap::IndexMap;
use s2protocol_port::{
    build_protocol_store, parse_file_with_store, process_details_data, process_init_data,
    ProtocolStore, ReplayEvent, ReplayParseMode, TrackerEvent, Value,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

#[path = "replay_event_handlers.rs"]
mod replay_event_handlers;

use crate::stats_counter_core::{
    ReplayDroneIdentifierCore, ReplayStatsCounterCore, StatsCounterDictionaries,
};
use replay_event_handlers::{
    replay_handle_archon_init_event_control_pid, replay_handle_game_user_leave_event_fields,
    replay_handle_player_stats_event_fields, replay_handle_unit_born_or_init_event_fields,
    replay_handle_unit_died_detail_event_fields, replay_handle_unit_died_kill_stats_event_fields,
    replay_handle_unit_owner_change_event_fields, replay_handle_unit_type_change_event_fields,
    replay_handle_upgrade_event_fields, IdentifiedWavesMap, StatsCounterTarget,
    UnitBornOrInitEventFields, UnitDiedEventFields, UnitStateMap, UnitTypeChangeEventFields,
    UnitTypeCountMap, WaveUnitsState,
};

const LOCUST_SOURCE_UNITS: [&str; 5] = [
    "AbathurLocust",
    "Locust",
    "LocustMP",
    "DehakaCreeperFlying",
    "DehakaLocust",
];

const BROODLING_SOURCE_UNITS: [&str; 6] = [
    "BroodlingEscortStetmann",
    "BroodlingEscort",
    "Broodling",
    "KerriganInfestBroodling",
    "StukovInfestBroodling",
    "BroodlingStetmann",
];

const ZERATUL_ARTIFACT_PICKUPS: [&str; 4] = [
    "ZeratulArtifactPickup1",
    "ZeratulArtifactPickup2",
    "ZeratulArtifactPickup3",
    "ZeratulArtifactPickupUnlimited",
];

const ZERATUL_SHADE_PROJECTIONS: [&str; 2] = [
    "ZeratulKhaydarinMonolithProjection",
    "ZeratulPhotonCannonProjection",
];

const CUSTOM_KILL_ICON_KEYS: [&str; 10] = [
    "hfts",
    "tus",
    "propagators",
    "voidrifts",
    "turkey",
    "voidreanimators",
    "deadofnight",
    "minesweeper",
    "missilecommand",
    "shuttles",
];

type UnitStats = (i64, i64, i64, f64);

#[derive(Debug, Error)]
pub enum DetailedReplayAnalysisError {
    #[error("failed to build protocol store: {0}")]
    ProtocolStore(String),
    #[error("failed to parse replay '{path}': {message}")]
    ReplayParse { path: String, message: String },
    #[error("SC2 dictionary data directory was not found from '{0}'")]
    DictionaryDirNotFound(PathBuf),
    #[error("failed to read '{path}': {message}")]
    IoRead { path: PathBuf, message: String },
    #[error("failed to parse JSON '{path}': {message}")]
    JsonParse { path: PathBuf, message: String },
    #[error("invalid dictionary file '{file}': {message}")]
    InvalidDictionaryData { file: &'static str, message: String },
    #[error("invalid replay data: {0}")]
    InvalidReplayData(String),
}

#[derive(Clone, Debug)]
struct ParsedReplayAnalysisInput {
    parser: ParsedReplayInput,
    events: Vec<ReplayEvent>,
    start_time: f64,
    end_time: f64,
}

fn protocol_store() -> Result<&'static ProtocolStore, DetailedReplayAnalysisError> {
    static STORE: OnceLock<Result<ProtocolStore, String>> = OnceLock::new();
    let store = STORE.get_or_init(|| {
        build_protocol_store().map_err(|error| format!("failed to build protocol store: {error}"))
    });

    store
        .as_ref()
        .map_err(|message| DetailedReplayAnalysisError::ProtocolStore(message.clone()))
}

fn load_sc2_dictionary_data() -> Result<CacheGenerationData<'static>, DetailedReplayAnalysisError> {
    dictionary_data::cache_generation_data().map_err(map_dictionary_data_error)
}

fn map_dictionary_data_error(error: DictionaryDataError) -> DetailedReplayAnalysisError {
    match error {
        DictionaryDataError::DictionaryDirNotFound(path) => {
            DetailedReplayAnalysisError::DictionaryDirNotFound(path)
        }
        DictionaryDataError::IoRead { path, message } => {
            DetailedReplayAnalysisError::IoRead { path, message }
        }
        DictionaryDataError::JsonParse { path, message } => {
            DetailedReplayAnalysisError::JsonParse { path, message }
        }
        DictionaryDataError::InvalidDictionaryData { file, message } => {
            DetailedReplayAnalysisError::InvalidDictionaryData { file, message }
        }
    }
}

pub fn cache_hidden_created_lost_units() -> Result<HashSet<String>, DetailedReplayAnalysisError> {
    Ok(load_sc2_dictionary_data()?
        .replay_analysis_data
        .dont_show_created_lost
        .iter()
        .cloned()
        .collect())
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i128()
        .and_then(|value| i64::try_from(value).ok())
        .or_else(|| match value {
            Value::Float(number) => Some(*number as i64),
            Value::String(text) => text.parse::<i64>().ok(),
            _ => None,
        })
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value_as_i64(value).map(|value| value as f64))
}

fn value_as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::Int(value) => Some(*value != 0),
        Value::Float(value) => Some(*value != 0.0),
        Value::String(text) => {
            if text.eq_ignore_ascii_case("true") || text == "1" {
                Some(true)
            } else if text.eq_ignore_ascii_case("false") || text == "0" {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn value_as_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(flag) => flag.to_string(),
        Value::Int(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
        Value::Array(_) | Value::Object(_) => String::new(),
    }
}

fn value_array(value: &Value) -> Option<&[Value]> {
    match value {
        Value::Array(values) => Some(values),
        _ => None,
    }
}

fn nested_value<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = current.get_key(key)?;
    }
    Some(current)
}

fn event_name(event: &ReplayEvent) -> &str {
    event._event()
}

fn event_gameloop(event: &ReplayEvent) -> i64 {
    event._gameloop()
}

fn event_control_id(event: &ReplayEvent) -> Option<i64> {
    match event {
        ReplayEvent::Game(event) => event.m_control_id,
        ReplayEvent::Tracker(_) => None,
    }
}

fn event_event_type(event: &ReplayEvent) -> Option<i64> {
    match event {
        ReplayEvent::Game(event) => event.m_event_type,
        ReplayEvent::Tracker(_) => None,
    }
}

fn event_user_id(event: &ReplayEvent) -> Option<i64> {
    match event {
        ReplayEvent::Game(event) => event.user_id,
        ReplayEvent::Tracker(_) => None,
    }
}

fn parse_masteries(values: Option<&[Value]>) -> [u32; 6] {
    let mut out = [0_u32; 6];
    if let Some(values) = values {
        for (index, value) in values.iter().take(6).enumerate() {
            out[index] = value_as_i64(value)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_default();
        }
    }
    out
}

fn file_date_string(file: &Path) -> Result<String, DetailedReplayAnalysisError> {
    let modified = fs::metadata(file)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| DetailedReplayAnalysisError::IoRead {
            path: file.to_path_buf(),
            message: error.to_string(),
        })?;
    let datetime: DateTime<Local> = DateTime::from(modified);
    Ok(datetime.format("%Y:%m:%d:%H:%M:%S").to_string())
}

pub fn calculate_replay_hash(path: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => format!("{:x}", md5::compute(bytes)),
        Err(_) => format!("{:x}", md5::compute(path.to_string_lossy().as_bytes())),
    }
}

fn difficulty_name(code: i64) -> &'static str {
    match code {
        1 => "Casual",
        2 => "Normal",
        3 => "Hard",
        4 => "Brutal",
        _ => "",
    }
}

fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "00:00".to_string();
    }

    let total = seconds.floor() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn duration_to_u64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round_ties_even() as u64
    }
}

fn region_name(code: i64) -> &'static str {
    match code {
        1 => "NA",
        2 => "EU",
        3 => "KR",
        5 => "CN",
        98 => "PTR",
        _ => "",
    }
}

fn valid_protocol_mapping(build: i64) -> Option<i64> {
    match build {
        81102 => Some(81433),
        80871 => Some(81433),
        76811 => Some(76114),
        80188 => Some(78285),
        79998 => Some(78285),
        81433 => Some(83830),
        84643 => Some(83830),
        _ => None,
    }
}

fn supported_legacy_protocol(build: i64) -> bool {
    matches!(build, 76114 | 78285 | 83830)
}

fn list_item(value: &Value, index: usize) -> Option<&Value> {
    value_array(value).and_then(|items| items.get(index))
}

fn cache_handle_id(handle: &Value) -> String {
    match handle {
        Value::Bytes(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(&mut hex, "{byte:02x}");
            }
            if hex.len() > 16 {
                hex[16..].to_string()
            } else {
                String::new()
            }
        }
        Value::String(text) => {
            let tail = text.rsplit('/').next().unwrap_or("");
            tail.split('.').next().unwrap_or("").to_string()
        }
        _ => String::new(),
    }
}

fn mutator_from_button(button: i64, panel: i64, mutators_list: &[String]) -> Option<String> {
    let idx = (button - 41) / 3 + (panel - 1) * 15;
    if idx < 0 {
        return None;
    }
    let Ok(index) = usize::try_from(idx) else {
        return None;
    };
    mutators_list.get(index).cloned()
}

fn get_last_deselect_event(events: &[ReplayEvent]) -> Option<f64> {
    let mut last_event: Option<f64> = None;
    for event in events {
        if event_name(event) == "NNet.Game.SSelectionDeltaEvent" {
            last_event = Some(event_gameloop(event) as f64 / 16.0 - 2.0);
        }
    }
    last_event
}

fn get_start_time(events: &[ReplayEvent]) -> f64 {
    for event in events {
        if let ReplayEvent::Tracker(event) = event {
            if event.event == "NNet.Replay.Tracker.SPlayerStatsEvent"
                && event.m_player_id == Some(1)
            {
                let minerals = event
                    .m_stats
                    .as_ref()
                    .and_then(|stats| stats.m_score_value_minerals_collection_rate)
                    .unwrap_or_default();
                if minerals > 0.0 {
                    return event.game_loop as f64 / 16.0;
                }
            }

            if event.event == "NNet.Replay.Tracker.SUpgradeEvent"
                && matches!(event.m_player_id, Some(1 | 2))
            {
                let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                if upgrade_name.contains("Spray") {
                    return event.game_loop as f64 / 16.0;
                }
            }
        }
    }

    0.0
}

fn identify_mutators(
    events: &[ReplayEvent],
    mutators_all: &[String],
    mutators_ui: &[String],
    mutator_ids: &MutatorIdsJson,
    cached_mutators: &CachedMutatorsJson,
    extension: bool,
    mm: bool,
    detailed_info: Option<&Value>,
) -> (Vec<String>, bool) {
    let mut mutators: Vec<String> = Vec::new();
    let mut weekly = false;

    if mm {
        for event in events {
            let ReplayEvent::Tracker(event) = event else {
                continue;
            };
            if event.event != "NNet.Replay.Tracker.SUpgradeEvent" || event.m_player_id != Some(0) {
                continue;
            }
            let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
            if !upgrade_name.contains("mutatorinfo") {
                continue;
            }
            let mutator_key = upgrade_name.get(12..).unwrap_or_default();
            if mutator_ids.contains_key(mutator_key) {
                mutators.push(mutator_key.to_string());
            }
        }
    }

    if extension {
        if let Some(handles) = detailed_info
            .and_then(|value| {
                nested_value(
                    value,
                    &["m_syncLobbyState", "m_gameDescription", "m_cacheHandles"],
                )
            })
            .and_then(value_array)
        {
            for handle in handles {
                let cached = cache_handle_id(handle);
                if cached.is_empty() {
                    continue;
                }
                if let Some(mutator_id) = cached_mutators.get(&cached) {
                    mutators.push(mutator_id.clone());
                    weekly = true;
                }
            }
        }
    }

    if !extension {
        if let Some(slot0) = detailed_info
            .and_then(|value| nested_value(value, &["m_syncLobbyState", "m_lobbyState", "m_slots"]))
            .and_then(|value| list_item(value, 0))
        {
            let brutal_plus = slot0
                .get_key("m_brutalPlusDifficulty")
                .and_then(value_as_i64)
                .unwrap_or_default();
            if brutal_plus > 0 {
                if let Some(indexes) = slot0
                    .get_key("m_retryMutationIndexes")
                    .and_then(value_array)
                {
                    for key in indexes {
                        let key = value_as_i64(key).unwrap_or_default();
                        if key <= 0 {
                            continue;
                        }
                        if let Ok(index) = usize::try_from(key - 1) {
                            if let Some(mutator) = mutators_all.get(index) {
                                mutators.push(mutator.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    if extension {
        let mut actions: Vec<i64> = Vec::new();
        let mut offset: i64 = 0;
        let mut last_gameloop: Option<i64> = None;

        for event in events {
            let gameloop = event_gameloop(event);
            let name = event_name(event);

            if gameloop == 0
                && name == "NNet.Game.STriggerDialogControlEvent"
                && event_event_type(event) == Some(3)
            {
                let contains_selection_changed = matches!(
                    event,
                    ReplayEvent::Game(event)
                        if event
                            .m_event_data
                            .as_ref()
                            .is_some_and(|data| data.contains_selection_changed)
                );
                if contains_selection_changed {
                    if let Some(control_id) = event_control_id(event) {
                        offset = 129 - control_id;
                    }
                    continue;
                }
            }

            if gameloop > 0
                && Some(gameloop) != last_gameloop
                && name == "NNet.Game.STriggerDialogControlEvent"
                && event_user_id(event) == Some(0)
            {
                let contains_none = matches!(
                    event,
                    ReplayEvent::Game(event)
                        if event.m_event_data.as_ref().is_some_and(|data| data.contains_none)
                );
                if !contains_none {
                    if let Some(control_id) = event_control_id(event) {
                        actions.push(control_id + offset);
                        last_gameloop = Some(gameloop);
                    }
                    continue;
                }
            }

            if let ReplayEvent::Tracker(event) = event {
                if event.event == "NNet.Replay.Tracker.SUpgradeEvent"
                    && matches!(event.m_player_id, Some(1 | 2))
                {
                    let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                    if upgrade_name.contains("Spray") {
                        break;
                    }
                }
            }
        }

        let mut panel: i64 = 1;
        for action in actions {
            if (41..=83).contains(&action) {
                if let Some(new_mutator) = mutator_from_button(action, panel, mutators_ui) {
                    if !mutators.contains(&new_mutator) || new_mutator == "Random" {
                        mutators.push(new_mutator);
                    } else if let Some(position) =
                        mutators.iter().position(|value| value == &new_mutator)
                    {
                        mutators.remove(position);
                    }
                }
            }

            if action == 123 && panel > 1 {
                panel -= 1;
            }
            if action == 124 && panel < 4 {
                panel += 1;
            }

            if (88..=106).contains(&action) {
                if let Ok(index) = usize::try_from((action - 88) / 2) {
                    if index < mutators.len() {
                        mutators.remove(index);
                    }
                }
            }
        }
    }

    let normalized = mutators
        .into_iter()
        .map(|value| {
            value
                .replace("Heroes from the Storm (old)", "Heroes from the Storm")
                .replace("Extreme Caution", "Afraid of the Dark")
        })
        .collect::<Vec<String>>();
    (normalized, weekly)
}

pub fn find_replay_player(players: &[ParsedReplayPlayer], pid: u8) -> Option<&ParsedReplayPlayer> {
    players.iter().find(|player| player.pid == pid)
}

fn resolve_main_player_pid(players: &[ParsedReplayPlayer], handles: &HashSet<String>) -> i64 {
    if handles.is_empty() {
        return 1;
    }

    players
        .iter()
        .filter(|player| player.pid == 1 || player.pid == 2)
        .find(|player| handles.contains(player.handle.as_str()))
        .map(|player| i64::from(player.pid))
        .unwrap_or(1)
}

fn parse_replay_file_input(
    replay_path: &Path,
) -> Result<ParsedReplayAnalysisInput, DetailedReplayAnalysisError> {
    let dictionaries = load_sc2_dictionary_data()?;
    let store = protocol_store()?;
    let parsed =
        parse_file_with_store(replay_path, store, ReplayParseMode::Detailed).map_err(|error| {
            DetailedReplayAnalysisError::ReplayParse {
                path: replay_path.display().to_string(),
                message: error.to_string(),
            }
        })?;

    let replay_build = i64::from(parsed.base_build);
    let latest_build = i64::from(
        store
            .latest()
            .map_err(|error| DetailedReplayAnalysisError::ProtocolStore(error.to_string()))?
            .build,
    );
    let selected_build = if store.build(parsed.base_build).is_ok() {
        replay_build
    } else {
        store
            .closest_build(parsed.base_build)
            .map(i64::from)
            .unwrap_or(latest_build)
    };
    let protocol_build = if let Some(mapped) = valid_protocol_mapping(replay_build) {
        if supported_legacy_protocol(mapped) {
            ProtocolBuildValue::Int(mapped as u32)
        } else {
            ProtocolBuildValue::Str(latest_build.to_string())
        }
    } else if replay_build == selected_build {
        ProtocolBuildValue::Int(replay_build as u32)
    } else {
        ProtocolBuildValue::Str(latest_build.to_string())
    };

    let details = parsed
        .details
        .as_ref()
        .cloned()
        .map(process_details_data)
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData("missing replay.details".to_string())
        })?;
    let init_data = parsed
        .init_data
        .as_ref()
        .cloned()
        .map(process_init_data)
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData("missing replay.initData".to_string())
        })?;
    let metadata = parsed.metadata.as_ref().cloned().ok_or_else(|| {
        DetailedReplayAnalysisError::InvalidReplayData(
            "missing replay.gamemetadata.json".to_string(),
        )
    })?;

    let mut events = Vec::new();
    events.extend(parsed.game_events.into_iter().map(ReplayEvent::Game));
    events.extend(parsed.tracker_events.into_iter().map(ReplayEvent::Tracker));
    events.sort_by_key(event_gameloop);

    let map_title = metadata
        .get_key("Title")
        .map(value_as_text)
        .unwrap_or_else(|| "Unknown map".to_string());
    let map_name = dictionaries
        .map_names
        .get(&map_title)
        .and_then(|row| row.get("EN"))
        .cloned()
        .unwrap_or(map_title);
    let _is_blizzard = details
        .get_key("m_isBlizzardMap")
        .and_then(value_as_bool)
        .unwrap_or(false);
    let _disable_recover = details
        .get_key("m_disableRecoverGame")
        .and_then(value_as_bool)
        .unwrap_or(false);

    let extension = nested_value(
        &init_data,
        &["m_syncLobbyState", "m_gameDescription", "m_hasExtensionMod"],
    )
    .and_then(value_as_bool)
    .unwrap_or(false);
    let brutal_plus = nested_value(&init_data, &["m_syncLobbyState", "m_lobbyState", "m_slots"])
        .and_then(|value| list_item(value, 0))
        .and_then(|value| value.get_key("m_brutalPlusDifficulty"))
        .and_then(value_as_i64)
        .unwrap_or_default() as u32;

    let length = metadata
        .get_key("Duration")
        .and_then(value_as_f64)
        .unwrap_or_default();
    let start_time = get_start_time(&events);
    let last_deselect_event = get_last_deselect_event(&events).unwrap_or(length);

    let metadata_players = metadata
        .get_key("Players")
        .and_then(value_array)
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData(
                "metadata Players must be array".to_string(),
            )
        })?;
    let player0_result = metadata_players
        .first()
        .and_then(|value| value.get_key("Result"))
        .map(value_as_text)
        .unwrap_or_default();
    let player1_result = metadata_players
        .get(1)
        .and_then(|value| value.get_key("Result"))
        .map(value_as_text)
        .unwrap_or_default();
    let result = if player0_result == "Win" || player1_result == "Win" {
        "Victory".to_string()
    } else {
        "Defeat".to_string()
    };

    let accurate_length = if result == "Victory" {
        last_deselect_event - start_time
    } else {
        length - start_time
    };
    let end_time = if result == "Victory" {
        last_deselect_event
    } else {
        length
    };

    let (mutators, weekly) = identify_mutators(
        &events,
        &dictionaries.mutators_all,
        &dictionaries.mutators_ui,
        &dictionaries.mutator_ids,
        &dictionaries.cached_mutators,
        extension,
        replay_path.to_string_lossy().contains("[MM]"),
        Some(&init_data),
    );

    let mut parsed_players = vec![ParsedReplayPlayer {
        pid: 0,
        name: String::new(),
        handle: String::new(),
        race: String::new(),
        observer: false,
        result: String::new(),
        commander: String::new(),
        commander_level: 0,
        commander_mastery_level: 0,
        prestige: 0,
        prestige_name: String::new(),
        apm: 0,
        masteries: [0, 0, 0, 0, 0, 0],
    }];

    for (index, player) in metadata_players.iter().take(2).enumerate() {
        let pid = (index + 1) as u8;
        let apm = player
            .get_key("APM")
            .and_then(value_as_f64)
            .map(|value| (value * length / accurate_length).round_ties_even() as u32)
            .unwrap_or_default();
        let player_result = player
            .get_key("Result")
            .map(value_as_text)
            .unwrap_or_default();
        parsed_players.push(ParsedReplayPlayer {
            pid,
            name: String::new(),
            handle: String::new(),
            race: String::new(),
            observer: false,
            result: player_result,
            commander: String::new(),
            commander_level: 0,
            commander_mastery_level: 0,
            prestige: 0,
            prestige_name: String::new(),
            apm,
            masteries: [0, 0, 0, 0, 0, 0],
        });
    }
    while parsed_players.len() < 3 {
        parsed_players.push(ParsedReplayPlayer {
            pid: 2,
            name: String::new(),
            handle: String::new(),
            race: String::new(),
            observer: false,
            result: String::new(),
            commander: String::new(),
            commander_level: 0,
            commander_mastery_level: 0,
            prestige: 0,
            prestige_name: String::new(),
            apm: 0,
            masteries: [0, 0, 0, 0, 0, 0],
        });
    }

    let player_list = details
        .get_key("m_playerList")
        .and_then(value_array)
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData(
                "details player list must be array".to_string(),
            )
        })?;
    let mut region = String::new();
    for (index, player) in player_list.iter().take(2).enumerate() {
        if let Some(target) = parsed_players.get_mut(index + 1) {
            target.name = player
                .get_key("m_name")
                .map(value_as_text)
                .unwrap_or_default();
            target.race = player
                .get_key("m_race")
                .map(value_as_text)
                .unwrap_or_default();
            target.observer = player
                .get_key("m_observe")
                .and_then(value_as_i64)
                .unwrap_or_default()
                != 0;
            if index == 0 {
                let region_code = player
                    .get_key("m_toon")
                    .and_then(|value| value.get_key("m_region"))
                    .and_then(value_as_i64)
                    .unwrap_or_default();
                region = region_name(region_code).to_string();
            }
        }
    }

    let slots = nested_value(&init_data, &["m_syncLobbyState", "m_lobbyState", "m_slots"])
        .and_then(value_array)
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData("init slots must be array".to_string())
        })?;
    for (index, slot) in slots.iter().take(2).enumerate() {
        if let Some(target) = parsed_players.get_mut(index + 1) {
            let commander = slot
                .get_key("m_commander")
                .map(value_as_text)
                .unwrap_or_default();
            let commander_level = slot
                .get_key("m_commanderLevel")
                .and_then(value_as_i64)
                .unwrap_or_default();
            let commander_mastery_level = slot
                .get_key("m_commanderMasteryLevel")
                .and_then(value_as_i64)
                .unwrap_or_default();
            let prestige = slot
                .get_key("m_selectedCommanderPrestige")
                .and_then(value_as_i64)
                .unwrap_or_default();
            target.commander = commander.clone();
            target.commander_level = commander_level as u32;
            target.commander_mastery_level = commander_mastery_level as u32;
            target.prestige = prestige as u32;
            target.prestige_name = dictionaries
                .prestige_names
                .get(&commander)
                .and_then(|row| row.get(&prestige))
                .cloned()
                .unwrap_or_default();
            target.handle = slot
                .get_key("m_toonHandle")
                .map(value_as_text)
                .unwrap_or_default();
            target.masteries = parse_masteries(
                slot.get_key("m_commanderMasteryTalents")
                    .and_then(value_array),
            );
        }
    }

    let user_initial = nested_value(&init_data, &["m_syncLobbyState", "m_userInitialData"])
        .and_then(value_array)
        .unwrap_or_default();
    for (index, user) in user_initial.iter().take(2).enumerate() {
        if let Some(target) = parsed_players.get_mut(index + 1) {
            let user_name = user
                .get_key("m_name")
                .map(value_as_text)
                .unwrap_or_default();
            if !user_name.is_empty() {
                target.name = user_name;
            }
        }
    }

    let enemy_race = slots
        .get(2)
        .and_then(|value| value.get_key("m_race"))
        .map(value_as_text)
        .or_else(|| {
            player_list
                .get(2)
                .and_then(|value| value.get_key("m_race"))
                .map(value_as_text)
        })
        .unwrap_or_default();
    let diff_1 = slots
        .get(2)
        .and_then(|value| value.get_key("m_difficulty"))
        .and_then(value_as_i64)
        .unwrap_or(4);
    let diff_2 = slots
        .get(3)
        .and_then(|value| value.get_key("m_difficulty"))
        .and_then(value_as_i64)
        .unwrap_or(4);
    let diff_1_name = difficulty_name(diff_1).to_string();
    let diff_2_name = difficulty_name(diff_2).to_string();
    let ext_difficulty = if brutal_plus > 0 {
        format!("B+{brutal_plus}")
    } else if diff_1_name == diff_2_name {
        diff_1_name.clone()
    } else {
        format!("{diff_1_name}/{diff_2_name}")
    };

    let messages = parsed
        .message_events
        .iter()
        .filter_map(|message| {
            let text =
                if let Some(value) = message.m_string.as_ref().filter(|value| !value.is_empty()) {
                    value.clone()
                } else if message.event == "NNet.Game.SPingMessage" {
                    "*pings*".to_string()
                } else {
                    return None;
                };
            let player = message.user_id.map(|value| value + 1).unwrap_or_default() as u8;
            let time = message.game_loop as f64 / 16.0;
            Some(ParsedReplayMessage { text, player, time })
        })
        .collect::<Vec<ParsedReplayMessage>>();

    let parser = ParsedReplayInput {
        file: replay_path.display().to_string(),
        map_name,
        extension,
        brutal_plus,
        result,
        players: parsed_players,
        difficulty: (diff_1_name, diff_2_name),
        accurate_length,
        form_alength: format_duration(accurate_length),
        length: duration_to_u64(length),
        mutators,
        weekly,
        messages,
        hash: Some(calculate_replay_hash(replay_path)),
        build: ReplayBuildInfo {
            replay_build: parsed.base_build,
            protocol_build,
        },
        date: file_date_string(replay_path)?,
        enemy_race,
        ext_difficulty,
        region,
    };

    Ok(ParsedReplayAnalysisInput {
        parser,
        events,
        start_time,
        end_time,
    })
}

fn map_name_has_amon_override(map_name: &str, candidate: &str) -> bool {
    map_name.contains(candidate) || (map_name.contains("[MM] Lnl") && candidate == "Lock & Load")
}

fn find_replay_player_mut(
    players: &mut [ParsedReplayPlayer],
    pid: u8,
) -> Option<&mut ParsedReplayPlayer> {
    players.iter_mut().find(|player| player.pid == pid)
}

fn replay_unitid_from_event(event: &TrackerEvent, killer: bool, creator: bool) -> Option<i64> {
    let (index, recycle_index) = if killer {
        (
            event.m_killer_unit_tag_index?,
            event.m_killer_unit_tag_recycle?,
        )
    } else if creator {
        (
            event.m_creator_unit_tag_index?,
            event.m_creator_unit_tag_recycle?,
        )
    } else {
        (event.m_unit_tag_index?, event.m_unit_tag_recycle?)
    };
    Some(recycle_index * 100_000 + index)
}

fn clamp_nonnegative_to_u64(value: i64) -> u64 {
    if value <= 0 {
        0
    } else {
        value as u64
    }
}

fn count_for_pid(values: &[i64], pid: i64) -> i64 {
    usize::try_from(pid)
        .ok()
        .and_then(|index| values.get(index))
        .copied()
        .unwrap_or_default()
}

fn round_to_digits_half_even(value: f64, digits: i32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let Ok(digits_u32) = u32::try_from(digits) else {
        return value;
    };
    let Some(scale10) = 10_u128.checked_pow(digits_u32) else {
        return value;
    };

    let bits = value.to_bits();
    let sign_negative = (bits >> 63) != 0;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let mantissa_bits = bits & ((1_u64 << 52) - 1);

    let (mantissa, exponent2) = if exponent_bits == 0 {
        (mantissa_bits as u128, -1074_i32)
    } else {
        (
            (mantissa_bits | (1_u64 << 52)) as u128,
            exponent_bits - 1075,
        )
    };
    if mantissa == 0 {
        return if sign_negative { -0.0 } else { 0.0 };
    }

    let Some(mut numerator) = mantissa.checked_mul(scale10) else {
        return value;
    };
    let mut denominator = 1_u128;
    if exponent2 >= 0 {
        let Ok(shift) = u32::try_from(exponent2) else {
            return value;
        };
        let Some(shifted) = numerator.checked_shl(shift) else {
            return value;
        };
        numerator = shifted;
    } else {
        let Ok(shift) = u32::try_from(-exponent2) else {
            return value;
        };
        let Some(shifted) = denominator.checked_shl(shift) else {
            return 0.0;
        };
        denominator = shifted;
    }

    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = match remainder.checked_mul(2) {
        Some(double_remainder) if double_remainder < denominator => quotient,
        Some(double_remainder) if double_remainder > denominator => quotient + 1,
        Some(_) => {
            if quotient % 2 == 0 {
                quotient
            } else {
                quotient + 1
            }
        }
        None => quotient,
    };

    let factor = 10_f64.powi(digits);
    if !factor.is_finite() || factor == 0.0 {
        return value;
    }

    let rounded_value = rounded as f64 / factor;
    if sign_negative {
        -rounded_value
    } else {
        rounded_value
    }
}

fn format_mm_ss(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "00:00".to_string();
    }
    let total = seconds as u64;
    let minutes = (total / 60) % 60;
    let secs = total % 60;
    format!("{minutes:02}:{secs:02}")
}

fn contains_skip_strings_text(unit_name: &str, skip_tokens: &[String]) -> bool {
    let lowered = unit_name.to_lowercase();
    skip_tokens.iter().any(|token| lowered.contains(token))
}

fn increment_icon_count(icons: &mut BTreeMap<String, u64>, key: &str, delta: i64) {
    if delta == 0 {
        return;
    }

    let current = icons.get(key).copied().unwrap_or_default() as i64;
    let next = current + delta;
    if next <= 0 {
        icons.remove(key);
    } else {
        icons.insert(key.to_string(), next as u64);
    }
}

fn set_icon_count(icons: &mut BTreeMap<String, u64>, key: &str, value: i64) {
    if value > 0 {
        icons.insert(key.to_string(), value as u64);
    } else {
        icons.remove(key);
    }
}

fn build_stats_counter_dictionaries(
    dictionaries: &CacheGenerationData<'_>,
) -> StatsCounterDictionaries {
    StatsCounterDictionaries {
        unit_base_costs: dictionaries.unit_base_costs.clone(),
        royal_guards: dictionaries.royal_guards.clone(),
        horners_units: dictionaries.horners_units.clone(),
        tychus_base_upgrades: dictionaries.tychus_base_upgrades.clone(),
        tychus_ultimate_upgrades: dictionaries.tychus_ultimate_upgrades.clone(),
        outlaws: dictionaries.outlaws.clone(),
    }
}

fn sorted_messages_with_leave_events(
    messages: &[ParsedReplayMessage],
    user_leave_times: &IndexMap<i64, f64>,
) -> Vec<ParsedReplayMessage> {
    let mut rows = messages
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, message)| (message.time, index, message))
        .collect::<Vec<(f64, usize, ParsedReplayMessage)>>();

    let base_index = rows.len();
    for (offset, (player, leave_time)) in user_leave_times.iter().enumerate() {
        if *player != 1 && *player != 2 {
            continue;
        }
        rows.push((
            *leave_time,
            base_index + offset,
            ParsedReplayMessage {
                player: *player as u8,
                text: "*has left the game*".to_string(),
                time: *leave_time,
            },
        ));
    }

    rows.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    rows.into_iter().map(|(_, _, message)| message).collect()
}

fn switched_unit_counts(
    counts: &UnitTypeCountMap,
    unit_name_dict: &UnitNamesJson,
    unit_add_kills_to: &UnitAddKillsToJson,
    unit_add_losses_to: &HashMap<String, String>,
    dont_include_units: &HashSet<String>,
) -> HashMap<String, [i64; 4]> {
    let mut switched: HashMap<String, [i64; 4]> = HashMap::new();

    for (unit_name, values) in counts {
        if dont_include_units.contains(unit_name) {
            continue;
        }

        let mut added = false;
        if let Some(target) = unit_add_kills_to.get(unit_name) {
            let entry = switched.entry(target.clone()).or_insert([0_i64; 4]);
            entry[2] += values[2];
            added = true;
        }

        if let Some(target) = unit_add_losses_to.get(unit_name) {
            let entry = switched.entry(target.clone()).or_insert([0_i64; 4]);
            entry[1] += values[1];
            added = true;
        }

        if !added {
            let mapped_name = unit_name_dict
                .get(unit_name)
                .cloned()
                .unwrap_or_else(|| unit_name.clone());
            let entry = switched.entry(mapped_name).or_insert([0_i64; 4]);
            for (index, value) in values.iter().enumerate() {
                entry[index] += *value;
            }
        }
    }

    switched
}

fn sorted_switch_name_entries(
    counts: &UnitTypeCountMap,
    unit_name_dict: &UnitNamesJson,
    unit_add_kills_to: &UnitAddKillsToJson,
    unit_add_losses_to: &HashMap<String, String>,
    dont_include_units: &HashSet<String>,
) -> Vec<(String, i64, i64, i64)> {
    let mut rows = switched_unit_counts(
        counts,
        unit_name_dict,
        unit_add_kills_to,
        unit_add_losses_to,
        dont_include_units,
    )
    .into_iter()
    .map(|(unit_name, values)| (unit_name, values[0], values[1], values[2]))
    .collect::<Vec<(String, i64, i64, i64)>>();

    rows.sort_by(|left, right| {
        right
            .3
            .cmp(&left.3)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    rows
}

fn unit_stats_tuple(created: i64, lost: i64, kills: i64, kill_fraction: f64) -> UnitStats {
    (created, lost, kills, kill_fraction)
}

fn fill_unit_kills_and_icons(
    base_icons: &BTreeMap<String, u64>,
    player: i64,
    main_player: i64,
    unit_counts: &UnitTypeCountMap,
    ally_kills_counted_toward_main: i64,
    killcounts: &[i64],
    unit_name_dict: &UnitNamesJson,
    unit_add_kills_to: &UnitAddKillsToJson,
    unit_add_losses_to: &HashMap<String, String>,
    dont_include_units: &HashSet<String>,
    icon_units: &HashSet<String>,
) -> (BTreeMap<String, UnitStats>, BTreeMap<String, u64>) {
    let mut icons = base_icons.clone();
    for (unit_name, values) in unit_counts {
        let created = values[0];
        if LOCUST_SOURCE_UNITS.contains(&unit_name.as_str()) {
            increment_icon_count(&mut icons, "locust", created);
        } else if BROODLING_SOURCE_UNITS.contains(&unit_name.as_str()) {
            increment_icon_count(&mut icons, "broodling", created);
        }
    }

    for icon_key in ["broodling", "locust"] {
        let count = icons.get(icon_key).copied().unwrap_or_default();
        if count > 0 && count < 200 {
            icons.remove(icon_key);
        }
    }

    let rows = sorted_switch_name_entries(
        unit_counts,
        unit_name_dict,
        unit_add_kills_to,
        unit_add_losses_to,
        dont_include_units,
    );
    let player_kills = count_for_pid(killcounts, player);
    let dehaka_created_lost = rows
        .iter()
        .find(|(unit_name, _, _, _)| unit_name == "Dehaka")
        .map(|(_, created, lost, _)| (*created, *lost));

    let mut units = BTreeMap::new();
    for (unit_name, mut created, mut lost, kills) in rows {
        let denominator = if ally_kills_counted_toward_main > 0 && player != main_player {
            player_kills + ally_kills_counted_toward_main
        } else if ally_kills_counted_toward_main > 0 && player == main_player {
            player_kills - ally_kills_counted_toward_main
        } else {
            player_kills
        };

        let kill_fraction = if denominator > 0 {
            round_to_digits_half_even(kills as f64 / denominator as f64, 2)
        } else {
            0.0
        };

        if unit_name == "Zweihaka" {
            if let Some((dehaka_created, dehaka_lost)) = dehaka_created_lost {
                created = dehaka_created;
                lost = dehaka_lost;
            }
        }

        units.insert(
            unit_name.clone(),
            unit_stats_tuple(created, lost, kills, kill_fraction),
        );

        if icon_units.contains(&unit_name) {
            set_icon_count(&mut icons, &unit_name, created);
        }
    }

    let mut artifacts_collected = 0_i64;
    for (unit_name, values) in unit_counts {
        let created = values[0];
        let lost = values[1];

        if ZERATUL_ARTIFACT_PICKUPS.contains(&unit_name.as_str()) {
            artifacts_collected += lost;
        }
        if ZERATUL_SHADE_PROJECTIONS.contains(&unit_name.as_str()) {
            increment_icon_count(&mut icons, "ShadeProjection", created);
        }
    }
    if artifacts_collected > 0 {
        set_icon_count(&mut icons, "Artifact", artifacts_collected);
    }

    (units, icons)
}

fn fill_amon_units(
    unit_counts: &UnitTypeCountMap,
    killcounts: &[i64],
    amon_players: &HashSet<i64>,
    unit_name_dict: &UnitNamesJson,
    unit_add_kills_to: &UnitAddKillsToJson,
    unit_add_losses_to: &HashMap<String, String>,
    dont_include_units: &HashSet<String>,
    skip_tokens: &[String],
) -> BTreeMap<String, UnitStats> {
    let rows = sorted_switch_name_entries(
        unit_counts,
        unit_name_dict,
        unit_add_kills_to,
        unit_add_losses_to,
        dont_include_units,
    );

    let mut total_amon_kills = amon_players
        .iter()
        .map(|player| count_for_pid(killcounts, *player))
        .sum::<i64>();
    if total_amon_kills == 0 {
        total_amon_kills = 1;
    }

    let mut amon_units = BTreeMap::new();
    for (unit_name, created, lost, kills) in rows {
        if contains_skip_strings_text(&unit_name, skip_tokens) {
            continue;
        }
        let kill_fraction = round_to_digits_half_even(kills as f64 / total_amon_kills as f64, 2);
        amon_units.insert(
            unit_name,
            unit_stats_tuple(created, lost, kills, kill_fraction),
        );
    }
    amon_units
}

fn enemy_comp_from_identified_waves(
    identified_waves: &IdentifiedWavesMap,
    unit_comp_dict: &HashMap<String, Vec<HashSet<String>>>,
) -> String {
    let mut ai_order = unit_comp_dict.keys().cloned().collect::<Vec<String>>();
    ai_order.sort();
    let mut results = HashMap::new();
    for ai in &ai_order {
        results.insert(ai.clone(), 0.0_f64);
    }

    for wave in identified_waves.values() {
        let types = wave.iter().cloned().collect::<HashSet<String>>();
        if types.is_empty() {
            continue;
        }

        for ai in &ai_order {
            let Some(waves) = unit_comp_dict.get(ai) else {
                continue;
            };
            let score = results.entry(ai.clone()).or_insert(0.0);
            for wave_row in waves {
                let mut wave_set = wave_row.clone();
                wave_set.remove("Medivac");
                if types == wave_set {
                    *score += wave_set.len() as f64;
                } else if types.is_subset(&wave_set)
                    && wave_set.len().saturating_sub(types.len()) == 1
                {
                    *score += 0.25 * wave_set.len() as f64;
                }
            }
        }
    }

    let mut best_ai: Option<String> = None;
    let mut best_score = 0.0_f64;
    for ai in ai_order {
        let score = results.get(&ai).copied().unwrap_or_default();
        if score > best_score {
            best_score = score;
            best_ai = Some(ai);
        }
    }

    best_ai.unwrap_or_else(|| "Unidentified AI".to_string())
}

fn apply_custom_kill_icons(
    main_icons: &mut BTreeMap<String, u64>,
    ally_icons: &mut BTreeMap<String, u64>,
    custom_kill_count: &replay_event_handlers::NestedPlayerCountMap,
    unit_type_dict_amon: &UnitTypeCountMap,
    map_name: &str,
    main_player: i64,
    ally_player: i64,
) {
    for key in CUSTOM_KILL_ICON_KEYS {
        let Some(player_counts) = custom_kill_count.get(key) else {
            continue;
        };
        if key == "deadofnight" && !map_name.contains("Dead of Night") {
            continue;
        }
        if key == "minesweeper" && !unit_type_dict_amon.contains_key("MutatorSpiderMine") {
            continue;
        }

        main_icons.insert(
            key.to_string(),
            player_counts
                .get(&main_player)
                .copied()
                .unwrap_or_default()
                .max(0) as u64,
        );
        ally_icons.insert(
            key.to_string(),
            player_counts
                .get(&ally_player)
                .copied()
                .unwrap_or_default()
                .max(0) as u64,
        );
    }
}

pub fn apply_parser_player_overrides(
    parser: &mut ParsedReplayInput,
    commander_by_player: &HashMap<i64, String>,
    mastery_by_player: &HashMap<i64, [i64; 6]>,
    prestige_by_player: &HashMap<i64, String>,
) {
    let is_mm = parser.file.contains("[MM]");

    for pid in [1_i64, 2_i64] {
        let Some(player) = find_replay_player_mut(&mut parser.players, pid as u8) else {
            continue;
        };

        if player.commander.trim().is_empty() {
            if let Some(commander_name) = commander_by_player
                .get(&pid)
                .filter(|value| !value.trim().is_empty())
            {
                player.commander = commander_name.clone();
            }
        }

        if let Some(prestige_name) = prestige_by_player
            .get(&pid)
            .filter(|value| !value.trim().is_empty())
        {
            player.prestige_name = prestige_name.clone();
        }

        if is_mm {
            if let Some(masteries) = mastery_by_player.get(&pid) {
                let mut parsed_masteries = [0_u32; 6];
                for (index, mastery_value) in masteries.iter().enumerate() {
                    parsed_masteries[index] =
                        u32::try_from((*mastery_value).max(0)).unwrap_or_default();
                }
                player.masteries = parsed_masteries;
            }
            player.commander_level = 15;
        }
    }
}

pub fn analyze_replay_file(
    replay_path: &Path,
    main_player_handles: &HashSet<String>,
) -> Result<ReplayReport, DetailedReplayAnalysisError> {
    let parsed_input = parse_replay_file_input(replay_path)?;
    analyze_replay_file_impl(replay_path, main_player_handles, parsed_input)
}

fn analyze_replay_file_impl(
    _replay_path: &Path,
    main_player_handles: &HashSet<String>,
    parsed_input: ParsedReplayAnalysisInput,
) -> Result<ReplayReport, DetailedReplayAnalysisError> {
    let dictionaries = load_sc2_dictionary_data()?;
    let ParsedReplayAnalysisInput {
        mut parser,
        events,
        start_time,
        end_time,
        ..
    } = parsed_input;

    let main_player = resolve_main_player_pid(&parser.players, main_player_handles);
    let ally_player = if main_player == 2 { 1 } else { 2 };

    let main_player_row = find_replay_player(&parser.players, main_player as u8);
    let ally_player_row = find_replay_player(&parser.players, ally_player as u8);
    let main_commander = main_player_row
        .map(|player| player.commander.clone())
        .filter(|value| !value.is_empty());
    let ally_commander = ally_player_row
        .map(|player| player.commander.clone())
        .filter(|value| !value.is_empty());
    let main_masteries = main_player_row
        .map(|player| player.masteries)
        .unwrap_or([0_u32; 6]);
    let ally_masteries = ally_player_row
        .map(|player| player.masteries)
        .unwrap_or([0_u32; 6]);

    let counter_dicts = build_stats_counter_dictionaries(&dictionaries);
    let mut vespene_drone_identifier =
        ReplayDroneIdentifierCore::new(main_commander.clone(), ally_commander.clone());
    let mut main_stats_counter =
        ReplayStatsCounterCore::new(counter_dicts.clone(), main_masteries, main_commander);
    let mut ally_stats_counter =
        ReplayStatsCounterCore::new(counter_dicts, ally_masteries, ally_commander);
    if parser.file.contains("[MM]") {
        main_stats_counter.set_enable_updates(true);
        ally_stats_counter.set_enable_updates(true);
    }

    let do_not_count_kills_set = dictionaries
        .replay_analysis_data
        .do_not_count_kills
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let duplicating_units_set = dictionaries
        .replay_analysis_data
        .duplicating_units
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let skip_tokens = dictionaries
        .replay_analysis_data
        .skip_strings
        .iter()
        .map(|value| value.to_lowercase())
        .collect::<Vec<String>>();
    let dont_count_morphs_set = dictionaries
        .replay_analysis_data
        .dont_count_morphs
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let self_killing_units_set = dictionaries
        .replay_analysis_data
        .self_killing_units
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let aoe_units_set = dictionaries
        .replay_analysis_data
        .aoe_units
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let tychus_outlaws_set = dictionaries
        .replay_analysis_data
        .tychus_outlaws
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let units_killed_in_morph_set = dictionaries
        .replay_analysis_data
        .units_killed_in_morph
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let dont_include_units_set = dictionaries
        .replay_analysis_data
        .dont_include_units
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let icon_units_set = dictionaries
        .replay_analysis_data
        .icon_units
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let _dont_show_created_lost_set = dictionaries
        .replay_analysis_data
        .dont_show_created_lost
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let salvage_units_set = dictionaries
        .replay_analysis_data
        .salvage_units
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    let unit_add_losses_to_set = dictionaries
        .replay_analysis_data
        .unit_add_losses_to
        .keys()
        .cloned()
        .collect::<HashSet<String>>();
    let mut commander_no_units_values_set = HashSet::new();
    for units in dictionaries
        .replay_analysis_data
        .commander_no_units
        .values()
    {
        for unit in units {
            commander_no_units_values_set.insert(unit.clone());
        }
    }

    let mut amon_player_ids_set: HashSet<i64> = HashSet::from([3_i64, 4_i64]);
    for (mission_name, player_ids) in dictionaries.amon_player_ids.iter() {
        if !map_name_has_amon_override(&parser.map_name, mission_name) {
            continue;
        }
        amon_player_ids_set.extend(player_ids.iter().copied());
        break;
    }

    let mut unit_type_dict_main: UnitTypeCountMap = IndexMap::new();
    let mut unit_type_dict_ally: UnitTypeCountMap = IndexMap::new();
    let mut unit_type_dict_amon: UnitTypeCountMap = IndexMap::new();
    let mut unit_dict: UnitStateMap = IndexMap::new();
    let mut dt_ht_ignore = vec![0_i64; 17];
    let mut killcounts = vec![0_i64; 18];
    let mut commander_by_player = HashMap::<i64, String>::new();
    let mut mastery_by_player = HashMap::from([(1_i64, [0_i64; 6]), (2_i64, [0_i64; 6])]);
    let mut prestige_by_player = HashMap::<i64, String>::new();
    let mut outlaw_order: Vec<String> = Vec::new();
    let mut wave_units = WaveUnitsState::default();
    let mut identified_waves: IdentifiedWavesMap = BTreeMap::new();
    let mut killbot_feed = vec![0_i64, 0, 0];
    let mut custom_kill_count: replay_event_handlers::NestedPlayerCountMap = IndexMap::new();
    let mut used_mutator_spider_mines: HashSet<i64> = HashSet::new();
    let mut bonus_timings: Vec<f64> = Vec::new();
    let mut research_vessel_landed_timing: Option<i64> = None;
    let mut unit_id = 0_i64;
    let mut last_biomass_position = [0_i64, 0, 0];
    let mut abathur_kill_locusts = HashSet::new();
    let mut mutator_dehaka_drag_unit_ids = HashSet::new();
    let mut mw_bonus_initial_timing = [0.0_f64, 0.0_f64];
    let mut murvar_spawns = HashSet::new();
    let mut glevig_spawns = HashSet::new();
    let mut broodlord_broodlings = HashSet::new();
    let mut user_leave_times: IndexMap<i64, f64> = IndexMap::new();
    let mut mind_controlled_units = HashSet::new();
    let mut zagaras_dummy_zerglings = HashSet::new();
    let mut unit_killed_by: replay_event_handlers::TextListMapping = IndexMap::new();
    let mut ally_kills_counted_toward_main = 0_i64;
    let mut last_aoe_unit_killed: Vec<Option<(String, f64)>> = vec![None; 17];
    let mut main_icons_base = BTreeMap::<String, u64>::new();
    let mut ally_icons_base = BTreeMap::<String, u64>::new();

    for event in &events {
        let current_event_name = event_name(event);

        if current_event_name == "NNet.Game.SGameUserLeaveEvent" {
            let user_id = event_user_id(event).unwrap_or_default();
            let gameloop = event_gameloop(event) as f64;
            replay_handle_game_user_leave_event_fields(user_id, gameloop, &mut user_leave_times);
        }

        if event_gameloop(event) as f64 / 16.0 > end_time {
            continue;
        }

        if let ReplayEvent::Game(game_event) = event {
            vespene_drone_identifier.event(game_event);
        }

        if let ReplayEvent::Tracker(event) = event {
            if current_event_name == "NNet.Replay.Tracker.SPlayerStatsEvent" {
                let player = event.m_player_id.unwrap_or_default();
                if let Some(stats) = event.m_stats.as_ref() {
                    let supply_used = stats.m_score_value_food_used.unwrap_or_default() / 4096.0;
                    let collection_rate = stats
                        .m_score_value_minerals_collection_rate
                        .unwrap_or_default()
                        + stats
                            .m_score_value_vespene_collection_rate
                            .unwrap_or_default();

                    if let Some(update) = replay_handle_player_stats_event_fields(
                        player,
                        main_player,
                        ally_player,
                        supply_used,
                        collection_rate,
                        &killcounts,
                    ) {
                        match update.target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.set_unit_dict(&unit_type_dict_main);
                                main_stats_counter.add_stats(
                                    &vespene_drone_identifier,
                                    update.kills,
                                    update.supply_used,
                                    update.collection_rate,
                                );
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.set_unit_dict(&unit_type_dict_ally);
                                ally_stats_counter.add_stats(
                                    &vespene_drone_identifier,
                                    update.kills,
                                    update.supply_used,
                                    update.collection_rate,
                                );
                            }
                        }
                    }
                }
            }

            if current_event_name == "NNet.Replay.Tracker.SUpgradeEvent"
                && matches!(event.m_player_id, Some(1 | 2))
            {
                let upg_name = event.m_upgrade_type_name.clone().unwrap_or_default();
                let upg_pid = event.m_player_id.unwrap_or_default();
                let upgrade_count = event.m_count.unwrap_or_default();
                let update = replay_handle_upgrade_event_fields(
                    upg_name.as_str(),
                    upg_pid,
                    upgrade_count,
                    main_player,
                    ally_player,
                    &dictionaries.replay_analysis_data.commander_upgrades,
                    &dictionaries.co_mastery_upgrades,
                    &dictionaries.prestige_upgrades,
                );

                if let Some(target) = update.target {
                    match target {
                        StatsCounterTarget::Main => {
                            main_stats_counter.upgrade_event(upg_name.as_str())
                        }
                        StatsCounterTarget::Ally => {
                            ally_stats_counter.upgrade_event(upg_name.as_str())
                        }
                    }
                }

                if let Some(commander_name) = update.commander_name.as_deref() {
                    commander_by_player.insert(upg_pid, commander_name.to_string());
                    vespene_drone_identifier.update_commanders(upg_pid, commander_name);

                    if let Some(target) = update.target {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.update_commander(commander_name);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.update_commander(commander_name);
                            }
                        }
                    }
                }

                if let Some(mastery_idx) = update.mastery_index {
                    if let Some(row) = mastery_by_player.get_mut(&upg_pid) {
                        if let Ok(index) = usize::try_from(mastery_idx) {
                            if index < row.len() {
                                row[index] = update.upgrade_count;
                            }
                        }
                    }

                    if let Some(target) = update.target {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter
                                    .update_mastery(mastery_idx, update.upgrade_count);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter
                                    .update_mastery(mastery_idx, update.upgrade_count);
                            }
                        }
                    }
                }

                if let Some(prestige_name) = update.prestige_name.as_deref() {
                    prestige_by_player.insert(upg_pid, prestige_name.to_string());
                    if let Some(target) = update.target {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.update_prestige(prestige_name);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.update_prestige(prestige_name);
                            }
                        }
                    }
                }
            }
        }

        if current_event_name == "NNet.Replay.Tracker.SUnitBornEvent"
            || current_event_name == "NNet.Replay.Tracker.SUnitInitEvent"
        {
            let ReplayEvent::Tracker(event) = event else {
                continue;
            };
            let event_fields = UnitBornOrInitEventFields {
                unit_type: event.m_unit_type_name.clone().unwrap_or_default(),
                ability_name: event.m_creator_ability_name.clone(),
                unit_id: replay_unitid_from_event(event, false, false).unwrap_or_default(),
                creator_unit_id: replay_unitid_from_event(event, false, true),
                control_pid: event.m_control_player_id.unwrap_or_default(),
                gameloop: event.game_loop,
                event_x: event.m_x.unwrap_or_default(),
                event_y: event.m_y.unwrap_or_default(),
            };
            let update = replay_handle_unit_born_or_init_event_fields(
                &event_fields,
                main_player,
                ally_player,
                &amon_player_ids_set,
                &mut unit_dict,
                start_time,
                &mut unit_type_dict_main,
                &mut unit_type_dict_ally,
                &mut unit_type_dict_amon,
                &mut mutator_dehaka_drag_unit_ids,
                &mut murvar_spawns,
                &mut glevig_spawns,
                &mut broodlord_broodlings,
                &mut outlaw_order,
                &mut wave_units,
                &mut identified_waves,
                &mut abathur_kill_locusts,
                last_biomass_position,
                &dictionaries.replay_analysis_data.revival_types,
                &dictionaries.replay_analysis_data.primal_combat_predecessors,
                &tychus_outlaws_set,
                &dictionaries.units_in_waves,
            );
            unit_id = update.unit_id;
            last_biomass_position = update.last_biomass_position;

            if let Some((target, unit_type)) = update.created_event.as_ref() {
                match target {
                    StatsCounterTarget::Main => {
                        main_stats_counter.unit_created_event(unit_type.as_str(), event);
                    }
                    StatsCounterTarget::Ally => {
                        ally_stats_counter.unit_created_event(unit_type.as_str(), event);
                    }
                }
            }
        }

        if current_event_name == "NNet.Replay.Tracker.SUnitInitEvent" {
            let ReplayEvent::Tracker(event) = event else {
                continue;
            };
            let event_unit_type = event.m_unit_type_name.clone().unwrap_or_default();
            if event_unit_type == "Archon" {
                let control_pid = event.m_control_player_id.unwrap_or_default();
                replay_handle_archon_init_event_control_pid(control_pid, &mut dt_ht_ignore);
            }
        }

        let event_unit_id = match event {
            ReplayEvent::Tracker(event) => replay_unitid_from_event(event, false, false),
            ReplayEvent::Game(_) => None,
        };
        if current_event_name == "NNet.Replay.Tracker.SUnitTypeChangeEvent"
            && event_unit_id
                .map(|value| unit_dict.contains_key(&value))
                .unwrap_or(false)
        {
            let ReplayEvent::Tracker(event) = event else {
                continue;
            };
            let event_fields = UnitTypeChangeEventFields {
                event_unit_id: event_unit_id.unwrap_or_default(),
                unit_type: event.m_unit_type_name.clone().unwrap_or_default(),
                gameloop: event.game_loop,
            };
            let update = replay_handle_unit_type_change_event_fields(
                &event_fields,
                parser.map_name.as_str(),
                main_player,
                ally_player,
                &amon_player_ids_set,
                &mut unit_dict,
                &mut unit_type_dict_main,
                &mut unit_type_dict_ally,
                &mut unit_type_dict_amon,
                start_time,
                &mut bonus_timings,
                unit_id,
                &glevig_spawns,
                &murvar_spawns,
                &mut zagaras_dummy_zerglings,
                &broodlord_broodlings,
                research_vessel_landed_timing,
                &units_killed_in_morph_set,
                &dictionaries.unit_name_dict,
                &unit_add_losses_to_set,
                &dont_count_morphs_set,
            );
            research_vessel_landed_timing = update.landed_timing;

            if let Some((target, new_unit, old_unit)) = update.unit_change_event.as_ref() {
                match target {
                    StatsCounterTarget::Main => {
                        main_stats_counter.unit_change_event(new_unit.as_str(), old_unit.as_str());
                    }
                    StatsCounterTarget::Ally => {
                        ally_stats_counter.unit_change_event(new_unit.as_str(), old_unit.as_str());
                    }
                }
            }
        }

        if current_event_name == "NNet.Replay.Tracker.SUnitOwnerChangeEvent"
            && event_unit_id
                .map(|value| unit_dict.contains_key(&value))
                .unwrap_or(false)
        {
            if let Some(changed_unit_id) = event_unit_id {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                let control_pid = event.m_control_player_id.unwrap_or_default();
                let game_time = event.game_loop as f64 / 16.0 - start_time;
                let update = replay_handle_unit_owner_change_event_fields(
                    changed_unit_id,
                    parser.map_name.as_str(),
                    control_pid,
                    main_player,
                    ally_player,
                    &amon_player_ids_set,
                    &mut unit_dict,
                    game_time,
                    &mut bonus_timings,
                    &mut mw_bonus_initial_timing,
                );

                if let Some(mindcontrolled_unit_id) = update.mind_controlled_unit_id {
                    mind_controlled_units.insert(mindcontrolled_unit_id);
                    match update.icon_target {
                        Some(StatsCounterTarget::Main) => {
                            increment_icon_count(&mut main_icons_base, "mc", 1);
                        }
                        Some(StatsCounterTarget::Ally) => {
                            increment_icon_count(&mut ally_icons_base, "mc", 1);
                        }
                        None => {}
                    }
                }
            }
        }

        if current_event_name == "NNet.Replay.Tracker.SUnitDiedEvent" {
            let ReplayEvent::Tracker(event) = event else {
                continue;
            };
            let unit_in_dict = event_unit_id
                .map(|value| unit_dict.contains_key(&value))
                .unwrap_or(false);
            if !unit_in_dict {
                let killed_unit_type = event.m_unit_type_name.clone().unwrap_or_default();
                if !do_not_count_kills_set.contains(killed_unit_type.as_str()) {
                    if let Some(killer_player) = event.m_killer_player_id {
                        if let Ok(index) = usize::try_from(killer_player) {
                            if let Some(value) = killcounts.get_mut(index) {
                                *value += 1;
                            }
                        }
                    }
                }
            }

            ally_kills_counted_toward_main = replay_handle_unit_died_kill_stats_event_fields(
                event_unit_id,
                event.m_killer_player_id,
                event.game_loop,
                main_player,
                ally_player,
                &amon_player_ids_set,
                &unit_dict,
                &mut killcounts,
                &user_leave_times,
                end_time,
                &mut last_aoe_unit_killed,
                ally_kills_counted_toward_main,
                &do_not_count_kills_set,
                &aoe_units_set,
            );
        }

        if current_event_name == "NNet.Replay.Tracker.SUnitDiedEvent"
            && event_unit_id
                .map(|value| unit_dict.contains_key(&value))
                .unwrap_or(false)
        {
            if let Some(detail_unit_id) = event_unit_id {
                let ReplayEvent::Tracker(event) = event else {
                    continue;
                };
                let event_fields = UnitDiedEventFields {
                    event_unit_id: detail_unit_id,
                    killing_unit_id: replay_unitid_from_event(event, true, false),
                    killing_player: event.m_killer_player_id,
                    gameloop: event.game_loop,
                    event_x: event.m_x.unwrap_or_default(),
                    event_y: event.m_y.unwrap_or_default(),
                };
                let update = replay_handle_unit_died_detail_event_fields(
                    &event_fields,
                    parser.map_name.as_str(),
                    main_player,
                    ally_player,
                    &amon_player_ids_set,
                    unit_id,
                    &mut unit_type_dict_main,
                    &mut unit_type_dict_ally,
                    &mut unit_type_dict_amon,
                    &unit_dict,
                    &mut dt_ht_ignore,
                    start_time,
                    &commander_by_player,
                    &mut killbot_feed,
                    &mut custom_kill_count,
                    &mut used_mutator_spider_mines,
                    &mut bonus_timings,
                    &abathur_kill_locusts,
                    &mutator_dehaka_drag_unit_ids,
                    &murvar_spawns,
                    &glevig_spawns,
                    &broodlord_broodlings,
                    &mut unit_killed_by,
                    &mind_controlled_units,
                    &zagaras_dummy_zerglings,
                    &last_aoe_unit_killed,
                    &dictionaries.replay_analysis_data.commander_no_units,
                    &commander_no_units_values_set,
                    &dictionaries.hfts_units,
                    &dictionaries.tus_units,
                    &do_not_count_kills_set,
                    &self_killing_units_set,
                    &duplicating_units_set,
                    &salvage_units_set,
                );
                unit_id = update.current_unit_id;

                if let Some((target, unit_name)) = update.salvaged_unit.as_ref() {
                    match target {
                        StatsCounterTarget::Main => {
                            main_stats_counter.append_salvaged_unit(unit_name.as_str());
                        }
                        StatsCounterTarget::Ally => {
                            ally_stats_counter.append_salvaged_unit(unit_name.as_str());
                        }
                    }
                }

                if let Some((target, unit_name)) = update.mindcontrolled_unit_died.as_ref() {
                    match target {
                        StatsCounterTarget::Main => {
                            main_stats_counter.mindcontrolled_unit_dies(unit_name.as_str());
                        }
                        StatsCounterTarget::Ally => {
                            ally_stats_counter.mindcontrolled_unit_dies(unit_name.as_str());
                        }
                    }
                }
            }
        }
    }

    apply_parser_player_overrides(
        &mut parser,
        &commander_by_player,
        &mastery_by_player,
        &prestige_by_player,
    );
    parser.messages = sorted_messages_with_leave_events(&parser.messages, &user_leave_times);

    let main_name = find_replay_player(&parser.players, main_player as u8)
        .map(|player| player.name.clone())
        .unwrap_or_default();
    let ally_name = find_replay_player(&parser.players, ally_player as u8)
        .map(|player| player.name.clone())
        .unwrap_or_default();

    let mut player_stats = BTreeMap::<u8, PlayerStatsSeries>::new();
    player_stats.insert(1, main_stats_counter.get_stats(main_name.as_str()));
    player_stats.insert(2, ally_stats_counter.get_stats(ally_name.as_str()));

    let bonus = bonus_timings
        .iter()
        .map(|value| format_mm_ss(*value))
        .collect::<Vec<String>>();
    let comp = enemy_comp_from_identified_waves(&identified_waves, &dictionaries.unit_comp_dict);

    apply_custom_kill_icons(
        &mut main_icons_base,
        &mut ally_icons_base,
        &custom_kill_count,
        &unit_type_dict_amon,
        parser.map_name.as_str(),
        main_player,
        ally_player,
    );

    let (main_units, mut main_icons) = fill_unit_kills_and_icons(
        &main_icons_base,
        main_player,
        main_player,
        &unit_type_dict_main,
        ally_kills_counted_toward_main,
        &killcounts,
        &dictionaries.unit_name_dict,
        &dictionaries.unit_add_kills_to,
        &dictionaries.replay_analysis_data.unit_add_losses_to,
        &dont_include_units_set,
        &icon_units_set,
    );
    let (ally_units, mut ally_icons) = fill_unit_kills_and_icons(
        &ally_icons_base,
        ally_player,
        main_player,
        &unit_type_dict_ally,
        ally_kills_counted_toward_main,
        &killcounts,
        &dictionaries.unit_name_dict,
        &dictionaries.unit_add_kills_to,
        &dictionaries.replay_analysis_data.unit_add_losses_to,
        &dont_include_units_set,
        &icon_units_set,
    );

    let main_killbot_feed = count_for_pid(&killbot_feed, main_player);
    if main_killbot_feed > 0 {
        set_icon_count(&mut main_icons, "killbots", main_killbot_feed);
    }
    let ally_killbot_feed = count_for_pid(&killbot_feed, ally_player);
    if ally_killbot_feed > 0 {
        set_icon_count(&mut ally_icons, "killbots", ally_killbot_feed);
    }

    let amon_units = fill_amon_units(
        &unit_type_dict_amon,
        &killcounts,
        &amon_player_ids_set,
        &dictionaries.unit_name_dict,
        &dictionaries.unit_add_kills_to,
        &dictionaries.replay_analysis_data.unit_add_losses_to,
        &dont_include_units_set,
        &skip_tokens,
    );

    let mut detailed_input = ReplayReportDetailedInput::from_parser(parser);
    detailed_input.positions = Some(PlayerPositions {
        main: main_player as u8,
        ally: ally_player as u8,
    });
    detailed_input.length = Some(detailed_input.parser.accurate_length / 1.4);
    detailed_input.bonus = Some(bonus);
    detailed_input.comp = Some(comp);
    detailed_input.main_kills = Some(clamp_nonnegative_to_u64(count_for_pid(
        &killcounts,
        main_player,
    )));
    detailed_input.ally_kills = Some(clamp_nonnegative_to_u64(count_for_pid(
        &killcounts,
        ally_player,
    )));
    detailed_input.main_icons = Some(main_icons);
    detailed_input.ally_icons = Some(ally_icons);
    detailed_input.main_units = Some(main_units);
    detailed_input.ally_units = Some(ally_units);
    detailed_input.amon_units = Some(amon_units);
    detailed_input.player_stats = Some(player_stats);
    if !outlaw_order.is_empty() {
        detailed_input.outlaw_order = Some(outlaw_order);
    }

    Ok(build_replay_report_detailed(
        &detailed_input.parser.file,
        &detailed_input,
        main_player_handles,
    ))
}
