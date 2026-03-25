use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rfd::FileDialog;
use s2coop_analyzer::cache_overall_stats_generator::{
    generate_cache_overall_stats_with_logger, serialize_cache_entries, CacheReplayEntry,
    GenerateCacheConfig,
};
use s2coop_analyzer::detailed_replay_analysis::calculate_replay_hash;
use serde::{Deserialize, Serialize};
use serde_json::{self, json, Map, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex, TryLockError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri_plugin_updater::UpdaterExt;

use tauri::{
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager, State, Wry,
};

#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(target_os = "windows")]
use winreg::RegKey;

mod dictionary_data;
mod logging;
mod overlay_info;
mod path_manager;
mod performance_overlay;
mod randomizer;
mod replay_analysis;
mod shared_types;

#[macro_export]
macro_rules! sco_log {
    ($($arg:tt)*) => {{
        $crate::logging::log_line(&format!($($arg)*));
    }};
}

use crate::path_manager::{get_cache_path, get_json_data_dir, is_dev_env};
use crate::replay_analysis::ReplayAnalysis;

const UNLIMITED_REPLAY_LIMIT: usize = 0;
const SCO_REPLAY_SCAN_PROGRESS_EVENT: &str = "sco://replay-scan-progress";
const WINDOWS_STARTUP_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const WINDOWS_STARTUP_VALUE_NAME: &str = "SCO Overlay";
static ACTIVE_SETTINGS: OnceLock<Mutex<Value>> = OnceLock::new();

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn mutator_icon_name(name: &str) -> &str {
    match name {
        "Moment Of Silence" => "Moment of Silence",
        _ => name,
    }
}

fn sanitize_settings_value(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            map.remove("fast_expand");
            map.remove("force_hide_overlay");
            Value::Object(map)
        }
        other => other,
    }
}

fn get_system_language() -> String {
    let default = "en";
    let locale = sys_locale::get_locale();

    let language = if let Some(locale) = locale.as_ref() {
        let language = locale.split("-").nth(0);

        if language.is_none() {
            default
        } else {
            let language = language.unwrap();

            if language.len() == 0 {
                default
            } else {
                language
            }
        }
    } else {
        "en"
    };

    language.to_string()
}

fn default_settings_value() -> Value {
    json!({
        "start_with_windows": false,
        "minimize_to_tray": true,
        "start_minimized": false,
        "auto_update": true,
        "duration": 30,
        "show_player_winrates": true,
        "show_replay_info_after_game": true,
        "show_session": true,
        "show_charts": true,
        "account_folder": get_default_accounts_folder(),
        "color_player1": "#0080F8",
        "color_player2": "#00D532",
        "color_amon": "#FF0000",
        "color_mastery": "#FFDC87",
        "hotkey_show/hide": "Ctrl+Shift+8",
        "hotkey_newer": "Ctrl+Alt+/",
        "hotkey_older": "Ctrl+Alt+8",
        "hotkey_winrates": "Ctrl+Alt+-",
        "enable_logging": true,
        "dark_theme": true,
        "language": get_system_language(),
    })
}

pub(crate) fn merge_settings_with_defaults(value: Value) -> Value {
    let sanitized = sanitize_settings_value(value);
    let mut merged = match default_settings_value() {
        Value::Object(defaults) => defaults,
        _ => Map::new(),
    };

    if let Value::Object(settings) = sanitized {
        merged.extend(settings);
    }

    Value::Object(merged)
}

fn read_saved_settings_file_from_path(path: &Path, create_if_missing: bool) -> Value {
    let defaults = default_settings_value();
    if !path.exists() {
        if create_if_missing {
            let _ = write_saved_settings_file_to_path(path, &defaults);
        }
        return defaults;
    }

    let text = std::fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
    let parsed = serde_json::from_str(&text).unwrap_or(Value::Object(Default::default()));
    merge_settings_with_defaults(parsed)
}

pub(crate) fn read_saved_settings_file() -> Value {
    let path = path_manager::get_settings_path();

    read_saved_settings_file_from_path(&path, !cfg!(test))
}

fn active_settings_store() -> &'static Mutex<Value> {
    ACTIVE_SETTINGS.get_or_init(|| Mutex::new(read_saved_settings_file()))
}

pub(crate) fn replace_active_settings(value: &Value) -> Value {
    let sanitized = sanitize_settings_value(value.clone());
    if let Ok(mut cached_settings) = active_settings_store().lock() {
        *cached_settings = sanitized.clone();
    }
    sanitized
}

fn read_settings_file() -> Value {
    active_settings_store()
        .lock()
        .map(|settings| settings.clone())
        .unwrap_or_else(|_| read_saved_settings_file())
}

fn write_saved_settings_file_to_path(path: &Path, value: &Value) -> Result<Value, String> {
    let sanitized = merge_settings_with_defaults(value.clone());
    let text = serde_json::to_string_pretty(&sanitized)
        .map_err(|error| format!("Failed to serialize settings: {error}"))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create settings directory: {error}"))?;
    }
    std::fs::write(path, text).map_err(|error| format!("Failed to write settings: {error}"))?;
    Ok(sanitized)
}

fn write_saved_settings_file(value: &Value) -> Result<Value, String> {
    let path = path_manager::get_settings_path();

    write_saved_settings_file_to_path(&path, value)
}

fn write_settings_file(value: &Value) -> Result<(), String> {
    let previous_start_with_windows = start_with_windows_enabled(&read_settings_file());
    let sanitized = write_saved_settings_file(value)?;
    replace_active_settings(&sanitized);
    let new_start_with_windows = start_with_windows_enabled(&sanitized);
    if previous_start_with_windows != new_start_with_windows {
        if let Err(error) = sync_start_with_windows_setting(&sanitized) {
            crate::sco_log!("[SCO/settings] Failed to sync start_with_windows: {error}");
        }
    }
    Ok(())
}

fn start_with_windows_enabled(settings: &Value) -> bool {
    settings
        .get("start_with_windows")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn windows_startup_command_value(executable_path: &Path) -> String {
    format!("\"{}\"", executable_path.display())
}

#[cfg(target_os = "windows")]
fn sync_windows_startup_registration(enabled: bool) -> Result<(), String> {
    if enabled {
        let executable_path = std::env::current_exe()
            .map_err(|error| format!("Failed to resolve executable path: {error}"))?;
        let command_value = windows_startup_command_value(&executable_path);
        let status = Command::new("reg")
            .args([
                "add",
                WINDOWS_STARTUP_RUN_KEY,
                "/v",
                WINDOWS_STARTUP_VALUE_NAME,
                "/t",
                "REG_SZ",
                "/d",
                &command_value,
                "/f",
            ])
            .status()
            .map_err(|error| format!("Failed to update Windows startup entry: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "reg add exited with status {}",
                status
                    .code()
                    .map_or_else(|| "unknown".to_string(), |code| code.to_string())
            ))
        }
    } else {
        let status = Command::new("reg")
            .args([
                "delete",
                WINDOWS_STARTUP_RUN_KEY,
                "/v",
                WINDOWS_STARTUP_VALUE_NAME,
                "/f",
            ])
            .status()
            .map_err(|error| format!("Failed to remove Windows startup entry: {error}"))?;
        if status.success() || status.code() == Some(1) {
            Ok(())
        } else {
            Err(format!(
                "reg delete exited with status {}",
                status
                    .code()
                    .map_or_else(|| "unknown".to_string(), |code| code.to_string())
            ))
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn sync_windows_startup_registration(_enabled: bool) -> Result<(), String> {
    Ok(())
}

fn sync_start_with_windows_setting(settings: &Value) -> Result<(), String> {
    sync_windows_startup_registration(start_with_windows_enabled(settings))
}

const OVERLAY_RUNTIME_SETTING_KEYS: [&str; 8] = [
    "color_player1",
    "color_player2",
    "color_amon",
    "color_mastery",
    "duration",
    "show_session",
    "show_charts",
    "language",
];

const OVERLAY_HOTKEY_SETTING_KEYS: [&str; 7] = [
    "hotkey_show/hide",
    "hotkey_show",
    "hotkey_hide",
    "hotkey_newer",
    "hotkey_older",
    "hotkey_winrates",
    "performance_hotkey",
];

const OVERLAY_PLACEMENT_SETTING_KEYS: [&str; 6] = [
    "monitor",
    "width",
    "height",
    "top_offset",
    "right_offset",
    "subtract_height",
];

const PERFORMANCE_RUNTIME_SETTING_KEYS: [&str; 4] = [
    "performance_show",
    "performance_geometry",
    "performance_processes",
    "monitor",
];

fn setting_value_changed(previous_settings: &Value, next_settings: &Value, key: &str) -> bool {
    previous_settings.get(key) != next_settings.get(key)
}

fn any_setting_changed(previous_settings: &Value, next_settings: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .any(|key| setting_value_changed(previous_settings, next_settings, key))
}

pub(crate) fn persist_single_setting_value(key: &str, value: Value) -> Result<(), String> {
    let mut saved_settings = read_saved_settings_file();
    if !saved_settings.is_object() {
        saved_settings = Value::Object(Default::default());
    }
    let saved_map = saved_settings
        .as_object_mut()
        .ok_or_else(|| "Settings root is not an object".to_string())?;
    saved_map.insert(key.to_string(), value.clone());
    write_saved_settings_file(&saved_settings)?;

    let mut active_settings = read_settings_file();
    if !active_settings.is_object() {
        active_settings = Value::Object(Default::default());
    }
    let active_map = active_settings
        .as_object_mut()
        .ok_or_else(|| "Active settings root is not an object".to_string())?;
    active_map.insert(key.to_string(), value);
    replace_active_settings(&active_settings);
    Ok(())
}

fn apply_runtime_settings(
    app: &tauri::AppHandle<Wry>,
    previous_settings: &Value,
    next_settings: &Value,
) {
    let next_settings = replace_active_settings(next_settings);
    logging::refresh_from_settings(&next_settings);
    let overlay_runtime_changed = any_setting_changed(
        previous_settings,
        &next_settings,
        &OVERLAY_RUNTIME_SETTING_KEYS,
    );
    let overlay_hotkeys_changed = any_setting_changed(
        previous_settings,
        &next_settings,
        &OVERLAY_HOTKEY_SETTING_KEYS,
    );
    let overlay_placement_changed = any_setting_changed(
        previous_settings,
        &next_settings,
        &OVERLAY_PLACEMENT_SETTING_KEYS,
    );
    let performance_runtime_changed = any_setting_changed(
        previous_settings,
        &next_settings,
        &PERFORMANCE_RUNTIME_SETTING_KEYS,
    );

    if overlay_runtime_changed {
        overlay_info::sync_overlay_runtime_settings(app);
    }

    let previous_show_charts = previous_settings
        .get("show_charts")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let show_charts = next_settings
        .get("show_charts")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if show_charts != previous_show_charts {
        let _ = app.emit(
            overlay_info::OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT,
            json!(show_charts),
        );
    }
    if overlay_hotkeys_changed {
        if let Err(error) = overlay_info::register_overlay_hotkeys(app) {
            crate::sco_log!("[SCO/hotkey] Failed to reload hotkeys: {error}");
        }
    }
    if overlay_placement_changed {
        if let Some(window) = app.get_webview_window("overlay") {
            if let Err(error) =
                overlay_info::apply_overlay_placement_from_settings(&window, &next_settings)
            {
                crate::sco_log!("[SCO/overlay] Failed to apply overlay placement: {error}");
            }
        }
    }
    if performance_runtime_changed {
        performance_overlay::apply_settings(app);
    }

    if let Ok(mut stats) = app.state::<BackendState>().stats.lock() {
        stats.detailed_analysis_atstart = next_settings
            .get("detailed_analysis_atstart")
            .and_then(Value::as_bool)
            .unwrap_or(stats.detailed_analysis_atstart);
    }
}

fn update_settings_player_note(
    settings: &mut Value,
    handle: &str,
    note_value: &str,
) -> Result<(), String> {
    let normalized_handle = ReplayAnalysis::normalized_handle_key(handle);
    if normalized_handle.is_empty() {
        return Err("Handle is empty".to_string());
    }

    if !settings.is_object() {
        *settings = Value::Object(Default::default());
    }
    let settings_map = settings
        .as_object_mut()
        .ok_or_else(|| "Settings root is not an object".to_string())?;

    let notes_value = settings_map
        .entry("player_notes".to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    if !notes_value.is_object() {
        *notes_value = Value::Object(Default::default());
    }
    let notes = notes_value
        .as_object_mut()
        .ok_or_else(|| "player_notes is not an object".to_string())?;

    let existing_key = notes
        .keys()
        .find(|key| ReplayAnalysis::normalized_handle_key(key) == normalized_handle)
        .cloned()
        .unwrap_or_else(|| sanitize_replay_text(handle).trim().to_string());

    let trimmed_note = note_value.trim();
    if trimmed_note.is_empty() {
        notes.remove(&existing_key);
    } else {
        notes.insert(existing_key, Value::String(note_value.to_string()));
    }

    Ok(())
}

fn folder_dialog_start_directory(directory: Option<String>) -> Option<PathBuf> {
    let trimmed = directory
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let candidate = PathBuf::from(trimmed);

    if candidate.is_dir() {
        return Some(candidate);
    }

    candidate.parent().and_then(|parent| {
        if parent.is_dir() {
            Some(parent.to_path_buf())
        } else {
            None
        }
    })
}

fn logging_enabled_from_settings(settings: &Value) -> bool {
    settings
        .get("enable_logging")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn session_counter_delta(result: &str) -> (u64, u64) {
    match result.trim().to_ascii_lowercase().as_str() {
        "victory" => (1, 0),
        "defeat" => (0, 1),
        _ => (0, 0),
    }
}

fn record_session_result(state: &BackendState, result: &str) {
    let (victories, defeats) = session_counter_delta(result);
    if victories > 0 {
        state
            .session_victories
            .fetch_add(victories, Ordering::AcqRel);
    }
    if defeats > 0 {
        state.session_defeats.fetch_add(defeats, Ordering::AcqRel);
    }
}

fn show_replay_info_after_game_from_settings(settings: &Value) -> bool {
    settings
        .get("show_replay_info_after_game")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn session_counts(state: &BackendState) -> (u64, u64) {
    (
        state.session_victories.load(Ordering::Acquire),
        state.session_defeats.load(Ordering::Acquire),
    )
}

fn units_to_stats() -> &'static HashSet<String> {
    dictionary_data::units_to_stats()
}

fn extract_account_handles_from_folder(account_root: &str) -> HashSet<String> {
    let mut handles = HashSet::new();
    let root = PathBuf::from(account_root);
    if !root.exists() || !root.is_dir() {
        return handles;
    }

    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let normalized = ReplayAnalysis::normalized_handle_key(name);
                    if !normalized.is_empty() {
                        handles.insert(normalized);
                    }
                }
                stack.push(path);
            }
        }
    }
    handles
}

fn infer_owner_handle_from_replay_path(path: &str) -> Option<String> {
    let replay_path = Path::new(path);
    for component in replay_path.components() {
        let raw = component.as_os_str().to_str()?;
        let normalized = ReplayAnalysis::normalized_handle_key(raw);
        if !normalized.is_empty() {
            return Some(normalized);
        }
    }
    None
}

fn get_default_accounts_folder() -> String {
    #[cfg(target_os = "windows")]
    {
        // Try to get StarCraft II accounts folder from registry
        if let Ok(sc2_key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Blizzard Entertainment\StarCraft II")
        {
            if let Ok(path) = sc2_key.get_value::<String, &str>("InstallPath") {
                let accounts_folder = Path::new(&path).join("Accounts");
                if let Some(accounts_str) = accounts_folder.to_str() {
                    return accounts_str.to_string();
                }
            }
        }

        // Fallback to standard user Documents path
        if let Ok(shell_folders) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        {
            if let Ok(documents) = shell_folders.get_value::<String, &str>("Personal") {
                let accounts_folder = Path::new(&documents).join("StarCraft II").join("Accounts");
                if let Some(accounts_str) = accounts_folder.to_str() {
                    return accounts_str.to_string();
                }
            }
        }

        String::new()
    }

    #[cfg(not(target_os = "windows"))]
    {
        String::new()
    }
}

fn configured_main_names_from_settings(settings: &Value) -> HashSet<String> {
    let mut names = settings
        .get("main_names")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(ReplayAnalysis::normalized_player_key)
                .filter(|name| !name.is_empty())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    if !names.is_empty() {
        return names;
    }

    let account_root = settings
        .get("account_folder")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if account_root.is_empty() {
        return names;
    }

    static DISCOVERED_MAIN_NAMES: OnceLock<Mutex<HashMap<String, HashSet<String>>>> =
        OnceLock::new();
    let discovered_cache =
        DISCOVERED_MAIN_NAMES.get_or_init(|| Mutex::new(HashMap::<String, HashSet<String>>::new()));
    if let Ok(cache) = discovered_cache.lock() {
        if let Some(cached) = cache.get(account_root) {
            return cached.clone();
        }
    }

    let root = PathBuf::from(account_root);
    if !root.exists() || !root.is_dir() {
        return names;
    }

    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let is_link = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
            if !is_link {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !stem.contains('_') || !stem.contains('@') {
                continue;
            }
            let name = stem.split('_').next().unwrap_or_default();
            let normalized = ReplayAnalysis::normalized_player_key(name);
            if !normalized.is_empty() {
                names.insert(normalized);
            }
        }
    }

    if let Ok(mut cache) = discovered_cache.lock() {
        cache.insert(account_root.to_string(), names.clone());
    }

    names
}

fn configured_main_names() -> HashSet<String> {
    configured_main_names_from_settings(&read_settings_file())
}

fn configured_main_handles_from_settings(settings: &Value) -> HashSet<String> {
    let account_root = settings
        .get("account_folder")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if account_root.is_empty() {
        return HashSet::new();
    }

    static DISCOVERED_MAIN_HANDLES: OnceLock<Mutex<HashMap<String, HashSet<String>>>> =
        OnceLock::new();
    let discovered_cache = DISCOVERED_MAIN_HANDLES
        .get_or_init(|| Mutex::new(HashMap::<String, HashSet<String>>::new()));

    if let Ok(cache) = discovered_cache.lock() {
        if let Some(cached) = cache.get(account_root) {
            return cached.clone();
        }
    }

    let handles = extract_account_handles_from_folder(account_root);
    if let Ok(mut cache) = discovered_cache.lock() {
        cache.insert(account_root.to_string(), handles.clone());
    }
    handles
}

fn configured_main_handles() -> HashSet<String> {
    configured_main_handles_from_settings(&read_settings_file())
}

fn replay_should_swap_main_and_ally(
    replay: &ReplayInfo,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> bool {
    let p1_handle = ReplayAnalysis::normalized_handle_key(&replay.p1_handle);
    let p2_handle = ReplayAnalysis::normalized_handle_key(&replay.p2_handle);
    if !main_handles.is_empty() && (!p1_handle.is_empty() || !p2_handle.is_empty()) {
        let p1_is_main = ReplayAnalysis::is_main_player_by_handle(&replay.p1_handle, main_handles);
        let p2_is_main = ReplayAnalysis::is_main_player_by_handle(&replay.p2_handle, main_handles);
        if p1_is_main != p2_is_main {
            return !p1_is_main && p2_is_main;
        }
    }

    if let Some(owner_handle) = infer_owner_handle_from_replay_path(&replay.file) {
        let p1_owner = !p1_handle.is_empty() && p1_handle == owner_handle;
        let p2_owner = !p2_handle.is_empty() && p2_handle == owner_handle;
        if p1_owner != p2_owner {
            return !p1_owner && p2_owner;
        }
    }

    if !main_names.is_empty() {
        let p1_is_main = ReplayAnalysis::is_main_player_by_name(&replay.p1, main_names);
        let p2_is_main = ReplayAnalysis::is_main_player_by_name(&replay.p2, main_names);
        if p1_is_main != p2_is_main {
            return !p1_is_main && p2_is_main;
        }
    }

    false
}

fn swap_player_stats_sides(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let one = obj.remove("1");
    let two = obj.remove("2");
    if let Some(v2) = two {
        obj.insert("1".to_string(), v2);
    }
    if let Some(v1) = one {
        obj.insert("2".to_string(), v1);
    }
}

fn orient_replay_for_main_names(
    mut replay: ReplayInfo,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> ReplayInfo {
    if !replay_should_swap_main_and_ally(&replay, main_names, main_handles) {
        return replay;
    }

    std::mem::swap(&mut replay.p1, &mut replay.p2);
    std::mem::swap(&mut replay.p1_handle, &mut replay.p2_handle);
    std::mem::swap(&mut replay.main_apm, &mut replay.ally_apm);
    std::mem::swap(&mut replay.main_kills, &mut replay.ally_kills);
    std::mem::swap(&mut replay.main_commander, &mut replay.ally_commander);
    std::mem::swap(
        &mut replay.main_commander_level,
        &mut replay.ally_commander_level,
    );
    std::mem::swap(
        &mut replay.main_mastery_level,
        &mut replay.ally_mastery_level,
    );
    std::mem::swap(&mut replay.main_prestige, &mut replay.ally_prestige);
    std::mem::swap(&mut replay.main_masteries, &mut replay.ally_masteries);
    std::mem::swap(&mut replay.main_units, &mut replay.ally_units);
    std::mem::swap(&mut replay.main_icons, &mut replay.ally_icons);
    swap_player_stats_sides(&mut replay.player_stats);
    replay
}

#[allow(dead_code)]
#[derive(Clone, Serialize, Deserialize, Default, PartialEq)]
struct ReplayChatMessage {
    player: u8,
    text: String,
    time: f64,
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq)]
struct ReplayChatPayload {
    file: String,
    date: u64,
    map: String,
    result: String,
    slot1_name: String,
    slot2_name: String,
    messages: Vec<ReplayChatMessage>,
}

#[allow(dead_code)]
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ReplayInfo {
    file: String,
    date: u64,
    map: String,
    result: String,
    difficulty: String,
    p1: String,
    p2: String,
    slot1_name: String,
    slot2_name: String,
    enemy: String,
    p1_handle: String,
    p2_handle: String,
    slot1_handle: String,
    slot2_handle: String,
    length: u64,
    accurate_length: f64,
    main_apm: u64,
    ally_apm: u64,
    main_kills: u64,
    ally_kills: u64,
    main_commander: String,
    ally_commander: String,
    slot1_commander: String,
    slot2_commander: String,
    main_commander_level: u64,
    ally_commander_level: u64,
    main_mastery_level: u64,
    ally_mastery_level: u64,
    main_prestige: u64,
    ally_prestige: u64,
    main_masteries: Vec<u64>,
    ally_masteries: Vec<u64>,
    main_units: Value,
    ally_units: Value,
    amon_units: Value,
    main_icons: Value,
    ally_icons: Value,
    player_stats: Value,
    extension: bool,
    brutal_plus: u64,
    weekly: bool,
    weekly_name: Option<String>,
    mutators: Vec<String>,
    comp: String,
    bonus: Vec<u64>,
    bonus_total: Option<u64>,
    messages: Vec<ReplayChatMessage>,
    is_detailed: bool,
}

struct ReplayPlayerInfo {
    name: String,
    handle: String,
    apm: u64,
    kills: u64,
    commander: String,
    commander_level: u64,
    mastery_level: u64,
    prestige: u64,
    masteries: Vec<u64>,
    units: Value,
    icons: Value,
    stats: Value,
}

impl ReplayInfo {
    fn as_games_row(&self) -> Value {
        let sanitized = self.sanitized_for_client();
        let p1 = if sanitized.slot1_name.trim().is_empty() {
            sanitized.p1.clone()
        } else {
            sanitized.slot1_name.clone()
        };
        let p2 = if sanitized.slot2_name.trim().is_empty() {
            sanitized.p2.clone()
        } else {
            sanitized.slot2_name.clone()
        };
        let p1_commander = if sanitized.slot1_commander.trim().is_empty() {
            sanitized.main_commander.clone()
        } else {
            sanitized.slot1_commander.clone()
        };
        let p2_commander = if sanitized.slot2_commander.trim().is_empty() {
            sanitized.ally_commander.clone()
        } else {
            sanitized.slot2_commander.clone()
        };
        let mutators = sanitized
            .mutators
            .iter()
            .map(|mutator_name| {
                let (name_en, name_ko, description_en, description_ko) =
                    dictionary_data::mutators()
                        .get(mutator_name)
                        .map(|value| {
                            (
                                decode_html_entities(&value.name_en),
                                decode_html_entities(&value.name_ko),
                                decode_html_entities(&value.description_en),
                                decode_html_entities(&value.description_ko),
                            )
                        })
                        .unwrap_or_default();
                json!({
                    "name": mutator_name,
                    "nameEn": if name_en.is_empty() { mutator_name.to_string() } else { name_en },
                    "nameKo": name_ko,
                    "iconName": mutator_icon_name(mutator_name),
                    "descriptionEn": description_en,
                    "descriptionKo": description_ko,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "file": sanitized.file,
            "date": sanitized.date,
            "map": sanitized.map,
            "result": sanitized.result,
            "difficulty": sanitized.difficulty,
            "p1": p1,
            "p2": p2,
            "enemy": sanitized.enemy,
            "main_commander": p1_commander,
            "ally_commander": p2_commander,
            "length": sanitized.length,
            "main_apm": sanitized.main_apm,
            "ally_apm": sanitized.ally_apm,
            "main_kills": sanitized.main_kills,
            "ally_kills": sanitized.ally_kills,
            "extension": sanitized.extension,
            "brutal_plus": sanitized.brutal_plus,
            "weekly": sanitized.weekly,
            "weekly_name": sanitized.weekly_name,
            "mutators": mutators,
            "is_mutation": sanitized.weekly || !sanitized.mutators.is_empty(),
        })
    }

    fn chat_payload(&self) -> ReplayChatPayload {
        let sanitized = self.sanitized_for_client();
        let slot1_name = if sanitized.slot1_name.trim().is_empty() {
            sanitized.p1.clone()
        } else {
            sanitized.slot1_name.clone()
        };
        let slot2_name = if sanitized.slot2_name.trim().is_empty() {
            sanitized.p2.clone()
        } else {
            sanitized.slot2_name.clone()
        };

        ReplayChatPayload {
            file: sanitized.file,
            date: sanitized.date,
            map: sanitized.map,
            result: sanitized.result,
            slot1_name,
            slot2_name,
            messages: sanitized.messages,
        }
    }

    pub(crate) fn sanitized_for_client(&self) -> Self {
        let client_result = if self.result.eq_ignore_ascii_case("Unparsed") {
            "Failed".to_string()
        } else {
            sanitize_replay_text(&self.result)
        };
        Self {
            file: self.file.clone(),
            date: self.date,
            map: sanitize_replay_text(&map_display_name(&self.map)),
            result: client_result,
            difficulty: sanitize_replay_text(&self.difficulty),
            p1: sanitize_replay_text(&self.p1),
            p2: sanitize_replay_text(&self.p2),
            slot1_name: sanitize_replay_text(&self.slot1_name),
            slot2_name: sanitize_replay_text(&self.slot2_name),
            enemy: sanitize_replay_text(&self.enemy),
            p1_handle: self.p1_handle.clone(),
            p2_handle: self.p2_handle.clone(),
            slot1_handle: self.slot1_handle.clone(),
            slot2_handle: self.slot2_handle.clone(),
            length: self.length,
            accurate_length: self.accurate_length,
            main_apm: self.main_apm,
            ally_apm: self.ally_apm,
            main_kills: self.main_kills,
            ally_kills: self.ally_kills,
            main_commander: sanitize_replay_text(&self.main_commander),
            ally_commander: sanitize_replay_text(&self.ally_commander),
            slot1_commander: sanitize_replay_text(&self.slot1_commander),
            slot2_commander: sanitize_replay_text(&self.slot2_commander),
            main_commander_level: self.main_commander_level,
            ally_commander_level: self.ally_commander_level,
            main_mastery_level: self.main_mastery_level,
            ally_mastery_level: self.ally_mastery_level,
            main_prestige: self.main_prestige,
            ally_prestige: self.ally_prestige,
            main_masteries: normalize_mastery_values(&self.main_masteries),
            ally_masteries: normalize_mastery_values(&self.ally_masteries),
            main_units: sanitize_unit_map(&self.main_units),
            ally_units: sanitize_unit_map(&self.ally_units),
            amon_units: sanitize_unit_map(&self.amon_units),
            main_icons: sanitize_icon_map(&self.main_icons),
            ally_icons: sanitize_icon_map(&self.ally_icons),
            player_stats: sanitize_player_stats_payload(&self.player_stats),
            extension: self.extension,
            brutal_plus: self.brutal_plus,
            weekly: self.weekly,
            weekly_name: self
                .weekly_name
                .as_ref()
                .map(|value| sanitize_replay_text(value))
                .filter(|value| !value.is_empty()),
            mutators: self.mutators.clone(),
            comp: self.comp.clone(),
            bonus: self.bonus.clone(),
            bonus_total: self.bonus_total,
            messages: self
                .messages
                .iter()
                .map(|message| ReplayChatMessage {
                    player: message.player,
                    text: sanitize_replay_text(&message.text),
                    time: if message.time.is_finite() {
                        message.time.max(0.0)
                    } else {
                        0.0
                    },
                })
                .collect(),
            is_detailed: self.is_detailed,
        }
    }

    fn sanitized(&self) -> Self {
        self.clone()
    }
}

static REPLAY_SCAN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static APP_EXIT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static REPLAY_SCAN_PROGRESS: OnceLock<ReplayScanProgress> = OnceLock::new();
static DELAYED_REPLAY_WINRATE_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct ReplayScanProgress {
    stage: Mutex<String>,
    status: Mutex<String>,
    total: AtomicU64,
    cache_hits: AtomicU64,
    to_parse: AtomicU64,
    newly_parsed: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    parse_skipped: AtomicU64,
    started_at_ms: AtomicU64,
    elapsed_ms: AtomicU64,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

impl Default for ReplayScanProgress {
    fn default() -> Self {
        Self {
            stage: Mutex::new("idle".to_string()),
            status: Mutex::new("Idle".to_string()),
            total: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            to_parse: AtomicU64::new(0),
            newly_parsed: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            parse_skipped: AtomicU64::new(0),
            started_at_ms: AtomicU64::new(0),
            elapsed_ms: AtomicU64::new(0),
        }
    }
}

impl ReplayScanProgress {
    fn reset(&self, stage: &str) {
        self.total.store(0, Ordering::Release);
        self.cache_hits.store(0, Ordering::Release);
        self.to_parse.store(0, Ordering::Release);
        self.newly_parsed.store(0, Ordering::Release);
        self.completed.store(0, Ordering::Release);
        self.failed.store(0, Ordering::Release);
        self.parse_skipped.store(0, Ordering::Release);
        self.started_at_ms.store(now_millis(), Ordering::Release);
        self.elapsed_ms.store(0, Ordering::Release);
        if let Ok(mut value) = self.stage.lock() {
            *value = stage.to_string();
        }
        if let Ok(mut value) = self.status.lock() {
            *value = "Parsing".to_string();
        }
    }

    fn set_stage(&self, stage: &str) {
        if let Ok(mut value) = self.stage.lock() {
            *value = stage.to_string();
        }
    }

    fn set_status(&self, status: &str) {
        if let Ok(mut value) = self.status.lock() {
            *value = status.to_string();
        }
        if status == "Completed" {
            let started_at = self.started_at_ms.load(Ordering::Acquire);
            if started_at > 0 {
                let elapsed = now_millis().saturating_sub(started_at);
                self.elapsed_ms.store(elapsed, Ordering::Release);
            }
        }
    }

    fn set_counts(&self, total: u64, completed: u64) {
        let bounded_completed = completed.min(total);
        self.total.store(total, Ordering::Release);
        self.completed.store(bounded_completed, Ordering::Release);
        self.to_parse
            .store(total.saturating_sub(bounded_completed), Ordering::Release);
        self.cache_hits.store(0, Ordering::Release);
        self.newly_parsed.store(0, Ordering::Release);
        self.failed.store(0, Ordering::Release);
        self.parse_skipped.store(0, Ordering::Release);
    }

    fn as_json(&self) -> Value {
        let stage = self
            .stage
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        let status = self
            .status
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "Parsing".to_string());
        let total = self.total.load(Ordering::Acquire);
        let cache_hits = self.cache_hits.load(Ordering::Acquire);
        let to_parse = self.to_parse.load(Ordering::Acquire);
        let newly_parsed = self.newly_parsed.load(Ordering::Acquire);
        let completed = self.completed.load(Ordering::Acquire);
        let failed = self.failed.load(Ordering::Acquire);
        let parse_skipped = self.parse_skipped.load(Ordering::Acquire);
        let started_at = self.started_at_ms.load(Ordering::Acquire);
        let stored_elapsed = self.elapsed_ms.load(Ordering::Acquire);
        let elapsed_ms = if status == "Parsing" && started_at > 0 {
            now_millis().saturating_sub(started_at)
        } else {
            stored_elapsed
        };
        let effective_total = if total > 0 {
            total
        } else {
            cache_hits.saturating_add(to_parse)
        };
        json!({
            "stage": stage,
            "status": status,
            "parsing_status": status,
            "total": effective_total,
            "total_replay_files": effective_total,
            "cache_hits": cache_hits,
            "files_already_cached": cache_hits,
            "to_parse": to_parse,
            "completed": completed,
            "newly_parsed": newly_parsed,
            "newly_parsed_files": newly_parsed,
            "failed": failed,
            "parse_failed_files": failed,
            "parse_skipped": parse_skipped,
            "parse_skipped_files": parse_skipped,
            "elapsed_ms": elapsed_ms,
            "total_time_taken_ms": elapsed_ms,
        })
    }
}

fn replay_scan_progress() -> &'static ReplayScanProgress {
    REPLAY_SCAN_PROGRESS.get_or_init(ReplayScanProgress::default)
}

fn emit_replay_scan_progress(app: &AppHandle<Wry>) {
    let payload = replay_scan_progress().as_json();
    if let Err(error) = app.emit(SCO_REPLAY_SCAN_PROGRESS_EVENT, payload) {
        crate::sco_log!("[SCO/stats] failed to emit scan progress: {error}");
    }
}

struct ScanInFlightGuard;

impl Drop for ScanInFlightGuard {
    fn drop(&mut self) {
        REPLAY_SCAN_IN_FLIGHT.store(false, Ordering::Release);
    }
}

#[derive(Default)]
struct Aggregate {
    wins: u64,
    losses: u64,
}

#[derive(Default)]
struct RegionAggregate {
    wins: u64,
    losses: u64,
    max_asc: u64,
    max_com: HashSet<String>,
    prestiges: HashMap<String, u64>,
}

#[derive(Default)]
struct CommanderAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    mastery_counts: [f64; 6],
    mastery_by_prestige_counts: [[f64; 6]; 4],
    prestige_counts: [u64; 4],
    detailed_count: u64,
}

#[derive(Default)]
struct PlayerAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    last_seen: u64,
    handles: BTreeSet<String>,
    names: HashMap<String, u64>,
    commander: String,
    commander_counts: HashMap<String, u64>,
}

#[allow(dead_code)]
#[derive(Default)]
struct MapAggregate {
    wins: u64,
    losses: u64,
    victory_length_sum: f64,
    victory_games: u64,
    bonus_fraction_sum: f64,
    bonus_games: u64,
    fastest_length: f64,
    fastest_file: String,
    fastest_p1: String,
    fastest_p2: String,
    fastest_p1_handle: String,
    fastest_p2_handle: String,
    fastest_p1_commander: String,
    fastest_p2_commander: String,
    fastest_p1_apm: u64,
    fastest_p2_apm: u64,
    fastest_p1_mastery_level: u64,
    fastest_p2_mastery_level: u64,
    fastest_p1_masteries: Vec<u64>,
    fastest_p2_masteries: Vec<u64>,
    fastest_p1_prestige: u64,
    fastest_p2_prestige: u64,
    fastest_date: u64,
    fastest_difficulty: String,
    fastest_enemy_race: String,
    detailed_count: u64,
}

fn canonicalize_coop_map_id(raw: &str) -> Option<String> {
    dictionary_data::canonicalize_coop_map_id(raw)
}

fn coop_map_id_to_english(map_id: &str) -> Option<String> {
    dictionary_data::coop_map_id_to_english(map_id)
}

fn canonicalize_coop_map_name(raw: &str) -> Option<String> {
    dictionary_data::coop_map_english_name(raw)
}

fn map_display_name(raw: &str) -> String {
    canonicalize_coop_map_name(raw).unwrap_or_else(|| raw.to_string())
}

fn is_official_coop_replay(replay: &ReplayInfo) -> bool {
    canonicalize_coop_map_id(&replay.map).is_some()
}

#[derive(Default, Clone)]
struct UnitStatsRollup {
    created: i64,
    created_hidden: bool,
    made: u64,
    lost: i64,
    lost_hidden: bool,
    kills: i64,
    kill_percentages: Vec<f64>,
}

#[derive(Default)]
struct CommanderUnitRollup {
    count: u64,
    units: HashMap<String, UnitStatsRollup>,
}

fn unit_excluded_from_stats_for_commander(commander: &str, unit: &str) -> bool {
    (unit == "MULE" && commander != "Raynor")
        || (unit == "Spider Mine" && commander != "Raynor" && commander != "Nova")
        || (unit == "Omega Worm" && commander != "Kerrigan")
        || (unit == "Nydus Worm" && commander != "Abathur")
}

fn unit_excluded_from_sum_for_commander(commander: &str, unit: &str) -> bool {
    matches!(
        unit,
        "Mecha Infestor"
            | "Havoc"
            | "SCV"
            | "Probe"
            | "Drone"
            | "Mecha Drone"
            | "Primal Drone"
            | "Infested SCV"
            | "Probius"
            | "Dominion Laborer"
            | "Primal Hive"
            | "Primal Warden"
            | "Imperial Intercessor"
            | "Archangel"
    ) || (commander != "Tychus" && unit == "Auto-Turret")
}

fn commander_mind_control_unit(commander: &str) -> Option<&'static str> {
    dictionary_data::commander_mind_control_unit(commander)
}

fn unit_rollup_count_value(value: i64, hidden: bool) -> Value {
    if hidden {
        Value::String("-".to_string())
    } else {
        json!(value)
    }
}

fn build_amon_unit_data(amon_rollup: std::collections::BTreeMap<String, UnitStatsRollup>) -> Value {
    const AMON_KD_MUTATORS: [&str; 4] = [
        "Twister",
        "Purifier Beam",
        "Moebius Corps Laser Drill",
        "Blizzard",
    ];

    const AMON_REMOVED_UNITS: [&str; 3] = [
        "AdeptPhaseShift",
        "Drakken Pulse Cannon",
        "James 'Sirius' Sykes",
    ];

    let mut rows = amon_rollup
        .into_iter()
        .collect::<Vec<(String, UnitStatsRollup)>>();

    rows.sort_by(|(left_name, left), (right_name, right)| {
        right
            .created
            .cmp(&left.created)
            .then_with(|| left_name.cmp(right_name))
    });

    let mut out = Map::new();
    let mut total = UnitStatsRollup::default();

    for (unit, mut row) in rows {
        if AMON_REMOVED_UNITS
            .iter()
            .any(|removed| removed == &unit.as_str())
        {
            continue;
        }

        if AMON_KD_MUTATORS
            .iter()
            .any(|mutator| mutator == &unit.as_str())
        {
            row.lost = 0;
            out.insert(
                unit,
                json!({
                    "created": row.created,
                    "lost": row.lost,
                    "kills": row.kills,
                    "KD": "-",
                }),
            );
        } else {
            out.insert(
                unit,
                json!({
                    "created": row.created,
                    "lost": row.lost,
                    "kills": row.kills,
                    "KD": if row.lost <= 0 {
                        0.0
                    } else {
                        row.kills as f64 / row.lost as f64
                    },
                }),
            );
        }

        total.created = total.created.saturating_add(row.created);
        total.lost = total.lost.saturating_add(row.lost);
        total.kills = total.kills.saturating_add(row.kills);
    }

    out.insert(
        "sum".to_string(),
        json!({
            "created": total.created,
            "lost": total.lost,
            "kills": total.kills,
            "KD": if total.lost <= 0 {
                0.0
            } else {
                total.kills as f64 / total.lost as f64
            },
        }),
    );

    Value::Object(out)
}

fn build_commander_unit_data(
    side_rollup: std::collections::BTreeMap<String, CommanderUnitRollup>,
) -> Value {
    let mut out = Map::new();

    for (commander, entry) in side_rollup {
        let mut rows = Map::new();
        let mut totals = UnitStatsRollup::default();
        let mut units_to_delete = HashSet::new();
        let mut units = entry.units.into_iter().collect::<Vec<_>>();
        let stats_units = units_to_stats();

        units.sort_by(|(left_name, left), (right_name, right)| {
            right
                .kills
                .cmp(&left.kills)
                .then_with(|| right.created.cmp(&left.created))
                .then_with(|| left_name.cmp(right_name))
        });

        for (unit, unit_row) in units {
            if unit_row.kills == 0 && !stats_units.contains(unit.as_str()) {
                units_to_delete.insert(unit);
                continue;
            }

            if unit_excluded_from_stats_for_commander(&commander, &unit) {
                units_to_delete.insert(unit);
                continue;
            }

            let made = if entry.count == 0 {
                0.0
            } else {
                unit_row.made as f64 / entry.count as f64
            };
            let lost_percent =
                if !unit_row.created_hidden && !unit_row.lost_hidden && unit_row.created > 0 {
                    Some(unit_row.lost as f64 / unit_row.created as f64)
                } else {
                    None
                };
            let kd = if !unit_row.lost_hidden && unit_row.lost > 0 {
                Some(unit_row.kills as f64 / unit_row.lost as f64)
            } else {
                None
            };
            let kill_percentage = if unit_row.kill_percentages.is_empty() {
                0.0
            } else {
                median_f64(&unit_row.kill_percentages)
            };

            if !unit_excluded_from_sum_for_commander(&commander, &unit) {
                if !unit_row.created_hidden {
                    totals.created = totals.created.saturating_add(unit_row.created);
                }
                if !unit_row.lost_hidden {
                    totals.lost = totals.lost.saturating_add(unit_row.lost);
                }
                totals.kills = totals.kills.saturating_add(unit_row.kills);
            }

            rows.insert(
                unit,
                json!({
                    "created": unit_rollup_count_value(unit_row.created, unit_row.created_hidden),
                    "made": made,
                    "lost": unit_rollup_count_value(unit_row.lost, unit_row.lost_hidden),
                    "lost_percent": lost_percent,
                    "kills": unit_row.kills,
                    "KD": kd,
                    "kill_percentage": kill_percentage,
                }),
            );
        }

        for unit in units_to_delete {
            rows.remove(&unit);
        }

        let total_lost_percent = if totals.created == 0 {
            0.0
        } else {
            totals.lost as f64 / totals.created as f64
        };
        let total_kd = if totals.lost <= 0 {
            0.0
        } else {
            totals.kills as f64 / totals.lost as f64
        };
        rows.insert(
            "sum".to_string(),
            json!({
                "created": totals.created,
                "made": 1.0,
                "lost": totals.lost,
                "lost_percent": total_lost_percent,
                "kills": totals.kills,
                "KD": total_kd,
                "kill_percentage": 1.0,
            }),
        );
        rows.insert("count".to_string(), json!(entry.count));
        out.insert(commander, Value::Object(rows));
    }

    Value::Object(out)
}

fn sanitize_replay_text(value: &str) -> String {
    fn strip_tags(value: &str) -> String {
        let mut output = String::with_capacity(value.len());
        let mut in_tag = false;
        for ch in value.chars() {
            match ch {
                '<' => {
                    in_tag = true;
                    output.push(' ');
                }
                '>' if in_tag => {
                    in_tag = false;
                    output.push(' ');
                }
                _ if !in_tag => output.push(ch),
                _ => {}
            }
        }
        output
    }

    fn decode_html_entities(value: &str) -> String {
        let mut output = String::with_capacity(value.len());
        let mut chars = value.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '&' {
                output.push(ch);
                continue;
            }

            let mut entity = String::from('&');
            while let Some(&next) = chars.peek() {
                entity.push(next);
                let _ = chars.next();
                if next == ';' {
                    break;
                }
            }

            let lower = entity.to_ascii_lowercase();
            let decoded = match lower.as_str() {
                "&lt;" => "<".to_string(),
                "&gt;" => ">".to_string(),
                "&amp;" => "&".to_string(),
                "&quot;" => "\"".to_string(),
                "&apos;" | "&#39;" => "'".to_string(),
                "&nbsp;" => " ".to_string(),
                _ if lower.starts_with("&#x") && lower.ends_with(';') => {
                    u32::from_str_radix(&lower[3..lower.len() - 1], 16)
                        .ok()
                        .and_then(std::char::from_u32)
                        .map(|ch| ch.to_string())
                        .unwrap_or(entity)
                }
                _ if lower.starts_with("&#") && lower.ends_with(';') => lower[2..lower.len() - 1]
                    .parse::<u32>()
                    .ok()
                    .and_then(std::char::from_u32)
                    .map(|ch| ch.to_string())
                    .unwrap_or(entity),
                _ => entity,
            };

            output.push_str(&decoded);
        }

        output
    }

    let mut text = value
        .trim()
        .trim_matches('\u{0}')
        .replace("\\u003c", "<")
        .replace("\\u003e", ">");
    text = decode_html_entities(&text);
    text = text.replace("\\u003c", "<").replace("\\u003e", ">");
    text = strip_tags(&text);

    let mut normalized = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_control() && ch != '\t' && ch != '\n' && ch != '\r' {
            if !last_space {
                normalized.push(' ');
                last_space = true;
            }
            continue;
        }
        if ch == ' ' {
            if !last_space {
                normalized.push(' ');
                last_space = true;
            }
            continue;
        }
        last_space = false;
        normalized.push(ch);
    }

    normalized.trim().to_string()
}

fn normalize_mastery_values(raw: &[u64]) -> Vec<u64> {
    let mut values = vec![0u64; 6];
    for (index, value) in raw.iter().take(6).enumerate() {
        values[index] = *value;
    }
    values
}

fn sanitize_unit_map(value: &Value) -> Value {
    if let Value::Object(raw) = value {
        let mut output = Map::new();
        for (key, raw_entry) in raw.iter() {
            if key.is_empty() {
                continue;
            }
            if let Some(arr) = raw_entry.as_array() {
                let mut values: [Value; 4] = [json!(0), json!(0), json!(0), json!(0.0)];
                for (idx, item) in arr.iter().take(4).enumerate() {
                    if idx < 3 {
                        if let Some(number) = item.as_f64() {
                            values[idx] = if number.is_finite() {
                                json!(number.round() as i64)
                            } else {
                                json!(0)
                            };
                        } else if item.is_string() {
                            values[idx] = item.clone();
                        }
                    } else if let Some(number) = item.as_f64() {
                        values[idx] = if number.is_finite() {
                            json!(number.max(0.0))
                        } else {
                            json!(0.0)
                        };
                    }
                }
                output.insert(
                    sanitize_replay_text(key),
                    json!([
                        values[0].clone(),
                        values[1].clone(),
                        values[2].clone(),
                        values[3].clone()
                    ]),
                );
            }
        }
        Value::Object(output)
    } else {
        Value::Object(Map::new())
    }
}

fn sanitize_icon_map(value: &Value) -> Value {
    let mut output = Map::new();
    if let Value::Object(raw) = value {
        for (key, raw_value) in raw.iter() {
            if key.is_empty() {
                continue;
            }

            if key == "outlaws" {
                if let Some(items) = raw_value.as_array() {
                    let outlaws = items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|name| name.to_string())
                        .collect::<Vec<_>>();
                    if !outlaws.is_empty() {
                        output.insert(
                            key.clone(),
                            Value::Array(outlaws.into_iter().map(Value::String).collect()),
                        );
                    }
                }
                continue;
            }

            if let Some(count) = raw_value.as_u64() {
                output.insert(key.clone(), json!(count));
            }
        }
    }
    Value::Object(output)
}

fn sanitize_player_stats_payload(value: &Value) -> Value {
    let mut output = Map::new();
    if let Value::Object(players) = value {
        for (key, raw_player) in players.iter() {
            if let Some(raw_player) = raw_player.as_object() {
                let sanitize_array = |entry: Option<&Vec<Value>>| -> Vec<f64> {
                    entry
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|value| value.as_f64())
                                .map(|value| if value.is_finite() { value } else { 0.0 })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };

                let kills = sanitize_array(raw_player.get("killed").and_then(Value::as_array));
                let army = sanitize_array(raw_player.get("army").and_then(Value::as_array));
                let supply = sanitize_array(raw_player.get("supply").and_then(Value::as_array));
                let mining = sanitize_array(raw_player.get("mining").and_then(Value::as_array));
                let name = raw_player
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                output.insert(
                    key.clone(),
                    json!({
                        "name": sanitize_replay_text(&name),
                        "killed": kills,
                        "army": army,
                        "supply": supply,
                        "mining": mining,
                    }),
                );
            }
        }
    }
    Value::Object(output)
}

static PLAYERS_SCAN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

fn normalize_known_commander_name(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "alarak" => Some("Alarak"),
        "artanis" => Some("Artanis"),
        "fenix" => Some("Fenix"),
        "karax" => Some("Karax"),
        "vorazun" => Some("Vorazun"),
        "zeratul" => Some("Zeratul"),
        "horner" | "han & horner" => Some("Han & Horner"),
        "mengsk" => Some("Mengsk"),
        "nova" => Some("Nova"),
        "raynor" => Some("Raynor"),
        "swann" => Some("Swann"),
        "tychus" => Some("Tychus"),
        "abathur" => Some("Abathur"),
        "dehaka" => Some("Dehaka"),
        "kerrigan" => Some("Kerrigan"),
        "stukov" => Some("Stukov"),
        "zagara" => Some("Zagara"),
        "stetmann" => Some("Stetmann"),
        _ => None,
    }
}

fn format_date_from_system_time(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn build_replay_root_candidates(raw: &str) -> Vec<PathBuf> {
    let trimmed = raw.trim().trim_matches(&['"', '\''][..]).to_string();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::with_capacity(3);
    let primary = PathBuf::from(&trimmed);
    candidates.push(primary.clone());

    let swapped = if trimmed.contains('\\') {
        trimmed.replace('\\', "/")
    } else {
        trimmed.replace('/', "\\")
    };
    if !swapped.is_empty() {
        let swapped_path = PathBuf::from(&swapped);
        if swapped_path != primary {
            candidates.push(swapped_path);
        }
    }

    let drive_prefix = trimmed.chars().nth(1) == Some(':');
    if drive_prefix {
        let windows_drive = trimmed
            .chars()
            .next()
            .unwrap_or('C')
            .to_ascii_lowercase()
            .to_string();
        let rest = trimmed[2..].trim_start_matches(['\\', '/'].as_ref());
        let wsl = format!(
            "/mnt/{}/{}",
            windows_drive,
            rest.replace('\\', "/").trim_start_matches('/')
        );
        let wsl_path = PathBuf::from(&wsl);
        if wsl_path != primary {
            candidates.push(wsl_path);
        }
    }
    candidates
}

fn resolve_replay_root() -> Option<PathBuf> {
    let settings = read_settings_file();
    if let Value::Object(map) = settings {
        if let Some(account_folder) = map.get("account_folder").and_then(Value::as_str) {
            let candidates = build_replay_root_candidates(account_folder);
            if let Some(path) = candidates.iter().find(|path| path.is_dir()) {
                return Some(path.clone());
            }
        }
    }

    None
}

fn replay_watch_root_from_settings() -> Option<PathBuf> {
    let settings = read_settings_file();
    let account_folder = settings
        .get("account_folder")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    build_replay_root_candidates(account_folder)
        .into_iter()
        .find(|candidate| candidate.is_dir())
}

fn clear_analysis_cache_files() {
    let cache_path = get_cache_path();
    let temp_path = PathBuf::from(format!("{}_temp", cache_path.display()));

    for path in [cache_path, temp_path] {
        if let Err(error) = std::fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                crate::sco_log!(
                    "[SCO/cache] failed to delete analysis cache file '{}': {error}",
                    path.display()
                );
            }
        }
    }
}

fn generate_detailed_analysis_cache(
    app: &AppHandle<Wry>,
    stats: &Arc<Mutex<StatsState>>,
) -> Result<usize, String> {
    let Some(account_dir) = resolve_replay_root() else {
        return Err("Replay root is not configured for detailed analysis.".to_string());
    };
    let output_file = get_cache_path();
    let logger = {
        let app = app.clone();
        let stats = Arc::clone(stats);
        move |message: String| {
            if let Some((completed, total)) = parse_detailed_analysis_progress_counts(&message) {
                replay_scan_progress().set_counts(total, completed);
            }
            let normalized = normalize_detailed_analysis_logger_message(&message);
            crate::sco_log!("[SCO/stats] {normalized}");
            replay_scan_progress().set_stage("detailed_analysis_running");
            replay_scan_progress().set_status("Parsing");
            if let Ok(mut guard) = stats.lock() {
                guard.detailed_analysis_status = normalized.clone();
                guard.message = normalized.clone();
            }
            emit_replay_scan_progress(&app);
        }
    };

    generate_cache_overall_stats_with_logger(
        &GenerateCacheConfig {
            account_dir,
            output_file: output_file.clone(),
        },
        &logger,
    )
    .map(|summary| summary.scanned_replays)
    .map_err(|error| format!("Failed to generate '{}': {error}", output_file.display()))
}

fn normalize_region_code(code: &str) -> Option<&'static str> {
    match code {
        "1" => Some("NA"),
        "2" => Some("EU"),
        "3" => Some("KR"),
        "4" => Some("SEA"),
        "5" => Some("CN"),
        "6" => Some("CN"),
        "8" => Some("KR"),
        "98" => Some("PTR"),
        _ => None,
    }
}

fn infer_region_from_handle(handle: &str) -> Option<String> {
    let region_code = handle.split('-').next().map(str::trim)?;
    if region_code.is_empty() {
        return None;
    }
    normalize_region_code(region_code).map(|region| region.to_string())
}

fn scan_replays(limit: usize) -> Vec<ReplayInfo> {
    ReplayAnalysis::scan_replays(limit)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupAnalysisTrigger {
    Setup,
    FrontendReady,
}

impl StartupAnalysisTrigger {
    fn label(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::FrontendReady => "frontend_ready",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StartupAnalysisRequestOutcome {
    include_detailed: bool,
    started: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AnalysisMode {
    Simple,
    Detailed,
}

impl AnalysisMode {
    fn from_include_detailed(include_detailed: bool) -> Self {
        if include_detailed {
            Self::Detailed
        } else {
            Self::Simple
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Detailed => "detailed",
        }
    }

    fn display(self) -> &'static str {
        match self {
            Self::Simple => "Simple analysis",
            Self::Detailed => "Detailed analysis",
        }
    }

    fn peer_display(self) -> &'static str {
        match self {
            Self::Simple => "Detailed analysis",
            Self::Detailed => "Simple analysis",
        }
    }
}

fn analysis_mode(include_detailed: bool) -> AnalysisMode {
    AnalysisMode::from_include_detailed(include_detailed)
}

fn analysis_status_text(mode: AnalysisMode, phase: &str) -> String {
    format!("{}: {phase}.", mode.display())
}

fn analysis_started_message(mode: AnalysisMode) -> String {
    analysis_status_text(mode, "started in background")
}

fn analysis_already_running_message(mode: AnalysisMode) -> String {
    analysis_status_text(mode, "already running")
}

fn analysis_blocked_by_other_mode_message(mode: AnalysisMode) -> String {
    format!(
        "{} cannot start while {} is running.",
        mode.display(),
        mode.peer_display()
    )
}

fn analysis_at_start_message(enabled: bool) -> String {
    if enabled {
        "Detailed analysis at startup enabled.".to_string()
    } else {
        "Detailed analysis at startup disabled.".to_string()
    }
}

fn analysis_error_status_text(mode: AnalysisMode, message: &str) -> String {
    format!("{}: {message}", mode.display())
}

fn normalize_detailed_analysis_logger_message(message: &str) -> String {
    let normalized = message.replace('\n', " | ");
    if normalized == "Starting detailed analysis!" {
        return analysis_status_text(AnalysisMode::Detailed, "generating cache");
    }
    if normalized.starts_with("Running... ") || normalized.starts_with("Estimated remaining time:")
    {
        return format!(
            "{}: cache generation progress | {normalized}",
            AnalysisMode::Detailed.display()
        );
    }
    if normalized.starts_with("Detailed analysis completed! ") {
        return analysis_status_text(AnalysisMode::Detailed, "cache generation completed");
    }
    if normalized.starts_with("Detailed analysis completed in ") {
        return format!("{}: {}", AnalysisMode::Detailed.display(), normalized);
    }
    normalized
}

fn parse_progress_fraction(value: &str) -> Option<(u64, u64)> {
    let (completed, remainder) = value.trim().split_once('/')?;
    let completed = completed.trim().parse::<u64>().ok()?;
    let total_text = remainder.trim();
    let total_end = total_text
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(total_text.len());
    let total = total_text.get(..total_end)?.trim().parse::<u64>().ok()?;
    Some((completed, total))
}

fn parse_detailed_analysis_progress_counts(message: &str) -> Option<(u64, u64)> {
    for line in message.lines().map(str::trim) {
        if let Some(progress) = line.strip_prefix("Running... ") {
            return parse_progress_fraction(progress);
        }
        if let Some(progress) = line.strip_prefix("Detailed analysis completed! ") {
            return parse_progress_fraction(progress);
        }
    }
    None
}

fn set_analysis_running_status(stats: &mut StatsState, mode: AnalysisMode, phase: &str) {
    let status = analysis_status_text(mode, phase);
    match mode {
        AnalysisMode::Simple => stats.simple_analysis_status = status,
        AnalysisMode::Detailed => stats.detailed_analysis_status = status,
    }
}

fn set_analysis_terminal_status(stats: &mut StatsState, mode: AnalysisMode, phase: &str) {
    match mode {
        AnalysisMode::Simple => {
            stats.simple_analysis_running = false;
            stats.simple_analysis_status = analysis_status_text(mode, phase);
        }
        AnalysisMode::Detailed => {
            stats.detailed_analysis_running = false;
            stats.detailed_analysis_status = analysis_status_text(mode, phase);
        }
    }
}

fn startup_analysis_mode(include_detailed: bool) -> &'static str {
    analysis_mode(include_detailed).slug()
}

fn prepare_startup_analysis_request(
    stats: &mut StatsState,
    trigger: StartupAnalysisTrigger,
) -> StartupAnalysisRequestOutcome {
    let include_detailed = stats.detailed_analysis_atstart;
    if stats.startup_analysis_requested {
        return StartupAnalysisRequestOutcome {
            include_detailed: include_detailed,
            started: false,
        };
    }

    stats.startup_analysis_requested = true;
    let mode = analysis_mode(include_detailed);
    stats.message = match trigger {
        StartupAnalysisTrigger::Setup => format!(
            "{}: startup requested while the frontend loads.",
            mode.display()
        ),
        StartupAnalysisTrigger::FrontendReady => {
            format!("{}: startup requested in background.", mode.display())
        }
    };

    StartupAnalysisRequestOutcome {
        include_detailed: include_detailed,
        started: true,
    }
}

fn request_startup_analysis(
    app: AppHandle<Wry>,
    stats: Arc<Mutex<StatsState>>,
    replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
    trigger: StartupAnalysisTrigger,
) -> Result<StartupAnalysisRequestOutcome, String> {
    let outcome = {
        let mut guard = stats
            .lock()
            .map_err(|error| format!("Failed to access stats state: {error}"))?;
        prepare_startup_analysis_request(&mut guard, trigger)
    };

    if outcome.started {
        crate::sco_log!(
            "[SCO/stats] startup analysis requested from {} mode={}",
            trigger.label(),
            startup_analysis_mode(outcome.include_detailed)
        );
        spawn_startup_analysis_task(
            app,
            stats,
            replays_slot,
            stats_replays_slot,
            stats_current_replay_files_slot,
            outcome.include_detailed,
        );
    } else {
        crate::sco_log!(
            "[SCO/stats] startup analysis already requested before {} mode={}",
            trigger.label(),
            startup_analysis_mode(outcome.include_detailed)
        );
    }

    Ok(outcome)
}

fn update_analysis_replay_cache_slots(
    replays: &[ReplayInfo],
    replays_slot: &Arc<Mutex<Vec<ReplayInfo>>>,
    stats_replays_slot: &Arc<Mutex<Vec<ReplayInfo>>>,
) {
    if let Ok(mut cache) = replays_slot.lock() {
        *cache = replays.to_vec();
    } else {
        crate::sco_log!("[SCO/stats] failed to update shared replay cache after scan");
    }
    if let Ok(mut cache) = stats_replays_slot.lock() {
        *cache = replays.to_vec();
    } else {
        crate::sco_log!("[SCO/stats] failed to update stats replay cache after scan");
    }
}

fn load_existing_cache_by_hash() -> HashMap<String, CacheReplayEntry> {
    let cache_path = get_cache_path();
    let payload = match std::fs::read(&cache_path) {
        Ok(payload) => payload,
        Err(_) => return HashMap::new(),
    };
    let entries = match serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload) {
        Ok(entries) => entries,
        Err(error) => {
            crate::sco_log!("[SCO/cache] failed to load existing cache for merging: {error}");
            return HashMap::new();
        }
    };

    entries
        .into_iter()
        .filter(|entry| !entry.hash.is_empty())
        .map(|entry| (entry.hash.clone(), entry))
        .collect()
}

fn merge_cache_entries(
    existing_by_hash: &HashMap<String, CacheReplayEntry>,
    mut new_entries: Vec<CacheReplayEntry>,
) -> Vec<CacheReplayEntry> {
    // First, add all existing entries
    let mut merged = existing_by_hash.clone();

    // Then, add new entries, but detailed > simple
    for entry in new_entries.drain(..) {
        let hash = entry.hash.clone();
        if hash.is_empty() {
            continue;
        }

        match merged.get(&hash) {
            Some(existing) => {
                // Keep detailed over simple
                if !existing.detailed_analysis && entry.detailed_analysis {
                    merged.insert(hash, entry);
                } else if existing.detailed_analysis && !entry.detailed_analysis {
                    // Keep existing detailed, ignore new simple
                } else {
                    // Both same type, use newest by date
                    if entry.date > existing.date {
                        merged.insert(hash, entry);
                    }
                }
            }
            None => {
                merged.insert(hash, entry);
            }
        }
    }

    let mut result: Vec<CacheReplayEntry> = merged.into_values().collect();
    result.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.file.cmp(&a.file))
            .then_with(|| b.hash.cmp(&a.hash))
    });
    result
}

fn spawn_analysis_task(
    app: AppHandle<Wry>,
    stats: Arc<Mutex<StatsState>>,
    replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
    include_detailed: bool,
    limit: usize,
) {
    let mode = analysis_mode(include_detailed);
    {
        let mut guard = match stats.lock() {
            Ok(guard) => guard,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/stats] failed to start background {} thread: {error}",
                    mode.display()
                );
                return;
            }
        };

        if include_detailed {
            if guard.simple_analysis_running {
                crate::sco_log!(
                    "[SCO/stats] {} blocked while {} is running",
                    mode.display(),
                    mode.peer_display()
                );
                guard.message = analysis_blocked_by_other_mode_message(mode);
                return;
            }
            if guard.detailed_analysis_running {
                crate::sco_log!("[SCO/stats] {} already running", mode.display());
                guard.message = analysis_already_running_message(mode);
                return;
            }
            guard.detailed_analysis_running = true;
            set_analysis_running_status(&mut guard, mode, "generating cache");
            guard.message = analysis_started_message(mode);
        } else {
            if guard.detailed_analysis_running {
                crate::sco_log!(
                    "[SCO/stats] {} blocked while {} is running",
                    mode.display(),
                    mode.peer_display()
                );
                guard.message = analysis_blocked_by_other_mode_message(mode);
                return;
            }
            if guard.simple_analysis_running {
                crate::sco_log!("[SCO/stats] {} already running", mode.display());
                guard.message = analysis_already_running_message(mode);
                return;
            }
            guard.simple_analysis_running = true;
            set_analysis_running_status(&mut guard, mode, "scanning replays");
            guard.message = analysis_started_message(mode);
        }

        guard.ready = false;
        guard.analysis = Some(empty_stats_payload());
        guard.games = 0;
        guard.main_players = Vec::new();
        guard.main_handles = Vec::new();
        guard.commander_mastery = Value::Object(Default::default());
        guard.prestige_names = Value::Object(Default::default());
        if guard.message.is_empty() {
            guard.message = analysis_started_message(mode);
        }
    }
    replay_scan_progress().reset("queued");

    let analysis_state = stats;
    let shared_replay_cache_slot = replays_slot;
    let replay_cache_slot = stats_replays_slot;
    let current_replay_files_slot = stats_current_replay_files_slot;
    let app_for_analysis = app.clone();
    let app_for_progress = app.clone();
    let app_for_progress_updates = app.clone();
    thread::spawn(move || {
        let started_at = Instant::now();
        crate::sco_log!("[SCO/stats] {} thread started", mode.display());
        replay_scan_progress().set_stage(if include_detailed {
            "detailed_analysis_running"
        } else {
            "scan_running"
        });
        replay_scan_progress().set_status("Parsing");
        emit_replay_scan_progress(&app_for_progress);

        let should_stop = Arc::new(AtomicBool::new(false));
        let stop_for_progress = should_stop.clone();
        let progress_handle = thread::spawn(move || {
            while !stop_for_progress.load(Ordering::Acquire) {
                emit_replay_scan_progress(&app_for_progress_updates);
                thread::sleep(Duration::from_millis(150));
            }
            emit_replay_scan_progress(&app_for_progress_updates);
        });

        // Load existing cache at start for merging and hash checking
        let existing_cache_by_hash = load_existing_cache_by_hash();
        let mut all_new_cache_entries = Vec::new();
        let all_replays;

        // Run analysis based on mode
        if include_detailed {
            // Generate detailed analysis, which produces detailed cache entries
            let generation_started_at = Instant::now();
            match generate_detailed_analysis_cache(&app_for_progress, &analysis_state) {
                Ok(scanned_replays) => {
                    crate::sco_log!(
                        "[SCO/stats] {} generated '{}' with {} replay(s) in {}ms",
                        mode.display(),
                        get_cache_path().display(),
                        scanned_replays,
                        generation_started_at.elapsed().as_millis()
                    );
                    if let Ok(mut guard) = analysis_state.lock() {
                        set_analysis_running_status(
                            &mut guard,
                            mode,
                            "refreshing replay summaries",
                        );
                        guard.message = format!(
                            "Generated '{}' with {} replay entr{}.",
                            get_cache_path().display(),
                            scanned_replays,
                            if scanned_replays == 1 { "y" } else { "ies" }
                        );
                    }

                    // Load the generated detailed cache
                    let cache_path = get_cache_path();
                    let payload = match std::fs::read(&cache_path) {
                        Ok(p) => p,
                        Err(error) => {
                            crate::sco_log!(
                                "[SCO/stats] failed to read generated detailed cache: {error}"
                            );
                            vec![]
                        }
                    };

                    if let Ok(entries) = serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload) {
                        all_new_cache_entries.extend(entries);
                    }

                    // Load detailed replays from generated cache
                    let main_names = configured_main_names();
                    let main_handles = configured_main_handles();
                    let detailed_replays = ReplayAnalysis::load_detailed_analysis_replays_snapshot(
                        limit,
                        &main_names,
                        &main_handles,
                    );
                    all_replays = detailed_replays;
                }
                Err(message) => {
                    crate::sco_log!("[SCO/stats] {} failed: {message}", mode.display());
                    if let Ok(mut guard) = analysis_state.lock() {
                        set_analysis_terminal_status(&mut guard, mode, "failed");
                        guard.detailed_analysis_status = analysis_error_status_text(mode, &message);
                        guard.message = message;
                    }
                    replay_scan_progress().set_stage("analysis_failed");
                    replay_scan_progress().set_status("Completed");
                    emit_replay_scan_progress(&app_for_analysis);
                    should_stop.store(true, Ordering::Release);
                    let _ = progress_handle.join();
                    return;
                }
            }
        } else {
            // Simple analysis mode - run scan_replays
            let scan_started_at = Instant::now();
            let scanned_replays = scan_replays(limit);
            crate::sco_log!(
                "[SCO/stats] {} scanned {} replay(s) in {}ms",
                mode.display(),
                scanned_replays.len(),
                scan_started_at.elapsed().as_millis()
            );
            all_replays = scanned_replays;

            // scan_replays already saves simple cache entries directly via persist_simple_analysis_cache
            // No need to save again here
        }

        let mut hashes = HashMap::new();

        let all_replays = all_replays
            .into_iter()
            .filter(|replay| {
                let hash = calculate_replay_hash(&PathBuf::from(&replay.file));

                let is_detailed = hashes.get(&hash);

                if is_detailed.is_some() && (*is_detailed.unwrap() || !replay.is_detailed) {
                    false
                } else {
                    hashes.insert(hash, replay.is_detailed);
                    true
                }
            })
            .collect::<Vec<_>>();

        let current_replay_files = current_replay_files_snapshot(UNLIMITED_REPLAY_LIMIT);
        update_analysis_replay_cache_slots(
            &all_replays,
            &shared_replay_cache_slot,
            &replay_cache_slot,
        );
        if let Ok(mut current_files) = current_replay_files_slot.lock() {
            *current_files = current_replay_files;
        } else {
            crate::sco_log!("[SCO/stats] failed to update current replay file set after scan");
        }

        // Load cache and merge entries for both modes
        let final_cache_entries = if include_detailed {
            merge_cache_entries(&existing_cache_by_hash, all_new_cache_entries)
        } else {
            // For simple mode, scan_replays already saved its entries
            // Just load them
            load_existing_cache_by_hash().into_values().collect()
        };

        // Save final detailed cache (simple mode already saved during scan_replays)
        if include_detailed {
            let cache_path = get_cache_path();
            if let Err(error) =
                s2coop_analyzer::cache_overall_stats_generator::persist_simple_analysis_cache(
                    &final_cache_entries,
                    &cache_path,
                )
            {
                crate::sco_log!("[SCO/stats] failed to persist final merged cache: {error}");
            }
        }

        replay_scan_progress().set_stage("building_statistics");
        let snapshot = ReplayAnalysis::build_rebuild_snapshot(&all_replays, include_detailed);

        let mut guard = match analysis_state.lock() {
            Ok(guard) => guard,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/stats] {} aborted before rebuild: {error}",
                    mode.display()
                );
                replay_scan_progress().set_stage("analysis_ready");
                replay_scan_progress().set_status("Completed");
                emit_replay_scan_progress(&app_for_analysis);
                should_stop.store(true, Ordering::Release);
                let _ = progress_handle.join();
                return;
            }
        };

        set_analysis_running_status(&mut guard, mode, "building statistics");

        apply_rebuild_snapshot(&mut guard, snapshot, mode);
        if !include_detailed {
            sync_detailed_analysis_status_from_replays(&mut guard, &all_replays);
        }
        replay_scan_progress().set_stage("analysis_ready");
        replay_scan_progress().set_status("Completed");
        emit_replay_scan_progress(&app_for_analysis);

        crate::sco_log!(
            "[SCO/stats] {} completed in {}ms for {} replay(s)",
            mode.display(),
            started_at.elapsed().as_millis(),
            all_replays.len()
        );

        should_stop.store(true, Ordering::Release);
        let _ = progress_handle.join();
    });
}

fn spawn_startup_analysis_task(
    app: AppHandle<Wry>,
    stats: Arc<Mutex<StatsState>>,
    replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
    include_detailed: bool,
) {
    crate::sco_log!(
        "[SCO/stats] startup analysis mode={}",
        startup_analysis_mode(include_detailed)
    );
    spawn_analysis_task(
        app,
        stats,
        replays_slot,
        stats_replays_slot,
        stats_current_replay_files_slot,
        include_detailed,
        UNLIMITED_REPLAY_LIMIT,
    );
}

fn parse_query_i64(path: &str, key: &str) -> Option<i64> {
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let parsed_key = parts.next()?;
        if parsed_key != key {
            continue;
        }
        let value = parts.next()?;
        if let Ok(number) = value.parse::<i64>() {
            return Some(number);
        }
    }
    None
}

fn query_hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_query_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let high = query_hex_value(bytes[index + 1]);
                let low = query_hex_value(bytes[index + 2]);
                if let (Some(high), Some(low)) = (high, low) {
                    decoded.push((high << 4) | low);
                    index += 3;
                    continue;
                }
                decoded.push(bytes[index]);
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn parse_query_usize(path: &str, key: &str, default: usize) -> usize {
    parse_query_i64(path, key)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_query_value(path: &str, key: &str) -> Option<String> {
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let parsed_key = parts.next()?;
        if parsed_key != key {
            continue;
        }
        let value = parts.next().unwrap_or_default();
        return Some(decode_query_component(value));
    }
    None
}

fn parse_query_bool(path: &str, key: &str, default: bool) -> bool {
    match parse_query_value(path, key)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn parse_query_csv(path: &str, key: &str) -> Vec<String> {
    parse_query_value(path, key)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

fn ymd_from_unix_seconds(seconds: u64) -> Option<u32> {
    let days = i64::try_from(seconds / 86_400).ok()?;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    if year < 0 {
        return None;
    }
    let year_u32 = u32::try_from(year).ok()?;
    let month_u32 = u32::try_from(m).ok()?;
    let day_u32 = u32::try_from(d).ok()?;
    if !(1..=12).contains(&month_u32) || !(1..=31).contains(&day_u32) {
        return None;
    }
    year_u32
        .checked_mul(10_000)
        .and_then(|value| {
            month_u32
                .checked_mul(100)
                .and_then(|month| value.checked_add(month))
        })
        .and_then(|value| value.checked_add(day_u32))
}

fn build_stats_response(
    path: &str,
    stats: &Arc<Mutex<StatsState>>,
    stats_replays: &Arc<Mutex<Vec<ReplayInfo>>>,
    stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
) -> Result<Value, String> {
    ReplayAnalysis::build_stats_response(path, stats, stats_replays, stats_current_replay_files)
}

fn spawn_players_scan_task(
    replays_slot: Arc<Mutex<Vec<ReplayInfo>>>,
    selected_replay_file: Arc<Mutex<Option<String>>>,
    limit: usize,
) {
    if PLAYERS_SCAN_IN_FLIGHT
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    thread::spawn(move || {
        crate::sco_log!("[SCO/players] background player scan started (limit={limit})");
        let replays = scan_replays(limit);
        let selected = replays.first().map(|replay| replay.file.clone());

        match replays_slot.lock() {
            Ok(mut cache) => {
                *cache = replays;
            }
            Err(error) => {
                crate::sco_log!("[SCO/players] failed to update player replay cache: {error}");
            }
        }

        if let Ok(mut selected_file) = selected_replay_file.lock() {
            match selected_file.as_ref() {
                Some(current)
                    if replays_slot.lock().ok().map_or(false, |cache| {
                        cache.iter().any(|replay| &replay.file == current)
                    }) => {}
                _ => {
                    *selected_file = selected;
                }
            }
        }

        PLAYERS_SCAN_IN_FLIGHT.store(false, Ordering::Release);
        crate::sco_log!("[SCO/players] background player scan completed");
    });
}

fn replay_index_by_file(replays: &[ReplayInfo], file: &Option<String>) -> Option<usize> {
    let needle = file.as_deref()?;
    replays.iter().position(|entry| entry.file == needle)
}

fn sync_full_replay_cache_slots(
    replays_slot: &Arc<Mutex<Vec<ReplayInfo>>>,
    selected_replay_file: &Arc<Mutex<Option<String>>>,
) -> Vec<ReplayInfo> {
    let cached = replays_slot
        .lock()
        .ok()
        .map(|cache| cache.clone())
        .unwrap_or_default();

    let replays = if cached.is_empty() {
        let main_names = configured_main_names();
        let main_handles = configured_main_handles();
        let from_detailed_analysis = ReplayAnalysis::load_detailed_analysis_replays_snapshot(
            UNLIMITED_REPLAY_LIMIT,
            &main_names,
            &main_handles,
        );
        let loaded = if from_detailed_analysis.is_empty() {
            scan_replays(UNLIMITED_REPLAY_LIMIT)
        } else {
            from_detailed_analysis
        };
        if let Ok(mut cache) = replays_slot.lock() {
            *cache = loaded.clone();
        }
        loaded
    } else {
        cached
    };

    let selected = replays.first().map(|replay| replay.file.clone());

    if let Ok(mut selected_file) = selected_replay_file.lock() {
        match selected_file.as_ref() {
            Some(current) if replays.iter().any(|replay| &replay.file == current) => {}
            _ => {
                *selected_file = selected;
            }
        }
    }

    replays
}

fn sync_replay_cache_slots(
    replays_slot: &Arc<Mutex<Vec<ReplayInfo>>>,
    selected_replay_file: &Arc<Mutex<Option<String>>>,
    limit: usize,
) -> Vec<ReplayInfo> {
    let replays = sync_full_replay_cache_slots(replays_slot, selected_replay_file);

    let mut limited = replays.clone();
    if limit > 0 && limited.len() > limit {
        limited.truncate(limit);
    }
    limited
}

fn sync_replay_cache(state: &BackendState, limit: usize) -> Vec<ReplayInfo> {
    sync_replay_cache_slots(&state.replays, &state.selected_replay_file, limit)
}

fn replay_chat_payload_from_slots(
    replays_slot: &Arc<Mutex<Vec<ReplayInfo>>>,
    selected_replay_file: &Arc<Mutex<Option<String>>>,
    file: &str,
) -> Result<ReplayChatPayload, String> {
    let requested_file = file.trim();
    if requested_file.is_empty() {
        return Err("No replay file specified.".to_string());
    }

    let replays =
        sync_replay_cache_slots(replays_slot, selected_replay_file, UNLIMITED_REPLAY_LIMIT);
    if let Some(replay) = replays.iter().find(|replay| replay.file == requested_file) {
        return Ok(replay.chat_payload());
    }

    let replay_path = Path::new(requested_file);
    if !replay_path.exists() {
        return Err(format!("Replay file not found: {requested_file}"));
    }

    Ok(ReplayAnalysis::summarize_replay(replay_path).chat_payload())
}

fn current_replay_files_snapshot(limit: usize) -> HashSet<String> {
    let Some(root) = resolve_replay_root() else {
        return HashSet::new();
    };

    ReplayAnalysis::collect_replay_paths(&root, limit)
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

fn path_is_sc2_replay(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("SC2Replay"))
}

fn is_replay_creation_event(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Any)
        || matches!(kind, EventKind::Create(_))
        || matches!(kind, EventKind::Modify(_))
}

fn parse_new_replay_with_retries(path: &Path) -> Option<(ReplayInfo, CacheReplayEntry)> {
    const MAX_ATTEMPTS: usize = 40;
    const RETRY_DELAY: Duration = Duration::from_millis(250);
    const MIN_REPLAY_SIZE_BYTES: u64 = 8 * 1024;
    let file = path.to_string_lossy().to_string();
    crate::sco_log!(
        "[SCO/watch] parse start file='{}' max_attempts={} retry_ms={}",
        file,
        MAX_ATTEMPTS,
        RETRY_DELAY.as_millis()
    );
    let mut previous_size: Option<u64> = None;

    for attempt in 0..MAX_ATTEMPTS {
        let attempt_num = attempt + 1;
        if !path.exists() {
            crate::sco_log!(
                "[SCO/watch] parse abort file='{}' attempt={}/{} reason=file_missing",
                file,
                attempt_num,
                MAX_ATTEMPTS
            );
            return None;
        }

        let (size_bytes, modified) = path
            .metadata()
            .ok()
            .map(|meta| {
                let modified = meta
                    .modified()
                    .ok()
                    .map(format_date_from_system_time)
                    .unwrap_or(0);
                (meta.len(), modified)
            })
            .unwrap_or((0, 0));
        crate::sco_log!(
            "[SCO/watch] parse attempt file='{}' attempt={}/{} size={} modified={}",
            file,
            attempt_num,
            MAX_ATTEMPTS,
            size_bytes,
            modified
        );

        if size_bytes < MIN_REPLAY_SIZE_BYTES {
            crate::sco_log!(
                "[SCO/watch] parse wait file='{}' attempt={}/{} reason=size_below_min min={} current={}",
                file, attempt_num, MAX_ATTEMPTS, MIN_REPLAY_SIZE_BYTES, size_bytes
            );
            previous_size = Some(size_bytes);
            if attempt + 1 < MAX_ATTEMPTS {
                thread::sleep(RETRY_DELAY);
            }
            continue;
        }

        match previous_size {
            None => {
                crate::sco_log!(
                    "[SCO/watch] parse wait file='{}' attempt={}/{} reason=awaiting_size_stability size={}",
                    file, attempt_num, MAX_ATTEMPTS, size_bytes
                );
                previous_size = Some(size_bytes);
                if attempt + 1 < MAX_ATTEMPTS {
                    thread::sleep(RETRY_DELAY);
                }
                continue;
            }
            Some(previous) if previous != size_bytes => {
                crate::sco_log!(
                    "[SCO/watch] parse wait file='{}' attempt={}/{} reason=size_changed previous={} current={}",
                    file, attempt_num, MAX_ATTEMPTS, previous, size_bytes
                );
                previous_size = Some(size_bytes);
                if attempt + 1 < MAX_ATTEMPTS {
                    thread::sleep(RETRY_DELAY);
                }
                continue;
            }
            Some(_) => {}
        }

        let parsed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ReplayAnalysis::summarize_replay_with_cache_entry(path)
        })) {
            Ok(parsed) => parsed,
            Err(panic_payload) => {
                let panic_message = if let Some(message) = panic_payload.downcast_ref::<&str>() {
                    (*message).to_string()
                } else if let Some(message) = panic_payload.downcast_ref::<String>() {
                    message.clone()
                } else {
                    "non-string panic payload".to_string()
                };
                crate::sco_log!(
                    "[SCO/watch] parse panic file='{}' attempt={}/{} message='{}'",
                    file,
                    attempt_num,
                    MAX_ATTEMPTS,
                    panic_message
                );
                if attempt + 1 < MAX_ATTEMPTS {
                    crate::sco_log!(
                        "[SCO/watch] parse retry scheduled file='{}' next_attempt={} wait_ms={}",
                        file,
                        attempt_num + 1,
                        RETRY_DELAY.as_millis()
                    );
                    thread::sleep(RETRY_DELAY);
                }
                continue;
            }
        };
        let Some((replay, cache_entry)) = parsed else {
            if attempt + 1 < MAX_ATTEMPTS {
                crate::sco_log!(
                    "[SCO/watch] parse retry scheduled file='{}' next_attempt={} wait_ms={}",
                    file,
                    attempt_num + 1,
                    RETRY_DELAY.as_millis()
                );
                thread::sleep(RETRY_DELAY);
            }
            continue;
        };
        if replay.result != "Unparsed" {
            crate::sco_log!(
                "[SCO/watch] parse success file='{}' attempt={}/{} result='{}' main='{}' ally='{}' main_comm='{}' ally_comm='{}' map='{}' length={}",
                file,
                attempt_num,
                MAX_ATTEMPTS,
                replay.result,
                replay.p1,
                replay.p2,
                replay.main_commander,
                replay.ally_commander,
                replay.map,
                replay.length
            );
            return Some((replay, cache_entry));
        }
        crate::sco_log!(
            "[SCO/watch] parse pending file='{}' attempt={}/{} result='Unparsed'",
            file,
            attempt_num,
            MAX_ATTEMPTS
        );

        if attempt + 1 < MAX_ATTEMPTS {
            crate::sco_log!(
                "[SCO/watch] parse retry scheduled file='{}' next_attempt={} wait_ms={}",
                file,
                attempt_num + 1,
                RETRY_DELAY.as_millis()
            );
            thread::sleep(RETRY_DELAY);
        }
    }
    crate::sco_log!(
        "[SCO/watch] parse failed file='{}' attempts_exhausted={}",
        file,
        MAX_ATTEMPTS
    );
    None
}

fn persist_detailed_cache_entry_to_path(
    cache_path: &Path,
    entry: &CacheReplayEntry,
) -> Result<(), String> {
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create cache directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let mut entries = match std::fs::read(cache_path) {
        Ok(payload) => {
            serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).map_err(|error| {
                format!(
                    "Failed to parse detailed-analysis cache '{}': {error}",
                    cache_path.display()
                )
            })?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(format!(
                "Failed to read detailed-analysis cache '{}': {error}",
                cache_path.display()
            ))
        }
    };

    entries.retain(|existing| {
        let same_hash = !entry.hash.is_empty() && existing.hash == entry.hash;
        let same_file = existing.file == entry.file;
        !(same_hash || same_file)
    });
    entries.push(entry.clone());
    entries.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| right.file.cmp(&left.file))
    });

    let payload = serialize_cache_entries(&entries)
        .map_err(|error| format!("Failed to serialize detailed-analysis cache: {error}"))?;
    std::fs::write(cache_path, payload).map_err(|error| {
        format!(
            "Failed to write detailed-analysis cache '{}': {error}",
            cache_path.display()
        )
    })?;
    Ok(())
}

fn persist_detailed_cache_entry(entry: &CacheReplayEntry) -> Result<(), String> {
    persist_detailed_cache_entry_to_path(&get_cache_path(), entry)
}

fn collect_sc2_replay_files(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }

    let mut out = Vec::<PathBuf>::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if meta.is_file() && path_is_sc2_replay(&path) {
                out.push(path);
            }
        }
    }
    out
}

fn upsert_replay_cache_slot(cache: &mut Vec<ReplayInfo>, replay: &ReplayInfo) {
    cache.retain(|entry| entry.file != replay.file);
    cache.push(replay.clone());
    cache.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| right.file.cmp(&left.file))
    });
}

fn include_detailed_stats_for_cache(stats: &StatsState, replays: &[ReplayInfo]) -> bool {
    stats
        .analysis
        .as_ref()
        .and_then(|analysis| analysis.get("UnitData"))
        .is_some_and(|value| !value.is_null())
        || replays
            .iter()
            .any(ReplayAnalysis::replay_has_detailed_unit_stats)
}

fn sync_detailed_analysis_status_from_replays(stats: &mut StatsState, replays: &[ReplayInfo]) {
    let total_valid_files = replays
        .iter()
        .filter(|replay| {
            replay.result != "Unparsed" && canonicalize_coop_map_id(&replay.map).is_some()
        })
        .count();
    let detailed_parsed_count = replays
        .iter()
        .filter(|replay| {
            replay.result != "Unparsed"
                && canonicalize_coop_map_id(&replay.map).is_some()
                && ReplayAnalysis::replay_has_detailed_unit_stats(replay)
        })
        .count();

    stats.detailed_analysis_running = false;
    stats.detailed_analysis_status = if detailed_parsed_count == 0 {
        analysis_status_text(AnalysisMode::Detailed, "not started")
    } else {
        format!(
            "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
        )
    };
}

fn refresh_stats_snapshot_after_replay_upsert(state: &BackendState) {
    let stats_replays = match state.stats_replays.lock() {
        Ok(replays) => replays.clone(),
        Err(_) => return,
    };

    let mut stats = match state.stats.lock() {
        Ok(stats) => stats,
        Err(_) => return,
    };

    if !stats.ready || stats.simple_analysis_running || stats.detailed_analysis_running {
        return;
    }

    let include_detailed = include_detailed_stats_for_cache(&stats, &stats_replays);
    let mode = AnalysisMode::from_include_detailed(include_detailed);
    let snapshot = ReplayAnalysis::build_rebuild_snapshot(&stats_replays, include_detailed);
    apply_rebuild_snapshot(&mut stats, snapshot, mode);
    if !include_detailed {
        sync_detailed_analysis_status_from_replays(&mut stats, &stats_replays);
    }
}

fn upsert_replay_in_memory_cache(state: &BackendState, replay: &ReplayInfo) {
    if let Ok(mut replays) = state.replays.lock() {
        upsert_replay_cache_slot(&mut replays, replay);
    }

    if let Ok(mut stats_replays) = state.stats_replays.lock() {
        upsert_replay_cache_slot(&mut stats_replays, replay);
    }

    if let Ok(mut current_replay_files) = state.stats_current_replay_files.lock() {
        current_replay_files.insert(replay.file.clone());
    }

    if let Ok(mut selected) = state.selected_replay_file.lock() {
        *selected = Some(replay.file.clone());
    }

    refresh_stats_snapshot_after_replay_upsert(state);
}

fn process_new_replay_path(
    app: &tauri::AppHandle<Wry>,
    path: &Path,
    handled_files: &mut HashSet<String>,
) -> ReplayProcessOutcome {
    if !path_is_sc2_replay(path) {
        return ReplayProcessOutcome::Ignored;
    }
    if !path.exists() {
        crate::sco_log!(
            "[SCO/watch] skip path='{}' reason=missing",
            path.to_string_lossy()
        );
        return ReplayProcessOutcome::RetryLater;
    }

    let file = path.to_string_lossy().to_string();
    if file.is_empty() {
        return ReplayProcessOutcome::Ignored;
    }
    if handled_files.contains(&file) {
        crate::sco_log!("[SCO/watch] skip file='{}' reason=already_handled", file);
        return ReplayProcessOutcome::AlreadyHandled;
    }
    crate::sco_log!("[SCO/watch] processing new replay file='{}'", file);

    let Some((parsed, cache_entry)) = parse_new_replay_with_retries(path) else {
        crate::sco_log!("[SCO/watch] failed to parse new replay '{}'", file);
        return ReplayProcessOutcome::RetryLater;
    };

    let main_names = configured_main_names();
    let main_handles = configured_main_handles();
    let replay = orient_replay_for_main_names(parsed, &main_names, &main_handles);
    if replay.main_commander.trim().is_empty() && replay.ally_commander.trim().is_empty() {
        crate::sco_log!(
            "[SCO/watch] parsed replay ignored file='{}' reason=missing_commanders main='{}' ally='{}'",
            replay.file, replay.main_commander, replay.ally_commander
        );
        handled_files.insert(file);
        return ReplayProcessOutcome::Ignored;
    }

    handled_files.insert(file);
    crate::sco_log!(
        "[SCO/watch] replay accepted file='{}' date={} result='{}' main='{}' ally='{}' main_comm='{}' ally_comm='{}'",
        replay.file,
        replay.date,
        replay.result,
        replay.p1,
        replay.p2,
        replay.main_commander,
        replay.ally_commander
    );
    let state = app.state::<BackendState>();
    upsert_replay_in_memory_cache(&state, &replay);
    if let Err(error) = persist_detailed_cache_entry(&cache_entry) {
        crate::sco_log!(
            "[SCO/watch] failed to persist detailed cache entry for '{}': {error}",
            replay.file
        );
    }
    record_session_result(&state, &replay.result);
    let settings = read_settings_file();
    let show_replay_info_after_game = show_replay_info_after_game_from_settings(&settings);

    if show_replay_info_after_game {
        crate::sco_log!(
            "[SCO/watch] emitting replay to overlay file='{}'",
            replay.file
        );
        overlay_info::emit_replay_to_overlay_from_replay(app, &replay, true);
        state
            .overlay_replay_data_active
            .store(true, Ordering::Release);
    } else {
        crate::sco_log!(
            "[SCO/watch] replay overlay suppressed by settings file='{}'",
            replay.file
        );
        state
            .overlay_replay_data_active
            .store(false, Ordering::Release);
    }

    let invalidation_generation = DELAYED_REPLAY_WINRATE_GENERATION
        .fetch_add(1, Ordering::AcqRel)
        .saturating_add(1);
    crate::sco_log!(
        "[SCO/watch] invalidated delayed replay winrate popups generation={} replay='{}'",
        invalidation_generation,
        replay.file
    );

    ReplayProcessOutcome::Processed
}

fn process_replay_detailed(
    state: &BackendState,
    path: &Path,
) -> (ReplayProcessOutcome, Option<ReplayInfo>) {
    if !path_is_sc2_replay(path) {
        return (ReplayProcessOutcome::Ignored, None);
    }

    if !path.exists() {
        crate::sco_log!(
            "[SCO/show] skip path='{}' reason=missing",
            path.to_string_lossy()
        );
        return (ReplayProcessOutcome::RetryLater, None);
    }

    let file = path.to_string_lossy().to_string();

    if file.is_empty() {
        return (ReplayProcessOutcome::Ignored, None);
    }

    crate::sco_log!("[SCO/show] processing existing replay file='{}'", file);

    let Some((parsed, cache_entry)) = parse_new_replay_with_retries(path) else {
        crate::sco_log!("[SCO/show] failed to parse existing replay '{}'", file);
        return (ReplayProcessOutcome::RetryLater, None);
    };

    let main_names = configured_main_names();
    let main_handles = configured_main_handles();
    let replay = orient_replay_for_main_names(parsed, &main_names, &main_handles);

    crate::sco_log!(
        "[SCO/show] replay accepted file='{}' date={} result='{}' main='{}' ally='{}' main_comm='{}' ally_comm='{}'",
        replay.file,
        replay.date,
        replay.result,
        replay.p1,
        replay.p2,
        replay.main_commander,
        replay.ally_commander
    );

    upsert_replay_in_memory_cache(&state, &replay);
    if let Err(error) = persist_detailed_cache_entry(&cache_entry) {
        crate::sco_log!(
            "[SCO/show] failed to persist detailed cache entry for '{}': {error}",
            replay.file
        );
    }

    (ReplayProcessOutcome::Processed, Some(replay))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayProcessOutcome {
    Processed,
    RetryLater,
    AlreadyHandled,
    Ignored,
}

fn update_pending_fallback_file(
    pending_fallback_files: &mut HashSet<String>,
    file: &str,
    outcome: ReplayProcessOutcome,
) {
    match outcome {
        ReplayProcessOutcome::RetryLater => {
            let should_log_start = pending_fallback_files.is_empty();
            if pending_fallback_files.insert(file.to_string()) {
                crate::sco_log!("[SCO/watch] fallback queued file='{}'", file);
            }
            if should_log_start && !pending_fallback_files.is_empty() {
                crate::sco_log!(
                    "[SCO/watch] fallback polling started pending={}",
                    pending_fallback_files.len()
                );
            }
        }
        ReplayProcessOutcome::Processed
        | ReplayProcessOutcome::AlreadyHandled
        | ReplayProcessOutcome::Ignored => {
            if pending_fallback_files.remove(file) {
                crate::sco_log!("[SCO/watch] fallback cleared file='{}'", file);
                if pending_fallback_files.is_empty() {
                    crate::sco_log!("[SCO/watch] fallback polling stopped");
                }
            }
        }
    }
}

fn spawn_replay_creation_watcher(app: tauri::AppHandle<Wry>) {
    thread::spawn(move || {
        let replay_root = loop {
            if let Some(root) = replay_watch_root_from_settings() {
                break root;
            }
            crate::sco_log!("[SCO/watch] account_folder replay root unavailable, retrying in 5s");
            thread::sleep(Duration::from_secs(5));
        };

        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = match RecommendedWatcher::new(
            move |event_result| {
                let _ = tx.send(event_result);
            },
            NotifyConfig::default(),
        ) {
            Ok(watcher) => watcher,
            Err(error) => {
                crate::sco_log!("[SCO/watch] failed to initialize replay watcher: {error}");
                return;
            }
        };

        if let Err(error) = watcher.watch(&replay_root, RecursiveMode::Recursive) {
            crate::sco_log!(
                "[SCO/watch] failed to watch replay root '{}': {error}",
                replay_root.display()
            );
            return;
        }
        crate::sco_log!(
            "[SCO/watch] replay watcher active on {}",
            replay_root.display()
        );

        let mut handled_files = HashSet::<String>::new();
        for path in collect_sc2_replay_files(&replay_root) {
            let key = path.to_string_lossy().to_string();
            if !key.is_empty() {
                handled_files.insert(key);
            }
        }
        let mut pending_fallback_files = HashSet::<String>::new();

        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(event)) => {
                    if !is_replay_creation_event(&event.kind) {
                        continue;
                    }
                    crate::sco_log!(
                        "[SCO/watch] notify event kind={:?} paths={}",
                        event.kind,
                        event.paths.len()
                    );

                    for path in event.paths {
                        if !path_is_sc2_replay(&path) {
                            continue;
                        }
                        let key = path.to_string_lossy().to_string();
                        if key.is_empty() {
                            continue;
                        }
                        let outcome = process_new_replay_path(&app, &path, &mut handled_files);
                        update_pending_fallback_file(&mut pending_fallback_files, &key, outcome);
                    }
                }
                Ok(Err(error)) => {
                    crate::sco_log!("[SCO/watch] watcher event error: {error}");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if pending_fallback_files.is_empty() {
                        continue;
                    }

                    let pending_snapshot =
                        pending_fallback_files.iter().cloned().collect::<Vec<_>>();
                    for file in pending_snapshot {
                        let path = PathBuf::from(&file);
                        let outcome = process_new_replay_path(&app, &path, &mut handled_files);
                        update_pending_fallback_file(&mut pending_fallback_files, &file, outcome);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    crate::sco_log!("[SCO/watch] replay watcher channel disconnected; stopping");
                    break;
                }
            }
        }
    });
}

#[derive(Debug, Clone)]
struct LiveGamePlayer {
    id: u64,
    name: String,
    kind: String,
    handle: String,
}

fn value_as_u64_lossy(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .and_then(Value::as_i64)
                .and_then(|entry| u64::try_from(entry).ok())
        })
        .or_else(|| {
            value
                .and_then(Value::as_f64)
                .filter(|entry| entry.is_finite() && *entry >= 0.0)
                .map(|entry| entry.floor() as u64)
        })
}

fn fetch_sc2_live_game_payload() -> Option<Value> {
    let mut stream = TcpStream::connect("127.0.0.1:6119").ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
    let request = b"GET /game HTTP/1.1\r\nHost: localhost:6119\r\nConnection: close\r\n\r\n";
    stream.write_all(request).ok()?;

    let mut response = Vec::<u8>::new();
    stream.read_to_end(&mut response).ok()?;
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let body = response.get((header_end + 4)..)?;
    serde_json::from_slice::<Value>(body).ok()
}

fn extract_live_game_players(payload: &Value) -> Vec<LiveGamePlayer> {
    let Some(players) = payload.get("players").and_then(Value::as_array) else {
        return Vec::new();
    };

    players
        .iter()
        .filter_map(|player| {
            let as_object = player.as_object()?;
            let id = value_as_u64_lossy(as_object.get("id"))
                .or_else(|| value_as_u64_lossy(as_object.get("playerId")))
                .or_else(|| value_as_u64_lossy(as_object.get("m_playerId")))
                .unwrap_or(0);

            let name = as_object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let kind = as_object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            let handle = as_object
                .get("handle")
                .or_else(|| as_object.get("toonHandle"))
                .or_else(|| as_object.get("toon_handle"))
                .or_else(|| as_object.get("battleTag"))
                .or_else(|| as_object.get("battletag"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();

            Some(LiveGamePlayer {
                id,
                name,
                kind,
                handle,
            })
        })
        .collect()
}

fn choose_other_coop_player_info(
    players: &[LiveGamePlayer],
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> Option<(String, String)> {
    let coop_players: Vec<&LiveGamePlayer> = players
        .iter()
        .filter(|player| player.id == 1 || player.id == 2)
        .filter(|player| !player.kind.eq_ignore_ascii_case("computer"))
        .collect();
    if coop_players.is_empty() {
        return None;
    }

    let mut main_marked_count = 0usize;
    let mut other_candidates = Vec::<&LiveGamePlayer>::new();
    for player in coop_players.iter() {
        let is_main = ReplayAnalysis::is_main_player_identity(
            &player.name,
            &player.handle,
            main_names,
            main_handles,
        );
        if is_main {
            main_marked_count += 1;
        } else {
            other_candidates.push(*player);
        }
    }

    if main_marked_count > 0 && !other_candidates.is_empty() {
        other_candidates.sort_by_key(|player| player.id);

        return other_candidates.into_iter().find_map(|player| {
            let name = player.name.trim();
            let handle = player.handle.to_string();

            Some((handle, name.to_string()))
        });
    }

    None
}

fn build_launch_main_identity(state: &BackendState) -> (HashSet<String>, HashSet<String>) {
    let mut main_names = configured_main_names();
    let mut main_handles = configured_main_handles();

    if let Ok(stats) = state.stats.lock() {
        for name in &stats.main_players {
            let normalized = ReplayAnalysis::normalized_player_key(name);
            if !normalized.is_empty() {
                main_names.insert(normalized);
            }
        }
    }

    let selected = state
        .selected_replay_file
        .lock()
        .ok()
        .and_then(|current| current.clone());
    if let Ok(replays) = state.replays.lock() {
        let seed = selected
            .as_ref()
            .and_then(|file| replays.iter().find(|replay| &replay.file == file))
            .or_else(|| replays.first());
        if let Some(seed) = seed {
            let normalized_name = ReplayAnalysis::normalized_player_key(&seed.p1);
            if !normalized_name.is_empty() {
                main_names.insert(normalized_name);
            }
            let normalized_handle = ReplayAnalysis::normalized_handle_key(&seed.p1_handle);
            if !normalized_handle.is_empty() {
                main_handles.insert(normalized_handle);
            }
        }
    }

    (main_names, main_handles)
}

fn stats_have_player_rows(state: &BackendState) -> bool {
    state
        .stats
        .lock()
        .ok()
        .and_then(|stats| stats.analysis.clone())
        .and_then(|analysis| {
            analysis
                .get("PlayerData")
                .and_then(Value::as_object)
                .cloned()
        })
        .is_some_and(|rows| !rows.is_empty())
}

fn replay_count_for_launch_detector(state: &BackendState) -> usize {
    state
        .replays
        .lock()
        .ok()
        .map(|replays| replays.len())
        .unwrap_or(0)
}

fn spawn_game_launch_winrate_task(app: tauri::AppHandle<Wry>) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(4));

        let mut last_game_time: Option<u64> = None;
        let mut last_replay_amount = 0usize;
        let mut last_replay_amount_flowing = 0usize;
        let mut last_replay_time = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);

        loop {
            thread::sleep(Duration::from_millis(500));

            let settings = read_settings_file();
            let show_player_winrates = settings
                .get("show_player_winrates")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if !show_player_winrates {
                continue;
            }

            let state = app.state::<BackendState>();
            let replay_count = replay_count_for_launch_detector(&state);
            if replay_count > last_replay_amount_flowing {
                last_replay_amount_flowing = replay_count;
                last_replay_time = Instant::now();
            }

            if !stats_have_player_rows(&state) || replay_count == last_replay_amount {
                continue;
            }

            let Some(payload) = fetch_sc2_live_game_payload() else {
                continue;
            };
            if payload
                .get("isReplay")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                continue;
            }

            let players = extract_live_game_players(&payload);
            if players.len() <= 2 {
                continue;
            }
            let all_users = players
                .iter()
                .all(|player| player.kind.eq_ignore_ascii_case("user"));
            if all_users {
                continue;
            }

            let display_time = value_as_u64_lossy(payload.get("displayTime")).unwrap_or(0);
            if last_game_time.is_none() || display_time == 0 {
                last_game_time = Some(display_time);
                continue;
            }
            if last_game_time == Some(display_time) {
                continue;
            }
            last_game_time = Some(display_time);

            if last_replay_time.elapsed() < Duration::from_secs(15) {
                continue;
            }

            let (main_names, main_handles) = build_launch_main_identity(&state);
            let Some((other_player_handle, other_player_name)) =
                choose_other_coop_player_info(&players, &main_names, &main_handles)
            else {
                continue;
            };

            let invalidation_generation = DELAYED_REPLAY_WINRATE_GENERATION
                .fetch_add(1, Ordering::AcqRel)
                .saturating_add(1);
            crate::sco_log!(
                "[SCO/launch] invalidated delayed replay winrate popups generation={}",
                invalidation_generation
            );

            if overlay_info::show_player_winrate_for_name(
                &app,
                &state,
                &other_player_handle,
                &other_player_name,
            ) {
                last_replay_amount = replay_count;
            }
        }
    });
}

fn spawn_protocol_store_warmup() {
    thread::spawn(|| {
        let started_at = Instant::now();
        match s2protocol_port::build_protocol_store() {
            Ok(_) => {
                crate::sco_log!(
                    "[SCO/protocol] warmup completed in {}ms",
                    started_at.elapsed().as_millis()
                );
            }
            Err(error) => {
                crate::sco_log!("[SCO/protocol] warmup failed: {error}");
            }
        }
    });
}

fn persist_setting_bool(key: &str, value: bool) {
    match persist_single_setting_value(key, Value::Bool(value)) {
        Ok(()) => logging::refresh_from_settings(&read_settings_file()),
        Err(error) => {
            crate::sco_log!("[SCO/settings] Failed to save {key}: {error}");
        }
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn median_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));

    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

fn median_u64(values: &[u64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();

    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid] as f64
    } else {
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    }
}

fn kill_fraction(main_kills: u64, ally_kills: u64) -> f64 {
    let total = main_kills + ally_kills;
    if total == 0 {
        0.0
    } else {
        main_kills as f64 / total as f64
    }
}

fn result_is_victory(result: &str) -> Option<bool> {
    let normalized = result.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "victory" | "win" | "1" | "true") {
        Some(true)
    } else if matches!(
        normalized.as_str(),
        "defeat" | "loss" | "lose" | "0" | "false"
    ) {
        Some(false)
    } else {
        None
    }
}

fn normalized_commander_name(commander: &str, _fallback: &str) -> String {
    let trimmed = commander.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        normalize_known_commander_name(trimmed)
            .unwrap_or(trimmed)
            .to_string()
    }
}

fn empty_stats_payload() -> Value {
    json!({
        "MapData": {},
        "CommanderData": {},
        "AllyCommanderData": {},
        "DifficultyData": {},
        "RegionData": {},
        "UnitData": Value::Null,
        "AmonData": {},
        "PlayerData": {},
    })
}

fn apply_rebuild_snapshot(stats: &mut StatsState, snapshot: StatsSnapshot, mode: AnalysisMode) {
    stats.ready = snapshot.ready;
    stats.games = snapshot.games;
    stats.main_players = snapshot.main_players;
    stats.main_handles = snapshot.main_handles;
    stats.analysis = Some(snapshot.analysis);
    stats.commander_mastery = snapshot.commander_mastery;
    stats.prestige_names = snapshot.prestige_names;
    stats.message = snapshot.message;

    set_analysis_terminal_status(stats, mode, "completed");
}

#[allow(dead_code)]
struct BackendState {
    tray_icon: Arc<Mutex<Option<TrayIcon<Wry>>>>,
    stats: Arc<Mutex<StatsState>>,
    replays: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_replays: Arc<Mutex<Vec<ReplayInfo>>>,
    stats_current_replay_files: Arc<Mutex<HashSet<String>>>,
    selected_replay_file: Arc<Mutex<Option<String>>>,
    overlay_replay_data_active: AtomicBool,
    session_victories: AtomicU64,
    session_defeats: AtomicU64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowCloseAction {
    AllowClose,
    HidePerformance,
    HideWindow,
    ExitApp,
}

fn window_close_action(
    label: &str,
    minimize_to_tray: bool,
    exit_in_progress: bool,
) -> WindowCloseAction {
    if exit_in_progress {
        return WindowCloseAction::AllowClose;
    }

    if label == "performance" {
        WindowCloseAction::HidePerformance
    } else if label == "overlay" || minimize_to_tray {
        WindowCloseAction::HideWindow
    } else {
        WindowCloseAction::ExitApp
    }
}

fn request_clean_exit(app: &AppHandle<Wry>, exit_code: i32) {
    if APP_EXIT_IN_PROGRESS.swap(true, Ordering::AcqRel) {
        return;
    }

    if let Ok(mut tray_icon) = app.state::<BackendState>().tray_icon.lock() {
        tray_icon.take();
    }

    for label in ["performance", "overlay", "config"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.hide();
            let _ = window.destroy();
        }
    }

    app.exit(exit_code);
}

#[derive(Debug)]
struct StatsState {
    ready: bool,
    analysis: Option<Value>,
    games: u64,
    main_players: Vec<String>,
    main_handles: Vec<String>,
    startup_analysis_requested: bool,
    simple_analysis_running: bool,
    simple_analysis_status: String,
    detailed_analysis_running: bool,
    detailed_analysis_status: String,
    detailed_analysis_atstart: bool,
    commander_mastery: Value,
    prestige_names: Value,
    message: String,
}

#[derive(Debug, Default)]
struct StatsSnapshot {
    ready: bool,
    games: u64,
    main_players: Vec<String>,
    main_handles: Vec<String>,
    analysis: Value,
    commander_mastery: Value,
    prestige_names: Value,
    message: String,
}

impl Default for StatsState {
    fn default() -> Self {
        Self {
            ready: false,
            analysis: Some(empty_stats_payload()),
            games: 0,
            main_players: vec![],
            main_handles: vec![],
            startup_analysis_requested: false,
            simple_analysis_running: false,
            simple_analysis_status: analysis_status_text(
                AnalysisMode::Simple,
                "waiting for startup",
            ),
            detailed_analysis_running: false,
            detailed_analysis_status: analysis_status_text(AnalysisMode::Detailed, "not started"),
            detailed_analysis_atstart: false,
            commander_mastery: Value::Object(Default::default()),
            prestige_names: Value::Object(Default::default()),
            message: "No parsed statistics available yet.".to_string(),
        }
    }
}

impl StatsState {
    fn from_settings() -> Self {
        let mut state = Self::default();
        if let Value::Object(settings) = read_settings_file() {
            state.detailed_analysis_atstart = settings
                .get("detailed_analysis_atstart")
                .and_then(Value::as_bool)
                .unwrap_or(state.detailed_analysis_atstart);
        }
        state
    }

    fn as_payload(&self) -> Value {
        let scan_progress = replay_scan_progress().as_json();
        let (
            analysis,
            main_players,
            main_handles,
            commander_mastery,
            prestige_names,
            games,
            message,
        ) = if self.ready {
            (
                self.analysis.clone(),
                self.main_players.clone(),
                self.main_handles.clone(),
                self.commander_mastery.clone(),
                self.prestige_names.clone(),
                self.games,
                self.message.clone(),
            )
        } else {
            (
                Some(empty_stats_payload()),
                Vec::new(),
                Vec::new(),
                Value::Object(Default::default()),
                Value::Object(Default::default()),
                0,
                if self.message.is_empty() {
                    "Statistics are updating. This may take a while.".to_string()
                } else {
                    self.message.clone()
                },
            )
        };

        json!({
            "ready": self.ready,
            "games": games,
            "detailed_parsed_count": 0,
            "total_valid_files": 0,
            "analysis": analysis,
            "main_players": main_players,
            "main_handles": main_handles,
            "simple_analysis_running": self.simple_analysis_running,
            "simple_analysis_status": self.simple_analysis_status,
            "detailed_analysis_running": self.detailed_analysis_running,
            "detailed_analysis_status": self.detailed_analysis_status,
            "detailed_analysis_atstart": self.detailed_analysis_atstart,
            "commander_mastery": commander_mastery,
            "prestige_names": prestige_names,
            "message": message,
            "scan_progress": scan_progress,
        })
    }
}

fn log_request(method: &str, path: &str, body: &Option<Value>) {
    let serialized_body = body
        .as_ref()
        .map(|payload| serde_json::to_string(payload).unwrap_or_else(|_| "<invalid-json>".into()));
    if let Some(serialized_body) = serialized_body {
        logging::append_line_if_enabled(&format!(
            "[SCO/request] method={} path={} body={}",
            method, path, serialized_body
        ));
    } else {
        logging::append_line_if_enabled(&format!("[SCO/request] method={} path={}", method, path));
    }
}

#[tauri::command]
fn performance_start_drag(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    performance_overlay::start_drag(&app)
}

#[tauri::command]
async fn pick_folder(title: String, directory: Option<String>) -> Result<Option<String>, String> {
    let start_directory = folder_dialog_start_directory(directory);
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = FileDialog::new().set_title(&title);
        if let Some(start_directory) = start_directory.as_ref() {
            dialog = dialog.set_directory(start_directory);
        }

        Ok(dialog
            .pick_folder()
            .map(|selected| selected.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| format!("Failed to open folder picker: {error}"))?
}

#[tauri::command]
fn is_dev() -> bool {
    is_dev_env()
}

#[tauri::command]
fn save_overlay_screenshot(path: String, png_bytes: Vec<u8>) -> Result<(), String> {
    overlay_info::save_overlay_screenshot(Path::new(&path), &png_bytes)
}

#[tauri::command]
fn open_folder_path(path: String) -> Result<(), String> {
    overlay_info::open_folder_in_explorer(&path)
}

#[tauri::command]
async fn config_request(
    app: tauri::AppHandle<Wry>,
    path: String,
    method: String,
    body: Option<Value>,
    state: State<'_, BackendState>,
) -> Result<Value, String> {
    let route = path.split('?').next().unwrap_or("");
    let method = method.to_ascii_lowercase();

    log_request(&method, &path, &body);

    match (method.as_str(), route) {
        ("get", "/config") => Ok(json!({
            "status": "ok",
            "settings": read_saved_settings_file(),
            "active_settings": read_settings_file(),
            "randomizer_catalog": randomizer::catalog_payload(),
            "monitor_catalog": overlay_info::available_monitor_catalog(&app),
        })),
        ("post", "/config") => {
            if let Some(payload) = body {
                if let Some(settings) = payload.get("settings") {
                    let mut next_settings = sanitize_settings_value(settings.clone());
                    let previous_settings = read_settings_file();
                    let persist = payload
                        .get("persist")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);

                    next_settings["performance_geometry"] =
                        previous_settings["performance_geometry"].clone();

                    if persist {
                        write_settings_file(&next_settings)?;
                    }
                    apply_runtime_settings(&app, &previous_settings, &next_settings);
                }
                Ok(json!({
                    "status": "ok",
                    "settings": read_saved_settings_file(),
                    "active_settings": read_settings_file(),
                    "randomizer_catalog": randomizer::catalog_payload(),
                    "monitor_catalog": overlay_info::available_monitor_catalog(&app),
                }))
            } else {
                Err("Missing payload".to_string())
            }
        }
        ("get", "/config/replays") => {
            let limit = parse_query_usize(&path, "limit", 300);
            let replays_slot = state.replays.clone();
            let selected_slot = state.selected_replay_file.clone();
            let (replays, total_replays, selected_replay_file) =
                tauri::async_runtime::spawn_blocking(move || {
                    let all_replays = sync_full_replay_cache_slots(&replays_slot, &selected_slot);
                    let total_replays = all_replays.len();
                    let mut replays = all_replays;
                    if limit > 0 && replays.len() > limit {
                        replays.truncate(limit);
                    }
                    let selected_replay_file = selected_slot
                        .lock()
                        .ok()
                        .and_then(|current| current.clone());
                    (replays, total_replays, selected_replay_file)
                })
                .await
                .map_err(|error| format!("Failed to load /config/replays: {error}"))?;
            Ok(json!({
                "status": "ok",
                "replays": replays.into_iter().map(|replay| replay.as_games_row()).collect::<Vec<_>>(),
                "total_replays": total_replays,
                "selected_replay_file": selected_replay_file,
            }))
        }
        ("get", "/config/players") => {
            let limit = parse_query_usize(&path, "limit", 500);
            let replays = match state.replays.try_lock() {
                Ok(replays) if !replays.is_empty() => replays.clone(),
                Ok(_) => {
                    crate::sco_log!(
                        "[SCO/players] replay cache empty, starting background scan for players"
                    );
                    spawn_players_scan_task(
                        state.replays.clone(),
                        state.selected_replay_file.clone(),
                        limit,
                    );
                    Vec::new()
                }
                Err(error) => match error {
                    TryLockError::WouldBlock => {
                        crate::sco_log!(
                            "[SCO/players] replay cache busy, starting background scan for players"
                        );
                        spawn_players_scan_task(
                            state.replays.clone(),
                            state.selected_replay_file.clone(),
                            limit,
                        );
                        Vec::new()
                    }
                    TryLockError::Poisoned(_) => {
                        return Err("Failed to access replay cache: mutex is poisoned".to_string());
                    }
                },
            };
            Ok(json!({
                "status": "ok",
                "players": ReplayAnalysis::rebuild_player_rows_fast(&replays),
                "loading": replays.is_empty(),
            }))
        }
        ("get", "/config/weeklies") => {
            let replays_slot = state.replays.clone();
            let selected_slot = state.selected_replay_file.clone();
            let replays = tauri::async_runtime::spawn_blocking(move || {
                sync_replay_cache_slots(&replays_slot, &selected_slot, UNLIMITED_REPLAY_LIMIT)
            })
            .await
            .map_err(|error| format!("Failed to load /config/weeklies: {error}"))?;
            Ok(json!({
                "status": "ok",
                "weeklies": ReplayAnalysis::rebuild_weeklies_rows(&replays),
            }))
        }
        ("get", "/config/stats") => {
            let stats = state.stats.clone();
            let stats_replays = state.stats_replays.clone();
            let stats_current_replay_files = state.stats_current_replay_files.clone();
            let path_for_worker = path.clone();
            let payload = tauri::async_runtime::spawn_blocking(move || {
                build_stats_response(
                    &path_for_worker,
                    &stats,
                    &stats_replays,
                    &stats_current_replay_files,
                )
            })
            .await
            .map_err(|error| format!("Failed to read /config/stats: {error}"))?
            .map_err(|error| error)?;
            Ok(payload)
        }
        ("post", "/config/replays/show") => {
            let requested = body
                .as_ref()
                .and_then(|payload| payload.get("file"))
                .and_then(Value::as_str);
            Ok(overlay_info::replay_show_for_window(
                &app, &state, requested,
            ))
        }
        ("post", "/config/replays/chat") => {
            let requested_file = body
                .as_ref()
                .and_then(|payload| payload.get("file"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let replays_slot = state.replays.clone();
            let selected_slot = state.selected_replay_file.clone();
            let chat = tauri::async_runtime::spawn_blocking(move || {
                replay_chat_payload_from_slots(&replays_slot, &selected_slot, &requested_file)
            })
            .await
            .map_err(|error| format!("Failed to load /config/replays/chat: {error}"))?
            .map_err(|error| error)?;
            Ok(json!({
                "status": "ok",
                "chat": chat,
            }))
        }
        ("post", "/config/replays/move") => {
            let delta = body
                .as_ref()
                .and_then(|payload| payload.get("delta"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            Ok(overlay_info::replay_move_window(&app, &state, delta))
        }
        ("post", "/config/action") => {
            let action = body
                .as_ref()
                .and_then(|payload| payload.get("action"))
                .and_then(Value::as_str)
                .unwrap_or("");

            match action {
                "set_player_note" => {
                    let player_name = body
                        .as_ref()
                        .and_then(|payload| payload.get("player"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let note_value = body
                        .as_ref()
                        .and_then(|payload| payload.get("note"))
                        .and_then(Value::as_str)
                        .unwrap_or("");

                    let mut saved_settings = read_saved_settings_file();
                    update_settings_player_note(&mut saved_settings, player_name, note_value)?;
                    write_saved_settings_file(&saved_settings)?;

                    let mut active_settings = read_settings_file();
                    update_settings_player_note(&mut active_settings, player_name, note_value)?;
                    replace_active_settings(&active_settings);

                    Ok(json!({
                        "status": "ok",
                        "result": { "ok": true },
                        "message": if note_value.trim().is_empty() {
                            "Player note cleared."
                        } else {
                            "Player note saved."
                        },
                    }))
                }
                _ => {
                    if let Some(response) =
                        overlay_info::perform_overlay_action(&app, &state, action, body.as_ref())
                    {
                        Ok(response)
                    } else {
                        Ok(json!({
                            "status": "ok",
                            "result": { "ok": false },
                            "message": format!("Unsupported action: {action}"),
                        }))
                    }
                }
            }
        }
        ("post", "/config/stats/action") => {
            let action = body
                .as_ref()
                .and_then(|payload| payload.get("action"))
                .and_then(Value::as_str)
                .unwrap_or("");

            if let Some(response) =
                overlay_info::perform_overlay_action(&app, &state, action, body.as_ref())
            {
                return Ok(response);
            }

            match action {
                "frontend_ready" => {
                    let request_started_at = Instant::now();
                    request_startup_analysis(
                        app.clone(),
                        state.stats.clone(),
                        state.replays.clone(),
                        state.stats_replays.clone(),
                        state.stats_current_replay_files.clone(),
                        StartupAnalysisTrigger::FrontendReady,
                    )?;
                    let stats = state
                        .stats
                        .lock()
                        .map_err(|error| format!("Failed to access stats state: {error}"))?;
                    crate::sco_log!(
                        "[SCO/stats] frontend_ready completed in {}ms",
                        request_started_at.elapsed().as_millis()
                    );
                    return Ok(json!({
                        "status": "ok",
                        "result": { "ok": true },
                        "message": stats.message.clone(),
                        "stats": stats.as_payload(),
                    }));
                }
                "start_simple_analysis" | "run_detailed_analysis" => {
                    let include_detailed = action == "run_detailed_analysis";
                    let mode = analysis_mode(include_detailed);

                    let limit = UNLIMITED_REPLAY_LIMIT;
                    crate::sco_log!(
                        "[SCO/stats] {action} requested replay_limit={limit} on thread"
                    );
                    spawn_analysis_task(
                        app.clone(),
                        state.stats.clone(),
                        state.replays.clone(),
                        state.stats_replays.clone(),
                        state.stats_current_replay_files.clone(),
                        include_detailed,
                        limit,
                    );
                    let status = state
                        .stats
                        .lock()
                        .ok()
                        .and_then(|stats| {
                            if stats.message.is_empty() {
                                None
                            } else {
                                Some(stats.message.clone())
                            }
                        })
                        .unwrap_or_else(|| analysis_started_message(mode));
                    return Ok(json!({
                        "status": "ok",
                        "result": { "ok": true },
                        "message": status,
                    }));
                }
                _ => {}
            }

            let mut stats = state
                .stats
                .lock()
                .map_err(|error| format!("Failed to access stats state: {error}"))?;
            let request_started_at = Instant::now();
            crate::sco_log!("[SCO/stats/action] action={action}");

            match action {
                "pause_detailed_analysis" => {
                    set_analysis_terminal_status(&mut stats, AnalysisMode::Detailed, "paused");
                    crate::sco_log!(
                        "[SCO/stats] pause_detailed_analysis requested elapsed={}ms",
                        request_started_at.elapsed().as_millis()
                    );
                }
                "dump_data" => {
                    let dump_path = PathBuf::from("SCO_analysis_dump.json");
                    let payload = json!({
                        "timestamp": format_date_from_system_time(SystemTime::now()),
                        "stats": stats.as_payload(),
                    });
                    match serde_json::to_string_pretty(&payload) {
                        Ok(contents) => match std::fs::write(&dump_path, contents) {
                            Ok(_) => {
                                let path = dump_path.display();
                                stats.message = format!("Data dumped to {path}");
                                crate::sco_log!("[SCO/stats] dump_data written to {path}");
                            }
                            Err(error) => {
                                let message = format!("Failed to write dump: {error}");
                                crate::sco_log!("[SCO/stats] {message}");
                                stats.message = message;
                            }
                        },
                        Err(error) => {
                            let message = format!("Failed to serialize dump: {error}");
                            crate::sco_log!("[SCO/stats] {message}");
                            stats.message = message;
                        }
                    }
                    crate::sco_log!(
                        "[SCO/stats] dump_data completed in {}ms",
                        request_started_at.elapsed().as_millis()
                    );
                }
                "delete_parsed_data" => {
                    stats.ready = false;
                    stats.analysis = Some(empty_stats_payload());
                    stats.commander_mastery = Value::Object(Default::default());
                    stats.prestige_names = Value::Object(Default::default());
                    set_analysis_terminal_status(&mut stats, AnalysisMode::Simple, "not started");
                    set_analysis_terminal_status(&mut stats, AnalysisMode::Detailed, "not started");
                    stats.message = "No parsed statistics available yet.".to_string();
                    if let Ok(mut replays) = state.replays.lock() {
                        replays.clear();
                    }
                    if let Ok(mut stats_replays) = state.stats_replays.lock() {
                        stats_replays.clear();
                    }
                    if let Ok(mut stats_current_replay_files) =
                        state.stats_current_replay_files.lock()
                    {
                        stats_current_replay_files.clear();
                    }
                    if let Ok(mut selected_replay_file) = state.selected_replay_file.lock() {
                        *selected_replay_file = None;
                    }
                    state
                        .overlay_replay_data_active
                        .store(false, Ordering::Release);
                    clear_analysis_cache_files();
                    crate::sco_log!(
                        "[SCO/stats] delete_parsed_data completed in {}ms",
                        request_started_at.elapsed().as_millis()
                    );
                }
                "set_detailed_analysis_atstart" => {
                    if let Some(payload) = body.as_ref() {
                        if let Some(enabled) = payload.get("enabled").and_then(Value::as_bool) {
                            stats.detailed_analysis_atstart = enabled;
                            persist_setting_bool("detailed_analysis_atstart", enabled);
                            stats.message = analysis_at_start_message(enabled);
                            crate::sco_log!(
                                "[SCO/stats] set_detailed_analysis_atstart requested: {enabled}"
                            );
                        }
                    }
                    crate::sco_log!(
                        "[SCO/stats] set_detailed_analysis_atstart completed in {}ms",
                        request_started_at.elapsed().as_millis()
                    );
                }
                "reveal_file" => {
                    let requested_file = body
                        .as_ref()
                        .and_then(|payload| payload.get("file"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let file = requested_file;
                    if file.is_empty() {
                        stats.message = "No replay file specified to reveal.".to_string();
                    } else {
                        match overlay_info::reveal_file_in_explorer(file) {
                            Ok(()) => stats.message = format!("Revealing file: {file}"),
                            Err(error) => {
                                let message = format!("Unable to reveal file: {error}");
                                crate::sco_log!("[SCO/stats] reveal_file failed: {error}");
                                stats.message = message;
                            }
                        }
                    }

                    crate::sco_log!(
                        "[SCO/stats] reveal_file requested: {} elapsed={}ms",
                        if !file.is_empty() { file } else { "<empty>" },
                        request_started_at.elapsed().as_millis()
                    );
                }
                _ => {
                    crate::sco_log!("[SCO/stats] unsupported action: {action}");
                    return Ok(json!({
                        "status": "ok",
                        "result": { "ok": false },
                        "message": format!("Unsupported action: {action}"),
                        "stats": stats.as_payload(),
                    }));
                }
            }

            crate::sco_log!(
                "[SCO/stats/action] done action={} elapsed={}ms",
                action,
                request_started_at.elapsed().as_millis()
            );
            Ok(json!({
                "status": "ok",
                "result": { "ok": true },
                "message": "Action processed",
                "stats": stats.as_payload(),
            }))
        }
        _ => Ok(json!({
            "status": "ok",
            "message": "unsupported endpoint"
        })),
    }
}

async fn auto_update(handle: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = handle.updater()?.check().await? {
        crate::sco_log!("Auto update begin");

        let mut downloaded = 0;

        // alternatively we could also call update.download() and update.install() separately
        update
            .download_and_install(
                |chunk_length, content_length| {
                    downloaded += chunk_length;
                    crate::sco_log!("downloaded {downloaded} from {content_length:?}");
                },
                || {
                    crate::sco_log!("download finished");
                },
            )
            .await?;

        crate::sco_log!("update installed");
        handle.restart();
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = read_settings_file();
    logging::refresh_from_settings(&settings);

    let data_dir = get_json_data_dir();
    let _ = s2coop_analyzer::dictionary_data::shared_dictionary_data(Some(data_dir));

    let state = BackendState {
        tray_icon: Arc::new(Mutex::new(None)),
        stats: Arc::new(Mutex::new(StatsState::from_settings())),
        replays: Arc::new(Mutex::new(Vec::new())),
        stats_replays: Arc::new(Mutex::new(Vec::new())),
        stats_current_replay_files: Arc::new(Mutex::new(HashSet::new())),
        selected_replay_file: Arc::new(Mutex::new(None)),
        overlay_replay_data_active: AtomicBool::new(false),
        session_victories: AtomicU64::new(0),
        session_defeats: AtomicU64::new(0),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_menu_event(|app, event| match event.id() {
            id if id == overlay_info::MENU_ITEM_SHOW_CONFIG => {
                overlay_info::show_config_window(app)
            }
            id if id == overlay_info::MENU_ITEM_SHOW_OVERLAY => {
                overlay_info::show_overlay_window(app);
                let _ = app.emit(overlay_info::OVERLAY_SHOWSTATS_EVENT, json!({}));
            }
            id if id == overlay_info::MENU_ITEM_QUIT => request_clean_exit(app, 0),
            _ => {}
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let flags = overlay_info::parse_runtime_flags();
                match window_close_action(
                    window.label(),
                    flags.minimize_to_tray,
                    APP_EXIT_IN_PROGRESS.load(Ordering::Acquire),
                ) {
                    WindowCloseAction::AllowClose => {}
                    WindowCloseAction::HidePerformance => {
                        api.prevent_close();
                        performance_overlay::hide_window(&window.app_handle());
                    }
                    WindowCloseAction::HideWindow => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    WindowCloseAction::ExitApp => {
                        api.prevent_close();
                        request_clean_exit(&window.app_handle(), 0);
                    }
                }
            }
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                if window.label() == "performance" {
                    if let Some(performance_window) =
                        window.app_handle().get_webview_window("performance")
                    {
                        performance_overlay::persist_geometry(&performance_window);
                    }
                }
            }
            _ => {}
        })
        .setup(|app| {
            spawn_protocol_store_warmup();

            let flags = overlay_info::parse_runtime_flags();

            if flags.auto_update {
                let handle = app.handle().clone();

                tauri::async_runtime::spawn(async move {
                    let result = auto_update(handle).await;

                    if result.is_err() {
                        crate::sco_log!("Auto update failed: {}", result.unwrap_err());
                    }
                });
            }

            // Always start with overlay hidden; user can show it via hotkey/tray/actions.
            overlay_info::hide_overlay_window(&app.app_handle());

            if flags.start_minimized {
                if let Some(config_window) = app.get_webview_window("config") {
                    let _ = config_window.hide();
                }
            } else {
                overlay_info::show_config_window(&app.app_handle());
            }

            let _ = app
                .get_webview_window("overlay")
                .and_then(|w| w.set_always_on_top(true).ok());
            let _ = app
                .get_webview_window("overlay")
                .and_then(|w| w.set_skip_taskbar(true).ok());
            let _ = app
                .get_webview_window("overlay")
                .and_then(|w| w.set_focusable(false).ok());
            let _ = app
                .get_webview_window("overlay")
                .and_then(|w| w.set_ignore_cursor_events(true).ok());
            if let Some(window) = app.get_webview_window("overlay") {
                if let Err(error) = overlay_info::apply_overlay_placement(&window) {
                    crate::sco_log!("Could not apply saved overlay placement: {error}");
                }
            }
            let _ = app
                .get_webview_window("performance")
                .and_then(|w| w.set_always_on_top(true).ok());
            let _ = app
                .get_webview_window("performance")
                .and_then(|w| w.set_skip_taskbar(true).ok());
            let _ = app
                .get_webview_window("performance")
                .and_then(|w| w.set_focusable(false).ok());
            let _ = app
                .get_webview_window("performance")
                .and_then(|w| w.set_ignore_cursor_events(true).ok());
            if let Some(window) = app.get_webview_window("performance") {
                if let Err(error) = performance_overlay::apply_saved_geometry(&window) {
                    crate::sco_log!("Could not apply saved performance placement: {error}");
                }
            }

            if let Some(tray_menu) = overlay_info::build_tray_menu(&app.app_handle()) {
                let mut tray_builder = TrayIconBuilder::new()
                    .menu(&tray_menu)
                    .show_menu_on_left_click(true)
                    .tooltip("SCO Overlay");

                if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                }

                if let Ok(tray) = tray_builder.build(app) {
                    if let Ok(mut tray_slot) = app.state::<BackendState>().tray_icon.lock() {
                        *tray_slot = Some(tray);
                    }
                } else {
                    crate::sco_log!("Failed to build system tray icon");
                }
            }

            let startup_settings = read_settings_file();
            if let Err(error) = sync_start_with_windows_setting(&startup_settings) {
                crate::sco_log!("[SCO/settings] Failed to initialize start_with_windows: {error}");
            }

            overlay_info::sync_overlay_runtime_settings(&app.app_handle());
            performance_overlay::apply_settings(&app.app_handle());

            if let Err(error) = overlay_info::register_overlay_hotkeys(&app.app_handle()) {
                crate::sco_log!("[SCO/hotkey] {error}");
            }

            spawn_replay_creation_watcher(app.app_handle().clone());
            spawn_game_launch_winrate_task(app.app_handle().clone());
            performance_overlay::spawn_monitor(app.app_handle().clone());
            let (stats, replays, stats_replays, stats_current_replay_files) = {
                let state = app.state::<BackendState>();
                (
                    state.stats.clone(),
                    state.replays.clone(),
                    state.stats_replays.clone(),
                    state.stats_current_replay_files.clone(),
                )
            };
            if let Err(error) = request_startup_analysis(
                app.app_handle().clone(),
                stats,
                replays,
                stats_replays,
                stats_current_replay_files,
                StartupAnalysisTrigger::Setup,
            ) {
                crate::sco_log!(
                    "[SCO/stats] failed to request startup analysis during setup: {error}"
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config_request,
            pick_folder,
            performance_start_drag,
            is_dev,
            save_overlay_screenshot,
            open_folder_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri");
}

#[cfg(test)]
fn test_path_root_from_env(var_name: &str, default: &str) -> PathBuf {
    std::env::var_os(var_name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

#[cfg(test)]
pub(crate) fn test_replay_path(file_name: &str) -> String {
    test_path_root_from_env("SCO_TEST_REPLAY_ROOT", r"")
        .join(file_name)
        .display()
        .to_string()
}

#[cfg(test)]
pub(crate) fn test_config_path(file_name: &str) -> PathBuf {
    test_path_root_from_env("SCO_TEST_CONFIG_ROOT", r"").join(file_name)
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/replay_rows.rs"]
mod replay_row_tests;

#[cfg(test)]
#[path = "tests/games_row_mutators.rs"]
mod games_row_mutators_tests;

#[cfg(test)]
#[path = "tests/player_rows.rs"]
mod player_row_tests;

#[cfg(test)]
#[path = "tests/player_note_persistence.rs"]
mod player_note_persistence_tests;

#[cfg(test)]
#[path = "tests/replay_chat.rs"]
mod replay_chat_tests;

#[cfg(test)]
#[path = "tests/replay_cache_slots.rs"]
mod replay_cache_slot_tests;

#[cfg(test)]
#[path = "tests/replay_watcher_cache.rs"]
mod replay_watcher_cache_tests;

#[cfg(test)]
#[path = "tests/randomizer.rs"]
mod randomizer_tests;

#[cfg(test)]
#[path = "tests/hotkey_reassign.rs"]
mod hotkey_reassign_tests;

#[cfg(test)]
#[path = "tests/overlay_settings.rs"]
mod overlay_settings_tests;

#[cfg(test)]
#[path = "tests/overlay_screenshot.rs"]
mod overlay_screenshot_tests;

#[cfg(test)]
#[path = "tests/logging.rs"]
mod logging_tests;

#[cfg(test)]
#[path = "tests/folder_picker.rs"]
mod folder_picker_tests;

#[cfg(test)]
#[path = "tests/folder_open.rs"]
mod folder_open_tests;

#[cfg(test)]
#[path = "tests/window_shutdown.rs"]
mod window_shutdown_tests;

#[cfg(test)]
#[path = "tests/runtime_settings.rs"]
mod runtime_settings_tests;

#[cfg(test)]
#[path = "tests/startup_settings.rs"]
mod startup_settings_tests;

#[cfg(test)]
#[path = "tests/default_settings.rs"]
mod default_settings_tests;
