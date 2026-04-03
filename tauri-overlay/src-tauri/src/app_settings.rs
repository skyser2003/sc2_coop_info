use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use ts_rs::TS;

use crate::{get_default_accounts_folder, get_system_language, path_manager};

pub type RandomizerChoices = BTreeMap<String, bool>;
pub type PlayerNotes = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct AppSettings {
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
    pub start_minimized: bool,
    pub auto_update: bool,
    pub duration: u32,
    pub show_player_winrates: bool,
    pub show_replay_info_after_game: bool,
    pub show_session: bool,
    pub show_charts: bool,
    pub hide_nicknames_in_overlay: bool,
    pub account_folder: String,
    pub screenshot_folder: String,
    pub color_player1: String,
    pub color_player2: String,
    pub color_amon: String,
    pub color_mastery: String,
    #[serde(rename = "hotkey_show/hide")]
    pub hotkey_show_hide: Option<String>,
    pub hotkey_show: Option<String>,
    pub hotkey_hide: Option<String>,
    pub hotkey_newer: Option<String>,
    pub hotkey_older: Option<String>,
    pub hotkey_winrates: Option<String>,
    pub enable_logging: bool,
    pub dark_theme: bool,
    pub language: String,
    pub monitor: usize,
    pub performance_show: bool,
    pub performance_hotkey: Option<String>,
    pub performance_processes: Vec<String>,
    #[ts(optional)]
    pub performance_geometry: Option<[i32; 4]>,
    pub rng_choices: RandomizerChoices,
    pub player_notes: PlayerNotes,
    pub main_names: Vec<String>,
    pub detailed_analysis_atstart: bool,
    pub analysis_worker_threads: usize,
    #[serde(skip)]
    #[ts(skip)]
    pub present_keys: BTreeSet<String>,
}

impl AppSettings {
    pub fn logical_core_count() -> usize {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    }

    pub fn default_analysis_worker_threads() -> usize {
        (Self::logical_core_count() / 2).max(1)
    }

    pub fn simple_analysis_worker_threads() -> usize {
        Self::default_analysis_worker_threads()
    }

    pub fn clamp_analysis_worker_threads(value: usize) -> usize {
        value.clamp(1, Self::logical_core_count())
    }

    pub fn from_value(value: Value) -> Result<Self, String> {
        serde_json::from_value(value).map_err(|error| format!("Invalid settings payload: {error}"))
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(&self).unwrap_or_else(|_| Value::Object(Default::default()))
    }

    pub fn from_saved_file() -> Self {
        let path = path_manager::get_settings_path();

        Self::read_saved_settings_file_from_path(&path, !cfg!(test))
    }

    pub fn setting_value_changed(prev: &AppSettings, next: &AppSettings, key: &str) -> bool {
        prev.settings_field_value(key) != next.settings_field_value(key)
    }

    pub fn any_setting_changed(prev: &AppSettings, next: &AppSettings, keys: &[&str]) -> bool {
        keys.iter()
            .any(|key| Self::setting_value_changed(prev, next, key))
    }

    pub fn merge_settings_with_defaults(value: Value) -> Self {
        let sanitized = Self::sanitize_settings_value(value);
        let present_keys = match &sanitized {
            Value::Object(settings) => settings.keys().cloned().collect(),
            _ => Default::default(),
        };

        let settings = AppSettings::default();

        let mut merged = match settings.to_value() {
            Value::Object(defaults) => defaults,
            _ => Map::new(),
        };

        if let Value::Object(settings) = sanitized {
            merged.extend(settings);
        }

        let mut settings = Self::from_value(Value::Object(merged)).unwrap_or_else(|_| settings);
        settings.initialize_unset_hotkeys();
        settings.analysis_worker_threads =
            Self::clamp_analysis_worker_threads(settings.analysis_worker_threads);
        settings.present_keys = present_keys;

        settings
    }

    pub fn read_saved_settings_file_from_path(path: &Path, create_if_missing: bool) -> Self {
        let defaults = AppSettings::default();

        if !path.exists() {
            if create_if_missing {
                let _ = defaults.write_saved_settings_file_to_path(path);
            }

            return defaults;
        }

        let text = std::fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
        let parsed = serde_json::from_str(&text).unwrap_or(Value::Object(Default::default()));
        Self::merge_settings_with_defaults(parsed)
    }

    pub fn write_saved_settings_file(&self) -> Result<Self, String> {
        let path = path_manager::get_settings_path();

        self.write_saved_settings_file_to_path(&path)
    }

    fn write_saved_settings_file_to_path(&self, path: &Path) -> Result<Self, String> {
        let sanitized = Self::merge_settings_with_defaults(self.to_value());
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

    pub fn sanitize_settings_value(value: Value) -> Value {
        match value {
            Value::Object(mut map) => {
                map.remove("fast_expand");
                map.remove("force_hide_overlay");
                Value::Object(map)
            }
            other => other,
        }
    }

    pub fn initialize_unset_hotkeys(&mut self) {
        if self.hotkey_show_hide.is_none() {
            self.hotkey_show_hide = Some("Ctrl+Shift+8".to_string());
        }
        if self.hotkey_newer.is_none() {
            self.hotkey_newer = Some("Ctrl+Alt+/".to_string());
        }
        if self.hotkey_older.is_none() {
            self.hotkey_older = Some("Ctrl+Alt+8".to_string());
        }
        if self.hotkey_winrates.is_none() {
            self.hotkey_winrates = Some("Ctrl+Alt+-".to_string());
        }
    }

    pub fn settings_field_value(&self, key: &str) -> Option<Value> {
        self.to_value().get(key).cloned()
    }

    pub fn has(&self, key: &str) -> bool {
        self.present_keys.contains(key)
    }

    pub fn normalized_analysis_worker_threads(&self) -> usize {
        Self::clamp_analysis_worker_threads(self.analysis_worker_threads)
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        let mut settings = AppSettings {
            start_with_windows: false,
            minimize_to_tray: true,
            start_minimized: false,
            auto_update: true,
            duration: 30,
            show_player_winrates: true,
            show_replay_info_after_game: true,
            show_session: true,
            show_charts: true,
            hide_nicknames_in_overlay: false,
            account_folder: get_default_accounts_folder(),
            screenshot_folder: String::new(),
            color_player1: "#0080F8".to_string(),
            color_player2: "#00D532".to_string(),
            color_amon: "#FF0000".to_string(),
            color_mastery: "#FFDC87".to_string(),
            hotkey_show_hide: None,
            hotkey_show: None,
            hotkey_hide: None,
            hotkey_newer: None,
            hotkey_older: None,
            hotkey_winrates: None,
            enable_logging: true,
            dark_theme: true,
            language: get_system_language(),
            monitor: 1,
            performance_show: false,
            performance_hotkey: None,
            performance_processes: Vec::new(),
            performance_geometry: None,
            rng_choices: Default::default(),
            player_notes: Default::default(),
            main_names: Vec::new(),
            detailed_analysis_atstart: false,
            analysis_worker_threads: Self::default_analysis_worker_threads(),
            present_keys: Default::default(),
        };

        settings.initialize_unset_hotkeys();

        settings
    }
}
