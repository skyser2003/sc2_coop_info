use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use ts_rs::TS;

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
    #[serde(skip)]
    #[ts(skip)]
    pub present_keys: BTreeSet<String>,
}
