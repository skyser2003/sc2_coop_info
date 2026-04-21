use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rfd::FileDialog;
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::calculate_replay_hash;
use s2coop_analyzer::detailed_replay_analysis::{
    GenerateCacheConfig, GenerateCacheRuntimeOptions, GenerateCacheStopController,
    ReplayAnalysisResources,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::{Deserialize, Serialize};
use serde_json::{self, Map, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri_plugin_updater::UpdaterExt;
use ts_rs::TS;

use tauri::{tray::TrayIconBuilder, AppHandle, Emitter, Manager, State, Wry};

#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(target_os = "windows")]
use winreg::RegKey;

mod app_settings;
mod backend_state;
mod game_launch_detector;
pub mod logging;
pub mod overlay_info;
pub mod path_manager;
mod performance_overlay;
pub mod randomizer;
pub mod replay_analysis;
pub mod shared_types;
pub mod test_helper;
pub use app_settings::AppSettings;
pub use backend_state::BackendState;
pub use game_launch_detector::{GameLaunchDetector, GameLaunchStatus};

#[macro_export]
macro_rules! sco_log {
    ($($arg:tt)*) => {{
        $crate::logging::log_line(&format!($($arg)*));
    }};
}

use crate::app_settings::PlayerNotes;
use crate::backend_state::ReplayState;
use crate::path_manager::{get_cache_path, is_dev_env};
use crate::replay_analysis::ReplayAnalysis;
use crate::shared_types::{LocalizedLabels, ReplayScanProgressPayload};

pub const UNLIMITED_REPLAY_LIMIT: usize = 0;
const SCO_REPLAY_SCAN_PROGRESS_EVENT: &str = "sco://replay-scan-progress";
const SCO_ANALYSIS_COMPLETED_EVENT: &str = "sco://analysis-completed";
const WINDOWS_STARTUP_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const WINDOWS_STARTUP_VALUE_NAME: &str = "SCO Overlay";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayActionResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct OverlayActionResponse {
    pub status: &'static str,
    pub result: OverlayActionResult,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub randomizer: Option<randomizer::RandomizerResult>,
}

impl OverlayActionResponse {
    fn success(message: impl Into<String>) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: true,
                path: None,
            },
            message: message.into(),
            randomizer: None,
        }
    }

    fn success_with_path(message: impl Into<String>, path: String) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: true,
                path: Some(path),
            },
            message: message.into(),
            randomizer: None,
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: false,
                path: None,
            },
            message: message.into(),
            randomizer: None,
        }
    }

    fn failure_with_path(message: impl Into<String>, path: String) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: false,
                path: Some(path),
            },
            message: message.into(),
            randomizer: None,
        }
    }
}

fn to_json_value<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigPayload {
    pub status: &'static str,
    pub settings: AppSettings,
    pub active_settings: AppSettings,
    pub randomizer_catalog: shared_types::OverlayRandomizerCatalog,
    pub monitor_catalog: Vec<shared_types::MonitorOption>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigReplaysPayload {
    pub status: &'static str,
    pub replays: Vec<GamesRowPayload>,
    #[ts(type = "number")]
    pub total_replays: usize,
    pub selected_replay_file: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigPlayersPayload {
    pub status: &'static str,
    pub players: Vec<replay_analysis::PlayerRowPayload>,
    pub loading: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigWeekliesPayload {
    pub status: &'static str,
    pub weeklies: Vec<replay_analysis::WeeklyRowPayload>,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ConfigChatPayload {
    pub status: &'static str,
    pub chat: ReplayChatPayload,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsStatePayload {
    pub ready: bool,
    #[ts(type = "number")]
    pub games: u64,
    #[ts(type = "number")]
    pub detailed_parsed_count: u64,
    #[ts(type = "number")]
    pub total_valid_files: u64,
    #[ts(type = "Record<string, any> | null")]
    #[ts(optional)]
    pub analysis: Option<Value>,
    pub main_players: Vec<String>,
    pub main_handles: Vec<String>,
    pub analysis_running: bool,
    #[ts(optional)]
    pub analysis_running_mode: Option<String>,
    pub simple_analysis_status: String,
    pub detailed_analysis_status: String,
    pub detailed_analysis_atstart: bool,
    pub prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    pub message: String,
    pub scan_progress: ReplayScanProgressPayload,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct StatsActionPayload {
    pub status: &'static str,
    pub result: OverlayActionResult,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<StatsStatePayload>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct AnalysisCompletedPayload {
    pub mode: String,
    pub message: String,
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn canonical_mutator_id_with_dictionary(mutator: &str, dictionary: &Sc2DictionaryData) -> String {
    if dictionary.mutator_data(mutator).is_some() {
        mutator.to_string()
    } else if let Some(mutator_id) = dictionary.mutator_id_from_name(mutator) {
        mutator_id.to_string()
    } else {
        mutator.to_string()
    }
}

fn mutator_display_name_en_with_dictionary(
    mutator: &str,
    dictionary: &Sc2DictionaryData,
) -> String {
    let mutator_id = canonical_mutator_id_with_dictionary(mutator, dictionary);
    dictionary
        .mutator_data(&mutator_id)
        .map(|value| decode_html_entities(&value.name.en))
        .filter(|value| !value.is_empty())
        .or_else(|| {
            dictionary
                .mutator_ids
                .get(&mutator_id)
                .map(|value| value.to_string())
        })
        .unwrap_or_default()
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

pub fn windows_startup_command_value(executable_path: &Path) -> String {
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

fn sync_start_with_windows_setting(settings: &AppSettings) -> Result<(), String> {
    sync_windows_startup_registration(settings.start_with_windows)
}

pub const OVERLAY_RUNTIME_SETTING_KEYS: [&str; 9] = [
    "color_player1",
    "color_player2",
    "color_amon",
    "color_mastery",
    "duration",
    "show_session",
    "show_charts",
    "hide_nicknames_in_overlay",
    "language",
];

pub const OVERLAY_HOTKEY_SETTING_KEYS: [&str; 7] = [
    "hotkey_show/hide",
    "hotkey_show",
    "hotkey_hide",
    "hotkey_newer",
    "hotkey_older",
    "hotkey_winrates",
    "performance_hotkey",
];

pub const OVERLAY_PLACEMENT_SETTING_KEYS: [&str; 1] = ["monitor"];

const PERFORMANCE_RUNTIME_SETTING_KEYS: [&str; 4] = [
    "performance_show",
    "performance_geometry",
    "performance_processes",
    "monitor",
];

fn apply_runtime_settings(
    app: &tauri::AppHandle<Wry>,
    previous_settings: &AppSettings,
    next_settings: &AppSettings,
) {
    let state = app.state::<BackendState>();
    let next_settings = state.replace_active_settings(next_settings);
    let overlay_runtime_changed = AppSettings::any_setting_changed(
        previous_settings,
        &next_settings,
        &OVERLAY_RUNTIME_SETTING_KEYS,
    );
    let overlay_hotkeys_changed = AppSettings::any_setting_changed(
        previous_settings,
        &next_settings,
        &OVERLAY_HOTKEY_SETTING_KEYS,
    );
    let overlay_placement_changed = AppSettings::any_setting_changed(
        previous_settings,
        &next_settings,
        &OVERLAY_PLACEMENT_SETTING_KEYS,
    );
    let performance_runtime_changed = AppSettings::any_setting_changed(
        previous_settings,
        &next_settings,
        &PERFORMANCE_RUNTIME_SETTING_KEYS,
    );

    if overlay_runtime_changed {
        overlay_info::sync_overlay_runtime_settings(app);
    }

    let previous_show_charts = previous_settings.show_charts;
    let show_charts = next_settings.show_charts;
    if show_charts != previous_show_charts {
        let _ = app.emit(
            overlay_info::OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT,
            show_charts,
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
        stats.detailed_analysis_atstart = next_settings.detailed_analysis_atstart;
    }
}

pub fn update_settings_player_note(
    settings: &mut AppSettings,
    handle: &str,
    note_value: &str,
) -> Result<(), String> {
    let normalized_handle = ReplayAnalysis::normalized_handle_key(handle);
    if normalized_handle.is_empty() {
        return Err("Handle is empty".to_string());
    }

    let notes: &mut PlayerNotes = &mut settings.player_notes;

    let existing_key = notes
        .keys()
        .find(|key| ReplayAnalysis::normalized_handle_key(key) == normalized_handle)
        .cloned()
        .unwrap_or_else(|| sanitize_replay_text(handle).trim().to_string());

    let trimmed_note = note_value.trim();
    if trimmed_note.is_empty() {
        notes.remove(&existing_key);
    } else {
        notes.insert(existing_key, note_value.to_string());
    }

    Ok(())
}

pub fn folder_dialog_start_directory(directory: Option<String>) -> Option<PathBuf> {
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

pub fn session_counter_delta(result: &str) -> (u64, u64) {
    match result.trim().to_ascii_lowercase().as_str() {
        "victory" => (1, 0),
        "defeat" => (0, 1),
        _ => (0, 0),
    }
}

fn units_to_stats_with_dictionary(dictionary: &Sc2DictionaryData) -> HashSet<String> {
    dictionary.units_to_stats.clone()
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

pub fn configured_main_names_from_settings(settings: &AppSettings) -> HashSet<String> {
    let mut names = settings
        .main_names
        .iter()
        .map(|name| ReplayAnalysis::normalized_player_key(name))
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();

    if !names.is_empty() {
        return names;
    }

    let account_root = settings.account_folder.trim();
    if account_root.is_empty() {
        return names;
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

    names
}

pub fn configured_main_handles_from_settings(settings: &AppSettings) -> HashSet<String> {
    let account_root = settings.account_folder.trim();
    if account_root.is_empty() {
        return HashSet::new();
    }
    extract_account_handles_from_folder(account_root)
}

fn replay_should_swap_main_and_ally(
    replay: &ReplayInfo,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> bool {
    let p1_handle = ReplayAnalysis::normalized_handle_key(&replay.main().handle);
    let p2_handle = ReplayAnalysis::normalized_handle_key(&replay.ally().handle);
    if !main_handles.is_empty() && (!p1_handle.is_empty() || !p2_handle.is_empty()) {
        let p1_is_main =
            ReplayAnalysis::is_main_player_by_handle(&replay.main().handle, main_handles);
        let p2_is_main =
            ReplayAnalysis::is_main_player_by_handle(&replay.ally().handle, main_handles);
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
        let p1_is_main = ReplayAnalysis::is_main_player_by_name(&replay.main().name, main_names);
        let p2_is_main = ReplayAnalysis::is_main_player_by_name(&replay.ally().name, main_names);
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

pub fn orient_replay_for_main_names(
    mut replay: ReplayInfo,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> ReplayInfo {
    if !replay_should_swap_main_and_ally(&replay, main_names, main_handles) {
        return replay;
    }

    replay.main_slot = replay.ally_index();
    swap_player_stats_sides(&mut replay.player_stats);
    replay
}

#[derive(Clone, Serialize, Default, PartialEq, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayChatMessage {
    pub player: u8,
    pub text: String,
    pub time: f64,
}

#[derive(Clone, Serialize, Default, PartialEq, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct ReplayChatPayload {
    pub file: String,
    #[ts(type = "number")]
    pub date: u64,
    pub map: String,
    pub result: String,
    pub slot1_name: String,
    pub slot2_name: String,
    pub messages: Vec<ReplayChatMessage>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct GamesRowPayload {
    pub file: String,
    #[ts(type = "number")]
    pub date: u64,
    pub map: String,
    pub result: String,
    pub difficulty: String,
    pub p1: String,
    pub p2: String,
    pub slot1_commander: String,
    pub slot2_commander: String,
    pub enemy: String,
    pub main_commander: String,
    pub ally_commander: String,
    #[ts(type = "number")]
    pub length: u64,
    #[ts(type = "number")]
    pub main_apm: u64,
    #[ts(type = "number")]
    pub ally_apm: u64,
    #[ts(type = "number")]
    pub main_kills: u64,
    #[ts(type = "number")]
    pub ally_kills: u64,
    pub extension: bool,
    #[ts(type = "number")]
    pub brutal_plus: u64,
    pub weekly: bool,
    #[ts(optional)]
    pub weekly_name: Option<String>,
    pub mutators: Vec<shared_types::UiMutatorRow>,
    pub is_mutation: bool,
}

#[derive(Clone, Default)]
pub struct ReplayInfo {
    pub file: String,
    pub date: u64,
    pub map: String,
    pub result: String,
    pub difficulty: String,
    pub enemy: String,
    pub length: u64,
    pub accurate_length: f64,
    slot1: ReplayPlayerInfo,
    slot2: ReplayPlayerInfo,
    main_slot: usize,
    pub amon_units: Value,
    pub player_stats: Value,
    pub extension: bool,
    pub brutal_plus: u64,
    pub weekly: bool,
    pub weekly_name: Option<String>,
    pub mutators: Vec<String>,
    pub comp: String,
    pub bonus: Vec<u64>,
    pub bonus_total: Option<u64>,
    pub messages: Vec<ReplayChatMessage>,
    pub is_detailed: bool,
}

#[derive(Clone, Default)]
pub struct ReplayPlayerInfo {
    pub name: String,
    pub handle: String,
    pub apm: u64,
    pub kills: u64,
    pub commander: String,
    pub commander_level: u64,
    pub mastery_level: u64,
    pub prestige: u64,
    pub masteries: Vec<u64>,
    pub units: Value,
    pub icons: Value,
}

impl ReplayPlayerInfo {
    fn sanitized_for_client(&self) -> Self {
        Self {
            name: sanitize_replay_text(&self.name),
            handle: self.handle.clone(),
            apm: self.apm,
            kills: self.kills,
            commander: sanitize_replay_text(&self.commander),
            commander_level: self.commander_level,
            mastery_level: self.mastery_level,
            prestige: self.prestige,
            masteries: normalize_mastery_values(&self.masteries),
            units: sanitize_unit_map(&self.units),
            icons: sanitize_icon_map(&self.icons),
        }
    }
}

impl ReplayInfo {
    pub(crate) fn should_keep_existing_detailed_variant(
        existing_is_detailed: bool,
        incoming_is_detailed: bool,
    ) -> bool {
        existing_is_detailed || !incoming_is_detailed
    }

    pub(crate) fn sort_replays(replays: &mut [Self]) {
        replays.sort_by(|left, right| {
            right
                .date
                .cmp(&left.date)
                .then_with(|| right.file.cmp(&left.file))
        });
    }

    pub fn with_players(
        slot1: ReplayPlayerInfo,
        slot2: ReplayPlayerInfo,
        main_slot: usize,
    ) -> Self {
        Self {
            slot1,
            slot2,
            main_slot: main_slot.min(1),
            ..Self::default()
        }
    }

    fn slot(&self, index: usize) -> &ReplayPlayerInfo {
        match index {
            0 => &self.slot1,
            1 => &self.slot2,
            _ => &self.slot1,
        }
    }

    pub fn slot1(&self) -> &ReplayPlayerInfo {
        &self.slot1
    }

    pub fn slot2(&self) -> &ReplayPlayerInfo {
        &self.slot2
    }

    pub fn main_index(&self) -> usize {
        self.main_slot.min(1)
    }

    pub fn ally_index(&self) -> usize {
        1 - self.main_index()
    }

    pub fn main(&self) -> &ReplayPlayerInfo {
        self.slot(self.main_index())
    }

    pub fn ally(&self) -> &ReplayPlayerInfo {
        self.slot(self.ally_index())
    }

    pub fn main_apm(&self) -> u64 {
        self.main().apm
    }

    pub fn ally_apm(&self) -> u64 {
        self.ally().apm
    }

    pub fn main_kills(&self) -> u64 {
        self.main().kills
    }

    pub fn ally_kills(&self) -> u64 {
        self.ally().kills
    }

    pub fn main_commander(&self) -> &str {
        &self.main().commander
    }

    pub fn ally_commander(&self) -> &str {
        &self.ally().commander
    }

    pub fn main_commander_level(&self) -> u64 {
        self.main().commander_level
    }

    pub fn ally_commander_level(&self) -> u64 {
        self.ally().commander_level
    }

    pub fn main_mastery_level(&self) -> u64 {
        self.main().mastery_level
    }

    pub fn ally_mastery_level(&self) -> u64 {
        self.ally().mastery_level
    }

    pub fn main_prestige(&self) -> u64 {
        self.main().prestige
    }

    pub fn ally_prestige(&self) -> u64 {
        self.ally().prestige
    }

    pub fn main_masteries(&self) -> &[u64] {
        &self.main().masteries
    }

    pub fn ally_masteries(&self) -> &[u64] {
        &self.ally().masteries
    }

    pub fn main_units(&self) -> &Value {
        &self.main().units
    }

    pub fn ally_units(&self) -> &Value {
        &self.ally().units
    }

    pub fn main_icons(&self) -> &Value {
        &self.main().icons
    }

    pub fn ally_icons(&self) -> &Value {
        &self.ally().icons
    }

    pub fn as_games_row_payload_with_dictionary(
        &self,
        dictionary: &Sc2DictionaryData,
    ) -> GamesRowPayload {
        let sanitized = self.sanitized_for_client_with_dictionary(dictionary);
        let mutators = sanitized
            .mutators
            .iter()
            .map(|mutator| {
                let mutator_id = canonical_mutator_id_with_dictionary(mutator, dictionary);
                let (name_en, name_ko, description_en, description_ko) = dictionary
                    .mutator_data(&mutator_id)
                    .map(|value| {
                        (
                            decode_html_entities(&value.name.en),
                            decode_html_entities(&value.name.ko),
                            decode_html_entities(&value.description.en),
                            decode_html_entities(&value.description.ko),
                        )
                    })
                    .unwrap_or_default();
                let fallback_name_en =
                    mutator_display_name_en_with_dictionary(&mutator_id, dictionary);
                let icon_name = if name_en.is_empty() {
                    fallback_name_en.to_string()
                } else {
                    name_en.to_string()
                };
                let display_name_en = if name_en.is_empty() {
                    fallback_name_en
                } else {
                    name_en
                };
                shared_types::UiMutatorRow {
                    id: mutator_id.clone(),
                    name: shared_types::LocalizedText {
                        en: display_name_en,
                        ko: name_ko,
                    },
                    icon_name,
                    description: shared_types::LocalizedText {
                        en: description_en,
                        ko: description_ko,
                    },
                }
            })
            .collect::<Vec<_>>();
        GamesRowPayload {
            file: sanitized.file.clone(),
            date: sanitized.date,
            map: sanitized.map.clone(),
            result: sanitized.result.clone(),
            difficulty: sanitized.difficulty.clone(),
            p1: sanitized.slot1().name.clone(),
            p2: sanitized.slot2().name.clone(),
            slot1_commander: sanitized.slot1().commander.clone(),
            slot2_commander: sanitized.slot2().commander.clone(),
            enemy: sanitized.enemy.clone(),
            main_commander: sanitized.main().commander.clone(),
            ally_commander: sanitized.ally().commander.clone(),
            length: sanitized.length,
            main_apm: sanitized.main().apm,
            ally_apm: sanitized.ally().apm,
            main_kills: sanitized.main().kills,
            ally_kills: sanitized.ally().kills,
            extension: sanitized.extension,
            brutal_plus: sanitized.brutal_plus,
            weekly: sanitized.weekly,
            weekly_name: sanitized.weekly_name,
            mutators,
            is_mutation: sanitized.weekly || !sanitized.mutators.is_empty(),
        }
    }

    pub fn as_games_row_payload(&self) -> GamesRowPayload {
        let sanitized = self.sanitized_for_client();
        let mutators = sanitized
            .mutators
            .iter()
            .map(|mutator| {
                let display_name = decode_html_entities(mutator);
                shared_types::UiMutatorRow {
                    id: mutator.clone(),
                    name: shared_types::LocalizedText {
                        en: display_name.clone(),
                        ko: String::new(),
                    },
                    icon_name: display_name,
                    description: shared_types::LocalizedText::default(),
                }
            })
            .collect::<Vec<_>>();
        GamesRowPayload {
            file: sanitized.file.clone(),
            date: sanitized.date,
            map: sanitized.map.clone(),
            result: sanitized.result.clone(),
            difficulty: sanitized.difficulty.clone(),
            p1: sanitized.slot1().name.clone(),
            p2: sanitized.slot2().name.clone(),
            slot1_commander: sanitized.slot1().commander.clone(),
            slot2_commander: sanitized.slot2().commander.clone(),
            enemy: sanitized.enemy.clone(),
            main_commander: sanitized.main().commander.clone(),
            ally_commander: sanitized.ally().commander.clone(),
            length: sanitized.length,
            main_apm: sanitized.main().apm,
            ally_apm: sanitized.ally().apm,
            main_kills: sanitized.main().kills,
            ally_kills: sanitized.ally().kills,
            extension: sanitized.extension,
            brutal_plus: sanitized.brutal_plus,
            weekly: sanitized.weekly,
            weekly_name: sanitized.weekly_name,
            mutators,
            is_mutation: sanitized.weekly || !sanitized.mutators.is_empty(),
        }
    }

    pub fn as_games_row_with_dictionary(&self, dictionary: &Sc2DictionaryData) -> Value {
        to_json_value(self.as_games_row_payload_with_dictionary(dictionary))
    }

    pub fn as_games_row(&self) -> Value {
        to_json_value(self.as_games_row_payload())
    }

    pub fn chat_payload_with_dictionary(
        &self,
        dictionary: &Sc2DictionaryData,
    ) -> ReplayChatPayload {
        let sanitized = self.sanitized_for_client_with_dictionary(dictionary);

        ReplayChatPayload {
            file: sanitized.file.clone(),
            date: sanitized.date,
            map: sanitized.map.clone(),
            result: sanitized.result.clone(),
            slot1_name: sanitized.slot1().name.clone(),
            slot2_name: sanitized.slot2().name.clone(),
            messages: sanitized.messages.clone(),
        }
    }

    pub fn chat_payload(&self) -> ReplayChatPayload {
        let sanitized = self.sanitized_for_client();

        ReplayChatPayload {
            file: sanitized.file.clone(),
            date: sanitized.date,
            map: sanitized.map.clone(),
            result: sanitized.result.clone(),
            slot1_name: sanitized.slot1().name.clone(),
            slot2_name: sanitized.slot2().name.clone(),
            messages: sanitized.messages.clone(),
        }
    }

    pub(crate) fn sanitized_for_client_with_dictionary(
        &self,
        dictionary: &Sc2DictionaryData,
    ) -> Self {
        let client_result = if self.result.eq_ignore_ascii_case("Unparsed") {
            "Failed".to_string()
        } else {
            sanitize_replay_text(&self.result)
        };
        Self {
            file: self.file.clone(),
            date: self.date,
            map: sanitize_replay_text(
                &dictionary
                    .coop_map_english_name(&self.map)
                    .unwrap_or_else(|| self.map.to_string()),
            ),
            result: client_result,
            difficulty: sanitize_replay_text(&self.difficulty),
            enemy: sanitize_replay_text(&self.enemy),
            length: self.length,
            accurate_length: self.accurate_length,
            slot1: self.slot1.sanitized_for_client(),
            slot2: self.slot2.sanitized_for_client(),
            main_slot: self.main_index(),
            amon_units: sanitize_unit_map(&self.amon_units),
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

    pub(crate) fn sanitized_for_client(&self) -> Self {
        let client_result = if self.result.eq_ignore_ascii_case("Unparsed") {
            "Failed".to_string()
        } else {
            sanitize_replay_text(&self.result)
        };
        Self {
            file: self.file.clone(),
            date: self.date,
            map: sanitize_replay_text(&self.map),
            result: client_result,
            difficulty: sanitize_replay_text(&self.difficulty),
            enemy: sanitize_replay_text(&self.enemy),
            length: self.length,
            accurate_length: self.accurate_length,
            slot1: self.slot1.sanitized_for_client(),
            slot2: self.slot2.sanitized_for_client(),
            main_slot: self.main_index(),
            amon_units: sanitize_unit_map(&self.amon_units),
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

fn emit_replay_scan_progress(app: &AppHandle<Wry>, log_event: bool) {
    let payload = app
        .state::<BackendState>()
        .replay_scan_progress()
        .as_payload();
    if log_event {
        crate::sco_log!(
            "[SCO/stats/event] emit {} stage={} status={} completed={} total={} elapsed_ms={}",
            SCO_REPLAY_SCAN_PROGRESS_EVENT,
            payload.stage,
            payload.status,
            payload.completed,
            payload.total,
            payload.elapsed_ms
        );
    }
    if let Err(error) = app.emit(SCO_REPLAY_SCAN_PROGRESS_EVENT, payload) {
        crate::sco_log!("[SCO/stats] failed to emit scan progress: {error}");
    }
}

enum ProgressEmitterCommand {
    Stop,
}

fn emit_analysis_completed(app: &AppHandle<Wry>, mode: AnalysisMode, message: &str) {
    let payload = AnalysisCompletedPayload {
        mode: mode.key().to_string(),
        message: message.to_string(),
    };
    crate::sco_log!(
        "[SCO/stats/event] emit {} mode={} message={}",
        SCO_ANALYSIS_COMPLETED_EVENT,
        payload.mode,
        payload.message
    );
    if let Err(error) = app.emit(SCO_ANALYSIS_COMPLETED_EVENT, payload) {
        crate::sco_log!("[SCO/stats] failed to emit analysis completed event: {error}");
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

pub fn canonicalize_coop_map_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.starts_with("AC_") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn map_display_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        raw.to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Default, Clone)]
pub struct UnitStatsRollup {
    pub created: i64,
    pub created_hidden: bool,
    pub made: u64,
    pub lost: i64,
    pub lost_hidden: bool,
    pub kills: i64,
    pub kill_percentages: Vec<f64>,
}

#[derive(Default)]
pub struct CommanderUnitRollup {
    pub count: u64,
    pub units: HashMap<String, UnitStatsRollup>,
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

fn unit_rollup_count_value(value: i64, hidden: bool) -> Value {
    if hidden {
        Value::String("-".to_string())
    } else {
        Value::from(value)
    }
}

fn build_amon_unit_data(amon_rollup: std::collections::BTreeMap<String, UnitStatsRollup>) -> Value {
    #[derive(Serialize)]
    struct AmonUnitRow {
        created: i64,
        lost: i64,
        kills: i64,
        #[serde(rename = "KD")]
        kd: Value,
    }

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
                to_json_value(AmonUnitRow {
                    created: row.created,
                    lost: row.lost,
                    kills: row.kills,
                    kd: Value::String("-".to_string()),
                }),
            );
        } else {
            out.insert(
                unit,
                to_json_value(AmonUnitRow {
                    created: row.created,
                    lost: row.lost,
                    kills: row.kills,
                    kd: Value::from(if row.lost <= 0 {
                        0.0
                    } else {
                        row.kills as f64 / row.lost as f64
                    }),
                }),
            );
        }

        total.created = total.created.saturating_add(row.created);
        total.lost = total.lost.saturating_add(row.lost);
        total.kills = total.kills.saturating_add(row.kills);
    }

    out.insert(
        "sum".to_string(),
        to_json_value(AmonUnitRow {
            created: total.created,
            lost: total.lost,
            kills: total.kills,
            kd: Value::from(if total.lost <= 0 {
                0.0
            } else {
                total.kills as f64 / total.lost as f64
            }),
        }),
    );

    Value::Object(out)
}

pub fn build_commander_unit_data_with_dictionary(
    side_rollup: std::collections::BTreeMap<String, CommanderUnitRollup>,
    dictionary: &Sc2DictionaryData,
) -> Value {
    #[derive(Serialize)]
    struct CommanderUnitRow {
        created: Value,
        made: f64,
        lost: Value,
        lost_percent: Option<f64>,
        kills: i64,
        #[serde(rename = "KD")]
        kd: Option<f64>,
        kill_percentage: f64,
    }

    let mut out = Map::new();

    for (commander, entry) in side_rollup {
        let mut rows = Map::new();
        let mut totals = UnitStatsRollup::default();
        let mut units_to_delete = HashSet::new();
        let mut units = entry.units.into_iter().collect::<Vec<_>>();
        let stats_units = units_to_stats_with_dictionary(dictionary);

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
                to_json_value(CommanderUnitRow {
                    created: unit_rollup_count_value(unit_row.created, unit_row.created_hidden),
                    made,
                    lost: unit_rollup_count_value(unit_row.lost, unit_row.lost_hidden),
                    lost_percent,
                    kills: unit_row.kills,
                    kd,
                    kill_percentage,
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
            to_json_value(CommanderUnitRow {
                created: Value::from(totals.created),
                made: 1.0,
                lost: Value::from(totals.lost),
                lost_percent: Some(total_lost_percent),
                kills: totals.kills,
                kd: Some(total_kd),
                kill_percentage: 1.0,
            }),
        );
        rows.insert("count".to_string(), Value::from(entry.count));
        out.insert(commander, Value::Object(rows));
    }

    Value::Object(out)
}

pub fn build_commander_unit_data(
    side_rollup: std::collections::BTreeMap<String, CommanderUnitRollup>,
) -> Value {
    let dictionary = Sc2DictionaryData::default();
    build_commander_unit_data_with_dictionary(side_rollup, &dictionary)
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

pub fn sanitize_unit_map(value: &Value) -> Value {
    if let Value::Object(raw) = value {
        let mut output = Map::new();
        for (key, raw_entry) in raw.iter() {
            if key.is_empty() {
                continue;
            }
            if let Some(arr) = raw_entry.as_array() {
                let mut values: [Value; 4] = [
                    Value::from(0),
                    Value::from(0),
                    Value::from(0),
                    Value::from(0.0),
                ];
                for (idx, item) in arr.iter().take(4).enumerate() {
                    if idx < 3 {
                        if let Some(number) = item.as_f64() {
                            values[idx] = if number.is_finite() {
                                Value::from(number.round() as i64)
                            } else {
                                Value::from(0)
                            };
                        } else if item.is_string() {
                            values[idx] = item.clone();
                        }
                    } else if let Some(number) = item.as_f64() {
                        values[idx] = if number.is_finite() {
                            Value::from(number.max(0.0))
                        } else {
                            Value::from(0.0)
                        };
                    }
                }
                output.insert(
                    sanitize_replay_text(key),
                    Value::Array(vec![
                        values[0].clone(),
                        values[1].clone(),
                        values[2].clone(),
                        values[3].clone(),
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
                output.insert(key.clone(), Value::from(count));
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
                    to_json_value(crate::shared_types::ReplayPlayerSeries {
                        name: sanitize_replay_text(&name),
                        killed: kills,
                        army,
                        supply,
                        mining,
                    }),
                );
            }
        }
    }
    Value::Object(output)
}

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

fn resolve_replay_root(settings: &AppSettings) -> Option<PathBuf> {
    let account_folder = settings.account_folder.trim();
    if !account_folder.is_empty() {
        let candidates = build_replay_root_candidates(account_folder);
        if let Some(path) = candidates.iter().find(|path| path.is_dir()) {
            return Some(path.clone());
        }
    }

    None
}

fn replay_watch_root_from_settings(settings: &AppSettings) -> Option<PathBuf> {
    let account_folder = settings.account_folder.trim();
    if account_folder.is_empty() {
        return None;
    }

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
    worker_count: usize,
    stop_controller: Arc<GenerateCacheStopController>,
) -> Result<(usize, bool), String> {
    let state = app.state::<BackendState>();
    let settings = state.read_settings_memory();
    let replay_scan_progress = state.replay_scan_progress();
    let Some(account_dir) = resolve_replay_root(&settings) else {
        return Err("Replay root is not configured for detailed analysis.".to_string());
    };
    let output_file = get_cache_path();
    let logger = {
        let app = app.clone();
        let stats = Arc::clone(stats);
        let replay_scan_progress = replay_scan_progress.clone();
        move |message: String| {
            if let Some((completed, total)) = parse_detailed_analysis_progress_counts(&message) {
                replay_scan_progress.set_counts(total, completed);
            }
            let normalized = normalize_detailed_analysis_logger_message(&message);
            crate::sco_log!("[SCO/stats] {normalized}");
            replay_scan_progress.set_stage("detailed_analysis_running");
            replay_scan_progress.set_status("Parsing");
            if let Ok(mut guard) = stats.lock() {
                guard.detailed_analysis_status = normalized.clone();
                guard.message = normalized.clone();
            }
            emit_replay_scan_progress(&app, false);
        }
    };

    let resources = state
        .replay_analysis_resources()
        .map_err(|error| format!("Failed to access replay analysis resources: {error}"))?;

    GenerateCacheConfig {
        account_dir,
        output_file: output_file.clone(),
        recent_replay_count: None,
    }
    .generate_with_resources_and_runtime_and_logger(
        resources.as_ref(),
        &logger,
        &GenerateCacheRuntimeOptions {
            worker_count: Some(worker_count),
            stop_controller: Some(stop_controller),
        },
    )
    .map(|summary| (summary.scanned_replays, summary.completed))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupAnalysisTrigger {
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
pub struct StartupAnalysisRequestOutcome {
    pub include_detailed: bool,
    pub started: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisMode {
    Simple,
    Detailed,
}

impl AnalysisMode {
    pub(crate) fn from_include_detailed(include_detailed: bool) -> Self {
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

    fn key(self) -> &'static str {
        self.slug()
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

fn analysis_elapsed_suffix(elapsed: Duration) -> String {
    format!("Time consumed: {:.2} s.", elapsed.as_secs_f64())
}

fn analysis_completed_message(mode: AnalysisMode, replay_count: u64, elapsed: Duration) -> String {
    let summary = if replay_count == 0 {
        "No replay files found.".to_string()
    } else {
        format!(
            "{} completed with {replay_count} replay file(s).",
            mode.display()
        )
    };
    format!("{summary} {}", analysis_elapsed_suffix(elapsed))
}

fn analysis_stopped_message(mode: AnalysisMode, detail: &str, elapsed: Duration) -> String {
    format!(
        "{} stopped. {} {}",
        mode.display(),
        detail,
        analysis_elapsed_suffix(elapsed)
    )
}

fn analysis_failed_message(mode: AnalysisMode, message: &str, elapsed: Duration) -> String {
    format!(
        "{} failed: {message} {}",
        mode.display(),
        analysis_elapsed_suffix(elapsed)
    )
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

pub fn parse_detailed_analysis_progress_counts(message: &str) -> Option<(u64, u64)> {
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
    stats.analysis_running = false;
    stats.analysis_running_mode = None;
    match mode {
        AnalysisMode::Simple => {
            stats.simple_analysis_status = analysis_status_text(mode, phase);
        }
        AnalysisMode::Detailed => {
            stats.detailed_analysis_status = analysis_status_text(mode, phase);
        }
    }
}

fn startup_analysis_mode(include_detailed: bool) -> &'static str {
    analysis_mode(include_detailed).slug()
}

pub fn prepare_startup_analysis_request(
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
    replays_slot: Arc<Mutex<HashMap<String, ReplayInfo>>>,
    stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
    detailed_stop_controller_slot: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
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
            stats_current_replay_files_slot,
            detailed_stop_controller_slot,
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

pub fn update_analysis_replay_cache_slots(
    replays: &[ReplayInfo],
    replays_slot: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
) {
    if let Ok(mut cache) = replays_slot.lock() {
        cache.clear();
        for replay in replays {
            let replay_hash = calculate_replay_hash(&PathBuf::from(&replay.file));
            if replay_hash.is_empty() {
                continue;
            }
            match cache.get(&replay_hash) {
                Some(existing)
                    if ReplayInfo::should_keep_existing_detailed_variant(
                        existing.is_detailed,
                        replay.is_detailed,
                    ) => {}
                _ => {
                    cache.retain(|hash, entry| hash == &replay_hash || entry.file != replay.file);
                    cache.insert(replay_hash.clone(), replay.clone());
                }
            }
        }
    } else {
        crate::sco_log!("[SCO/stats] failed to update shared replay cache after scan");
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

struct AnalysisOutcome {
    reported_replay_count: usize,
    replays: Vec<ReplayInfo>,
    final_cache_entries: Vec<CacheReplayEntry>,
    analysis_completed: bool,
}

fn run_analysis(
    app: &AppHandle<Wry>,
    analysis_state: &Arc<Mutex<StatsState>>,
    detailed_stop_controller_slot: &Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
    limit: usize,
    include_detailed: bool,
) -> Result<AnalysisOutcome, String> {
    let state = app.state::<BackendState>();
    if include_detailed {
        let existing_cache_by_hash = load_existing_cache_by_hash();
        let worker_count = state
            .read_settings_memory()
            .normalized_analysis_worker_threads();
        let stop_controller = Arc::new(GenerateCacheStopController::new());
        if let Ok(mut slot) = detailed_stop_controller_slot.lock() {
            *slot = Some(stop_controller.clone());
        }

        let generation_result =
            generate_detailed_analysis_cache(app, analysis_state, worker_count, stop_controller);

        if let Ok(mut slot) = detailed_stop_controller_slot.lock() {
            slot.take();
        }

        let (scanned_replays, completed) = generation_result?;
        crate::sco_log!(
            "[SCO/stats] detailed scan generated '{}' with {} replay(s) completed={completed}",
            get_cache_path().display(),
            scanned_replays
        );

        let cache_path = get_cache_path();
        let payload = match std::fs::read(&cache_path) {
            Ok(payload) => payload,
            Err(error) => {
                crate::sco_log!("[SCO/stats] failed to read generated detailed cache: {error}");
                Vec::new()
            }
        };
        let new_cache_entries = serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload)
            .unwrap_or_else(|error| {
                crate::sco_log!("[SCO/stats] failed to parse generated detailed cache: {error}");
                Vec::new()
            });

        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let replays = ReplayAnalysis::load_detailed_analysis_replays_snapshot(
            limit,
            &main_names,
            &main_handles,
        );
        let final_cache_entries = merge_cache_entries(&existing_cache_by_hash, new_cache_entries);

        Ok(AnalysisOutcome {
            reported_replay_count: scanned_replays,
            replays,
            final_cache_entries,
            analysis_completed: completed,
        })
    } else {
        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let replay_scan_progress = state.replay_scan_progress();
        let replay_scan_in_flight = state.replay_scan_in_flight();
        let resources = state.replay_analysis_resources()?;
        let replays = ReplayAnalysis::analyze_replays_with_resources(
            limit,
            &state.read_settings_memory(),
            &main_names,
            &main_handles,
            replay_scan_progress.as_ref(),
            replay_scan_in_flight.as_ref(),
            &resources,
        );
        let final_cache_entries = load_existing_cache_by_hash().into_values().collect();

        Ok(AnalysisOutcome {
            reported_replay_count: replays.len(),
            replays,
            final_cache_entries,
            analysis_completed: true,
        })
    }
}

fn spawn_analysis_task(
    app: AppHandle<Wry>,
    stats: Arc<Mutex<StatsState>>,
    replays_slot: Arc<Mutex<HashMap<String, ReplayInfo>>>,
    stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
    detailed_stop_controller_slot: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
    include_detailed: bool,
    limit: usize,
) {
    let mode = analysis_mode(include_detailed);
    let state = app.state::<BackendState>();
    let replay_scan_progress = state.replay_scan_progress();
    let settings = state.read_settings_memory();
    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
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

        if guard.analysis_running {
            let active_mode = guard.analysis_running_mode;
            if active_mode == Some(mode) {
                crate::sco_log!("[SCO/stats] {} already running", mode.display());
                guard.message = analysis_already_running_message(mode);
            } else {
                crate::sco_log!(
                    "[SCO/stats] {} blocked while another analysis is running",
                    mode.display()
                );
                guard.message = analysis_blocked_by_other_mode_message(mode);
            }
            return;
        }
        guard.analysis_running = true;
        guard.analysis_running_mode = Some(mode);
        set_analysis_running_status(
            &mut guard,
            mode,
            if include_detailed {
                "generating cache"
            } else {
                "scanning replays"
            },
        );
        guard.message = analysis_started_message(mode);

        guard.ready = false;
        guard.analysis = Some(empty_stats_payload());
        guard.games = 0;
        guard.main_players = Vec::new();
        guard.main_handles = Vec::new();
        guard.prestige_names = Default::default();
        if guard.message.is_empty() {
            guard.message = analysis_started_message(mode);
        }
    }
    replay_scan_progress.reset("queued");

    let analysis_state = stats;
    let shared_replay_cache_slot = replays_slot;
    let current_replay_files_slot = stats_current_replay_files_slot;
    let detailed_stop_controller_slot_for_thread = detailed_stop_controller_slot;
    let app_for_analysis = app.clone();
    let app_for_progress = app.clone();
    let app_for_progress_updates = app.clone();
    let replay_scan_progress_for_thread = replay_scan_progress.clone();
    let settings_for_thread = settings.clone();
    let main_names_for_thread = main_names.clone();
    let main_handles_for_thread = main_handles.clone();
    thread::spawn(move || {
        let started_at = Instant::now();
        crate::sco_log!("[SCO/stats] {} thread started", mode.display());
        replay_scan_progress_for_thread.set_stage(if include_detailed {
            "detailed_analysis_running"
        } else {
            "scan_running"
        });
        replay_scan_progress_for_thread.set_status("Parsing");
        emit_replay_scan_progress(&app_for_progress, true);

        let (progress_tx, progress_rx) = mpsc::channel::<ProgressEmitterCommand>();
        let progress_handle = thread::spawn(move || loop {
            match progress_rx.recv_timeout(Duration::from_millis(150)) {
                Ok(ProgressEmitterCommand::Stop) => {
                    emit_replay_scan_progress(&app_for_progress_updates, true);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    emit_replay_scan_progress(&app_for_progress_updates, false);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        });

        let analysis_outcome = match run_analysis(
            &app_for_progress,
            &analysis_state,
            &detailed_stop_controller_slot_for_thread,
            limit,
            include_detailed,
        ) {
            Ok(outcome) => outcome,
            Err(message) => {
                let elapsed = started_at.elapsed();
                crate::sco_log!("[SCO/stats] {} failed: {message}", mode.display());
                if let Ok(mut guard) = analysis_state.lock() {
                    set_analysis_terminal_status(&mut guard, mode, "failed");
                    guard.detailed_analysis_status = analysis_error_status_text(mode, &message);
                    guard.message = analysis_failed_message(mode, &message, elapsed);
                }
                replay_scan_progress_for_thread.set_stage("analysis_failed");
                replay_scan_progress_for_thread.set_status("Completed");
                let _ = progress_tx.send(ProgressEmitterCommand::Stop);
                let completion_message = analysis_state
                    .lock()
                    .map(|guard| guard.message.clone())
                    .unwrap_or_else(|_| analysis_failed_message(mode, &message, elapsed));
                emit_analysis_completed(&app_for_analysis, mode, &completion_message);
                let _ = progress_handle.join();
                return;
            }
        };

        if let Ok(mut guard) = analysis_state.lock() {
            if include_detailed {
                let replay_count = analysis_outcome.reported_replay_count;
                if analysis_outcome.analysis_completed {
                    set_analysis_running_status(&mut guard, mode, "refreshing replay summaries");
                    guard.message = format!(
                        "Generated '{}' with {} replay entr{}.",
                        get_cache_path().display(),
                        replay_count,
                        if replay_count == 1 { "y" } else { "ies" }
                    );
                } else {
                    guard.analysis_running = false;
                    guard.analysis_running_mode = None;
                    guard.detailed_analysis_status = analysis_status_text(mode, "stopped");
                    guard.message = format!(
                        "Detailed analysis stopped after saving {} replay entr{}.",
                        replay_count,
                        if replay_count == 1 { "y" } else { "ies" }
                    );
                }
            }
        }

        let AnalysisOutcome {
            reported_replay_count: _reported_replay_count,
            replays: all_replays,
            final_cache_entries,
            analysis_completed: detailed_completed,
        } = analysis_outcome;

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

        let current_replay_files =
            current_replay_files_snapshot(UNLIMITED_REPLAY_LIMIT, &settings_for_thread);
        if include_detailed && !detailed_completed {
            replay_scan_progress_for_thread
                .total
                .store(current_replay_files.len() as u64, Ordering::Release);
        }
        update_analysis_replay_cache_slots(&all_replays, &shared_replay_cache_slot);
        if let Ok(mut current_files) = current_replay_files_slot.lock() {
            *current_files = current_replay_files;
        } else {
            crate::sco_log!("[SCO/stats] failed to update current replay file set after scan");
        }

        if include_detailed {
            let cache_path = get_cache_path();
            if let Err(error) =
                CacheReplayEntry::persist_simple_analysis(&final_cache_entries, &cache_path)
            {
                crate::sco_log!("[SCO/stats] failed to persist final merged cache: {error}");
            }
        }

        replay_scan_progress_for_thread.set_stage("building_statistics");
        let dictionary = app_for_analysis
            .state::<BackendState>()
            .dictionary_data()
            .ok();
        let snapshot = dictionary
            .as_deref()
            .map(|dictionary| {
                ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
                    &all_replays,
                    include_detailed,
                    &main_names_for_thread,
                    &main_handles_for_thread,
                    dictionary,
                )
            })
            .unwrap_or_else(|| StatsSnapshot {
                ready: true,
                games: all_replays.len() as u64,
                message: "Dictionary data is unavailable.".to_string(),
                ..StatsSnapshot::default()
            });

        let mut guard = match analysis_state.lock() {
            Ok(guard) => guard,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/stats] {} aborted before rebuild: {error}",
                    mode.display()
                );
                replay_scan_progress_for_thread.set_stage("analysis_ready");
                replay_scan_progress_for_thread.set_status("Completed");
                let _ = progress_tx.send(ProgressEmitterCommand::Stop);
                emit_analysis_completed(
                    &app_for_analysis,
                    mode,
                    &analysis_error_status_text(mode, "analysis aborted before rebuild"),
                );
                let _ = progress_handle.join();
                return;
            }
        };

        if include_detailed && !detailed_completed {
            guard.analysis_running = false;
            guard.analysis_running_mode = None;
        } else {
            set_analysis_running_status(&mut guard, mode, "building statistics");
        }

        apply_rebuild_snapshot(&mut guard, snapshot, mode);
        if include_detailed && !detailed_completed {
            guard.analysis_running = false;
            guard.analysis_running_mode = None;
            guard.detailed_analysis_status = analysis_status_text(mode, "stopped");
            guard.message = analysis_stopped_message(
                mode,
                "Run detailed analysis to continue generating cache.",
                started_at.elapsed(),
            );
        } else {
            guard.message = analysis_completed_message(mode, guard.games, started_at.elapsed());
        }
        if !include_detailed {
            if let Some(dictionary) = dictionary.as_deref() {
                sync_detailed_analysis_status_from_replays_with_dictionary(
                    &mut guard,
                    &all_replays,
                    dictionary,
                );
            } else {
                sync_detailed_analysis_status_from_replays(&mut guard, &all_replays);
            }
        }
        replay_scan_progress_for_thread.set_stage("analysis_ready");
        replay_scan_progress_for_thread.set_status("Completed");
        let _ = progress_tx.send(ProgressEmitterCommand::Stop);

        crate::sco_log!(
            "[SCO/stats] {} finished in {}ms for {} replay(s) completed={}",
            mode.display(),
            started_at.elapsed().as_millis(),
            all_replays.len(),
            if include_detailed {
                detailed_completed
            } else {
                true
            }
        );

        let completion_message = guard.message.clone();
        drop(guard);
        emit_analysis_completed(&app_for_analysis, mode, &completion_message);
        let _ = progress_handle.join();
    });
}

fn spawn_startup_analysis_task(
    app: AppHandle<Wry>,
    stats: Arc<Mutex<StatsState>>,
    replays_slot: Arc<Mutex<HashMap<String, ReplayInfo>>>,
    stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
    detailed_stop_controller_slot: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
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
        stats_current_replay_files_slot,
        detailed_stop_controller_slot,
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

fn replay_index_by_file(replays: &[ReplayInfo], file: &Option<String>) -> Option<usize> {
    let needle = file.as_deref()?;
    replays.iter().position(|entry| entry.file == needle)
}

fn replay_chat_payload_from_slots(
    replay_state: Arc<Mutex<ReplayState>>,
    settings: AppSettings,
    main_names: HashSet<String>,
    main_handles: HashSet<String>,
    file: &str,
    dictionary: Option<Arc<Sc2DictionaryData>>,
    resources: Option<Arc<ReplayAnalysisResources>>,
) -> Result<ReplayChatPayload, String> {
    let requested_file = file.trim();
    if requested_file.is_empty() {
        return Err("No replay file specified.".to_string());
    }

    let replays = replay_state
        .lock()
        .map(|state| {
            state.sync_replay_cache_slots_with_resources(
                UNLIMITED_REPLAY_LIMIT,
                &settings,
                &main_names,
                &main_handles,
                resources.as_deref(),
            )
        })
        .unwrap_or_default();

    if let Some(replay) = replays.iter().find(|replay| replay.file == requested_file) {
        return Ok(dictionary
            .as_deref()
            .map(|dictionary| replay.chat_payload_with_dictionary(dictionary))
            .unwrap_or_else(|| replay.chat_payload()));
    }

    let replay_path = Path::new(requested_file);
    if !replay_path.exists() {
        return Err(format!("Replay file not found: {requested_file}"));
    }

    let resources = resources
        .as_deref()
        .ok_or_else(|| "Replay analysis resources are unavailable.".to_string())?;
    let (replay, _) =
        ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(replay_path, resources)
            .ok_or_else(|| format!("Failed to parse replay file: {requested_file}"))?;
    Ok(dictionary
        .as_deref()
        .map(|dictionary| replay.chat_payload_with_dictionary(dictionary))
        .unwrap_or_else(|| replay.chat_payload_with_dictionary(resources.dictionary_data())))
}

fn current_replay_files_snapshot(limit: usize, settings: &AppSettings) -> HashSet<String> {
    let Some(root) = resolve_replay_root(settings) else {
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

fn parse_new_replay_with_retries(
    path: &Path,
    resources: Option<&ReplayAnalysisResources>,
) -> Option<(ReplayInfo, Option<CacheReplayEntry>)> {
    const MAX_ATTEMPTS: usize = 40;
    const RETRY_DELAY: Duration = Duration::from_millis(250);
    const MIN_REPLAY_SIZE_BYTES: u64 = 8 * 1024;
    let resources = match resources {
        Some(resources) => resources,
        None => {
            crate::sco_log!(
                "[SCO/watch] parse abort file='{}' reason=replay_analysis_resources_unavailable",
                path.to_string_lossy()
            );
            return None;
        }
    };
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
            ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(path, resources)
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
                replay.main().name,
                replay.ally().name,
                replay.main_commander(),
                replay.ally_commander(),
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

pub fn persist_detailed_cache_entry_to_path(
    cache_path: &Path,
    entry: &CacheReplayEntry,
) -> Result<(), String> {
    let local_lock = Mutex::new(());
    persist_detailed_cache_entry_to_path_with_lock(cache_path, entry, &local_lock)
}

fn persist_detailed_cache_entry_to_path_with_lock(
    cache_path: &Path,
    entry: &CacheReplayEntry,
    persist_lock: &Mutex<()>,
) -> Result<(), String> {
    let _persist_guard = persist_lock
        .lock()
        .map_err(|_| "Failed to acquire detailed cache persistence lock".to_string())?;

    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create cache directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let entries = match std::fs::read(cache_path) {
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

    let mut merged = entries
        .into_iter()
        .filter(|existing| !existing.hash.is_empty())
        .map(|existing| (existing.hash.clone(), existing))
        .collect::<HashMap<_, _>>();

    if !entry.hash.is_empty() {
        merged.retain(|hash, existing| hash == &entry.hash || existing.file != entry.file);
        match merged.get(&entry.hash) {
            Some(existing)
                if ReplayInfo::should_keep_existing_detailed_variant(
                    existing.detailed_analysis,
                    entry.detailed_analysis,
                ) => {}
            _ => {
                merged.insert(entry.hash.clone(), entry.clone());
            }
        }
    }

    let mut entries = merged.into_values().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| right.file.cmp(&left.file))
    });

    CacheReplayEntry::write_entries(&entries, cache_path).map_err(|err| err.to_string())
}

fn spawn_detailed_cache_persist(
    state: &BackendState,
    entry: CacheReplayEntry,
    log_prefix: &'static str,
) {
    let persist_lock = state.detailed_cache_persist_lock();
    thread::spawn(move || {
        let replay_file = entry.file.clone();
        if let Err(error) = persist_detailed_cache_entry_to_path_with_lock(
            &get_cache_path(),
            &entry,
            persist_lock.as_ref(),
        ) {
            crate::sco_log!(
                "[SCO/{log_prefix}] failed to persist detailed cache entry for '{}': {error}",
                replay_file
            );
            return;
        }

        crate::sco_log!(
            "[SCO/{log_prefix}] persisted detailed cache entry for '{}'",
            replay_file
        );
    });
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

pub fn sync_detailed_analysis_status_from_replays(stats: &mut StatsState, replays: &[ReplayInfo]) {
    let total_valid_files = replays
        .iter()
        .filter(|replay| replay.result != "Unparsed" && replay.map.trim().starts_with("AC_"))
        .count();
    let detailed_parsed_count = replays
        .iter()
        .filter(|replay| {
            replay.result != "Unparsed"
                && replay.map.trim().starts_with("AC_")
                && ReplayAnalysis::replay_has_detailed_unit_stats(replay)
        })
        .count();

    stats.analysis_running = false;
    stats.analysis_running_mode = None;
    stats.detailed_analysis_status = if detailed_parsed_count == 0 {
        analysis_status_text(AnalysisMode::Detailed, "not started")
    } else {
        format!(
            "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
        )
    };
}

pub fn sync_detailed_analysis_status_from_replays_with_dictionary(
    stats: &mut StatsState,
    replays: &[ReplayInfo],
    dictionary: &Sc2DictionaryData,
) {
    let total_valid_files = replays
        .iter()
        .filter(|replay| {
            replay.result != "Unparsed"
                && dictionary.canonicalize_coop_map_id(&replay.map).is_some()
        })
        .count();
    let detailed_parsed_count = replays
        .iter()
        .filter(|replay| {
            replay.result != "Unparsed"
                && dictionary.canonicalize_coop_map_id(&replay.map).is_some()
                && ReplayAnalysis::replay_has_detailed_unit_stats(replay)
        })
        .count();

    stats.analysis_running = false;
    stats.analysis_running_mode = None;
    stats.detailed_analysis_status = if detailed_parsed_count == 0 {
        analysis_status_text(AnalysisMode::Detailed, "not started")
    } else {
        format!(
            "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
        )
    };
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

    let state = app.state::<BackendState>();
    let resources = state.replay_analysis_resources().ok();
    let Some((parsed, cache_entry)) = parse_new_replay_with_retries(path, resources.as_deref())
    else {
        crate::sco_log!("[SCO/watch] failed to parse new replay '{}'", file);
        return ReplayProcessOutcome::RetryLater;
    };

    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
    let replay = orient_replay_for_main_names(parsed, &main_names, &main_handles);
    let replay_hash = cache_entry
        .as_ref()
        .map(|entry| entry.hash.clone())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| calculate_replay_hash(path));
    if replay.main_commander().trim().is_empty() && replay.ally_commander().trim().is_empty() {
        crate::sco_log!(
            "[SCO/watch] parsed replay ignored file='{}' reason=missing_commanders main='{}' ally='{}'",
            replay.file, replay.main_commander(), replay.ally_commander()
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
        replay.main().name,
        replay.ally().name,
        replay.main_commander(),
        replay.ally_commander()
    );
    state.upsert_replay_in_memory_cache(&replay_hash, &replay);
    state.record_session_result(&replay.result);
    let settings = state.read_settings_memory();
    let show_replay_info_after_game = settings.show_replay_info_after_game;

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

    if let Some(cache_entry) = cache_entry {
        spawn_detailed_cache_persist(&state, cache_entry, "watch");
    }

    let invalidation_generation = state.invalidate_delayed_player_stats_popup_generation();
    crate::sco_log!(
        "[SCO/watch] invalidated delayed player stats popups generation={} replay='{}'",
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

    let replay_hash = calculate_replay_hash(path);
    if let Some(existing) = state.cached_replay_by_hash(&replay_hash) {
        if existing.is_detailed {
            return (ReplayProcessOutcome::Processed, Some(existing));
        }
    }

    let resources = state.replay_analysis_resources().ok();
    let Some((parsed, cache_entry)) = parse_new_replay_with_retries(path, resources.as_deref())
    else {
        crate::sco_log!("[SCO/show] failed to parse existing replay '{}'", file);
        return (ReplayProcessOutcome::RetryLater, None);
    };

    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
    let replay = orient_replay_for_main_names(parsed, &main_names, &main_handles);

    crate::sco_log!(
        "[SCO/show] replay accepted file='{}' date={} result='{}' main='{}' ally='{}' main_comm='{}' ally_comm='{}'",
        replay.file,
        replay.date,
        replay.result,
        replay.main().name,
        replay.ally().name,
        replay.main_commander(),
        replay.ally_commander()
    );

    let replay_hash = cache_entry
        .as_ref()
        .map(|entry| entry.hash.clone())
        .filter(|hash| !hash.is_empty())
        .unwrap_or_else(|| calculate_replay_hash(path));
    state.upsert_replay_in_memory_cache(&replay_hash, &replay);
    if let Some(cache_entry) = cache_entry {
        spawn_detailed_cache_persist(state, cache_entry, "show");
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
            let settings = app.state::<BackendState>().read_settings_memory();
            if let Some(root) = replay_watch_root_from_settings(&settings) {
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

fn choose_other_coop_player_stats(
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

fn spawn_game_launch_player_stats_task(app: tauri::AppHandle<Wry>) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(4));

        let mut launch_detector = GameLaunchDetector::new(Instant::now());

        loop {
            thread::sleep(Duration::from_millis(500));

            let state = app.state::<BackendState>();
            let settings = state.read_settings_memory();
            let show_player_stats_popups = settings.show_player_winrates;
            if !show_player_stats_popups {
                continue;
            }

            let replay_count = state.replay_count_for_launch_detector();
            let now = Instant::now();
            launch_detector.observe_replay_count(replay_count, now);

            let Some(payload) = fetch_sc2_live_game_payload() else {
                launch_detector.observe_non_live_state();
                continue;
            };
            if payload
                .get("isReplay")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                launch_detector.observe_non_live_state();
                continue;
            }

            let players = extract_live_game_players(&payload);
            if players.len() <= 2 {
                launch_detector.observe_non_live_state();
                continue;
            }
            let all_users = players
                .iter()
                .all(|player| player.kind.eq_ignore_ascii_case("user"));
            if all_users {
                launch_detector.observe_non_live_state();
                continue;
            }

            let display_time = value_as_u64_lossy(payload.get("displayTime")).unwrap_or(0);
            match launch_detector.update_display_time_status(display_time) {
                GameLaunchStatus::Started => {}
                GameLaunchStatus::Unknown
                | GameLaunchStatus::Idle
                | GameLaunchStatus::Running
                | GameLaunchStatus::Ended => continue,
            }

            if !launch_detector.should_attempt_popup(state.stats_have_player_rows(), replay_count) {
                continue;
            }
            if !launch_detector.replay_change_settled(now) {
                continue;
            }

            let (main_names, main_handles) = state.build_launch_main_identity();
            let Some((other_player_handle, other_player_name)) =
                choose_other_coop_player_stats(&players, &main_names, &main_handles)
            else {
                continue;
            };

            let invalidation_generation = state.invalidate_delayed_player_stats_popup_generation();
            crate::sco_log!(
                "[SCO/launch] invalidated delayed player stats popups generation={}",
                invalidation_generation
            );

            if overlay_info::show_player_stats_for_name(
                &app,
                &state,
                &other_player_handle,
                &other_player_name,
            ) {
                launch_detector.record_popup_shown(replay_count);
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

fn spawn_replay_analysis_resource_warmup(app: AppHandle<Wry>) {
    thread::spawn(move || {
        let started_at = Instant::now();
        let state = app.state::<BackendState>();
        match state.replay_analysis_resources() {
            Ok(_) => {
                crate::sco_log!(
                    "[SCO/analyzer] warmup completed in {}ms",
                    started_at.elapsed().as_millis()
                );
            }
            Err(error) => {
                crate::sco_log!("[SCO/analyzer] warmup failed: {error}");
            }
        }
    });
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
    #[derive(Serialize)]
    struct EmptyStatsPayload {
        #[serde(rename = "MapData")]
        map_data: Map<String, Value>,
        #[serde(rename = "CommanderData")]
        commander_data: Map<String, Value>,
        #[serde(rename = "AllyCommanderData")]
        ally_commander_data: Map<String, Value>,
        #[serde(rename = "DifficultyData")]
        difficulty_data: Map<String, Value>,
        #[serde(rename = "RegionData")]
        region_data: Map<String, Value>,
        #[serde(rename = "UnitData")]
        unit_data: Value,
        #[serde(rename = "AmonData")]
        amon_data: Map<String, Value>,
        #[serde(rename = "PlayerData")]
        player_data: Map<String, Value>,
    }

    to_json_value(EmptyStatsPayload {
        map_data: Map::new(),
        commander_data: Map::new(),
        ally_commander_data: Map::new(),
        difficulty_data: Map::new(),
        region_data: Map::new(),
        unit_data: Value::Null,
        amon_data: Map::new(),
        player_data: Map::new(),
    })
}

pub(crate) fn apply_rebuild_snapshot(
    stats: &mut StatsState,
    snapshot: StatsSnapshot,
    mode: AnalysisMode,
) {
    stats.ready = snapshot.ready;
    stats.games = snapshot.games;
    stats.main_players = snapshot.main_players;
    stats.main_handles = snapshot.main_handles;
    stats.analysis = Some(snapshot.analysis);
    stats.prestige_names = snapshot.prestige_names;
    stats.message = snapshot.message;

    set_analysis_terminal_status(stats, mode, "completed");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowCloseAction {
    AllowClose,
    HidePerformance,
    HideWindow,
    ExitApp,
}

pub fn window_close_action(
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
    let state = app.state::<BackendState>();
    if !state.try_begin_exit() {
        return;
    }

    if let Ok(mut tray_icon) = state.tray_icon.lock() {
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
pub struct StatsState {
    pub ready: bool,
    pub analysis: Option<Value>,
    pub games: u64,
    pub main_players: Vec<String>,
    pub main_handles: Vec<String>,
    pub startup_analysis_requested: bool,
    pub analysis_running: bool,
    pub analysis_running_mode: Option<AnalysisMode>,
    pub simple_analysis_status: String,
    pub detailed_analysis_status: String,
    pub detailed_analysis_atstart: bool,
    pub prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct StatsSnapshot {
    pub ready: bool,
    pub games: u64,
    pub main_players: Vec<String>,
    pub main_handles: Vec<String>,
    pub analysis: Value,
    pub prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
    pub message: String,
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
            analysis_running: false,
            analysis_running_mode: None,
            simple_analysis_status: analysis_status_text(
                AnalysisMode::Simple,
                "waiting for startup",
            ),
            detailed_analysis_status: analysis_status_text(AnalysisMode::Detailed, "not started"),
            detailed_analysis_atstart: false,
            prestige_names: Default::default(),
            message: "No parsed statistics available yet.".to_string(),
        }
    }
}

impl StatsState {
    fn from_settings(settings: &AppSettings) -> Self {
        let mut state = Self::default();
        state.detailed_analysis_atstart = settings.detailed_analysis_atstart;
        state
    }

    fn as_payload(&self, scan_progress: ReplayScanProgressPayload) -> Value {
        let (analysis, main_players, main_handles, prestige_names, games, message) = if self.ready {
            (
                self.analysis.clone(),
                self.main_players.clone(),
                self.main_handles.clone(),
                self.prestige_names.clone(),
                self.games,
                self.message.clone(),
            )
        } else {
            (
                Some(empty_stats_payload()),
                Vec::new(),
                Vec::new(),
                Default::default(),
                0,
                if self.message.is_empty() {
                    "Statistics are updating. This may take a while.".to_string()
                } else {
                    self.message.clone()
                },
            )
        };

        to_json_value(StatsStatePayload {
            ready: self.ready,
            games,
            detailed_parsed_count: 0,
            total_valid_files: 0,
            analysis,
            main_players,
            main_handles,
            analysis_running: self.analysis_running,
            analysis_running_mode: self
                .analysis_running_mode
                .map(|mode| mode.key().to_string()),
            simple_analysis_status: self.simple_analysis_status.clone(),
            detailed_analysis_status: self.detailed_analysis_status.clone(),
            detailed_analysis_atstart: self.detailed_analysis_atstart,
            prestige_names,
            message,
            scan_progress,
        })
    }

    fn as_payload_typed(&self, scan_progress: ReplayScanProgressPayload) -> StatsStatePayload {
        serde_json::from_value(self.as_payload(scan_progress))
            .unwrap_or_else(|error| panic!("Failed to convert stats payload: {error}"))
    }
}

fn log_request(state: &BackendState, method: &str, path: &str, body: &Option<Value>) {
    let serialized_body = body
        .as_ref()
        .map(|payload| serde_json::to_string(payload).unwrap_or_else(|_| "<invalid-json>".into()));
    if let Some(serialized_body) = serialized_body {
        logging::append_line_if_enabled_from_state(
            state,
            &format!(
                "[SCO/request] method={} path={} body={}",
                method, path, serialized_body
            ),
        );
    } else {
        logging::append_line_if_enabled_from_state(
            state,
            &format!("[SCO/request] method={} path={}", method, path),
        );
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
async fn config_get(
    app: tauri::AppHandle<Wry>,
    state: State<'_, BackendState>,
) -> Result<ConfigPayload, String> {
    log_request(&state, "get", "/config", &None);
    Ok(ConfigPayload {
        status: "ok",
        settings: AppSettings::from_saved_file(),
        active_settings: state.read_settings_memory(),
        randomizer_catalog: state
            .dictionary_data()
            .map(|dictionary| randomizer::catalog_payload_with_dictionary(&dictionary))
            .unwrap_or_default(),
        monitor_catalog: overlay_info::available_monitor_catalog(&app),
    })
}

#[tauri::command]
async fn config_update(
    app: tauri::AppHandle<Wry>,
    settings: Value,
    persist: Option<bool>,
    state: State<'_, BackendState>,
) -> Result<ConfigPayload, String> {
    let body = Some(to_json_value(serde_json::json!({
        "settings": settings,
        "persist": persist.unwrap_or(true),
    })));
    log_request(&state, "post", "/config", &body);

    let settings_value = body
        .as_ref()
        .and_then(|payload| payload.get("settings"))
        .cloned()
        .ok_or_else(|| "Missing payload".to_string())?;

    let mut next_settings = AppSettings::merge_settings_with_defaults(settings_value);
    let previous_settings = state.read_settings_memory();
    let persist = body
        .as_ref()
        .and_then(|payload| payload.get("persist"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    next_settings.performance_geometry = previous_settings.performance_geometry.clone();

    if persist {
        state.write_settings_file(&next_settings)?;
    }
    apply_runtime_settings(&app, &previous_settings, &next_settings);

    Ok(ConfigPayload {
        status: "ok",
        settings: AppSettings::from_saved_file(),
        active_settings: state.read_settings_memory(),
        randomizer_catalog: state
            .dictionary_data()
            .map(|dictionary| randomizer::catalog_payload_with_dictionary(&dictionary))
            .unwrap_or_default(),
        monitor_catalog: overlay_info::available_monitor_catalog(&app),
    })
}

#[tauri::command]
async fn config_replays_get(
    _app: tauri::AppHandle<Wry>,
    limit: Option<usize>,
    state: State<'_, BackendState>,
) -> Result<ConfigReplaysPayload, String> {
    let path = format!("/config/replays?limit={}", limit.unwrap_or(300));
    log_request(&state, "get", &path, &None);
    let limit = parse_query_usize(&path, "limit", 300);
    let replay_state = state.get_replay_state();
    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
    let settings = state.read_settings_memory();
    let resources = state.replay_analysis_resources().ok();
    let dictionary = state.dictionary_data().ok();

    let (replays, total_replays, selected_replay_file) =
        tauri::async_runtime::spawn_blocking(move || {
            let replay_state = replay_state.lock().ok();
            let all_replays = replay_state
                .as_ref()
                .map(|state| {
                    state.sync_full_replay_cache_slots_with_resources(
                        &settings,
                        &main_names,
                        &main_handles,
                        resources.as_deref(),
                    )
                })
                .unwrap_or_default();
            let total_replays = all_replays.len();
            let mut replays = all_replays;
            if limit > 0 && replays.len() > limit {
                replays.truncate(limit);
            }
            let selected_replay_file = replay_state
                .as_ref()
                .and_then(|state| state.get_current_replay_file());

            (replays, total_replays, selected_replay_file)
        })
        .await
        .map_err(|error| format!("Failed to load /config/replays: {error}"))?;

    Ok(ConfigReplaysPayload {
        status: "ok",
        replays: replays
            .into_iter()
            .map(|replay| {
                dictionary
                    .as_deref()
                    .map(|dictionary| replay.as_games_row_payload_with_dictionary(dictionary))
                    .unwrap_or_else(|| replay.as_games_row_payload())
            })
            .collect(),
        total_replays,
        selected_replay_file,
    })
}

#[tauri::command]
async fn config_players_get(
    _app: tauri::AppHandle<Wry>,
    limit: Option<usize>,
    state: State<'_, BackendState>,
) -> Result<ConfigPlayersPayload, String> {
    let path = format!("/config/players?limit={}", limit.unwrap_or(500));
    log_request(&state, "get", &path, &None);
    let limit = parse_query_usize(&path, "limit", 500);
    let replay_state = state.get_replay_state();
    let replays = match replay_state.try_lock() {
        Ok(replay_state) => match replay_state.replays.try_lock() {
            Ok(replays) if !replays.is_empty() => {
                let mut replays = replays.values().cloned().collect::<Vec<_>>();
                ReplayInfo::sort_replays(&mut replays);
                replays
            }
            Ok(_) => {
                crate::sco_log!(
                    "[SCO/players] replay cache empty, starting background scan for players"
                );
                state.spawn_players_scan_task(limit);
                Vec::new()
            }
            Err(error) => match error {
                TryLockError::WouldBlock => {
                    crate::sco_log!(
                        "[SCO/players] replay cache busy, starting background scan for players"
                    );
                    state.spawn_players_scan_task(limit);
                    Vec::new()
                }
                TryLockError::Poisoned(_) => {
                    return Err("Failed to access replay cache: mutex is poisoned".to_string());
                }
            },
        },
        Err(error) => match error {
            TryLockError::WouldBlock => {
                crate::sco_log!(
                    "[SCO/players] replay state busy, starting background scan for players"
                );
                state.spawn_players_scan_task(limit);
                Vec::new()
            }
            TryLockError::Poisoned(_) => {
                return Err("Failed to access replay state: mutex is poisoned".to_string());
            }
        },
    };
    Ok(ConfigPlayersPayload {
        status: "ok",
        players: ReplayAnalysis::rebuild_player_rows_fast(&replays),
        loading: replays.is_empty(),
    })
}

#[tauri::command]
async fn config_weeklies_get(
    _app: tauri::AppHandle<Wry>,
    state: State<'_, BackendState>,
) -> Result<ConfigWeekliesPayload, String> {
    log_request(&state, "get", "/config/weeklies", &None);
    let replay_state = state.get_replay_state();
    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
    let settings = state.read_settings_memory();
    let resources = state.replay_analysis_resources().ok();

    let replays = tauri::async_runtime::spawn_blocking(move || {
        replay_state
            .lock()
            .map(|state| {
                state.sync_replay_cache_slots_with_resources(
                    UNLIMITED_REPLAY_LIMIT,
                    &settings,
                    &main_names,
                    &main_handles,
                    resources.as_deref(),
                )
            })
            .unwrap_or_default()
    })
    .await
    .map_err(|error| format!("Failed to load /config/weeklies: {error}"))?;
    let dictionary = state.dictionary_data().ok();
    Ok(ConfigWeekliesPayload {
        status: "ok",
        weeklies: dictionary
            .as_deref()
            .map(|dictionary| {
                ReplayAnalysis::rebuild_weeklies_rows_with_dictionary(
                    &replays,
                    chrono::Local::now().date_naive(),
                    dictionary,
                )
            })
            .unwrap_or_default(),
    })
}

#[tauri::command]
async fn config_stats_get(
    _app: tauri::AppHandle<Wry>,
    query: Option<String>,
    state: State<'_, BackendState>,
) -> Result<StatsStatePayload, String> {
    let path = if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        format!("/config/stats?{query}")
    } else {
        "/config/stats".to_string()
    };
    log_request(&state, "get", &path, &None);
    let stats = state.stats.clone();
    let replays = state
        .get_replay_state()
        .lock()
        .map(|replay_state| replay_state.replays.clone())
        .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new())));
    let stats_current_replay_files = state.stats_current_replay_files.clone();
    let state_snapshot = (
        state.configured_main_names(),
        state.configured_main_handles(),
        state.replay_scan_progress().as_payload(),
        state.dictionary_data().ok(),
    );
    let path_for_worker = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (main_names, main_handles, scan_progress, dictionary) = state_snapshot;
        let payload = match dictionary.as_deref() {
            Some(dictionary) => ReplayAnalysis::build_stats_response_with_dictionary(
                &path_for_worker,
                &stats,
                &replays,
                &stats_current_replay_files,
                scan_progress.clone(),
                &main_names,
                &main_handles,
                dictionary,
            )?,
            None => match stats.try_lock() {
                Ok(state) => state.as_payload(scan_progress),
                Err(TryLockError::WouldBlock) => {
                    let fallback = StatsState::default();
                    let mut payload = fallback.as_payload(scan_progress);
                    payload["message"] = Value::from("Dictionary data is unavailable.");
                    payload
                }
                Err(TryLockError::Poisoned(_)) => {
                    return Err("Failed to access stats state: mutex is poisoned".to_string());
                }
            },
        };
        serde_json::from_value(payload).map_err(|error| format!("Invalid stats payload: {error}"))
    })
    .await
    .map_err(|error| format!("Failed to read /config/stats: {error}"))?
    .map_err(|error| error)
}

#[tauri::command]
async fn config_replay_show(
    app: tauri::AppHandle<Wry>,
    file: Option<String>,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    let body = Some(to_json_value(serde_json::json!({ "file": file })));
    log_request(&state, "post", "/config/replays/show", &body);
    let requested = body
        .as_ref()
        .and_then(|payload| payload.get("file"))
        .and_then(Value::as_str);
    Ok(overlay_info::replay_show_for_window(
        &app, &state, requested,
    ))
}

#[tauri::command]
async fn config_replay_chat(
    _app: tauri::AppHandle<Wry>,
    file: String,
    state: State<'_, BackendState>,
) -> Result<ConfigChatPayload, String> {
    let body = Some(to_json_value(serde_json::json!({ "file": file })));
    log_request(&state, "post", "/config/replays/chat", &body);
    let requested_file = body
        .as_ref()
        .and_then(|payload| payload.get("file"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let replay_state = state.get_replay_state();
    let settings = state.read_settings_memory();
    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
    let dictionary = state.dictionary_data().ok();
    let resources = state.replay_analysis_resources().ok();
    let chat = tauri::async_runtime::spawn_blocking(move || {
        replay_chat_payload_from_slots(
            replay_state,
            settings,
            main_names,
            main_handles,
            &requested_file,
            dictionary,
            resources,
        )
    })
    .await
    .map_err(|error| format!("Failed to load /config/replays/chat: {error}"))?
    .map_err(|error| error)?;
    Ok(ConfigChatPayload { status: "ok", chat })
}

#[tauri::command]
async fn config_replay_move(
    app: tauri::AppHandle<Wry>,
    delta: i64,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    let body = Some(to_json_value(serde_json::json!({ "delta": delta })));
    log_request(&state, "post", "/config/replays/move", &body);
    let delta = body
        .as_ref()
        .and_then(|payload| payload.get("delta"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Ok(overlay_info::replay_move_window(&app, &state, delta))
}

#[tauri::command]
async fn config_action(
    app: tauri::AppHandle<Wry>,
    action: String,
    payload: Option<Value>,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    let body = if let Some(Value::Object(mut object)) = payload {
        object.insert("action".to_string(), Value::String(action));
        Some(Value::Object(object))
    } else {
        Some(to_json_value(serde_json::json!({ "action": action })))
    };
    log_request(&state, "post", "/config/action", &body);
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

            let mut saved_settings = AppSettings::from_saved_file();
            update_settings_player_note(&mut saved_settings, player_name, note_value)?;
            saved_settings.write_saved_settings_file()?;

            let mut active_settings = state.read_settings_memory();
            update_settings_player_note(&mut active_settings, player_name, note_value)?;
            state.replace_active_settings(&active_settings);

            Ok(OverlayActionResponse::success(
                if note_value.trim().is_empty() {
                    "Player note cleared."
                } else {
                    "Player note saved."
                },
            ))
        }
        _ => {
            if let Some(response) =
                overlay_info::perform_overlay_action(&app, &state, action, body.as_ref())
            {
                Ok(response)
            } else {
                Ok(OverlayActionResponse::failure(format!(
                    "Unsupported action: {action}"
                )))
            }
        }
    }
}

#[tauri::command]
async fn config_stats_action(
    app: tauri::AppHandle<Wry>,
    action: String,
    payload: Option<Value>,
    state: State<'_, BackendState>,
) -> Result<StatsActionPayload, String> {
    let body = if let Some(Value::Object(mut object)) = payload {
        object.insert("action".to_string(), Value::String(action));
        Some(Value::Object(object))
    } else {
        Some(to_json_value(serde_json::json!({ "action": action })))
    };
    log_request(&state, "post", "/config/stats/action", &body);
    let action = body
        .as_ref()
        .and_then(|payload| payload.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if let Some(response) =
        overlay_info::perform_overlay_action(&app, &state, action, body.as_ref())
    {
        return Ok(StatsActionPayload {
            status: response.status,
            result: response.result,
            message: response.message,
            stats: None,
        });
    }

    match action {
        "frontend_ready" => {
            let request_started_at = Instant::now();
            crate::sco_log!("[SCO/stats/action] frontend_ready requested");
            request_startup_analysis(
                app.clone(),
                state.stats.clone(),
                state
                    .get_replay_state()
                    .lock()
                    .map(|replay_state| replay_state.replays.clone())
                    .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new()))),
                state.stats_current_replay_files.clone(),
                state.detailed_analysis_stop_controller_slot(),
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
            return Ok(StatsActionPayload {
                status: "ok",
                result: OverlayActionResult {
                    ok: true,
                    path: None,
                },
                message: stats.message.clone(),
                stats: Some(stats.as_payload_typed(state.replay_scan_progress().as_payload())),
            });
        }
        "start_simple_analysis" | "run_detailed_analysis" => {
            let include_detailed = action == "run_detailed_analysis";
            let mode = analysis_mode(include_detailed);

            let limit = UNLIMITED_REPLAY_LIMIT;
            crate::sco_log!("[SCO/stats] {action} requested replay_limit={limit} on thread");
            spawn_analysis_task(
                app.clone(),
                state.stats.clone(),
                state
                    .get_replay_state()
                    .lock()
                    .map(|replay_state| replay_state.replays.clone())
                    .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new()))),
                state.stats_current_replay_files.clone(),
                state.detailed_analysis_stop_controller_slot(),
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
            crate::sco_log!(
                "[SCO/stats/action] {} immediate response message={}",
                action,
                status
            );
            let stats_payload = state
                .stats
                .lock()
                .ok()
                .map(|stats| stats.as_payload_typed(state.replay_scan_progress().as_payload()));
            return Ok(StatsActionPayload {
                status: "ok",
                result: OverlayActionResult {
                    ok: true,
                    path: None,
                },
                message: status,
                stats: stats_payload,
            });
        }
        "stop_detailed_analysis" => {}
        _ => {}
    }

    let mut stats = state
        .stats
        .lock()
        .map_err(|error| format!("Failed to access stats state: {error}"))?;
    let request_started_at = Instant::now();
    crate::sco_log!("[SCO/stats/action] action={action}");

    match action {
        "stop_detailed_analysis" => {
            if !stats.analysis_running
                || stats.analysis_running_mode != Some(AnalysisMode::Detailed)
            {
                stats.message = "Detailed analysis is not running.".to_string();
            } else if state.request_detailed_analysis_stop() {
                stats.detailed_analysis_status =
                    analysis_status_text(AnalysisMode::Detailed, "stopping");
                stats.message =
                    "Detailed analysis will stop after the current work finishes.".to_string();
            } else {
                stats.message = "Detailed analysis stop could not be requested.".to_string();
            }
            crate::sco_log!(
                "[SCO/stats] stop_detailed_analysis requested elapsed={}ms",
                request_started_at.elapsed().as_millis()
            );
        }
        "dump_data" => {
            let dump_path = PathBuf::from("SCO_analysis_dump.json");
            #[derive(Serialize)]
            struct DumpPayload {
                timestamp: u64,
                stats: Value,
            }

            let payload = to_json_value(DumpPayload {
                timestamp: format_date_from_system_time(SystemTime::now()),
                stats: stats.as_payload(state.replay_scan_progress().as_payload()),
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
            crate::sco_log!("[SCO/stats/action] delete_parsed_data requested");
            stats.ready = false;
            stats.startup_analysis_requested = false;
            stats.analysis = Some(empty_stats_payload());
            stats.prestige_names = Default::default();
            set_analysis_terminal_status(&mut stats, AnalysisMode::Simple, "not started");
            set_analysis_terminal_status(&mut stats, AnalysisMode::Detailed, "not started");
            state.set_detailed_analysis_stop_controller(None);
            stats.message = "No parsed statistics available yet.".to_string();
            state.clear_replay_cache_slots();
            if let Ok(mut stats_current_replay_files) = state.stats_current_replay_files.lock() {
                stats_current_replay_files.clear();
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
                    if let Err(error) =
                        state.persist_bool_setting("detailed_analysis_atstart", enabled)
                    {
                        crate::sco_log!(
                            "[SCO/settings] Failed to save detailed_analysis_atstart: {error}"
                        );
                    }
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
            return Ok(StatsActionPayload {
                status: "ok",
                result: OverlayActionResult {
                    ok: false,
                    path: None,
                },
                message: format!("Unsupported action: {action}"),
                stats: Some(stats.as_payload_typed(state.replay_scan_progress().as_payload())),
            });
        }
    }

    crate::sco_log!(
        "[SCO/stats/action] done action={} elapsed={}ms",
        action,
        request_started_at.elapsed().as_millis()
    );
    Ok(StatsActionPayload {
        status: "ok",
        result: OverlayActionResult {
            ok: true,
            path: None,
        },
        message: "Action processed".to_string(),
        stats: Some(stats.as_payload_typed(state.replay_scan_progress().as_payload())),
    })
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
    let settings = AppSettings::from_saved_file();

    let state = BackendState::new_with_settings(settings.clone());

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
                let _ = app.emit(
                    overlay_info::OVERLAY_SHOWSTATS_EVENT,
                    shared_types::EmptyPayload::default(),
                );
            }
            id if id == overlay_info::MENU_ITEM_QUIT => request_clean_exit(app, 0),
            _ => {}
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let state = window.app_handle().state::<BackendState>();
                let flags = overlay_info::parse_runtime_flags_from_state(&state);
                match window_close_action(
                    window.label(),
                    flags.minimize_to_tray,
                    state.exit_in_progress(),
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
            spawn_replay_analysis_resource_warmup(app.handle().clone());

            let state = app.state::<BackendState>();
            let flags = overlay_info::parse_runtime_flags_from_state(&state);

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

            let startup_settings = state.read_settings_memory();
            if let Err(error) = sync_start_with_windows_setting(&startup_settings) {
                crate::sco_log!("[SCO/settings] Failed to initialize start_with_windows: {error}");
            }

            overlay_info::sync_overlay_runtime_settings(&app.app_handle());
            performance_overlay::apply_settings(&app.app_handle());

            if let Err(error) = overlay_info::register_overlay_hotkeys(&app.app_handle()) {
                crate::sco_log!("[SCO/hotkey] {error}");
            }

            spawn_replay_creation_watcher(app.app_handle().clone());
            spawn_game_launch_player_stats_task(app.app_handle().clone());
            performance_overlay::spawn_monitor(app.app_handle().clone());
            let (stats, replays, stats_current_replay_files, detailed_stop_controller_slot) = {
                let state = app.state::<BackendState>();
                let replays = state
                    .get_replay_state()
                    .lock()
                    .map(|replay_state| replay_state.replays.clone())
                    .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new())));
                (
                    state.stats.clone(),
                    replays,
                    state.stats_current_replay_files.clone(),
                    state.detailed_analysis_stop_controller_slot(),
                )
            };
            if let Err(error) = request_startup_analysis(
                app.app_handle().clone(),
                stats,
                replays,
                stats_current_replay_files,
                detailed_stop_controller_slot,
                StartupAnalysisTrigger::Setup,
            ) {
                crate::sco_log!(
                    "[SCO/stats] failed to request startup analysis during setup: {error}"
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config_get,
            config_update,
            config_replays_get,
            config_players_get,
            config_weeklies_get,
            config_stats_get,
            config_replay_show,
            config_replay_chat,
            config_replay_move,
            config_action,
            config_stats_action,
            pick_folder,
            performance_start_drag,
            is_dev,
            save_overlay_screenshot,
            open_folder_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri");
}
