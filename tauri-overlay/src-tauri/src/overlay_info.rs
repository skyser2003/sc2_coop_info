use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::thread;
use std::time::SystemTime;

use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde_json::Value;
use tauri::{
    menu::{MenuBuilder, MenuItem},
    Emitter, Manager, Runtime, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::app_settings::AppSettings;
use crate::monitor_settings;
use crate::randomizer;
use crate::shared_types::{
    EmptyPayload, OverlayReplayPayload, OverlayScreenshotRequestPayload, ReplayDataRecord,
    ReplayPlayerSeries, SharedTypesOps,
};
use crate::{BackendState, TauriOverlayOps, UNLIMITED_REPLAY_LIMIT};

pub(crate) const MENU_ITEM_SHOW_CONFIG: &str = "show_config";
pub(crate) const MENU_ITEM_SHOW_OVERLAY: &str = "show_overlay";
pub(crate) const MENU_ITEM_QUIT: &str = "quit";

pub(crate) const OVERLAY_REPLAY_PAYLOAD_EVENT: &str = "sco://overlay-replay-payload";
pub(crate) const OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT: &str =
    "sco://overlay-show-hide-player-stats";
pub(crate) const OVERLAY_PLAYER_STATS_EVENT: &str = "sco://overlay-player-stats";
pub(crate) const OVERLAY_INIT_COLORS_DURATION_EVENT: &str = "sco://overlay-init-colors-duration";
pub(crate) const OVERLAY_SHOWSTATS_EVENT: &str = "sco://overlay-showstats";
pub(crate) const OVERLAY_HIDESTATS_EVENT: &str = "sco://overlay-hidestats";
pub(crate) const OVERLAY_SHOWHIDE_EVENT: &str = "sco://overlay-showhide";
pub(crate) const OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT: &str =
    "sco://overlay-set-show-charts-from-config";
pub(crate) const OVERLAY_SCREENSHOT_REQUEST_EVENT: &str = "sco://overlay-screenshot-request";

pub(crate) const OVERLAY_HOTKEY_DEFAULTS: [(&str, &str); 6] = [
    ("hotkey_show/hide", "Ctrl+Shift+*"),
    ("hotkey_show", ""),
    ("hotkey_hide", ""),
    ("hotkey_newer", "Ctrl+Alt+/"),
    ("hotkey_older", "Ctrl+Alt+*"),
    ("hotkey_winrates", "Ctrl+Alt+-"),
];

pub(crate) const OVERLAY_HOTKEY_BINDINGS: [(&str, &str); 7] = [
    ("hotkey_show/hide", "overlay_show_hide"),
    ("hotkey_show", "overlay_show"),
    ("hotkey_hide", "overlay_hide"),
    ("hotkey_newer", "overlay_newer"),
    ("hotkey_older", "overlay_older"),
    ("hotkey_winrates", "overlay_player_stats"),
    ("performance_hotkey", "performance_show_hide"),
];

pub struct OverlayInfoOps;

impl OverlayInfoOps {
    fn as_u32(value: u64) -> u32 {
        u32::try_from(value).unwrap_or(u32::MAX)
    }
}

impl OverlayInfoOps {
    fn as_u32_vec(values: &[u64]) -> Vec<u32> {
        values.iter().copied().map(OverlayInfoOps::as_u32).collect()
    }
}

impl OverlayInfoOps {
    fn overlay_mutator_name_with_dictionary(
        mutator_id: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        let canonical = if dictionary.mutator_data(mutator_id).is_some() {
            mutator_id.to_string()
        } else if let Some(mapped) = dictionary.mutator_id_from_name(mutator_id) {
            mapped.to_string()
        } else {
            mutator_id.to_string()
        };

        dictionary
            .mutator_data(&canonical)
            .map(|value| value.name.en.trim())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                dictionary
                    .mutator_ids
                    .get(&canonical)
                    .map(|value| value.to_string())
            })
            .unwrap_or_default()
    }
}

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
    start_minimized: bool,
    minimize_to_tray: bool,
    auto_update: bool,
}

#[derive(Clone)]
pub struct ResolvedHotkeyBinding {
    path: &'static str,
    action: &'static str,
    shortcut: String,
    canonical: String,
}

impl OverlayPlacement {
    pub(crate) fn new(
        monitor: usize,
        width: f64,
        height: f64,
        top_offset: i32,
        right_offset: i32,
        subtract_height: i32,
    ) -> Self {
        Self {
            monitor,
            width,
            height,
            top_offset,
            right_offset,
            subtract_height,
        }
    }

    pub(crate) fn monitor(&self) -> usize {
        self.monitor
    }

    pub(crate) fn width(&self) -> f64 {
        self.width
    }

    pub(crate) fn height(&self) -> f64 {
        self.height
    }

    pub(crate) fn top_offset(&self) -> i32 {
        self.top_offset
    }

    pub(crate) fn right_offset(&self) -> i32 {
        self.right_offset
    }

    pub(crate) fn subtract_height(&self) -> i32 {
        self.subtract_height
    }
}

impl RuntimeFlags {
    pub fn new(start_minimized: bool, minimize_to_tray: bool, auto_update: bool) -> Self {
        Self {
            start_minimized,
            minimize_to_tray,
            auto_update,
        }
    }

    pub fn start_minimized(&self) -> bool {
        self.start_minimized
    }

    pub fn minimize_to_tray(&self) -> bool {
        self.minimize_to_tray
    }

    pub fn auto_update(&self) -> bool {
        self.auto_update
    }
}

impl ResolvedHotkeyBinding {
    pub fn new(
        path: &'static str,
        action: &'static str,
        shortcut: impl Into<String>,
        canonical: impl Into<String>,
    ) -> Self {
        Self {
            path,
            action,
            shortcut: shortcut.into(),
            canonical: canonical.into(),
        }
    }

    pub fn path(&self) -> &'static str {
        self.path
    }

    pub fn action(&self) -> &'static str {
        self.action
    }

    pub fn shortcut(&self) -> &str {
        &self.shortcut
    }

    pub fn canonical(&self) -> &str {
        &self.canonical
    }
}

impl OverlayInfoOps {
    fn selected_monitor_from_settings<R: Runtime>(
        window: &tauri::WebviewWindow<R>,
        settings_value: &AppSettings,
    ) -> Result<monitor_settings::MonitorDescriptor, String> {
        monitor_settings::MonitorSettingsOps::selected_monitor_for_window(
            window,
            settings_value.overlay_placement().monitor(),
        )
    }
}

impl OverlayReplayPayload {
    pub fn localized_prestige_text_with_dictionary(
        commander: &str,
        prestige: u64,
        language: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        if prestige == 0 {
            return String::new();
        }

        let commander = TauriOverlayOps::sanitize_replay_text(commander);
        let Some(index) = usize::try_from(prestige).ok() else {
            return format!("P{prestige}");
        };
        if let Some(lookup) = dictionary
            .prestige_names_json
            .get(&commander)
            .and_then(|value| match language {
                "ko" => value.ko.get(index).or_else(|| value.en.get(index)),
                _ => value.en.get(index),
            })
            .map(String::as_str)
        {
            return lookup.to_string();
        }

        if let Some(lookup) = dictionary.prestige_name(&commander, prestige) {
            return lookup.to_string();
        }

        format!("P{prestige}")
    }

    pub fn localized_prestige_text(prestige: u64) -> String {
        if prestige == 0 {
            return String::new();
        }

        format!("P{prestige}")
    }

    fn from_replay_with_dictionary(
        replay: &crate::ReplayInfo,
        language: &str,
        dictionary: &Sc2DictionaryData,
    ) -> Self {
        let sanitized = replay.sanitized_for_client_with_dictionary(dictionary);
        let main_prestige = Self::localized_prestige_text_with_dictionary(
            sanitized.main_commander(),
            sanitized.main_prestige(),
            language,
            dictionary,
        );
        let ally_prestige = Self::localized_prestige_text_with_dictionary(
            sanitized.ally_commander(),
            sanitized.ally_prestige(),
            language,
            dictionary,
        );
        let player_stats = SharedTypesOps::replay_data_record_from_value(&sanitized.player_stats);
        let (main_player_stats, ally_player_stats) =
            OverlayInfoOps::semantic_player_stats_from_record(
                &player_stats,
                &sanitized.main().name,
                &sanitized.ally().name,
            );
        Self {
            file: sanitized.file.clone(),
            map_name: sanitized.map.clone(),
            main: sanitized.main().name.clone(),
            ally: sanitized.ally().name.clone(),
            main_commander: sanitized.main_commander().to_string(),
            ally_commander: sanitized.ally_commander().to_string(),
            main_apm: OverlayInfoOps::as_u32(sanitized.main_apm()),
            ally_apm: OverlayInfoOps::as_u32(sanitized.ally_apm()),
            mainkills: OverlayInfoOps::as_u32(sanitized.main_kills()),
            allykills: OverlayInfoOps::as_u32(sanitized.ally_kills()),
            result: sanitized.result.clone(),
            difficulty: sanitized.difficulty.clone(),
            length: OverlayInfoOps::as_u32(sanitized.length),
            brutal_plus: OverlayInfoOps::as_u32(sanitized.brutal_plus),
            weekly: sanitized.weekly,
            weekly_name: sanitized.weekly_name.clone(),
            extension: sanitized.extension,
            main_commander_level: OverlayInfoOps::as_u32(sanitized.main_commander_level()),
            ally_commander_level: OverlayInfoOps::as_u32(sanitized.ally_commander_level()),
            main_mastery_level: OverlayInfoOps::as_u32(sanitized.main_mastery_level()),
            ally_mastery_level: OverlayInfoOps::as_u32(sanitized.ally_mastery_level()),
            main_masteries: OverlayInfoOps::as_u32_vec(sanitized.main_masteries()),
            ally_masteries: OverlayInfoOps::as_u32_vec(sanitized.ally_masteries()),
            main_units: SharedTypesOps::unit_stats_map_from_value(sanitized.main_units()),
            ally_units: SharedTypesOps::unit_stats_map_from_value(sanitized.ally_units()),
            amon_units: SharedTypesOps::unit_stats_map_from_value(&sanitized.amon_units),
            main_icons: SharedTypesOps::overlay_icon_payload_from_value(sanitized.main_icons()),
            ally_icons: SharedTypesOps::overlay_icon_payload_from_value(sanitized.ally_icons()),
            mutators: sanitized
                .mutators
                .iter()
                .map(|mutator_id| {
                    OverlayInfoOps::overlay_mutator_name_with_dictionary(mutator_id, dictionary)
                })
                .collect(),
            bonus: sanitized
                .bonus
                .iter()
                .copied()
                .map(OverlayInfoOps::as_u32)
                .collect(),
            bonus_total: sanitized.bonus_total.map(OverlayInfoOps::as_u32),
            player_stats: Some(player_stats),
            main_player_stats,
            ally_player_stats,
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

    fn from_replay(replay: &crate::ReplayInfo, language: &str) -> Self {
        let _ = language;
        let sanitized = replay.sanitized_for_client();
        let main_prestige = Self::localized_prestige_text(sanitized.main_prestige());
        let ally_prestige = Self::localized_prestige_text(sanitized.ally_prestige());
        let player_stats = SharedTypesOps::replay_data_record_from_value(&sanitized.player_stats);
        let (main_player_stats, ally_player_stats) =
            OverlayInfoOps::semantic_player_stats_from_record(
                &player_stats,
                &sanitized.main().name,
                &sanitized.ally().name,
            );
        Self {
            file: sanitized.file.clone(),
            map_name: sanitized.map.clone(),
            main: sanitized.main().name.clone(),
            ally: sanitized.ally().name.clone(),
            main_commander: sanitized.main_commander().to_string(),
            ally_commander: sanitized.ally_commander().to_string(),
            main_apm: OverlayInfoOps::as_u32(sanitized.main_apm()),
            ally_apm: OverlayInfoOps::as_u32(sanitized.ally_apm()),
            mainkills: OverlayInfoOps::as_u32(sanitized.main_kills()),
            allykills: OverlayInfoOps::as_u32(sanitized.ally_kills()),
            result: sanitized.result.clone(),
            difficulty: sanitized.difficulty.clone(),
            length: OverlayInfoOps::as_u32(sanitized.length),
            brutal_plus: OverlayInfoOps::as_u32(sanitized.brutal_plus),
            weekly: sanitized.weekly,
            weekly_name: sanitized.weekly_name.clone(),
            extension: sanitized.extension,
            main_commander_level: OverlayInfoOps::as_u32(sanitized.main_commander_level()),
            ally_commander_level: OverlayInfoOps::as_u32(sanitized.ally_commander_level()),
            main_mastery_level: OverlayInfoOps::as_u32(sanitized.main_mastery_level()),
            ally_mastery_level: OverlayInfoOps::as_u32(sanitized.ally_mastery_level()),
            main_masteries: OverlayInfoOps::as_u32_vec(sanitized.main_masteries()),
            ally_masteries: OverlayInfoOps::as_u32_vec(sanitized.ally_masteries()),
            main_units: SharedTypesOps::unit_stats_map_from_value(sanitized.main_units()),
            ally_units: SharedTypesOps::unit_stats_map_from_value(sanitized.ally_units()),
            amon_units: SharedTypesOps::unit_stats_map_from_value(&sanitized.amon_units),
            main_icons: SharedTypesOps::overlay_icon_payload_from_value(sanitized.main_icons()),
            ally_icons: SharedTypesOps::overlay_icon_payload_from_value(sanitized.ally_icons()),
            mutators: sanitized.mutators.clone(),
            bonus: sanitized
                .bonus
                .iter()
                .copied()
                .map(OverlayInfoOps::as_u32)
                .collect(),
            bonus_total: sanitized.bonus_total.map(OverlayInfoOps::as_u32),
            player_stats: Some(player_stats),
            main_player_stats,
            ally_player_stats,
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
        std::mem::swap(&mut self.main_mastery_level, &mut self.ally_mastery_level);
        std::mem::swap(&mut self.main_masteries, &mut self.ally_masteries);
        std::mem::swap(&mut self.main_units, &mut self.ally_units);
        std::mem::swap(&mut self.main_icons, &mut self.ally_icons);
        std::mem::swap(&mut self.main_prestige, &mut self.ally_prestige);
        std::mem::swap(&mut self.main_player_stats, &mut self.ally_player_stats);
        SharedTypesOps::swap_replay_data_record_sides(&mut self.player_stats);
    }
}

impl OverlayInfoOps {
    fn player_series_by_name(
        player_stats: &ReplayDataRecord,
        player_name: &str,
        excluded_index: Option<usize>,
    ) -> Option<ReplayPlayerSeries> {
        let target_name = player_name.trim();
        if target_name.is_empty() {
            return None;
        }

        player_stats
            .values()
            .enumerate()
            .find(|(index, series)| {
                Some(*index) != excluded_index && series.name.trim() == target_name
            })
            .map(|(_, series)| series.clone())
    }
}

impl OverlayInfoOps {
    fn semantic_player_stats_from_record(
        player_stats: &ReplayDataRecord,
        main_name: &str,
        ally_name: &str,
    ) -> (Option<ReplayPlayerSeries>, Option<ReplayPlayerSeries>) {
        let main_player_stats =
            OverlayInfoOps::player_series_by_name(player_stats, main_name, None)
                .or_else(|| player_stats.get("1").cloned());
        let ally_player_stats = OverlayInfoOps::player_series_by_name(
            player_stats,
            ally_name,
            main_player_stats
                .as_ref()
                .and_then(|target| player_stats.values().position(|series| series == target)),
        )
        .or_else(|| player_stats.get("2").cloned());

        (main_player_stats, ally_player_stats)
    }
}

impl OverlayInfoOps {
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
        if monitor_width == 0 || monitor_height == 0 {
            let size = tauri::PhysicalSize {
                width: 1,
                height: 1,
            };
            let position = OverlayInfoOps::overlay_window_position_for_monitor(
                monitor_x,
                monitor_y,
                monitor_width,
                size.width,
                top_offset,
                right_offset,
            );
            return (size, position);
        }

        let effective_width_ratio = if monitor_height > monitor_width {
            1.0
        } else {
            width_ratio
        };

        let mut target_width = (monitor_width as f64 * effective_width_ratio).max(1.0) as i64;
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
        let position = OverlayInfoOps::overlay_window_position_for_monitor(
            monitor_x,
            monitor_y,
            monitor_width,
            size.width,
            top_offset,
            right_offset,
        );
        (size, position)
    }
}

impl OverlayInfoOps {
    pub fn overlay_window_position_for_monitor(
        monitor_x: i32,
        monitor_y: i32,
        monitor_width: u32,
        window_width: u32,
        top_offset: i32,
        right_offset: i32,
    ) -> tauri::PhysicalPosition<i32> {
        tauri::PhysicalPosition {
            x: monitor_x
                + i32::try_from(monitor_width.saturating_sub(window_width)).unwrap_or(0)
                + right_offset,
            y: monitor_y + top_offset,
        }
    }
}

impl OverlayInfoOps {
    pub fn overlay_window_size_matches_target(
        actual_size: tauri::PhysicalSize<u32>,
        target_size: tauri::PhysicalSize<u32>,
    ) -> bool {
        const SIZE_TOLERANCE_PX: u32 = 1;

        actual_size.width.abs_diff(target_size.width) <= SIZE_TOLERANCE_PX
            && actual_size.height.abs_diff(target_size.height) <= SIZE_TOLERANCE_PX
    }
}

impl OverlayInfoOps {
    pub fn parse_runtime_flags() -> RuntimeFlags {
        AppSettings::from_saved_file().runtime_flags()
    }
}

impl OverlayInfoOps {
    pub(crate) fn apply_overlay_placement(window: &tauri::WebviewWindow) -> Result<(), String> {
        let state = window.state::<BackendState>();
        OverlayInfoOps::apply_overlay_placement_from_settings(window, &state.read_settings_memory())
    }
}

impl OverlayInfoOps {
    pub(crate) fn apply_overlay_placement_from_settings(
        window: &tauri::WebviewWindow,
        settings_value: &AppSettings,
    ) -> Result<(), String> {
        let settings = settings_value.overlay_placement();
        let selected = OverlayInfoOps::selected_monitor_from_settings(window, settings_value)?;
        let (size, _) = OverlayInfoOps::overlay_window_bounds_for_monitor(
            selected.position_x(),
            selected.position_y(),
            selected.width(),
            selected.height(),
            settings.width(),
            settings.height(),
            settings.top_offset(),
            settings.right_offset(),
            settings.subtract_height(),
        );
        let provisional_position = tauri::PhysicalPosition {
            x: selected.position_x(),
            y: selected.position_y(),
        };

        window
            .set_position(provisional_position)
            .map_err(|error| format!("Failed to move overlay to target monitor: {error}"))?;
        window
            .set_size(size)
            .map_err(|error| format!("Failed to set overlay size: {error}"))?;

        OverlayInfoOps::stabilize_overlay_bounds_from_settings(window, settings_value)
    }
}

impl OverlayInfoOps {
    pub(crate) fn stabilize_overlay_bounds(window: &tauri::WebviewWindow) -> Result<(), String> {
        let state = window.state::<BackendState>();
        OverlayInfoOps::stabilize_overlay_bounds_from_settings(
            window,
            &state.read_settings_memory(),
        )
    }
}

impl OverlayInfoOps {
    fn stabilize_overlay_bounds_from_settings(
        window: &tauri::WebviewWindow,
        settings_value: &AppSettings,
    ) -> Result<(), String> {
        let settings = settings_value.overlay_placement();
        let selected = OverlayInfoOps::selected_monitor_from_settings(window, settings_value)?;
        let (target_size, _) = OverlayInfoOps::overlay_window_bounds_for_monitor(
            selected.position_x(),
            selected.position_y(),
            selected.width(),
            selected.height(),
            settings.width(),
            settings.height(),
            settings.top_offset(),
            settings.right_offset(),
            settings.subtract_height(),
        );
        let current_size = window
            .outer_size()
            .map_err(|error| format!("Failed to read overlay size: {error}"))?;

        if !OverlayInfoOps::overlay_window_size_matches_target(current_size, target_size) {
            window
                .set_size(target_size)
                .map_err(|error| format!("Failed to stabilize overlay size: {error}"))?;
            return Ok(());
        }

        let final_position = OverlayInfoOps::overlay_window_position_for_monitor(
            selected.position_x(),
            selected.position_y(),
            selected.width(),
            current_size.width,
            settings.top_offset(),
            settings.right_offset(),
        );

        window
            .set_position(final_position)
            .map_err(|error| format!("Failed to set overlay position: {error}"))
    }
}

impl OverlayInfoOps {
    fn is_valid_hotkey(shortcut: &str) -> bool {
        Shortcut::from_str(shortcut).is_ok()
    }
}

impl OverlayInfoOps {
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

        if OverlayInfoOps::is_valid_hotkey(&canonical) {
            return Some(canonical);
        }

        crate::sco_log!("[SCO/hotkey] Ignoring invalid hotkey '{raw}'");
        None
    }
}

impl OverlayInfoOps {
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
            "overlay_newer" | "overlay_older" | "overlay_player_stats" => {
                let state = app_handle.state::<BackendState>();
                if !state.try_begin_hotkey_action() {
                    crate::sco_log!(
                    "[SCO/hotkey] Ignoring '{pressed}' because another hotkey action is running"
                );
                    return;
                }
                let action_name = action.to_string();
                let app_handle = app_handle.clone();
                thread::spawn(move || {
                    let state = app_handle.state::<BackendState>();
                    let _ = OverlayInfoOps::perform_overlay_action(
                        &app_handle,
                        &state,
                        &action_name,
                        None,
                    );
                    state.finish_hotkey_action();
                });
            }
            _ => {
                let state = app_handle.state::<BackendState>();
                let _ = OverlayInfoOps::perform_overlay_action(app_handle, &state, action, None);
            }
        }
    }
}

impl OverlayInfoOps {
    fn register_hotkey_binding(
        app: &tauri::AppHandle<Wry>,
        binding: &ResolvedHotkeyBinding,
    ) -> Result<(), String> {
        let parsed = Shortcut::from_str(binding.shortcut())
            .map_err(|error| format!("Failed to parse hotkey '{}': {error}", binding.shortcut()))?;
        let action = binding.action();
        app.global_shortcut()
            .on_shortcut(parsed, move |app_handle, shortcut, event| {
                OverlayInfoOps::register_shortcut_action(app_handle, shortcut, action, event.state);
            })
            .map_err(|error| {
                format!(
                    "Failed to register hotkey '{}': {error}",
                    binding.shortcut()
                )
            })
    }
}

impl OverlayInfoOps {
    fn unregister_hotkey_binding(
        app: &tauri::AppHandle<Wry>,
        binding: &ResolvedHotkeyBinding,
    ) -> Result<(), String> {
        let parsed = Shortcut::from_str(binding.shortcut())
            .map_err(|error| format!("Failed to parse hotkey '{}': {error}", binding.shortcut()))?;
        if !app.global_shortcut().is_registered(parsed) {
            return Ok(());
        }
        app.global_shortcut().unregister(parsed).map_err(|error| {
            format!(
                "Failed to unregister hotkey '{}': {error}",
                binding.shortcut()
            )
        })
    }
}

impl OverlayInfoOps {
    pub(crate) fn register_overlay_hotkeys(app: &tauri::AppHandle<Wry>) -> Result<(), String> {
        let _ = app.global_shortcut().unregister_all();
        let state = app.state::<BackendState>();

        let active_reassign_path = state.active_hotkey_reassign_path();
        let mut registered: HashMap<String, &'static str> = HashMap::new();
        let mut registered_count = 0usize;

        for binding in state.resolved_overlay_hotkey_bindings() {
            if active_reassign_path.as_deref() == Some(binding.path()) {
                crate::sco_log!(
                    "[SCO/hotkey] Skipping '{}' because it is currently being reassigned",
                    binding.path()
                );
                continue;
            }
            if let Some(existing_action) = registered.get(binding.canonical()) {
                if *existing_action == binding.action() {
                    crate::sco_log!(
                        "[SCO/hotkey] Duplicate hotkey '{}' for '{}' ignored.",
                        binding.canonical(),
                        binding.action()
                    );
                } else {
                    crate::sco_log!(
                        "[SCO/hotkey] Hotkey '{}' already bound to '{}', skipping '{}'.",
                        binding.canonical(),
                        existing_action,
                        binding.action()
                    );
                }
                continue;
            }
            crate::sco_log!(
                "[SCO/hotkey] Registering '{}' for '{}'",
                binding.shortcut(),
                binding.action()
            );
            OverlayInfoOps::register_hotkey_binding(app, &binding)?;
            registered.insert(binding.canonical().to_string(), binding.action());
            registered_count += 1;
        }

        if registered_count == 0 {
            crate::sco_log!("[SCO/hotkey] No overlay hotkeys configured.");
        }

        Ok(())
    }
}

impl OverlayInfoOps {
    pub(crate) fn begin_hotkey_reassign(
        app: &tauri::AppHandle<Wry>,
        path: &str,
    ) -> Result<(), String> {
        let state = app.state::<BackendState>();
        if let Some(previous_path) = state.active_hotkey_reassign_path() {
            if previous_path != path {
                OverlayInfoOps::end_hotkey_reassign(app, &previous_path)?;
            }
        }

        state.set_active_hotkey_reassign_path(Some(path.to_string()));
        let binding = state
            .resolved_overlay_hotkey_bindings()
            .into_iter()
            .find(|binding| binding.path() == path);
        state.set_active_hotkey_reassign_binding(binding.clone());

        if let Some(binding) = binding {
            OverlayInfoOps::unregister_hotkey_binding(app, &binding)?;
            crate::sco_log!(
                "[SCO/hotkey] Removed hotkey trigger for '{}' while it is being reassigned",
                path
            );
        }

        Ok(())
    }
}

impl OverlayInfoOps {
    pub(crate) fn end_hotkey_reassign(
        app: &tauri::AppHandle<Wry>,
        path: &str,
    ) -> Result<(), String> {
        let state = app.state::<BackendState>();
        if state.active_hotkey_reassign_path().as_deref() == Some(path) {
            state.set_active_hotkey_reassign_path(None);
        }

        let settings_value = state.read_settings_memory();
        let fallback_binding = state.active_hotkey_reassign_binding();
        let Some(binding) =
            settings_value.hotkey_binding_for_reassign_end(path, fallback_binding.as_ref())
        else {
            state.set_active_hotkey_reassign_binding(None);
            crate::sco_log!("[SCO/hotkey] '{path}' has no active binding after reassignment");
            return Ok(());
        };

        let bindings = settings_value.resolved_overlay_hotkey_bindings();
        if bindings
            .iter()
            .any(|other| other.path() != binding.path() && other.canonical() == binding.canonical())
        {
            state.set_active_hotkey_reassign_binding(None);
            crate::sco_log!(
                "[SCO/hotkey] Hotkey '{}' conflicts with another binding, skipping '{}'.",
                binding.canonical(),
                binding.path()
            );
            return Ok(());
        }

        OverlayInfoOps::register_hotkey_binding(app, &binding)?;
        state.set_active_hotkey_reassign_binding(None);
        crate::sco_log!(
            "[SCO/hotkey] Recreated hotkey trigger for '{}' as '{}'",
            path,
            binding.shortcut()
        );
        Ok(())
    }
}

impl OverlayInfoOps {
    pub fn overlay_payload_from_replay(
        state: &BackendState,
        replay: &crate::ReplayInfo,
        mark_new_replay: bool,
        show_session: bool,
        session_victories: u64,
        session_defeats: u64,
    ) -> OverlayReplayPayload {
        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let settings = state.read_settings_memory();
        let language = settings.overlay_language();
        let dictionary = state.dictionary_data().ok();
        let mut payload = dictionary
            .as_deref()
            .map(|dictionary| {
                OverlayReplayPayload::from_replay_with_dictionary(replay, language, dictionary)
            })
            .unwrap_or_else(|| OverlayReplayPayload::from_replay(replay, language));
        if TauriOverlayOps::replay_should_swap_main_and_ally(replay, &main_names, &main_handles) {
            payload.swap_sides();
        }
        if show_session {
            payload.victory = Some(OverlayInfoOps::as_u32(session_victories));
            payload.defeat = Some(OverlayInfoOps::as_u32(session_defeats));
        }
        payload.new_replay = mark_new_replay.then_some(true);
        payload
    }
}

impl OverlayInfoOps {
    fn emit_overlay_replay_payload(app: &tauri::AppHandle<Wry>, payload: &OverlayReplayPayload) {
        OverlayInfoOps::sync_overlay_runtime_settings(app);
        let _ = app.emit(OVERLAY_REPLAY_PAYLOAD_EVENT, payload);
        OverlayInfoOps::show_overlay_window(app);
    }
}

impl OverlayInfoOps {
    pub(crate) fn emit_replay_to_overlay_from_replay(
        app: &tauri::AppHandle<Wry>,
        replay: &crate::ReplayInfo,
        mark_new_replay: bool,
    ) {
        let state = app.state::<BackendState>();

        let replay = (!replay.is_detailed)
            .then(|| {
                TauriOverlayOps::process_replay_detailed(&state, &PathBuf::from(&replay.file)).1
            })
            .flatten()
            .unwrap_or_else(|| replay.clone());

        let settings = state.read_settings_memory();
        let show_session = settings.show_session();
        let (session_victories, session_defeats) = state.session_counts();
        let payload = OverlayInfoOps::overlay_payload_from_replay(
            &state,
            &replay,
            mark_new_replay,
            show_session,
            session_victories,
            session_defeats,
        );
        OverlayInfoOps::emit_overlay_replay_payload(app, &payload);
    }
}

impl OverlayInfoOps {
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
}

impl OverlayInfoOps {
    pub fn replay_move_target_index(
        replays: &[crate::ReplayInfo],
        selected: &Option<String>,
        delta: i64,
        replay_data_active: bool,
    ) -> usize {
        if replays.is_empty() || !replay_data_active {
            return 0;
        }

        let mut index = TauriOverlayOps::replay_index_by_file(replays, selected).unwrap_or(0);
        if delta > 0 {
            index = index.saturating_sub(delta as usize);
        } else if delta < 0 {
            let steps = delta.wrapping_abs() as usize;
            index = (index + steps).min(replays.len().saturating_sub(1));
        }

        index
    }
}

impl OverlayInfoOps {
    pub fn replay_move_should_be_ignored(
        current_index: Option<usize>,
        target_index: usize,
        replay_data_active: bool,
    ) -> bool {
        replay_data_active && current_index.is_some_and(|index| index == target_index)
    }
}

impl OverlayInfoOps {
    pub(crate) fn replay_show_for_window(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        requested: Option<&str>,
    ) -> crate::OverlayActionResponse {
        let replays = state.sync_replay_cache_slots(UNLIMITED_REPLAY_LIMIT);
        let selected = state.get_current_replay_file();
        let Some(replay) = OverlayInfoOps::replay_for_display(&replays, requested, &selected)
        else {
            return crate::OverlayActionResponse::failure("No replay selected");
        };
        let file = replay.file.clone();

        OverlayInfoOps::emit_replay_to_overlay_from_replay(app, replay, false);
        state.set_overlay_replay_data_active(true);
        state.set_current_replay_file(Some(&file));

        crate::OverlayActionResponse::success("Replay shown")
    }
}

impl OverlayInfoOps {
    pub(crate) fn replay_move_window(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        delta: i64,
    ) -> crate::OverlayActionResponse {
        let cached = state.replay_cache_snapshot();

        let replays = if cached.is_empty() {
            state.sync_replay_cache_slots(UNLIMITED_REPLAY_LIMIT)
        } else {
            cached
        };

        if replays.is_empty() {
            return crate::OverlayActionResponse::failure("No replays available");
        }

        let selected = state.get_current_replay_file();
        let replay_data_active = state.overlay_replay_data_active();
        let current_index = TauriOverlayOps::replay_index_by_file(&replays, &selected);
        let index = OverlayInfoOps::replay_move_target_index(
            &replays,
            &selected,
            delta,
            replay_data_active,
        );
        if OverlayInfoOps::replay_move_should_be_ignored(current_index, index, replay_data_active) {
            return crate::OverlayActionResponse::success("Replay move ignored");
        }

        let replay = &replays[index];
        let file = replay.file.clone();

        OverlayInfoOps::emit_replay_to_overlay_from_replay(app, replay, false);
        state.set_overlay_replay_data_active(true);
        state.set_current_replay_file(Some(&file));

        crate::OverlayActionResponse::success("Replay moved")
    }
}

impl OverlayInfoOps {
    pub(crate) fn perform_overlay_action(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        action: &str,
        body: Option<&Value>,
    ) -> Option<crate::OverlayActionResponse> {
        match action {
            "overlay_show_hide" => {
                let overlay_visible = app
                    .get_webview_window("overlay")
                    .and_then(|window| window.is_visible().ok())
                    .unwrap_or(false);
                if overlay_visible {
                    let _ = app.emit(OVERLAY_SHOWHIDE_EVENT, EmptyPayload::default());
                } else {
                    OverlayInfoOps::show_overlay_window(app);
                    let _ = app.emit(OVERLAY_SHOWSTATS_EVENT, EmptyPayload::default());
                }
                Some(crate::OverlayActionResponse::success(
                    "Overlay visibility toggled",
                ))
            }
            "overlay_show" => {
                OverlayInfoOps::show_overlay_window(app);
                let _ = app.emit(OVERLAY_SHOWSTATS_EVENT, EmptyPayload::default());
                Some(crate::OverlayActionResponse::success("Overlay shown"))
            }
            "overlay_hide" => {
                OverlayInfoOps::hide_overlay_window(app);
                let _ = app.emit(OVERLAY_HIDESTATS_EVENT, EmptyPayload::default());
                Some(crate::OverlayActionResponse::success("Overlay hidden"))
            }
            "overlay_replay_data_state" => {
                let active = body
                    .and_then(|payload| payload.get("active"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                state.set_overlay_replay_data_active(active);
                if !active {
                    state.set_current_replay_file(None);
                }
                Some(crate::OverlayActionResponse::success(if active {
                    "Overlay replay data marked active"
                } else {
                    "Overlay replay data cleared"
                }))
            }
            "overlay_newer" => Some(OverlayInfoOps::replay_move_window(app, state, 1)),
            "overlay_older" => Some(OverlayInfoOps::replay_move_window(app, state, -1)),
            "overlay_player_stats" => {
                let payload = state.overlay_player_stats_payload();
                let _ = app.emit(OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT, payload);
                OverlayInfoOps::show_overlay_window(app);

                Some(crate::OverlayActionResponse::success(
                    "Overlay player stats toggled",
                ))
            }
            "performance_show_hide" => {
                let performance_visible = app
                    .get_webview_window("performance")
                    .and_then(|window| window.is_visible().ok())
                    .unwrap_or(false);
                let next_visible = !performance_visible;
                match crate::performance_overlay::PerformanceOverlayOps::set_visibility(
                    app,
                    next_visible,
                    true,
                ) {
                    Ok(()) => Some(crate::OverlayActionResponse::success(if next_visible {
                        "Performance overlay shown"
                    } else {
                        "Performance overlay hidden"
                    })),
                    Err(error) => Some(crate::OverlayActionResponse::failure(error)),
                }
            }
            "performance_toggle_reposition" => {
                let enabled =
                    crate::performance_overlay::PerformanceOverlayOps::toggle_edit_mode(app);
                Some(crate::OverlayActionResponse::success(if enabled {
                    "Performance overlay reposition mode enabled"
                } else {
                    "Performance overlay reposition mode disabled"
                }))
            }
            "hotkey_reassign_begin" => {
                let path = body
                    .and_then(|payload| payload.get("path"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match OverlayInfoOps::begin_hotkey_reassign(app, path) {
                    Ok(()) => Some(crate::OverlayActionResponse::success_with_path(
                        format!("Removed hotkey trigger for {path}"),
                        path.to_string(),
                    )),
                    Err(error) => Some(crate::OverlayActionResponse::failure_with_path(
                        error,
                        path.to_string(),
                    )),
                }
            }
            "hotkey_reassign_end" => {
                let path = body
                    .and_then(|payload| payload.get("path"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match OverlayInfoOps::end_hotkey_reassign(app, path) {
                    Ok(()) => Some(crate::OverlayActionResponse::success_with_path(
                        format!("Recreated hotkey trigger for {path}"),
                        path.to_string(),
                    )),
                    Err(error) => Some(crate::OverlayActionResponse::failure_with_path(
                        error,
                        path.to_string(),
                    )),
                }
            }
            "parse_replay" => {
                let requested = body
                    .and_then(|payload| payload.get("file"))
                    .and_then(Value::as_str);
                Some(OverlayInfoOps::replay_show_for_window(
                    app, state, requested,
                ))
            }
            "overlay_screenshot" => Some(match OverlayInfoOps::request_overlay_screenshot(app) {
                Ok(path) => crate::OverlayActionResponse::success_with_path(
                    format!("Overlay screenshot requested for {path}"),
                    path,
                ),
                Err(error) => crate::OverlayActionResponse::failure(error),
            }),
            "create_desktop_shortcut" => Some(crate::OverlayActionResponse::success(
                "Create desktop shortcut is not available in this build",
            )),
            "randomizer_generate" => Some(match state.dictionary_data() {
                Ok(dictionary) => {
                    match randomizer::RandomizerOps::generate_from_body_with_dictionary(
                        body,
                        &dictionary,
                    ) {
                        Ok(result) => crate::OverlayActionResponse {
                            status: "ok",
                            result: crate::OverlayActionResult {
                                ok: true,
                                path: None,
                            },
                            message: "Generated random commander".to_string(),
                            randomizer: Some(result),
                        },
                        Err(error) => crate::OverlayActionResponse {
                            status: "ok",
                            result: crate::OverlayActionResult {
                                ok: false,
                                path: None,
                            },
                            message: error,
                            randomizer: None,
                        },
                    }
                }
                Err(error) => crate::OverlayActionResponse::failure(error),
            }),
            _ => None,
        }
    }
}

impl OverlayInfoOps {
    pub(crate) fn show_player_stats_for_name(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        player_handle: &str,
        player_name: &str,
    ) -> bool {
        if player_name.trim().is_empty() {
            return false;
        }

        let payload = state.overlay_player_stats_payload_for_player(player_handle, player_name);
        let _ = app.emit(OVERLAY_HIDESTATS_EVENT, EmptyPayload::default());
        let _ = app.emit(OVERLAY_PLAYER_STATS_EVENT, payload);
        OverlayInfoOps::show_overlay_window(app);
        true
    }
}

impl OverlayInfoOps {
    fn request_overlay_screenshot(app: &tauri::AppHandle<Wry>) -> Result<String, String> {
        if app.get_webview_window("overlay").is_none() {
            return Err("Overlay window is not available".to_string());
        }

        let settings = app.state::<BackendState>().read_settings_memory();
        let path = settings.overlay_screenshot_output_path(SystemTime::now())?;
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
}

impl OverlayInfoOps {
    fn is_png_signature(bytes: &[u8]) -> bool {
        const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
        bytes.starts_with(&PNG_SIGNATURE)
    }
}

impl OverlayInfoOps {
    pub fn save_overlay_screenshot(path: &Path, png_bytes: &[u8]) -> Result<(), String> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if !extension {
            return Err("Overlay screenshot path must end with .png".to_string());
        }
        if !OverlayInfoOps::is_png_signature(png_bytes) {
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
}

impl OverlayInfoOps {
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
}

impl OverlayInfoOps {
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
}

impl OverlayInfoOps {
    pub fn open_folder_in_explorer(folder: &str) -> Result<(), String> {
        let path = OverlayInfoOps::existing_folder_path(folder)?;

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
}

impl OverlayInfoOps {
    pub(crate) fn sync_overlay_runtime_settings<R: Runtime>(app: &tauri::AppHandle<R>) {
        let state = app.state::<crate::BackendState>();
        let settings = state.read_settings_memory();
        let (session_victories, session_defeats) = state.session_counts();
        let payload = settings.overlay_runtime_settings_payload(session_victories, session_defeats);
        let _ = app.emit(OVERLAY_INIT_COLORS_DURATION_EVENT, payload);
    }
}

impl OverlayInfoOps {
    pub(crate) fn show_overlay_window<R: Runtime>(app: &tauri::AppHandle<R>) {
        OverlayInfoOps::sync_overlay_runtime_settings(app);
        if let Some(overlay_window) = app.get_webview_window("overlay") {
            let _ = overlay_window.set_focusable(false);
            let _ = overlay_window.show();
        }
    }
}

impl OverlayInfoOps {
    pub(crate) fn hide_overlay_window<R: Runtime>(app: &tauri::AppHandle<R>) {
        if let Some(overlay_window) = app.get_webview_window("overlay") {
            let _ = overlay_window.hide();
        }
    }
}

impl OverlayInfoOps {
    pub(crate) fn show_config_window<R: Runtime>(app: &tauri::AppHandle<R>) {
        if let Some(config_window) = app.get_webview_window("config") {
            let _ = config_window.show();
            let _ = config_window.set_focus();
        }
    }
}

impl OverlayInfoOps {
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
}
