use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use display_info::DisplayInfo;
use s2coop_analyzer::dictionary_data;
use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{
    menu::{MenuBuilder, MenuItem},
    Emitter, Manager, Runtime, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::app_settings::AppSettings;
use crate::process_replay_detailed;
use crate::randomizer;
use crate::replay_analysis::ReplayAnalysis;
use crate::shared_types::{
    overlay_icon_payload_from_value, replay_data_record_from_value, swap_replay_data_record_sides,
    unit_stats_map_from_value, EmptyPayload, MonitorOption, OverlayInitColorsDurationPayload,
    OverlayPlayerInfoPayload, OverlayPlayerInfoRow, OverlayReplayPayload,
    OverlayScreenshotRequestPayload,
};
use crate::{
    configured_main_handles, configured_main_names, replay_index_by_file,
    replay_should_swap_main_and_ally, sanitize_replay_text, BackendState, UNLIMITED_REPLAY_LIMIT,
};

pub(crate) const MENU_ITEM_SHOW_CONFIG: &str = "show_config";
pub(crate) const MENU_ITEM_SHOW_OVERLAY: &str = "show_overlay";
pub(crate) const MENU_ITEM_QUIT: &str = "quit";

pub(crate) const OVERLAY_REPLAY_PAYLOAD_EVENT: &str = "sco://overlay-replay-payload";
pub(crate) const OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT: &str =
    "sco://overlay-show-hide-player-winrate";
pub(crate) const OVERLAY_PLAYER_WINRATE_EVENT: &str = "sco://overlay-player-winrate";
pub(crate) const OVERLAY_INIT_COLORS_DURATION_EVENT: &str = "sco://overlay-init-colors-duration";
pub(crate) const OVERLAY_SHOWSTATS_EVENT: &str = "sco://overlay-showstats";
pub(crate) const OVERLAY_HIDESTATS_EVENT: &str = "sco://overlay-hidestats";
pub(crate) const OVERLAY_SHOWHIDE_EVENT: &str = "sco://overlay-showhide";
pub(crate) const OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT: &str =
    "sco://overlay-set-show-charts-from-config";
pub(crate) const OVERLAY_SCREENSHOT_REQUEST_EVENT: &str = "sco://overlay-screenshot-request";

const OVERLAY_HOTKEY_DEFAULTS: [(&str, &str); 6] = [
    ("hotkey_show/hide", "Ctrl+Shift+*"),
    ("hotkey_show", ""),
    ("hotkey_hide", ""),
    ("hotkey_newer", "Ctrl+Alt+/"),
    ("hotkey_older", "Ctrl+Alt+*"),
    ("hotkey_winrates", "Ctrl+Alt+-"),
];

const OVERLAY_HOTKEY_BINDINGS: [(&str, &str); 7] = [
    ("hotkey_show/hide", "overlay_show_hide"),
    ("hotkey_show", "overlay_show"),
    ("hotkey_hide", "overlay_hide"),
    ("hotkey_newer", "overlay_newer"),
    ("hotkey_older", "overlay_older"),
    ("hotkey_winrates", "overlay_player_info"),
    ("performance_hotkey", "performance_show_hide"),
];

static HOTKEY_ACTION_INFLIGHT: AtomicBool = AtomicBool::new(false);

fn as_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn as_u32_vec(values: &[u64]) -> Vec<u32> {
    values.iter().copied().map(as_u32).collect()
}

fn overlay_mutator_name(mutator_id: &str) -> String {
    let canonical = if dictionary_data::mutator_data(mutator_id).is_some() {
        mutator_id.to_string()
    } else if let Some(mapped) = dictionary_data::mutator_id_from_name(mutator_id) {
        mapped.to_string()
    } else {
        mutator_id.to_string()
    };

    dictionary_data::mutator_data(&canonical)
        .map(|value| value.name.en.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            dictionary_data::mutator_ids()
                .get(&canonical)
                .map(|value| value.to_string())
        })
        .unwrap_or_default()
}

fn active_hotkey_reassign_path_slot() -> &'static Mutex<Option<String>> {
    static ACTIVE_HOTKEY_REASSIGN_PATH: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    ACTIVE_HOTKEY_REASSIGN_PATH.get_or_init(|| Mutex::new(None))
}

fn active_hotkey_reassign_binding_slot() -> &'static Mutex<Option<ResolvedHotkeyBinding>> {
    static ACTIVE_HOTKEY_REASSIGN_BINDING: OnceLock<Mutex<Option<ResolvedHotkeyBinding>>> =
        OnceLock::new();
    ACTIVE_HOTKEY_REASSIGN_BINDING.get_or_init(|| Mutex::new(None))
}

fn active_hotkey_reassign_path() -> Option<String> {
    active_hotkey_reassign_path_slot()
        .lock()
        .ok()
        .and_then(|path| path.clone())
}

fn active_hotkey_reassign_binding() -> Option<ResolvedHotkeyBinding> {
    active_hotkey_reassign_binding_slot()
        .lock()
        .ok()
        .and_then(|binding| binding.clone())
}

fn set_active_hotkey_reassign_path(path: Option<String>) {
    if let Ok(mut current) = active_hotkey_reassign_path_slot().lock() {
        *current = path;
    }
}

fn set_active_hotkey_reassign_binding(binding: Option<ResolvedHotkeyBinding>) {
    if let Ok(mut current) = active_hotkey_reassign_binding_slot().lock() {
        *current = binding;
    }
}

#[allow(dead_code)]
pub(crate) struct OverlayPlacement {
    monitor: usize,
    width: f64,
    height: f64,
    top_offset: i32,
    right_offset: i32,
    subtract_height: i32,
}

#[derive(Clone, Copy)]
pub struct RuntimeFlags {
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub auto_update: bool,
}

#[derive(Clone)]
struct MonitorDescriptor {
    name: String,
    position_x: i32,
    position_y: i32,
    width: u32,
    height: u32,
}

#[derive(Clone)]
pub struct ResolvedHotkeyBinding {
    pub path: &'static str,
    pub action: &'static str,
    pub shortcut: String,
    pub canonical: String,
}

impl OverlayReplayPayload {
    pub fn localized_prestige_text(commander: &str, prestige: u64, language: &str) -> String {
        if prestige == 0 {
            return String::new();
        }

        let commander = sanitize_replay_text(commander);
        let Some(index) = usize::try_from(prestige).ok() else {
            return format!("P{prestige}");
        };
        if let Some(lookup) = dictionary_data::prestige_names()
            .get(&commander)
            .and_then(|value| match language {
                "ko" => value.ko.get(index).or_else(|| value.en.get(index)),
                _ => value.en.get(index),
            })
            .map(String::as_str)
        {
            return lookup.to_string();
        }

        if let Some(lookup) = dictionary_data::prestige_name(&commander, prestige) {
            return lookup.to_string();
        }

        format!("P{prestige}")
    }

    fn from_replay(replay: &crate::ReplayInfo, language: &str) -> Self {
        let sanitized = replay.sanitized_for_client();
        let main_prestige = Self::localized_prestige_text(
            &sanitized.main_commander,
            sanitized.main_prestige,
            language,
        );
        let ally_prestige = Self::localized_prestige_text(
            &sanitized.ally_commander,
            sanitized.ally_prestige,
            language,
        );
        Self {
            file: sanitized.file,
            map_name: sanitized.map,
            main: sanitized.p1,
            ally: sanitized.p2,
            main_commander: sanitized.main_commander,
            ally_commander: sanitized.ally_commander,
            main_apm: as_u32(sanitized.main_apm),
            ally_apm: as_u32(sanitized.ally_apm),
            mainkills: as_u32(sanitized.main_kills),
            allykills: as_u32(sanitized.ally_kills),
            result: sanitized.result,
            difficulty: sanitized.difficulty,
            length: as_u32(sanitized.length),
            brutal_plus: as_u32(sanitized.brutal_plus),
            weekly: sanitized.weekly,
            weekly_name: sanitized.weekly_name,
            extension: sanitized.extension,
            main_commander_level: as_u32(sanitized.main_commander_level),
            ally_commander_level: as_u32(sanitized.ally_commander_level),
            main_masteries: as_u32_vec(&sanitized.main_masteries),
            ally_masteries: as_u32_vec(&sanitized.ally_masteries),
            main_units: unit_stats_map_from_value(&sanitized.main_units),
            ally_units: unit_stats_map_from_value(&sanitized.ally_units),
            amon_units: unit_stats_map_from_value(&sanitized.amon_units),
            main_icons: overlay_icon_payload_from_value(&sanitized.main_icons),
            ally_icons: overlay_icon_payload_from_value(&sanitized.ally_icons),
            mutators: sanitized
                .mutators
                .iter()
                .map(|mutator_id| overlay_mutator_name(mutator_id))
                .collect(),
            bonus: sanitized.bonus.iter().copied().map(as_u32).collect(),
            bonus_total: sanitized.bonus_total.map(as_u32),
            player_stats: Some(replay_data_record_from_value(&sanitized.player_stats)),
            main_prestige,
            ally_prestige,
            victory: None,
            defeat: None,
            commander: None,
            prestige: None,
            new_replay: None,
            fastest: None,
            comp: sanitized.comp,
        }
    }

    fn swap_sides(&mut self) {
        std::mem::swap(&mut self.main, &mut self.ally);
        std::mem::swap(&mut self.main_commander, &mut self.ally_commander);
        std::mem::swap(&mut self.main_apm, &mut self.ally_apm);
        std::mem::swap(&mut self.mainkills, &mut self.allykills);
        std::mem::swap(
            &mut self.main_commander_level,
            &mut self.ally_commander_level,
        );
        std::mem::swap(&mut self.main_masteries, &mut self.ally_masteries);
        std::mem::swap(&mut self.main_units, &mut self.ally_units);
        std::mem::swap(&mut self.main_icons, &mut self.ally_icons);
        std::mem::swap(&mut self.main_prestige, &mut self.ally_prestige);
        swap_replay_data_record_sides(&mut self.player_stats);
    }
}

#[allow(dead_code)]
fn default_overlay_placement() -> OverlayPlacement {
    OverlayPlacement {
        monitor: 1,
        width: 0.7,
        height: 1.0,
        top_offset: 0,
        right_offset: 0,
        subtract_height: 1,
    }
}

fn default_runtime_flags() -> RuntimeFlags {
    RuntimeFlags {
        start_minimized: false,
        minimize_to_tray: true,
        auto_update: true,
    }
}

fn overlay_placement_from_settings(settings: &AppSettings) -> OverlayPlacement {
    let defaults = default_overlay_placement();
    let monitor = settings.monitor.max(1);

    OverlayPlacement {
        monitor,
        width: defaults.width,
        height: defaults.height,
        top_offset: defaults.top_offset,
        right_offset: defaults.right_offset,
        subtract_height: defaults.subtract_height,
    }
}

pub fn overlay_window_bounds_for_monitor(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    width_ratio: f64,
    height_ratio: f64,
    top_offset: i32,
    right_offset: i32,
    subtract_height: i32,
) -> (tauri::PhysicalSize<u32>, tauri::PhysicalPosition<i32>) {
    let mut target_width = (monitor_width as f64 * width_ratio).max(1.0) as i64;
    let mut target_height =
        (monitor_height as f64 * height_ratio) as i64 - i64::from(subtract_height);

    if target_width > i64::from(monitor_width) {
        target_width = i64::from(monitor_width);
    }
    if target_height > i64::from(monitor_height) {
        target_height = i64::from(monitor_height);
    }
    target_width = target_width.max(1);
    target_height = target_height.max(1);

    let size = tauri::PhysicalSize {
        width: u32::try_from(target_width).unwrap_or(1),
        height: u32::try_from(target_height).unwrap_or(1),
    };
    let position = tauri::PhysicalPosition {
        x: monitor_x
            + i32::try_from(monitor_width.saturating_sub(size.width)).unwrap_or(0)
            + right_offset,
        y: monitor_y + top_offset,
    };
    (size, position)
}

pub fn parse_runtime_flags() -> RuntimeFlags {
    let settings = crate::read_settings_file();
    let minimize_to_tray = settings.minimize_to_tray;
    let start_minimized = if minimize_to_tray {
        settings.start_minimized
    } else {
        false
    };
    let auto_update = settings.auto_update;

    RuntimeFlags {
        start_minimized,
        minimize_to_tray,
        auto_update,
    }
}

pub(crate) fn apply_overlay_placement(window: &tauri::WebviewWindow) -> Result<(), String> {
    apply_overlay_placement_from_settings(window, &crate::read_settings_file())
}

pub(crate) fn apply_overlay_placement_from_settings(
    window: &tauri::WebviewWindow,
    settings_value: &AppSettings,
) -> Result<(), String> {
    let settings = overlay_placement_from_settings(settings_value);
    let monitor_index = if settings.monitor == 0 {
        0
    } else {
        settings.monitor - 1
    };
    let monitors = monitor_descriptors(window);
    if monitors.is_empty() {
        return Err("No monitors detected".into());
    }

    let selected = if monitor_index < monitors.len() {
        &monitors[monitor_index]
    } else {
        &monitors[monitors.len().saturating_sub(1)]
    };
    let (size, final_position) = overlay_window_bounds_for_monitor(
        selected.position_x,
        selected.position_y,
        selected.width,
        selected.height,
        settings.width,
        settings.height,
        settings.top_offset,
        settings.right_offset,
        settings.subtract_height,
    );
    let provisional_position = tauri::PhysicalPosition {
        x: selected.position_x,
        y: selected.position_y,
    };

    window
        .set_position(provisional_position)
        .map_err(|error| format!("Failed to move overlay to target monitor: {error}"))?;
    window
        .set_size(size)
        .map_err(|error| format!("Failed to set overlay size: {error}"))?;

    window
        .set_position(final_position)
        .map_err(|error| format!("Failed to set overlay position: {error}"))?;
    Ok(())
}

pub(crate) fn available_monitor_catalog<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Vec<MonitorOption> {
    let window = app
        .get_webview_window("config")
        .or_else(|| app.get_webview_window("overlay"))
        .or_else(|| app.get_webview_window("performance"));
    let Some(window) = window else {
        return Vec::new();
    };

    monitor_descriptors(&window)
        .into_iter()
        .enumerate()
        .map(|(idx, monitor)| {
            let index = idx + 1;
            MonitorOption {
                index,
                label: format!("{index} - {}", monitor.name),
            }
        })
        .collect()
}

fn monitor_descriptors<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Vec<MonitorDescriptor> {
    let display_info_monitors = display_info_monitors();
    if !display_info_monitors.is_empty() {
        return display_info_monitors;
    }

    let mut monitors = window.available_monitors().unwrap_or_default();
    monitors.sort_by(|left, right| {
        let left_pos = left.position();
        let right_pos = right.position();
        left_pos
            .x
            .cmp(&right_pos.x)
            .then(left_pos.y.cmp(&right_pos.y))
    });
    monitors
        .into_iter()
        .enumerate()
        .map(|(idx, monitor)| {
            let name = monitor
                .name()
                .map(|value| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("Monitor {}", idx + 1));
            MonitorDescriptor {
                name,
                position_x: monitor.position().x,
                position_y: monitor.position().y,
                width: monitor.size().width,
                height: monitor.size().height,
            }
        })
        .collect()
}

fn display_info_monitors() -> Vec<MonitorDescriptor> {
    let mut monitors = DisplayInfo::all()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(idx, display)| {
            let friendly_name = display.friendly_name.trim();
            let technical_name = display.name.trim();
            let name = if !friendly_name.is_empty() {
                friendly_name.to_string()
            } else if !technical_name.is_empty() {
                technical_name.to_string()
            } else {
                format!("Monitor {}", idx + 1)
            };

            MonitorDescriptor {
                name,
                position_x: display.x,
                position_y: display.y,
                width: display.width.max(1),
                height: display.height.max(1),
            }
        })
        .collect::<Vec<_>>();

    monitors.sort_by(|left, right| {
        left.position_x
            .cmp(&right.position_x)
            .then(left.position_y.cmp(&right.position_y))
            .then(left.name.cmp(&right.name))
    });
    monitors
}

fn is_valid_hotkey(shortcut: &str) -> bool {
    Shortcut::from_str(shortcut).is_ok()
}

pub fn normalize_hotkey(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let normalized: String = raw
        .chars()
        .filter(|value| !value.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();

    let mut blocked = false;
    let canonical = normalized
        .split('+')
        .filter(|token| !token.is_empty())
        .filter_map(|token| {
            let normalized_token = match token {
                "backspace" | "delete" => {
                    blocked = true;
                    return None;
                }
                "control" => "control",
                "ctrl" => "control",
                "shift" => "shift",
                "alt" => "alt",
                "meta" => "super",
                "super" => "super",
                "cmd" => "super",
                "command" => "super",
                "win" => "super",
                "windows" => "super",
                "commandorcontrol" | "commandorctrl" | "cmdorcontrol" | "cmdorctrl" => {
                    #[cfg(target_os = "macos")]
                    {
                        "super"
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        "control"
                    }
                }
                "!" => "1",
                "@" => "2",
                "#" => "3",
                "$" => "4",
                "%" => "5",
                "^" => "6",
                "&" => "7",
                "*" => "8",
                "(" => "9",
                ")" => "0",
                "_" => "-",
                "plus" => "=",
                "+" => "=",
                "asterisk" => "8",
                "{" => "[",
                "}" => "]",
                "|" => "\\",
                ":" => ";",
                "\"" => "'",
                "<" => ",",
                ">" => ".",
                "?" => "/",
                "~" => "`",
                other => other,
            };
            Some(normalized_token)
        })
        .collect::<Vec<&str>>()
        .join("+");

    if blocked {
        crate::sco_log!("[SCO/hotkey] Backspace/Delete cannot be used as global hotkey");
        return None;
    }

    if is_valid_hotkey(&canonical) {
        return Some(canonical);
    }

    crate::sco_log!("[SCO/hotkey] Ignoring invalid hotkey '{raw}'");
    None
}

fn resolved_overlay_hotkey_bindings_from_settings(
    settings_value: &AppSettings,
) -> Vec<ResolvedHotkeyBinding> {
    let mut bindings = Vec::new();

    for (path, action) in OVERLAY_HOTKEY_BINDINGS {
        let configured = crate::settings_field_value(settings_value, path);
        let using_default =
            configured.is_none() || matches!(configured.as_ref(), Some(Value::Null));
        let shortcut = match configured.as_ref() {
            None => OVERLAY_HOTKEY_DEFAULTS
                .iter()
                .find(|(default_path, _)| *default_path == path)
                .and_then(|(_, default_value)| normalize_hotkey(default_value)),
            Some(Value::Null) => OVERLAY_HOTKEY_DEFAULTS
                .iter()
                .find(|(default_path, _)| *default_path == path)
                .and_then(|(_, default_value)| normalize_hotkey(default_value)),
            Some(Value::Bool(false)) => {
                crate::sco_log!("[SCO/hotkey] '{path}' disabled by settings.");
                None
            }
            Some(Value::Bool(true)) => {
                crate::sco_log!("[SCO/hotkey] '{path}' has invalid non-string binding, skipping.");
                None
            }
            Some(Value::String(raw)) => {
                let raw = raw.trim();
                if raw.is_empty() {
                    crate::sco_log!("[SCO/hotkey] '{path}' is empty, disabled by settings.");
                    None
                } else {
                    normalize_hotkey(raw)
                }
            }
            Some(_) => {
                crate::sco_log!("[SCO/hotkey] '{path}' has invalid binding type, skipping.");
                None
            }
        };

        let Some(shortcut) = shortcut else {
            if using_default {
                crate::sco_log!("[SCO/hotkey] Missing binding for '{path}', skipping.");
            }
            continue;
        };

        let parsed = match Shortcut::from_str(&shortcut) {
            Ok(parsed) => parsed,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/hotkey] Failed to parse hotkey '{shortcut}' for '{path}': {error}"
                );
                continue;
            }
        };

        if using_default {
            crate::sco_log!("[SCO/hotkey] Falling back to default for '{path}'.");
        }

        bindings.push(ResolvedHotkeyBinding {
            path,
            action,
            shortcut,
            canonical: parsed.to_string().to_ascii_lowercase(),
        });
    }

    bindings
}

fn resolved_overlay_hotkey_bindings() -> Vec<ResolvedHotkeyBinding> {
    resolved_overlay_hotkey_bindings_from_settings(&crate::read_settings_file())
}

pub fn resolve_hotkey_binding_for_reassign_end(
    settings_value: &AppSettings,
    path: &str,
    fallback_binding: Option<&ResolvedHotkeyBinding>,
) -> Option<ResolvedHotkeyBinding> {
    let bindings = resolved_overlay_hotkey_bindings_from_settings(settings_value);
    if let Some(binding) = bindings.into_iter().find(|binding| binding.path == path) {
        return Some(binding);
    }

    let configured_value = crate::settings_field_value(settings_value, path);
    let explicitly_disabled = if !crate::settings_has_explicit_key(settings_value, path) {
        false
    } else {
        match configured_value.as_ref() {
            Some(Value::Null) => false,
            Some(Value::Bool(false)) => true,
            Some(Value::String(raw)) => raw.trim().is_empty(),
            _ => false,
        }
    };
    if explicitly_disabled {
        return None;
    }

    fallback_binding
        .filter(|binding| binding.path == path)
        .cloned()
}

fn register_shortcut_action(
    app_handle: &tauri::AppHandle<Wry>,
    shortcut: &Shortcut,
    action: &'static str,
    event_state: ShortcutState,
) {
    if event_state != ShortcutState::Pressed {
        return;
    }

    let pressed = shortcut.into_string().to_ascii_lowercase();
    crate::sco_log!("[SCO/hotkey] Triggered shortcut '{pressed}' => '{action}'");

    match action {
        "overlay_newer" | "overlay_older" | "overlay_player_info" => {
            if HOTKEY_ACTION_INFLIGHT
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                crate::sco_log!(
                    "[SCO/hotkey] Ignoring '{pressed}' because another hotkey action is running"
                );
                return;
            }
            let action_name = action.to_string();
            let app_handle = app_handle.clone();
            thread::spawn(move || {
                let state = app_handle.state::<BackendState>();
                let _ = perform_overlay_action(&app_handle, &state, &action_name, None);
                HOTKEY_ACTION_INFLIGHT.store(false, Ordering::Release);
            });
        }
        _ => {
            let state = app_handle.state::<BackendState>();
            let _ = perform_overlay_action(app_handle, &state, action, None);
        }
    }
}

fn register_hotkey_binding(
    app: &tauri::AppHandle<Wry>,
    binding: &ResolvedHotkeyBinding,
) -> Result<(), String> {
    let parsed = Shortcut::from_str(&binding.shortcut)
        .map_err(|error| format!("Failed to parse hotkey '{}': {error}", binding.shortcut))?;
    let action = binding.action;
    app.global_shortcut()
        .on_shortcut(parsed, move |app_handle, shortcut, event| {
            register_shortcut_action(app_handle, shortcut, action, event.state);
        })
        .map_err(|error| format!("Failed to register hotkey '{}': {error}", binding.shortcut))
}

fn unregister_hotkey_binding(
    app: &tauri::AppHandle<Wry>,
    binding: &ResolvedHotkeyBinding,
) -> Result<(), String> {
    let parsed = Shortcut::from_str(&binding.shortcut)
        .map_err(|error| format!("Failed to parse hotkey '{}': {error}", binding.shortcut))?;
    if !app.global_shortcut().is_registered(parsed) {
        return Ok(());
    }
    app.global_shortcut().unregister(parsed).map_err(|error| {
        format!(
            "Failed to unregister hotkey '{}': {error}",
            binding.shortcut
        )
    })
}

pub(crate) fn register_overlay_hotkeys(app: &tauri::AppHandle<Wry>) -> Result<(), String> {
    let _ = app.global_shortcut().unregister_all();

    let active_reassign_path = active_hotkey_reassign_path();
    let mut registered: HashMap<String, &'static str> = HashMap::new();
    let mut registered_count = 0usize;

    for binding in resolved_overlay_hotkey_bindings() {
        if active_reassign_path.as_deref() == Some(binding.path) {
            crate::sco_log!(
                "[SCO/hotkey] Skipping '{}' because it is currently being reassigned",
                binding.path
            );
            continue;
        }
        if let Some(existing_action) = registered.get(&binding.canonical) {
            if *existing_action == binding.action {
                crate::sco_log!(
                    "[SCO/hotkey] Duplicate hotkey '{}' for '{}' ignored.",
                    binding.canonical,
                    binding.action
                );
            } else {
                crate::sco_log!(
                    "[SCO/hotkey] Hotkey '{}' already bound to '{}', skipping '{}'.",
                    binding.canonical,
                    existing_action,
                    binding.action
                );
            }
            continue;
        }
        crate::sco_log!(
            "[SCO/hotkey] Registering '{}' for '{}'",
            binding.shortcut,
            binding.action
        );
        register_hotkey_binding(app, &binding)?;
        registered.insert(binding.canonical.clone(), binding.action);
        registered_count += 1;
    }

    if registered_count == 0 {
        crate::sco_log!("[SCO/hotkey] No overlay hotkeys configured.");
    }

    Ok(())
}

pub(crate) fn begin_hotkey_reassign(app: &tauri::AppHandle<Wry>, path: &str) -> Result<(), String> {
    if let Some(previous_path) = active_hotkey_reassign_path() {
        if previous_path != path {
            end_hotkey_reassign(app, &previous_path)?;
        }
    }

    set_active_hotkey_reassign_path(Some(path.to_string()));
    let binding = resolved_overlay_hotkey_bindings()
        .into_iter()
        .find(|binding| binding.path == path);
    set_active_hotkey_reassign_binding(binding.clone());

    if let Some(binding) = binding {
        unregister_hotkey_binding(app, &binding)?;
        crate::sco_log!(
            "[SCO/hotkey] Removed hotkey trigger for '{}' while it is being reassigned",
            path
        );
    }

    Ok(())
}

pub(crate) fn end_hotkey_reassign(app: &tauri::AppHandle<Wry>, path: &str) -> Result<(), String> {
    if active_hotkey_reassign_path().as_deref() == Some(path) {
        set_active_hotkey_reassign_path(None);
    }

    let settings_value = crate::read_settings_file();
    let fallback_binding = active_hotkey_reassign_binding();
    let Some(binding) =
        resolve_hotkey_binding_for_reassign_end(&settings_value, path, fallback_binding.as_ref())
    else {
        set_active_hotkey_reassign_binding(None);
        crate::sco_log!("[SCO/hotkey] '{path}' has no active binding after reassignment");
        return Ok(());
    };

    let bindings = resolved_overlay_hotkey_bindings_from_settings(&settings_value);
    if bindings
        .iter()
        .any(|other| other.path != binding.path && other.canonical == binding.canonical)
    {
        set_active_hotkey_reassign_binding(None);
        crate::sco_log!(
            "[SCO/hotkey] Hotkey '{}' conflicts with another binding, skipping '{}'.",
            binding.canonical,
            binding.path
        );
        return Ok(());
    }

    register_hotkey_binding(app, &binding)?;
    set_active_hotkey_reassign_binding(None);
    crate::sco_log!(
        "[SCO/hotkey] Recreated hotkey trigger for '{}' as '{}'",
        path,
        binding.shortcut
    );
    Ok(())
}

pub fn overlay_payload_from_replay(
    replay: &crate::ReplayInfo,
    mark_new_replay: bool,
    show_session: bool,
    session_victories: u64,
    session_defeats: u64,
) -> OverlayReplayPayload {
    let main_names = configured_main_names();
    let main_handles = configured_main_handles();
    let settings = crate::read_settings_file();
    let language = overlay_language_from_settings(&settings);
    let mut payload = OverlayReplayPayload::from_replay(replay, language);
    if replay_should_swap_main_and_ally(replay, &main_names, &main_handles) {
        payload.swap_sides();
    }
    if show_session {
        payload.victory = Some(as_u32(session_victories));
        payload.defeat = Some(as_u32(session_defeats));
    }
    payload.new_replay = mark_new_replay.then_some(true);
    payload
}

fn emit_overlay_replay_payload(app: &tauri::AppHandle<Wry>, payload: &OverlayReplayPayload) {
    sync_overlay_runtime_settings(app);
    let _ = app.emit(OVERLAY_REPLAY_PAYLOAD_EVENT, payload);
    show_overlay_window(app);
}

pub(crate) fn emit_replay_to_overlay_from_replay(
    app: &tauri::AppHandle<Wry>,
    replay: &crate::ReplayInfo,
    mark_new_replay: bool,
) {
    let state = app.state::<BackendState>();

    let replay = (!replay.is_detailed)
        .then(|| process_replay_detailed(&state, &PathBuf::from(&replay.file)).1)
        .flatten()
        .unwrap_or_else(|| replay.clone());

    let settings = crate::read_settings_file();
    let show_session = settings.show_session;
    let (session_victories, session_defeats) = state.session_counts();
    let payload = overlay_payload_from_replay(
        &replay,
        mark_new_replay,
        show_session,
        session_victories,
        session_defeats,
    );
    emit_overlay_replay_payload(app, &payload);
}

pub fn replay_for_display<'a>(
    replays: &'a [crate::ReplayInfo],
    requested: Option<&str>,
    selected: &Option<String>,
) -> Option<&'a crate::ReplayInfo> {
    requested
        .and_then(|requested_file| replays.iter().find(|replay| replay.file == requested_file))
        .or_else(|| {
            requested.and_then(|requested_file| {
                Path::new(requested_file).file_name().and_then(|name| {
                    let file_name = name.to_string_lossy();
                    replays.iter().find(|replay| {
                        Path::new(&replay.file)
                            .file_name()
                            .is_some_and(|current| current == file_name.as_ref())
                    })
                })
            })
        })
        .or_else(|| {
            selected
                .as_deref()
                .and_then(|current| replays.iter().find(|replay| replay.file == current))
        })
        .or_else(|| replays.first())
}

pub fn replay_move_target_index(
    replays: &[crate::ReplayInfo],
    selected: &Option<String>,
    delta: i64,
    replay_data_active: bool,
) -> usize {
    if replays.is_empty() || !replay_data_active {
        return 0;
    }

    let mut index = replay_index_by_file(replays, selected).unwrap_or(0);
    if delta > 0 {
        index = index.saturating_sub(delta as usize);
    } else if delta < 0 {
        let steps = delta.wrapping_abs() as usize;
        index = (index + steps).min(replays.len().saturating_sub(1));
    }

    index
}

pub fn replay_move_should_be_ignored(
    current_index: Option<usize>,
    target_index: usize,
    replay_data_active: bool,
) -> bool {
    replay_data_active && current_index.is_some_and(|index| index == target_index)
}

pub(crate) fn replay_show_for_window(
    app: &tauri::AppHandle<Wry>,
    state: &BackendState,
    requested: Option<&str>,
) -> Value {
    let replays = state.sync_replay_cache_slots(UNLIMITED_REPLAY_LIMIT);
    let selected = state.get_current_replay_file();
    let Some(replay) = replay_for_display(&replays, requested, &selected) else {
        return serde_json::to_value(crate::OverlayActionResponse::failure("No replay selected"))
            .unwrap_or_else(|_| Value::Object(Default::default()));
    };
    let file = replay.file.clone();

    emit_replay_to_overlay_from_replay(app, replay, false);
    state
        .overlay_replay_data_active
        .store(true, Ordering::Release);
    state.set_current_replay_file(Some(&file));

    serde_json::to_value(crate::OverlayActionResponse::success("Replay shown"))
        .unwrap_or_else(|_| Value::Object(Default::default()))
}

pub(crate) fn replay_move_window(
    app: &tauri::AppHandle<Wry>,
    state: &BackendState,
    delta: i64,
) -> Value {
    let cached = state.replay_cache_snapshot();

    let replays = if cached.is_empty() {
        state.sync_replay_cache_slots(UNLIMITED_REPLAY_LIMIT)
    } else {
        cached
    };

    if replays.is_empty() {
        return serde_json::to_value(crate::OverlayActionResponse::failure(
            "No replays available",
        ))
        .unwrap_or_else(|_| Value::Object(Default::default()));
    }

    let selected = state.get_current_replay_file();
    let replay_data_active = state.overlay_replay_data_active.load(Ordering::Acquire);
    let current_index = replay_index_by_file(&replays, &selected);
    let index = replay_move_target_index(&replays, &selected, delta, replay_data_active);
    if replay_move_should_be_ignored(current_index, index, replay_data_active) {
        return serde_json::to_value(crate::OverlayActionResponse::success("Replay move ignored"))
            .unwrap_or_else(|_| Value::Object(Default::default()));
    }

    let replay = &replays[index];
    let file = replay.file.clone();

    emit_replay_to_overlay_from_replay(app, replay, false);
    state
        .overlay_replay_data_active
        .store(true, Ordering::Release);
    state.set_current_replay_file(Some(&file));

    serde_json::to_value(crate::OverlayActionResponse::success("Replay moved"))
        .unwrap_or_else(|_| Value::Object(Default::default()))
}

pub(crate) fn perform_overlay_action(
    app: &tauri::AppHandle<Wry>,
    state: &BackendState,
    action: &str,
    body: Option<&Value>,
) -> Option<Value> {
    match action {
        "overlay_show_hide" => {
            let overlay_visible = app
                .get_webview_window("overlay")
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false);
            if overlay_visible {
                let _ = app.emit(OVERLAY_SHOWHIDE_EVENT, EmptyPayload::default());
            } else {
                show_overlay_window(app);
                let _ = app.emit(OVERLAY_SHOWSTATS_EVENT, EmptyPayload::default());
            }
            Some(
                serde_json::to_value(crate::OverlayActionResponse::success(
                    "Overlay visibility toggled",
                ))
                .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        "overlay_show" => {
            show_overlay_window(app);
            let _ = app.emit(OVERLAY_SHOWSTATS_EVENT, EmptyPayload::default());
            Some(
                serde_json::to_value(crate::OverlayActionResponse::success("Overlay shown"))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        "overlay_hide" => {
            hide_overlay_window(app);
            let _ = app.emit(OVERLAY_HIDESTATS_EVENT, EmptyPayload::default());
            Some(
                serde_json::to_value(crate::OverlayActionResponse::success("Overlay hidden"))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        "overlay_replay_data_state" => {
            let active = body
                .and_then(|payload| payload.get("active"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            state
                .overlay_replay_data_active
                .store(active, Ordering::Release);
            if !active {
                state.set_current_replay_file(None);
            }
            Some(
                serde_json::to_value(crate::OverlayActionResponse::success(if active {
                    "Overlay replay data marked active"
                } else {
                    "Overlay replay data cleared"
                }))
                .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        "overlay_newer" => Some(replay_move_window(app, state, 1)),
        "overlay_older" => Some(replay_move_window(app, state, -1)),
        "overlay_player_info" => {
            let payload = build_overlay_player_info_payload(state);
            let _ = app.emit(OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT, payload);
            show_overlay_window(app);

            Some(
                serde_json::to_value(crate::OverlayActionResponse::success(
                    "Overlay player info toggled",
                ))
                .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        "performance_show_hide" => {
            let performance_visible = app
                .get_webview_window("performance")
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false);
            let next_visible = !performance_visible;
            match crate::performance_overlay::set_visibility(app, next_visible, true) {
                Ok(()) => Some(
                    serde_json::to_value(crate::OverlayActionResponse::success(if next_visible {
                        "Performance overlay shown"
                    } else {
                        "Performance overlay hidden"
                    }))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                ),
                Err(error) => Some(
                    serde_json::to_value(crate::OverlayActionResponse::failure(error))
                        .unwrap_or_else(|_| Value::Object(Default::default())),
                ),
            }
        }
        "performance_toggle_reposition" => {
            let enabled = crate::performance_overlay::toggle_edit_mode(app);
            Some(
                serde_json::to_value(crate::OverlayActionResponse::success(if enabled {
                    "Performance overlay reposition mode enabled"
                } else {
                    "Performance overlay reposition mode disabled"
                }))
                .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        "hotkey_reassign_begin" => {
            let path = body
                .and_then(|payload| payload.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match begin_hotkey_reassign(app, path) {
                Ok(()) => Some(
                    serde_json::to_value(crate::OverlayActionResponse::success_with_path(
                        format!("Removed hotkey trigger for {path}"),
                        path.to_string(),
                    ))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                ),
                Err(error) => Some(
                    serde_json::to_value(crate::OverlayActionResponse::failure_with_path(
                        error,
                        path.to_string(),
                    ))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                ),
            }
        }
        "hotkey_reassign_end" => {
            let path = body
                .and_then(|payload| payload.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match end_hotkey_reassign(app, path) {
                Ok(()) => Some(
                    serde_json::to_value(crate::OverlayActionResponse::success_with_path(
                        format!("Recreated hotkey trigger for {path}"),
                        path.to_string(),
                    ))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                ),
                Err(error) => Some(
                    serde_json::to_value(crate::OverlayActionResponse::failure_with_path(
                        error,
                        path.to_string(),
                    ))
                    .unwrap_or_else(|_| Value::Object(Default::default())),
                ),
            }
        }
        "parse_replay" => {
            let requested = body
                .and_then(|payload| payload.get("file"))
                .and_then(Value::as_str);
            Some(replay_show_for_window(app, state, requested))
        }
        "overlay_screenshot" => Some(
            match request_overlay_screenshot(app) {
                Ok(path) => serde_json::to_value(crate::OverlayActionResponse::success_with_path(
                    format!("Overlay screenshot requested for {path}"),
                    path,
                )),
                Err(error) => serde_json::to_value(crate::OverlayActionResponse::failure(error)),
            }
            .unwrap_or_else(|_| Value::Object(Default::default())),
        ),
        "create_desktop_shortcut" => Some(
            serde_json::to_value(crate::OverlayActionResponse::success(
                "Create desktop shortcut is not available in this build",
            ))
            .unwrap_or_else(|_| Value::Object(Default::default())),
        ),
        "randomizer_generate" => {
            #[derive(Serialize)]
            struct RandomizerGenerateResponse {
                status: &'static str,
                result: crate::OverlayActionResult,
                message: String,
                #[serde(skip_serializing_if = "Option::is_none")]
                randomizer: Option<randomizer::RandomizerResult>,
            }

            Some(
                serde_json::to_value(match randomizer::generate_from_body(body) {
                    Ok(result) => RandomizerGenerateResponse {
                        status: "ok",
                        result: crate::OverlayActionResult {
                            ok: true,
                            path: None,
                        },
                        message: "Generated random commander".to_string(),
                        randomizer: Some(result),
                    },
                    Err(error) => RandomizerGenerateResponse {
                        status: "ok",
                        result: crate::OverlayActionResult {
                            ok: false,
                            path: None,
                        },
                        message: error,
                        randomizer: None,
                    },
                })
                .unwrap_or_else(|_| Value::Object(Default::default())),
            )
        }
        _ => None,
    }
}

fn build_overlay_player_info_payload(state: &BackendState) -> OverlayPlayerInfoPayload {
    let replays = state.sync_replay_cache_slots(UNLIMITED_REPLAY_LIMIT);
    let selected_file = state.get_current_replay_file();

    let selected = selected_file
        .and_then(|file| replays.iter().find(|replay| replay.file == file).cloned())
        .or_else(|| replays.first().cloned());

    let Some(selected) = selected else {
        return OverlayPlayerInfoPayload::default();
    };

    let main_names = configured_main_names();
    let main_handles = configured_main_handles();
    let player_info = select_other_player_from_replay(&selected, &main_names, &main_handles)
        .or_else(|| {
            let ally = selected.p2.trim();
            if !ally.is_empty() {
                Some((selected.p2_handle.to_string(), ally.to_string()))
            } else {
                None
            }
        })
        .or_else(|| {
            let main = selected.p1.trim();
            if !main.is_empty() {
                Some((selected.p1_handle.to_string(), main.to_string()))
            } else {
                None
            }
        });

    let Some((player_handle, player_name)) = player_info else {
        return OverlayPlayerInfoPayload::default();
    };

    build_overlay_player_info_payload_for_player(state, &player_handle, &player_name)
}

fn select_other_player_from_replay(
    replay: &crate::ReplayInfo,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> Option<(String, String)> {
    let p1 = replay.p1.trim();
    let p2 = replay.p2.trim();

    if p1.is_empty() && p2.is_empty() {
        return None;
    }

    let p1_handle = replay.p1_handle.to_string();
    let p2_handle = replay.p2_handle.to_string();

    let p1_is_main = ReplayAnalysis::is_main_player_identity(
        &replay.p1,
        &replay.p1_handle,
        main_names,
        main_handles,
    );
    let p2_is_main = ReplayAnalysis::is_main_player_identity(
        &replay.p2,
        &replay.p2_handle,
        main_names,
        main_handles,
    );

    match (p1_is_main, p2_is_main) {
        (true, false) => (!p2.is_empty()).then_some((p2_handle, p2.to_string())),
        (false, true) => (!p1.is_empty()).then_some((p1_handle, p1.to_string())),
        _ => {
            if !p2.is_empty() {
                Some((p2_handle, p2.to_string()))
            } else if !p1.is_empty() {
                Some((p1_handle, p1.to_string()))
            } else {
                None
            }
        }
    }
}

fn lookup_player_stats_row(
    player_data: &Map<String, Value>,
    player_name: &str,
) -> Option<(String, Map<String, Value>)> {
    if let Some(value) = player_data.get(player_name).and_then(Value::as_object) {
        return Some((player_name.to_string(), value.clone()));
    }

    let player_key = ReplayAnalysis::normalized_player_key(player_name);
    player_data.iter().find_map(|(name, value)| {
        if ReplayAnalysis::normalized_player_key(name) != player_key {
            return None;
        }
        value
            .as_object()
            .map(|entry| (name.to_string(), entry.clone()))
    })
}

pub fn player_note_from_settings_value(
    settings: &AppSettings,
    player_handle: &str,
) -> Option<String> {
    let direct = settings
        .player_notes
        .get(player_handle)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    direct
}

fn player_note_from_settings(player_handle: &str) -> Option<String> {
    let settings = crate::read_settings_file();
    player_note_from_settings_value(&settings, player_handle)
}

fn relative_last_seen_text(last_seen: u64) -> String {
    if last_seen == 0 {
        return String::new();
    }

    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(delta) => delta.as_secs(),
        Err(_) => return String::new(),
    };
    let mut delta = now.saturating_sub(last_seen);

    let years = delta / 31_557_600;
    delta %= 31_557_600;
    let days = delta / 86_400;
    delta %= 86_400;
    let hours = delta / 3_600;
    delta %= 3_600;
    let minutes = delta / 60;

    let mut parts = Vec::<String>::new();
    if years > 0 {
        parts.push(format!("{years} years"));
    }
    if days > 0 {
        parts.push(format!("{days} days"));
    }
    if hours > 0 {
        parts.push(format!("{hours} hours"));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(format!("{minutes} minutes"));
    }
    format!("{} ago", parts.join(" "))
}

fn build_overlay_player_info_payload_for_player(
    state: &BackendState,
    player_handle: &str,
    player_name: &str,
) -> OverlayPlayerInfoPayload {
    let player_data = state
        .stats
        .lock()
        .ok()
        .and_then(|stats| stats.analysis.clone())
        .and_then(|analysis| {
            analysis
                .get("PlayerData")
                .and_then(Value::as_object)
                .map(|value| value.clone())
        });

    let input_name = sanitize_replay_text(player_name);
    let fallback_name = if input_name.trim().is_empty() {
        "Unknown".to_string()
    } else {
        input_name.trim().to_string()
    };

    let mut data = std::collections::BTreeMap::new();
    let resolved = player_data
        .as_ref()
        .and_then(|rows| lookup_player_stats_row(rows, &fallback_name));

    let (display_name, value) = if let Some((resolved_name, row)) = resolved {
        let wins = row.get("wins").and_then(Value::as_u64).unwrap_or(0);
        let losses = row.get("losses").and_then(Value::as_u64).unwrap_or(0);
        let apm = row
            .get("apm")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .round();
        let commander = row
            .get("commander")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let frequency = row.get("frequency").and_then(Value::as_f64).unwrap_or(0.0);
        let kills = row.get("kills").and_then(Value::as_f64).unwrap_or(0.0);
        let last_seen = row.get("last_seen").and_then(Value::as_u64).unwrap_or(0);
        let relative_last_seen = relative_last_seen_text(last_seen);

        let note = player_note_from_settings(player_handle);
        (
            sanitize_replay_text(&resolved_name),
            OverlayPlayerInfoRow::Stats {
                wins: as_u32(wins),
                losses: as_u32(losses),
                apm: as_u32(apm as u64),
                commander: sanitize_replay_text(commander),
                frequency,
                kills,
                last_seen_relative: relative_last_seen,
                note,
            },
        )
    } else {
        let note = player_note_from_settings(&fallback_name);
        (fallback_name, OverlayPlayerInfoRow::NoGames { note })
    };

    data.insert(display_name, value);

    OverlayPlayerInfoPayload { data }
}

pub(crate) fn show_player_winrate_for_name(
    app: &tauri::AppHandle<Wry>,
    state: &BackendState,
    player_handle: &str,
    player_name: &str,
) -> bool {
    if player_name.trim().is_empty() {
        return false;
    }

    let payload = build_overlay_player_info_payload_for_player(state, player_handle, player_name);
    let _ = app.emit(OVERLAY_HIDESTATS_EVENT, EmptyPayload::default());
    let _ = app.emit(OVERLAY_PLAYER_WINRATE_EVENT, payload);
    show_overlay_window(app);
    true
}

fn overlay_screenshot_directory_from_settings(settings: &AppSettings) -> Result<PathBuf, String> {
    let folder = settings.screenshot_folder.trim();
    if folder.is_empty() {
        return Err("Screenshot folder is not configured".to_string());
    }
    Ok(PathBuf::from(folder))
}

pub fn overlay_screenshot_output_path_from_settings(
    settings: &AppSettings,
    captured_at: SystemTime,
) -> Result<PathBuf, String> {
    let directory = overlay_screenshot_directory_from_settings(settings)?;
    let timestamp = captured_at
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("Failed to build screenshot timestamp: {error}"))?
        .as_secs();
    Ok(directory.join(format!("overlay-{timestamp}.png")))
}

fn overlay_screenshot_output_path() -> Result<PathBuf, String> {
    overlay_screenshot_output_path_from_settings(&crate::read_settings_file(), SystemTime::now())
}

fn request_overlay_screenshot(app: &tauri::AppHandle<Wry>) -> Result<String, String> {
    if app.get_webview_window("overlay").is_none() {
        return Err("Overlay window is not available".to_string());
    }

    let path = overlay_screenshot_output_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "Screenshot folder path is invalid".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create screenshot folder: {error}"))?;
    app.emit(
        OVERLAY_SCREENSHOT_REQUEST_EVENT,
        OverlayScreenshotRequestPayload {
            path: path.display().to_string(),
        },
    )
    .map_err(|error| format!("Failed to request overlay screenshot: {error}"))?;
    Ok(path.display().to_string())
}

fn is_png_signature(bytes: &[u8]) -> bool {
    const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    bytes.starts_with(&PNG_SIGNATURE)
}

pub fn save_overlay_screenshot(path: &Path, png_bytes: &[u8]) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("png"))
        .unwrap_or(false);
    if !extension {
        return Err("Overlay screenshot path must end with .png".to_string());
    }
    if !is_png_signature(png_bytes) {
        return Err("Overlay screenshot data is not a PNG image".to_string());
    }

    let parent = path
        .parent()
        .ok_or_else(|| "Screenshot folder path is invalid".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create screenshot folder: {error}"))?;
    std::fs::write(path, png_bytes)
        .map_err(|error| format!("Failed to write screenshot file: {error}"))
}

pub(crate) fn reveal_file_in_explorer(file: &str) -> Result<(), String> {
    let original_path = Path::new(file);
    if !original_path.exists() {
        return Err("Replay file does not exist".to_string());
    }

    if cfg!(target_os = "windows") {
        let mut windows_path = original_path.to_string_lossy().replace('/', "\\");
        if let Some(stripped) = windows_path.strip_prefix(r"\\?\") {
            windows_path = stripped.to_string();
        }

        Command::new("explorer")
            .arg("/select,")
            .arg(&windows_path)
            .spawn()
            .map_err(|error| format!("failed to launch explorer: {error}"))?;
        return Ok(());
    }

    let path = original_path
        .canonicalize()
        .unwrap_or_else(|_| original_path.to_path_buf());

    if cfg!(target_os = "macos") {
        Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("failed to launch finder: {error}"))?;
        return Ok(());
    }

    if cfg!(target_family = "unix") {
        let uri_path = path.to_string_lossy().replace(' ', "%20");
        let file_uri = format!("file://{uri_path}");

        let dbus_status = Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("array:string:\"{file_uri}\""))
            .arg("string:\"\"")
            .status();
        if dbus_status.map(|status| status.success()).unwrap_or(false) {
            return Ok(());
        }

        if let Some(parent) = path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|error| format!("failed to launch file browser: {error}"))?;
            return Ok(());
        }

        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("failed to launch file browser: {error}"))?;
        return Ok(());
    }

    Err("File reveal is not supported on this platform".to_string())
}

fn existing_folder_path(folder: &str) -> Result<PathBuf, String> {
    let trimmed = folder.trim();
    if trimmed.is_empty() {
        return Err("Folder path is empty".to_string());
    }

    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err("Folder does not exist".to_string());
    }
    if !path.is_dir() {
        return Err("Path is not a folder".to_string());
    }

    Ok(path)
}

pub fn open_folder_in_explorer(folder: &str) -> Result<(), String> {
    let path = existing_folder_path(folder)?;

    if cfg!(target_os = "windows") {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("failed to launch explorer: {error}"))?;
        return Ok(());
    }

    if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("failed to launch finder: {error}"))?;
        return Ok(());
    }

    if cfg!(target_family = "unix") {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|error| format!("failed to launch file browser: {error}"))?;
        return Ok(());
    }

    Err("Folder opening is not supported on this platform".to_string())
}

fn overlay_setting_string(settings: &AppSettings, key: &str) -> Option<String> {
    crate::settings_field_value(settings, key)
        .and_then(|value| value.as_str().map(ToString::to_string))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn overlay_duration_from_settings(settings: &AppSettings) -> u32 {
    settings.duration.max(1)
}

fn overlay_show_charts_from_settings(settings: &AppSettings) -> bool {
    settings.show_charts
}

fn overlay_show_session_from_settings(settings: &AppSettings) -> bool {
    settings.show_session
}

fn overlay_language_from_settings(settings: &AppSettings) -> &'static str {
    match settings.language.as_str() {
        "ko" => "ko",
        _ => "en",
    }
}

pub fn overlay_runtime_settings_payload(
    settings: &AppSettings,
    session_victories: u64,
    session_defeats: u64,
) -> Value {
    serde_json::to_value(OverlayInitColorsDurationPayload {
        colors: [
            overlay_setting_string(settings, "color_player1"),
            overlay_setting_string(settings, "color_player2"),
            overlay_setting_string(settings, "color_amon"),
            overlay_setting_string(settings, "color_mastery"),
        ],
        duration: overlay_duration_from_settings(settings),
        show_charts: overlay_show_charts_from_settings(settings),
        show_session: overlay_show_session_from_settings(settings),
        session_victory: as_u32(session_victories),
        session_defeat: as_u32(session_defeats),
        language: overlay_language_from_settings(settings).to_string(),
    })
    .unwrap_or_else(|_| Value::Object(Default::default()))
}

pub(crate) fn sync_overlay_runtime_settings<R: Runtime>(app: &tauri::AppHandle<R>) {
    let settings = crate::read_settings_file();
    let state = app.state::<crate::BackendState>();
    let (session_victories, session_defeats) = state.session_counts();
    let payload = overlay_runtime_settings_payload(&settings, session_victories, session_defeats);
    let _ = app.emit(OVERLAY_INIT_COLORS_DURATION_EVENT, payload);
}

pub(crate) fn show_overlay_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    sync_overlay_runtime_settings(app);
    if let Some(overlay_window) = app.get_webview_window("overlay") {
        let _ = overlay_window.set_focusable(false);
        let _ = overlay_window.show();
    }
}

pub(crate) fn hide_overlay_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(overlay_window) = app.get_webview_window("overlay") {
        let _ = overlay_window.hide();
    }
}

pub(crate) fn show_config_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(config_window) = app.get_webview_window("config") {
        let _ = config_window.show();
        let _ = config_window.set_focus();
    }
}

pub(crate) fn build_tray_menu<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<tauri::menu::Menu<R>> {
    let show_item = MenuItem::with_id(
        app,
        MENU_ITEM_SHOW_CONFIG,
        "Show Config",
        true,
        None::<&str>,
    )
    .inspect_err(|error| {
        crate::sco_log!("Failed to create tray menu item '{MENU_ITEM_SHOW_CONFIG}': {error}");
    })
    .ok()?;

    let show_overlay_item = MenuItem::with_id(
        app,
        MENU_ITEM_SHOW_OVERLAY,
        "Show Overlay",
        true,
        None::<&str>,
    )
    .inspect_err(|error| {
        crate::sco_log!("Failed to create tray menu item '{MENU_ITEM_SHOW_OVERLAY}': {error}");
    })
    .ok()?;

    let quit_item = MenuItem::with_id(app, MENU_ITEM_QUIT, "Quit", true, None::<&str>)
        .inspect_err(|error| {
            crate::sco_log!("Failed to create tray menu item '{MENU_ITEM_QUIT}': {error}");
        })
        .ok()?;

    MenuBuilder::new(app)
        .items(&[&show_item, &show_overlay_item, &quit_item])
        .build()
        .ok()
}
