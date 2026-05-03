use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rfd::FileDialog;
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, GenerateCacheConfig, GenerateCacheRuntimeOptions,
    GenerateCacheStopController, GenerateCacheSummary, ReplayAnalysisResources, ReplayFileIdentity,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::Serialize;
use serde_json::{self, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri_plugin_updater::UpdaterExt;

use tauri::{tray::TrayIconBuilder, AppHandle, Emitter, Manager, State, Wry};

mod app_settings;
mod backend_state;
mod command_payloads;
mod game_launch_detector;
mod live_game;
mod logging;
mod monitor_settings;
mod overlay_info;
mod path_manager;
mod performance_overlay;
mod randomizer;
mod replay_analysis;
mod replay_info;
mod replay_visual;
mod shared_types;
mod stats_state;
mod test_helper;
pub use app_settings::{AppSettings, PlayerNotes, RandomizerChoices};
pub use backend_state::BackendState;
pub use command_payloads::{
    AnalysisCompletedPayload, ConfigChatPayload, ConfigPayload, ConfigPlayersPayload,
    ConfigReplayVisualPayload, ConfigReplaysPayload, ConfigWeekliesPayload, OverlayActionResponse,
    OverlayActionResult, StatsActionPayload, StatsStatePayload,
};
pub use game_launch_detector::{GameLaunchDetector, GameLaunchStatus};
pub use logging::LoggingOps;
pub use monitor_settings::{MonitorDescriptor, MonitorSettingsOps};
pub use overlay_info::{OverlayInfoOps, ResolvedHotkeyBinding};
pub use path_manager::PathManagerOps;
pub use randomizer::{RandomizerMutatorResult, RandomizerOps, RandomizerRequest, RandomizerResult};
pub use replay_analysis::{PlayerRowPayload, ReplayAnalysis, ReplayAnalysisOps, WeeklyRowPayload};
pub use replay_info::{
    CommanderUnitRollup, GamesRowPayload, ReplayChatMessage, ReplayChatPayload, ReplayInfo,
    ReplayPlayerInfo, UnitStatsRollup,
};
pub use replay_visual::{
    ReplayVisualAssault, ReplayVisualBuildInput, ReplayVisualContext, ReplayVisualDictionaries,
    ReplayVisualFrame, ReplayVisualOps, ReplayVisualOwnerKind, ReplayVisualPayload,
    ReplayVisualPlayer, ReplayVisualUnit, ReplayVisualUnitCount, ReplayVisualUnitGroup,
};
pub use shared_types::*;
pub use stats_state::{
    AnalysisMode, StartupAnalysisRequestOutcome, StartupAnalysisTrigger, StatsSnapshot, StatsState,
};
pub use test_helper::TestHelperOps;

#[macro_export]
macro_rules! sco_log {
    ($($arg:tt)*) => {{
        $crate::LoggingOps::log_line(&format!($($arg)*));
    }};
}

use crate::backend_state::ReplayState;
use crate::live_game::LiveGameOps;
use crate::stats_state::AnalysisOutcome;

pub const UNLIMITED_REPLAY_LIMIT: usize = 0;
const SCO_REPLAY_SCAN_PROGRESS_EVENT: &str = "sco://replay-scan-progress";
const SCO_ANALYSIS_COMPLETED_EVENT: &str = "sco://analysis-completed";

#[derive(Default)]
struct TrayState {
    tray_icon: Mutex<Option<tauri::tray::TrayIcon<Wry>>>,
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

pub struct TauriOverlayOps;

impl TauriOverlayOps {
    fn to_json_value<T: Serialize>(value: T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

impl TauriOverlayOps {
    fn decode_html_entities(value: &str) -> String {
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
    }
}

impl TauriOverlayOps {
    fn canonical_mutator_id_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        if dictionary.mutator_data(mutator).is_some() {
            mutator.to_string()
        } else if let Some(mutator_id) = dictionary.mutator_id_from_name(mutator) {
            mutator_id.to_string()
        } else {
            mutator.to_string()
        }
    }
}

impl TauriOverlayOps {
    fn mutator_display_name_en_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        let mutator_id = TauriOverlayOps::canonical_mutator_id_with_dictionary(mutator, dictionary);
        dictionary
            .mutator_data(&mutator_id)
            .map(|value| TauriOverlayOps::decode_html_entities(&value.name.en))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                dictionary
                    .mutator_ids
                    .get(&mutator_id)
                    .map(|value| value.to_string())
            })
            .unwrap_or_default()
    }
}

impl TauriOverlayOps {
    pub fn windows_startup_command_value(executable_path: &Path) -> String {
        format!("\"{}\"", executable_path.display())
    }
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

impl TauriOverlayOps {
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
            overlay_info::OverlayInfoOps::sync_overlay_runtime_settings(app);
        }

        let previous_show_charts = previous_settings.show_charts();
        let show_charts = next_settings.show_charts();
        if show_charts != previous_show_charts {
            let _ = app.emit(
                overlay_info::OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT,
                show_charts,
            );
        }
        if overlay_hotkeys_changed {
            if let Err(error) = overlay_info::OverlayInfoOps::register_overlay_hotkeys(app) {
                crate::sco_log!("[SCO/hotkey] Failed to reload hotkeys: {error}");
            }
        }
        if overlay_placement_changed {
            if let Some(window) = app.get_webview_window("overlay") {
                if let Err(error) =
                    overlay_info::OverlayInfoOps::apply_overlay_placement_from_settings(
                        &window,
                        &next_settings,
                    )
                {
                    crate::sco_log!("[SCO/overlay] Failed to apply overlay placement: {error}");
                }
            }
        }
        if performance_runtime_changed {
            performance_overlay::PerformanceOverlayOps::apply_settings(app);
        }

        if let Ok(mut stats) = app.state::<BackendState>().stats_handle().lock() {
            stats.set_detailed_analysis_atstart(next_settings.detailed_analysis_atstart());
        }
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    pub fn session_counter_delta(result: &str) -> (u64, u64) {
        match result.trim().to_ascii_lowercase().as_str() {
            "victory" => (1, 0),
            "defeat" => (0, 1),
            _ => (0, 0),
        }
    }
}

impl TauriOverlayOps {
    fn units_to_stats_with_dictionary(dictionary: &Sc2DictionaryData) -> HashSet<String> {
        dictionary.units_to_stats.clone()
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
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

        if let Some(owner_handle) =
            TauriOverlayOps::infer_owner_handle_from_replay_path(&replay.file)
        {
            let p1_owner = !p1_handle.is_empty() && p1_handle == owner_handle;
            let p2_owner = !p2_handle.is_empty() && p2_handle == owner_handle;
            if p1_owner != p2_owner {
                return !p1_owner && p2_owner;
            }
        }

        if !main_names.is_empty() {
            let p1_is_main =
                ReplayAnalysis::is_main_player_by_name(&replay.main().name, main_names);
            let p2_is_main =
                ReplayAnalysis::is_main_player_by_name(&replay.ally().name, main_names);
            if p1_is_main != p2_is_main {
                return !p1_is_main && p2_is_main;
            }
        }

        false
    }
}

impl TauriOverlayOps {
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
}

impl ReplayPlayerInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn apm(&self) -> u64 {
        self.apm
    }

    pub fn kills(&self) -> u64 {
        self.kills
    }

    pub fn commander(&self) -> &str {
        &self.commander
    }

    pub fn commander_level(&self) -> u64 {
        self.commander_level
    }

    pub fn mastery_level(&self) -> u64 {
        self.mastery_level
    }

    pub fn prestige(&self) -> u64 {
        self.prestige
    }

    pub fn masteries(&self) -> &[u64] {
        &self.masteries
    }

    pub fn units(&self) -> &Value {
        &self.units
    }

    pub fn icons(&self) -> &Value {
        &self.icons
    }

    pub fn set_name(&mut self, value: impl Into<String>) {
        self.name = value.into();
    }

    pub fn with_name(mut self, value: impl Into<String>) -> Self {
        self.set_name(value);
        self
    }

    pub fn set_handle(&mut self, value: impl Into<String>) {
        self.handle = value.into();
    }

    pub fn with_handle(mut self, value: impl Into<String>) -> Self {
        self.set_handle(value);
        self
    }

    pub fn set_apm(&mut self, value: u64) {
        self.apm = value;
    }

    pub fn with_apm(mut self, value: u64) -> Self {
        self.set_apm(value);
        self
    }

    pub fn set_kills(&mut self, value: u64) {
        self.kills = value;
    }

    pub fn with_kills(mut self, value: u64) -> Self {
        self.set_kills(value);
        self
    }

    pub fn set_commander(&mut self, value: impl Into<String>) {
        self.commander = value.into();
    }

    pub fn with_commander(mut self, value: impl Into<String>) -> Self {
        self.set_commander(value);
        self
    }

    pub fn set_commander_level(&mut self, value: u64) {
        self.commander_level = value;
    }

    pub fn with_commander_level(mut self, value: u64) -> Self {
        self.set_commander_level(value);
        self
    }

    pub fn set_mastery_level(&mut self, value: u64) {
        self.mastery_level = value;
    }

    pub fn with_mastery_level(mut self, value: u64) -> Self {
        self.set_mastery_level(value);
        self
    }

    pub fn set_prestige(&mut self, value: u64) {
        self.prestige = value;
    }

    pub fn with_prestige(mut self, value: u64) -> Self {
        self.set_prestige(value);
        self
    }

    pub fn set_masteries(&mut self, value: Vec<u64>) {
        self.masteries = value;
    }

    pub fn with_masteries(mut self, value: Vec<u64>) -> Self {
        self.set_masteries(value);
        self
    }

    pub fn set_units(&mut self, value: Value) {
        self.units = value;
    }

    pub fn with_units(mut self, value: Value) -> Self {
        self.set_units(value);
        self
    }

    pub fn set_icons(&mut self, value: Value) {
        self.icons = value;
    }

    pub fn with_icons(mut self, value: Value) -> Self {
        self.set_icons(value);
        self
    }

    fn sanitized_for_client(&self) -> Self {
        Self {
            name: TauriOverlayOps::sanitize_replay_text(&self.name),
            handle: self.handle.clone(),
            apm: self.apm,
            kills: self.kills,
            commander: TauriOverlayOps::sanitize_replay_text(&self.commander),
            commander_level: self.commander_level,
            mastery_level: self.mastery_level,
            prestige: self.prestige,
            masteries: TauriOverlayOps::normalize_mastery_values(&self.masteries),
            units: TauriOverlayOps::sanitize_unit_map(&self.units),
            icons: TauriOverlayOps::sanitize_icon_map(&self.icons),
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

    pub fn oriented_for_main_identity(
        mut self,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Self {
        if !TauriOverlayOps::replay_should_swap_main_and_ally(&self, main_names, main_handles) {
            return self;
        }

        self.main_slot = self.ally_index();
        TauriOverlayOps::swap_player_stats_sides(&mut self.player_stats);
        self
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

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn date(&self) -> u64 {
        self.date
    }

    pub fn map(&self) -> &str {
        &self.map
    }

    pub fn result(&self) -> &str {
        &self.result
    }

    pub fn difficulty(&self) -> &str {
        &self.difficulty
    }

    pub fn enemy(&self) -> &str {
        &self.enemy
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn accurate_length(&self) -> f64 {
        self.accurate_length
    }

    pub fn amon_units(&self) -> &Value {
        &self.amon_units
    }

    pub fn player_stats(&self) -> &Value {
        &self.player_stats
    }

    pub fn extension(&self) -> bool {
        self.extension
    }

    pub fn brutal_plus(&self) -> u64 {
        self.brutal_plus
    }

    pub fn weekly(&self) -> bool {
        self.weekly
    }

    pub fn weekly_name(&self) -> Option<&str> {
        self.weekly_name.as_deref()
    }

    pub fn mutators(&self) -> &[String] {
        &self.mutators
    }

    pub fn comp(&self) -> &str {
        &self.comp
    }

    pub fn bonus(&self) -> &[u64] {
        &self.bonus
    }

    pub fn bonus_total(&self) -> Option<u64> {
        self.bonus_total
    }

    pub fn messages(&self) -> &[ReplayChatMessage] {
        &self.messages
    }

    pub fn is_detailed(&self) -> bool {
        self.is_detailed
    }

    pub fn set_file(&mut self, value: impl Into<String>) {
        self.file = value.into();
    }

    pub fn set_date(&mut self, value: u64) {
        self.date = value;
    }

    pub fn set_map(&mut self, value: impl Into<String>) {
        self.map = value.into();
    }

    pub fn set_result(&mut self, value: impl Into<String>) {
        self.result = value.into();
    }

    pub fn set_difficulty(&mut self, value: impl Into<String>) {
        self.difficulty = value.into();
    }

    pub fn set_enemy(&mut self, value: impl Into<String>) {
        self.enemy = value.into();
    }

    pub fn set_length(&mut self, value: u64) {
        self.length = value;
    }

    pub fn set_accurate_length(&mut self, value: f64) {
        self.accurate_length = value;
    }

    pub fn set_amon_units(&mut self, value: Value) {
        self.amon_units = value;
    }

    pub fn set_player_stats(&mut self, value: Value) {
        self.player_stats = value;
    }

    pub fn set_extension(&mut self, value: bool) {
        self.extension = value;
    }

    pub fn set_brutal_plus(&mut self, value: u64) {
        self.brutal_plus = value;
    }

    pub fn set_weekly(&mut self, value: bool) {
        self.weekly = value;
    }

    pub fn set_weekly_name(&mut self, value: Option<String>) {
        self.weekly_name = value;
    }

    pub fn set_mutators(&mut self, value: Vec<String>) {
        self.mutators = value;
    }

    pub fn set_comp(&mut self, value: impl Into<String>) {
        self.comp = value.into();
    }

    pub fn set_bonus(&mut self, value: Vec<u64>) {
        self.bonus = value;
    }

    pub fn set_bonus_total(&mut self, value: Option<u64>) {
        self.bonus_total = value;
    }

    pub fn set_messages(&mut self, value: Vec<ReplayChatMessage>) {
        self.messages = value;
    }

    pub fn set_is_detailed(&mut self, value: bool) {
        self.is_detailed = value;
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

    pub(crate) fn date_seconds_for_filter(&self) -> u64 {
        if self.date > 0 {
            return self.date;
        }

        ReplayAnalysis::modified_seconds(Path::new(&self.file))
    }

    pub(crate) fn has_detailed_unit_stats(&self) -> bool {
        self.main_units()
            .as_object()
            .is_some_and(|units| !units.is_empty())
            || self
                .ally_units()
                .as_object()
                .is_some_and(|units| !units.is_empty())
            || self
                .amon_units
                .as_object()
                .is_some_and(|units| !units.is_empty())
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
                let mutator_id =
                    TauriOverlayOps::canonical_mutator_id_with_dictionary(mutator, dictionary);
                let (name_en, name_ko, description_en, description_ko) = dictionary
                    .mutator_data(&mutator_id)
                    .map(|value| {
                        (
                            TauriOverlayOps::decode_html_entities(&value.name.en),
                            TauriOverlayOps::decode_html_entities(&value.name.ko),
                            TauriOverlayOps::decode_html_entities(&value.description.en),
                            TauriOverlayOps::decode_html_entities(&value.description.ko),
                        )
                    })
                    .unwrap_or_default();
                let fallback_name_en = TauriOverlayOps::mutator_display_name_en_with_dictionary(
                    &mutator_id,
                    dictionary,
                );
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
                let display_name = TauriOverlayOps::decode_html_entities(mutator);
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
        TauriOverlayOps::to_json_value(self.as_games_row_payload_with_dictionary(dictionary))
    }

    pub fn as_games_row(&self) -> Value {
        TauriOverlayOps::to_json_value(self.as_games_row_payload())
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
            TauriOverlayOps::sanitize_replay_text(&self.result)
        };
        Self {
            file: self.file.clone(),
            date: self.date,
            map: TauriOverlayOps::sanitize_replay_text(
                &dictionary
                    .coop_map_english_name(&self.map)
                    .unwrap_or_else(|| self.map.to_string()),
            ),
            result: client_result,
            difficulty: TauriOverlayOps::sanitize_replay_text(&self.difficulty),
            enemy: TauriOverlayOps::sanitize_replay_text(&self.enemy),
            length: self.length,
            accurate_length: self.accurate_length,
            slot1: self.slot1.sanitized_for_client(),
            slot2: self.slot2.sanitized_for_client(),
            main_slot: self.main_index(),
            amon_units: TauriOverlayOps::sanitize_unit_map(&self.amon_units),
            player_stats: TauriOverlayOps::sanitize_player_stats_payload(&self.player_stats),
            extension: self.extension,
            brutal_plus: self.brutal_plus,
            weekly: self.weekly,
            weekly_name: self
                .weekly_name
                .as_ref()
                .map(|value| TauriOverlayOps::sanitize_replay_text(value))
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
                    text: TauriOverlayOps::sanitize_replay_text(&message.text),
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
            TauriOverlayOps::sanitize_replay_text(&self.result)
        };
        Self {
            file: self.file.clone(),
            date: self.date,
            map: TauriOverlayOps::sanitize_replay_text(&self.map),
            result: client_result,
            difficulty: TauriOverlayOps::sanitize_replay_text(&self.difficulty),
            enemy: TauriOverlayOps::sanitize_replay_text(&self.enemy),
            length: self.length,
            accurate_length: self.accurate_length,
            slot1: self.slot1.sanitized_for_client(),
            slot2: self.slot2.sanitized_for_client(),
            main_slot: self.main_index(),
            amon_units: TauriOverlayOps::sanitize_unit_map(&self.amon_units),
            player_stats: TauriOverlayOps::sanitize_player_stats_payload(&self.player_stats),
            extension: self.extension,
            brutal_plus: self.brutal_plus,
            weekly: self.weekly,
            weekly_name: self
                .weekly_name
                .as_ref()
                .map(|value| TauriOverlayOps::sanitize_replay_text(value))
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
                    text: TauriOverlayOps::sanitize_replay_text(&message.text),
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

impl TauriOverlayOps {
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
}

enum ProgressEmitterCommand {
    Stop,
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn map_display_name(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            raw.to_string()
        } else {
            trimmed.to_string()
        }
    }
}

impl TauriOverlayOps {
    fn unit_excluded_from_stats_for_commander(commander: &str, unit: &str) -> bool {
        (unit == "MULE" && commander != "Raynor")
            || (unit == "Spider Mine" && commander != "Raynor" && commander != "Nova")
            || (unit == "Omega Worm" && commander != "Kerrigan")
            || (unit == "Nydus Worm" && commander != "Abathur")
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn unit_rollup_count_value(value: i64, hidden: bool) -> Value {
        if hidden {
            Value::String("-".to_string())
        } else {
            Value::from(value)
        }
    }
}

impl TauriOverlayOps {
    fn build_amon_unit_data(
        amon_rollup: std::collections::BTreeMap<String, UnitStatsRollup>,
    ) -> Value {
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
                    TauriOverlayOps::to_json_value(AmonUnitRow {
                        created: row.created,
                        lost: row.lost,
                        kills: row.kills,
                        kd: Value::String("-".to_string()),
                    }),
                );
            } else {
                out.insert(
                    unit,
                    TauriOverlayOps::to_json_value(AmonUnitRow {
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
            TauriOverlayOps::to_json_value(AmonUnitRow {
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
}

impl TauriOverlayOps {
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
            let stats_units = TauriOverlayOps::units_to_stats_with_dictionary(dictionary);

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

                if TauriOverlayOps::unit_excluded_from_stats_for_commander(&commander, &unit) {
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
                    TauriOverlayOps::median_f64(&unit_row.kill_percentages)
                };

                if !TauriOverlayOps::unit_excluded_from_sum_for_commander(&commander, &unit) {
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
                    TauriOverlayOps::to_json_value(CommanderUnitRow {
                        created: TauriOverlayOps::unit_rollup_count_value(
                            unit_row.created,
                            unit_row.created_hidden,
                        ),
                        made,
                        lost: TauriOverlayOps::unit_rollup_count_value(
                            unit_row.lost,
                            unit_row.lost_hidden,
                        ),
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
                TauriOverlayOps::to_json_value(CommanderUnitRow {
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
}

impl TauriOverlayOps {
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
                    _ if lower.starts_with("&#") && lower.ends_with(';') => lower
                        [2..lower.len() - 1]
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
}

impl TauriOverlayOps {
    fn normalize_mastery_values(raw: &[u64]) -> Vec<u64> {
        let mut values = vec![0u64; 6];
        for (index, value) in raw.iter().take(6).enumerate() {
            values[index] = *value;
        }
        values
    }
}

impl TauriOverlayOps {
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
                        TauriOverlayOps::sanitize_replay_text(key),
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
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
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
                        TauriOverlayOps::to_json_value(crate::shared_types::ReplayPlayerSeries {
                            name: TauriOverlayOps::sanitize_replay_text(&name),
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
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn format_date_from_system_time(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }
}

impl TauriOverlayOps {
    fn clear_analysis_cache_files() {
        let cache_path = PathManagerOps::get_cache_path();
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
}

impl TauriOverlayOps {
    fn generate_detailed_analysis_cache(
        app: &AppHandle<Wry>,
        stats: &Arc<Mutex<StatsState>>,
        worker_count: usize,
        stop_controller: Arc<GenerateCacheStopController>,
    ) -> Result<GenerateCacheSummary, String> {
        let state = app.state::<BackendState>();
        let settings = state.read_settings_memory();
        let replay_scan_progress = state.replay_scan_progress();
        let Some(account_dir) = settings.resolve_replay_root() else {
            return Err("Replay root is not configured for detailed analysis.".to_string());
        };
        let output_file = PathManagerOps::get_cache_path();
        let logger = {
            let app = app.clone();
            let stats = Arc::clone(stats);
            let replay_scan_progress = replay_scan_progress.clone();
            move |message: String| {
                if let Some((completed, total)) =
                    TauriOverlayOps::parse_detailed_analysis_progress_counts(&message)
                {
                    replay_scan_progress.set_counts(total, completed);
                }
                let normalized =
                    TauriOverlayOps::normalize_detailed_analysis_logger_message(&message);
                crate::sco_log!("[SCO/stats] {normalized}");
                replay_scan_progress.set_stage("detailed_analysis_running");
                replay_scan_progress.set_status("Parsing");
                if let Ok(mut guard) = stats.lock() {
                    guard.set_detailed_analysis_status(normalized.clone());
                    guard.set_message(normalized.clone());
                }
                TauriOverlayOps::emit_replay_scan_progress(&app, false);
            }
        };

        let resources = state
            .replay_analysis_resources()
            .map_err(|error| format!("Failed to access replay analysis resources: {error}"))?;

        let config = GenerateCacheConfig::new(account_dir, output_file.clone());
        let runtime = GenerateCacheRuntimeOptions::default()
            .with_worker_count(worker_count)
            .with_stop_controller(stop_controller);
        DetailedReplayAnalyzer::analyze_full_detailed(
            &config,
            resources.as_ref(),
            Some(&logger),
            &runtime,
        )
        .map_err(|error| format!("Failed to generate '{}': {error}", output_file.display()))
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn infer_region_from_handle(handle: &str) -> Option<String> {
        let region_code = handle.split('-').next().map(str::trim)?;
        if region_code.is_empty() {
            return None;
        }
        TauriOverlayOps::normalize_region_code(region_code).map(|region| region.to_string())
    }
}

impl StartupAnalysisTrigger {
    fn label(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::FrontendReady => "frontend_ready",
        }
    }
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

impl TauriOverlayOps {
    fn analysis_mode(include_detailed: bool) -> AnalysisMode {
        AnalysisMode::from_include_detailed(include_detailed)
    }
}

impl TauriOverlayOps {
    fn analysis_status_text(mode: AnalysisMode, phase: &str) -> String {
        format!("{}: {phase}.", mode.display())
    }
}

impl TauriOverlayOps {
    fn analysis_started_message(mode: AnalysisMode) -> String {
        TauriOverlayOps::analysis_status_text(mode, "started in background")
    }
}

impl TauriOverlayOps {
    fn analysis_already_running_message(mode: AnalysisMode) -> String {
        TauriOverlayOps::analysis_status_text(mode, "already running")
    }
}

impl TauriOverlayOps {
    fn analysis_blocked_by_other_mode_message(mode: AnalysisMode) -> String {
        format!(
            "{} cannot start while {} is running.",
            mode.display(),
            mode.peer_display()
        )
    }
}

impl TauriOverlayOps {
    fn analysis_at_start_message(enabled: bool) -> String {
        if enabled {
            "Detailed analysis at startup enabled.".to_string()
        } else {
            "Detailed analysis at startup disabled.".to_string()
        }
    }
}

impl TauriOverlayOps {
    fn analysis_error_status_text(mode: AnalysisMode, message: &str) -> String {
        format!("{}: {message}", mode.display())
    }
}

impl TauriOverlayOps {
    fn analysis_elapsed_suffix(elapsed: Duration) -> String {
        format!("Time consumed: {:.2} s.", elapsed.as_secs_f64())
    }
}

impl TauriOverlayOps {
    fn analysis_completed_message(
        mode: AnalysisMode,
        replay_count: u64,
        elapsed: Duration,
    ) -> String {
        let summary = if replay_count == 0 {
            "No replay files found.".to_string()
        } else {
            format!(
                "{} completed with {replay_count} replay file(s).",
                mode.display()
            )
        };
        format!(
            "{summary} {}",
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }
}

impl TauriOverlayOps {
    fn analysis_stopped_message(mode: AnalysisMode, detail: &str, elapsed: Duration) -> String {
        format!(
            "{} stopped. {} {}",
            mode.display(),
            detail,
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }
}

impl TauriOverlayOps {
    fn analysis_failed_message(mode: AnalysisMode, message: &str, elapsed: Duration) -> String {
        format!(
            "{} failed: {message} {}",
            mode.display(),
            TauriOverlayOps::analysis_elapsed_suffix(elapsed)
        )
    }
}

impl TauriOverlayOps {
    fn normalize_detailed_analysis_logger_message(message: &str) -> String {
        let normalized = message.replace('\n', " | ");
        if normalized == "Starting detailed analysis!" {
            return TauriOverlayOps::analysis_status_text(
                AnalysisMode::Detailed,
                "generating cache",
            );
        }
        if normalized.starts_with("Running... ")
            || normalized.starts_with("Estimated remaining time:")
        {
            return format!(
                "{}: cache generation progress | {normalized}",
                AnalysisMode::Detailed.display()
            );
        }
        if normalized.starts_with("Detailed analysis completed! ") {
            return TauriOverlayOps::analysis_status_text(
                AnalysisMode::Detailed,
                "cache generation completed",
            );
        }
        if normalized.starts_with("Detailed analysis completed in ") {
            return format!("{}: {}", AnalysisMode::Detailed.display(), normalized);
        }
        normalized
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    pub fn parse_detailed_analysis_progress_counts(message: &str) -> Option<(u64, u64)> {
        for line in message.lines().map(str::trim) {
            if let Some(progress) = line.strip_prefix("Running... ") {
                return TauriOverlayOps::parse_progress_fraction(progress);
            }
            if let Some(progress) = line.strip_prefix("Detailed analysis completed! ") {
                return TauriOverlayOps::parse_progress_fraction(progress);
            }
        }
        None
    }
}

impl TauriOverlayOps {
    fn startup_analysis_mode(include_detailed: bool) -> &'static str {
        TauriOverlayOps::analysis_mode(include_detailed).slug()
    }
}

impl TauriOverlayOps {
    pub fn prepare_startup_analysis_request(
        stats: &mut StatsState,
        trigger: StartupAnalysisTrigger,
    ) -> StartupAnalysisRequestOutcome {
        let include_detailed = stats.detailed_analysis_atstart();
        if stats.startup_analysis_requested() {
            return StartupAnalysisRequestOutcome {
                include_detailed,
                started: false,
            };
        }

        stats.set_startup_analysis_requested(true);
        let mode = TauriOverlayOps::analysis_mode(include_detailed);
        stats.set_message(match trigger {
            StartupAnalysisTrigger::Setup => format!(
                "{}: startup requested while the frontend loads.",
                mode.display()
            ),
            StartupAnalysisTrigger::FrontendReady => {
                format!("{}: startup requested in background.", mode.display())
            }
        });

        StartupAnalysisRequestOutcome {
            include_detailed,
            started: true,
        }
    }
}

impl TauriOverlayOps {
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
            TauriOverlayOps::prepare_startup_analysis_request(&mut guard, trigger)
        };

        if outcome.started {
            crate::sco_log!(
                "[SCO/stats] startup analysis requested from {} mode={}",
                trigger.label(),
                TauriOverlayOps::startup_analysis_mode(outcome.include_detailed)
            );
            TauriOverlayOps::spawn_startup_analysis_task(
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
                TauriOverlayOps::startup_analysis_mode(outcome.include_detailed)
            );
        }

        Ok(outcome)
    }
}

impl TauriOverlayOps {
    pub fn update_analysis_replay_cache_slots(
        replays: &[ReplayInfo],
        replays_slot: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
    ) {
        if let Ok(mut cache) = replays_slot.lock() {
            for replay in replays {
                let replay_hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(&replay.file));
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
                        cache.retain(|hash, entry| {
                            hash == &replay_hash || entry.file != replay.file
                        });
                        cache.insert(replay_hash.clone(), replay.clone());
                    }
                }
            }
        } else {
            crate::sco_log!("[SCO/stats] failed to update shared replay cache after scan");
        }
    }
}

impl TauriOverlayOps {
    fn load_existing_cache_by_hash() -> HashMap<String, CacheReplayEntry> {
        let cache_path = PathManagerOps::get_cache_path();
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
}

impl TauriOverlayOps {
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

            if merged.values().any(|existing| {
                existing.file == entry.file
                    && existing.hash != entry.hash
                    && existing.detailed_analysis
                    && !entry.detailed_analysis
            }) {
                continue;
            }
            merged.retain(|existing_hash, existing| {
                existing_hash == &hash || existing.file != entry.file
            });

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
}

impl TauriOverlayOps {
    fn run_analysis(
        app: &AppHandle<Wry>,
        analysis_state: &Arc<Mutex<StatsState>>,
        detailed_stop_controller_slot: &Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
        limit: usize,
        include_detailed: bool,
    ) -> Result<AnalysisOutcome, String> {
        let state = app.state::<BackendState>();
        if include_detailed {
            let existing_cache_by_hash = TauriOverlayOps::load_existing_cache_by_hash();
            let worker_count = state
                .read_settings_memory()
                .normalized_analysis_worker_threads();
            let stop_controller = Arc::new(GenerateCacheStopController::new());
            if let Ok(mut slot) = detailed_stop_controller_slot.lock() {
                *slot = Some(stop_controller.clone());
            }

            let generation_result = TauriOverlayOps::generate_detailed_analysis_cache(
                app,
                analysis_state,
                worker_count,
                stop_controller,
            );

            if let Ok(mut slot) = detailed_stop_controller_slot.lock() {
                slot.take();
            }

            let generation_summary = generation_result?;
            let scanned_replays = generation_summary.scanned_replays();
            let completed = generation_summary.completed();
            crate::sco_log!(
                "[SCO/stats] detailed scan generated '{}' with {} replay(s) completed={completed}",
                PathManagerOps::get_cache_path().display(),
                scanned_replays
            );

            let main_names = state.configured_main_names();
            let main_handles = state.configured_main_handles();
            let dictionary = state.dictionary_data()?;
            let new_cache_entries = generation_summary.into_cache_entries();
            let replays =
                ReplayAnalysis::detailed_analysis_replays_snapshot_from_entries_with_dictionary(
                    &new_cache_entries,
                    limit,
                    &main_names,
                    &main_handles,
                    dictionary.as_ref(),
                );
            let final_cache_entries =
                TauriOverlayOps::merge_cache_entries(&existing_cache_by_hash, new_cache_entries);

            Ok(AnalysisOutcome::new(
                scanned_replays,
                replays,
                final_cache_entries,
                completed,
            ))
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
            let final_cache_entries = TauriOverlayOps::load_existing_cache_by_hash()
                .into_values()
                .collect();

            Ok(AnalysisOutcome::new(
                replays.len(),
                replays,
                final_cache_entries,
                true,
            ))
        }
    }
}

impl TauriOverlayOps {
    fn spawn_analysis_task(
        app: AppHandle<Wry>,
        stats: Arc<Mutex<StatsState>>,
        replays_slot: Arc<Mutex<HashMap<String, ReplayInfo>>>,
        stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
        detailed_stop_controller_slot: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
        include_detailed: bool,
        limit: usize,
    ) {
        let mode = TauriOverlayOps::analysis_mode(include_detailed);
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

            if guard.analysis_running() {
                let active_mode = guard.analysis_running_mode();
                if active_mode == Some(mode) {
                    crate::sco_log!("[SCO/stats] {} already running", mode.display());
                    guard.set_message(TauriOverlayOps::analysis_already_running_message(mode));
                } else {
                    crate::sco_log!(
                        "[SCO/stats] {} blocked while another analysis is running",
                        mode.display()
                    );
                    guard.set_message(TauriOverlayOps::analysis_blocked_by_other_mode_message(
                        mode,
                    ));
                }
                return;
            }
            guard.start_analysis(mode);
            guard.set_analysis_running_status(
                mode,
                if include_detailed {
                    "generating cache"
                } else {
                    "scanning replays"
                },
            );
            guard.set_message(TauriOverlayOps::analysis_started_message(mode));

            guard.set_ready(false);
            guard.set_analysis(Some(TauriOverlayOps::empty_stats_payload()));
            guard.set_games(0);
            guard.clear_main_identities();
            guard.clear_prestige_names();
            if guard.message().is_empty() {
                guard.set_message(TauriOverlayOps::analysis_started_message(mode));
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
            TauriOverlayOps::emit_replay_scan_progress(&app_for_progress, true);

            let (progress_tx, progress_rx) = mpsc::channel::<ProgressEmitterCommand>();
            let progress_handle = thread::spawn(move || loop {
                match progress_rx.recv_timeout(Duration::from_millis(150)) {
                    Ok(ProgressEmitterCommand::Stop) => {
                        TauriOverlayOps::emit_replay_scan_progress(&app_for_progress_updates, true);
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        TauriOverlayOps::emit_replay_scan_progress(
                            &app_for_progress_updates,
                            false,
                        );
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            });

            let analysis_outcome = match TauriOverlayOps::run_analysis(
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
                        guard.set_analysis_terminal_status(mode, "failed");
                        guard.set_detailed_analysis_status(
                            TauriOverlayOps::analysis_error_status_text(mode, &message),
                        );
                        guard.set_message(TauriOverlayOps::analysis_failed_message(
                            mode, &message, elapsed,
                        ));
                    }
                    replay_scan_progress_for_thread.set_stage("analysis_failed");
                    replay_scan_progress_for_thread.set_status("Completed");
                    let _ = progress_tx.send(ProgressEmitterCommand::Stop);
                    let completion_message = analysis_state
                        .lock()
                        .map(|guard| guard.message().to_string())
                        .unwrap_or_else(|_| {
                            TauriOverlayOps::analysis_failed_message(mode, &message, elapsed)
                        });
                    TauriOverlayOps::emit_analysis_completed(
                        &app_for_analysis,
                        mode,
                        &completion_message,
                    );
                    let _ = progress_handle.join();
                    return;
                }
            };

            if let Ok(mut guard) = analysis_state.lock() {
                if include_detailed {
                    let replay_count = analysis_outcome.reported_replay_count();
                    if analysis_outcome.analysis_completed() {
                        guard.set_analysis_running_status(mode, "refreshing replay summaries");
                        guard.set_message(format!(
                            "Generated '{}' with {} replay entr{}.",
                            PathManagerOps::get_cache_path().display(),
                            replay_count,
                            if replay_count == 1 { "y" } else { "ies" }
                        ));
                    } else {
                        guard.set_analysis_running(false);
                        guard.set_detailed_analysis_status(TauriOverlayOps::analysis_status_text(
                            mode, "stopped",
                        ));
                        guard.set_message(format!(
                            "Detailed analysis stopped after saving {} replay entr{}.",
                            replay_count,
                            if replay_count == 1 { "y" } else { "ies" }
                        ));
                    }
                }
            }

            let (_reported_replay_count, all_replays, final_cache_entries, detailed_completed) =
                analysis_outcome.into_parts();

            let mut hashes = HashMap::new();

            let all_replays = all_replays
                .into_iter()
                .filter(|replay| {
                    let hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(&replay.file));

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
                settings_for_thread.current_replay_files_snapshot(UNLIMITED_REPLAY_LIMIT);
            if include_detailed && !detailed_completed {
                replay_scan_progress_for_thread.set_total(current_replay_files.len() as u64);
            }
            TauriOverlayOps::update_analysis_replay_cache_slots(
                &all_replays,
                &shared_replay_cache_slot,
            );
            if let Ok(mut current_files) = current_replay_files_slot.lock() {
                *current_files = current_replay_files;
            } else {
                crate::sco_log!("[SCO/stats] failed to update current replay file set after scan");
            }

            if include_detailed {
                let cache_path = PathManagerOps::get_cache_path();
                if let Err(error) =
                    CacheReplayEntry::write_entries(&final_cache_entries, &cache_path)
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
                .unwrap_or_else(|| {
                    StatsSnapshot::new(
                        true,
                        all_replays.len() as u64,
                        Vec::new(),
                        Vec::new(),
                        Value::Null,
                        Default::default(),
                        "Dictionary data is unavailable.",
                    )
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
                    TauriOverlayOps::emit_analysis_completed(
                        &app_for_analysis,
                        mode,
                        &TauriOverlayOps::analysis_error_status_text(
                            mode,
                            "analysis aborted before rebuild",
                        ),
                    );
                    let _ = progress_handle.join();
                    return;
                }
            };

            if include_detailed && !detailed_completed {
                guard.set_analysis_running(false);
            } else {
                guard.set_analysis_running_status(mode, "building statistics");
            }

            TauriOverlayOps::apply_rebuild_snapshot(&mut guard, snapshot, mode);
            if include_detailed && !detailed_completed {
                guard.set_analysis_running(false);
                guard.set_detailed_analysis_status(TauriOverlayOps::analysis_status_text(
                    mode, "stopped",
                ));
                guard.set_message(TauriOverlayOps::analysis_stopped_message(
                    mode,
                    "Run detailed analysis to continue generating cache.",
                    started_at.elapsed(),
                ));
            } else {
                let games = guard.games();
                guard.set_message(TauriOverlayOps::analysis_completed_message(
                    mode,
                    games,
                    started_at.elapsed(),
                ));
            }
            if !include_detailed {
                if let Some(dictionary) = dictionary.as_deref() {
                    guard.sync_detailed_analysis_status_from_replays_with_dictionary(
                        &all_replays,
                        dictionary,
                    );
                } else {
                    guard.sync_detailed_analysis_status_from_replays(&all_replays);
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

            let completion_message = guard.message().to_string();
            drop(guard);
            TauriOverlayOps::emit_analysis_completed(&app_for_analysis, mode, &completion_message);
            let _ = progress_handle.join();
        });
    }
}

impl TauriOverlayOps {
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
            TauriOverlayOps::startup_analysis_mode(include_detailed)
        );
        TauriOverlayOps::spawn_analysis_task(
            app,
            stats,
            replays_slot,
            stats_current_replay_files_slot,
            detailed_stop_controller_slot,
            include_detailed,
            UNLIMITED_REPLAY_LIMIT,
        );
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn query_hex_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
}

impl TauriOverlayOps {
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
                    let high = TauriOverlayOps::query_hex_value(bytes[index + 1]);
                    let low = TauriOverlayOps::query_hex_value(bytes[index + 2]);
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
}

impl TauriOverlayOps {
    fn parse_query_usize(path: &str, key: &str, default: usize) -> usize {
        TauriOverlayOps::parse_query_i64(path, key)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(default)
    }
}

impl TauriOverlayOps {
    fn parse_query_value(path: &str, key: &str) -> Option<String> {
        let query = path.split('?').nth(1)?;
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let parsed_key = parts.next()?;
            if parsed_key != key {
                continue;
            }
            let value = parts.next().unwrap_or_default();
            return Some(TauriOverlayOps::decode_query_component(value));
        }
        None
    }
}

impl TauriOverlayOps {
    fn parse_query_bool(path: &str, key: &str, default: bool) -> bool {
        match TauriOverlayOps::parse_query_value(path, key)
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
}

impl TauriOverlayOps {
    fn parse_query_csv(path: &str, key: &str) -> Vec<String> {
        TauriOverlayOps::parse_query_value(path, key)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect()
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn replay_index_by_file(replays: &[ReplayInfo], file: &Option<String>) -> Option<usize> {
        let needle = file.as_deref()?;
        replays.iter().position(|entry| entry.file == needle)
    }
}

impl TauriOverlayOps {
    fn replay_visual_context_from_replay(replay: &ReplayInfo) -> ReplayVisualContext {
        let main_player_id = if replay.main_index() == 0 { 1 } else { 2 };
        let duration_seconds =
            if replay.accurate_length().is_finite() && replay.accurate_length() > 0.0 {
                replay.accurate_length().round() as u64
            } else {
                replay.length()
            };
        ReplayVisualContext::new(
            replay.file(),
            replay.map(),
            replay.result(),
            duration_seconds,
            main_player_id,
        )
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
        let (replay, _) = ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(
            replay_path,
            resources,
        )
        .ok_or_else(|| format!("Failed to parse replay file: {requested_file}"))?;
        Ok(dictionary
            .as_deref()
            .map(|dictionary| replay.chat_payload_with_dictionary(dictionary))
            .unwrap_or_else(|| replay.chat_payload_with_dictionary(resources.dictionary_data())))
    }

    fn replay_visual_payload_from_slots(
        replay_state: Arc<Mutex<ReplayState>>,
        settings: AppSettings,
        main_names: HashSet<String>,
        main_handles: HashSet<String>,
        file: &str,
        dictionary: Arc<Sc2DictionaryData>,
        resources: Arc<ReplayAnalysisResources>,
    ) -> Result<ReplayVisualPayload, String> {
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
                    Some(resources.as_ref()),
                )
            })
            .unwrap_or_default();

        let replay_path = Path::new(requested_file);
        if !replay_path.exists() {
            return Err(format!("Replay file not found: {requested_file}"));
        }

        if let Some(replay) = replays
            .iter()
            .find(|replay| replay.file() == requested_file)
        {
            let context = Self::replay_visual_context_from_replay(replay);
            return ReplayVisualOps::payload_from_file(
                replay_path,
                resources.as_ref(),
                dictionary.as_ref(),
                &context,
            );
        }

        let (replay, _) = ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(
            replay_path,
            resources.as_ref(),
        )
        .ok_or_else(|| format!("Failed to parse replay file: {requested_file}"))?;
        let context = Self::replay_visual_context_from_replay(&replay);
        ReplayVisualOps::payload_from_file(
            replay_path,
            resources.as_ref(),
            dictionary.as_ref(),
            &context,
        )
    }
}

impl TauriOverlayOps {
    fn path_is_sc2_replay(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("SC2Replay"))
    }
}

impl TauriOverlayOps {
    fn is_replay_creation_event(kind: &EventKind) -> bool {
        matches!(kind, EventKind::Any)
            || matches!(kind, EventKind::Create(_))
            || matches!(kind, EventKind::Modify(_))
    }
}

impl TauriOverlayOps {
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
                        .map(TauriOverlayOps::format_date_from_system_time)
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
                    let panic_message = if let Some(message) = panic_payload.downcast_ref::<&str>()
                    {
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
}

impl TauriOverlayOps {
    pub fn persist_detailed_cache_entry_to_path(
        cache_path: &Path,
        entry: &CacheReplayEntry,
    ) -> Result<(), String> {
        let local_lock = Mutex::new(());
        TauriOverlayOps::persist_detailed_cache_entry_to_path_with_lock(
            cache_path,
            entry,
            &local_lock,
        )
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn spawn_detailed_cache_persist(
        state: &BackendState,
        entry: CacheReplayEntry,
        log_prefix: &'static str,
    ) {
        let persist_lock = state.detailed_cache_persist_lock();
        thread::spawn(move || {
            let replay_file = entry.file.clone();
            if let Err(error) = TauriOverlayOps::persist_detailed_cache_entry_to_path_with_lock(
                &PathManagerOps::get_cache_path(),
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
}

impl TauriOverlayOps {
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
                if meta.is_file() && TauriOverlayOps::path_is_sc2_replay(&path) {
                    out.push(path);
                }
            }
        }
        out
    }
}

impl StatsState {
    pub fn sync_detailed_analysis_status_from_replays(&mut self, replays: &[ReplayInfo]) {
        let total_valid_files = replays
            .iter()
            .filter(|replay| replay.result != "Unparsed" && replay.map.trim().starts_with("AC_"))
            .count();
        let detailed_parsed_count = replays
            .iter()
            .filter(|replay| {
                replay.result != "Unparsed"
                    && replay.map.trim().starts_with("AC_")
                    && replay.has_detailed_unit_stats()
            })
            .count();

        self.set_analysis_running(false);
        self.set_detailed_analysis_status(if detailed_parsed_count == 0 {
            TauriOverlayOps::analysis_status_text(AnalysisMode::Detailed, "not started")
        } else {
            format!(
                "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
            )
        });
    }

    pub fn sync_detailed_analysis_status_from_replays_with_dictionary(
        &mut self,
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
                    && replay.has_detailed_unit_stats()
            })
            .count();

        self.set_analysis_running(false);
        self.set_detailed_analysis_status(if detailed_parsed_count == 0 {
            TauriOverlayOps::analysis_status_text(AnalysisMode::Detailed, "not started")
        } else {
            format!(
                "Detailed analysis: loaded from cache ({detailed_parsed_count}/{total_valid_files})."
            )
        });
    }
}

impl TauriOverlayOps {
    fn process_new_replay_path(
        app: &tauri::AppHandle<Wry>,
        path: &Path,
        handled_files: &mut HashSet<String>,
    ) -> ReplayProcessOutcome {
        if !TauriOverlayOps::path_is_sc2_replay(path) {
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
        let Some((parsed, cache_entry)) =
            TauriOverlayOps::parse_new_replay_with_retries(path, resources.as_deref())
        else {
            crate::sco_log!("[SCO/watch] failed to parse new replay '{}'", file);
            return ReplayProcessOutcome::RetryLater;
        };

        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let replay = parsed.oriented_for_main_identity(&main_names, &main_handles);
        let replay_hash = cache_entry
            .as_ref()
            .map(|entry| entry.hash.clone())
            .filter(|hash| !hash.is_empty())
            .unwrap_or_else(|| ReplayFileIdentity::calculate_hash(path));
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
        let show_replay_info_after_game = settings.show_replay_info_after_game();

        if show_replay_info_after_game {
            crate::sco_log!(
                "[SCO/watch] emitting replay to overlay file='{}'",
                replay.file
            );
            overlay_info::OverlayInfoOps::emit_replay_to_overlay_from_replay(app, &replay, true);
            state.set_overlay_replay_data_active(true);
        } else {
            crate::sco_log!(
                "[SCO/watch] replay overlay suppressed by settings file='{}'",
                replay.file
            );
            state.set_overlay_replay_data_active(false);
        }

        if let Some(cache_entry) = cache_entry {
            TauriOverlayOps::spawn_detailed_cache_persist(&state, cache_entry, "watch");
        }

        let invalidation_generation = state.invalidate_delayed_player_stats_popup_generation();
        crate::sco_log!(
            "[SCO/watch] invalidated delayed player stats popups generation={} replay='{}'",
            invalidation_generation,
            replay.file
        );

        ReplayProcessOutcome::Processed
    }
}

impl TauriOverlayOps {
    fn process_replay_detailed(
        state: &BackendState,
        path: &Path,
    ) -> (ReplayProcessOutcome, Option<ReplayInfo>) {
        if !TauriOverlayOps::path_is_sc2_replay(path) {
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

        let replay_hash = ReplayFileIdentity::calculate_hash(path);
        if let Some(existing) = state.cached_replay_by_hash(&replay_hash) {
            if existing.is_detailed {
                return (ReplayProcessOutcome::Processed, Some(existing));
            }
        }

        let resources = state.replay_analysis_resources().ok();
        let Some((parsed, cache_entry)) =
            TauriOverlayOps::parse_new_replay_with_retries(path, resources.as_deref())
        else {
            crate::sco_log!("[SCO/show] failed to parse existing replay '{}'", file);
            return (ReplayProcessOutcome::RetryLater, None);
        };

        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let replay = parsed.oriented_for_main_identity(&main_names, &main_handles);

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
            .unwrap_or_else(|| ReplayFileIdentity::calculate_hash(path));
        state.upsert_replay_in_memory_cache(&replay_hash, &replay);
        if let Some(cache_entry) = cache_entry {
            TauriOverlayOps::spawn_detailed_cache_persist(state, cache_entry, "show");
        }

        (ReplayProcessOutcome::Processed, Some(replay))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayProcessOutcome {
    Processed,
    RetryLater,
    AlreadyHandled,
    Ignored,
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn spawn_replay_creation_watcher(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            let replay_root = loop {
                let settings = app.state::<BackendState>().read_settings_memory();
                if let Some(root) = settings.replay_watch_root() {
                    break root;
                }
                crate::sco_log!(
                    "[SCO/watch] account_folder replay root unavailable, retrying in 5s"
                );
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
            for path in TauriOverlayOps::collect_sc2_replay_files(&replay_root) {
                let key = path.to_string_lossy().to_string();
                if !key.is_empty() {
                    handled_files.insert(key);
                }
            }
            let mut pending_fallback_files = HashSet::<String>::new();

            loop {
                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(Ok(event)) => {
                        if !TauriOverlayOps::is_replay_creation_event(&event.kind) {
                            continue;
                        }
                        crate::sco_log!(
                            "[SCO/watch] notify event kind={:?} paths={}",
                            event.kind,
                            event.paths.len()
                        );

                        for path in event.paths {
                            if !TauriOverlayOps::path_is_sc2_replay(&path) {
                                continue;
                            }
                            let key = path.to_string_lossy().to_string();
                            if key.is_empty() {
                                continue;
                            }
                            let outcome = TauriOverlayOps::process_new_replay_path(
                                &app,
                                &path,
                                &mut handled_files,
                            );
                            TauriOverlayOps::update_pending_fallback_file(
                                &mut pending_fallback_files,
                                &key,
                                outcome,
                            );
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
                            let outcome = TauriOverlayOps::process_new_replay_path(
                                &app,
                                &path,
                                &mut handled_files,
                            );
                            TauriOverlayOps::update_pending_fallback_file(
                                &mut pending_fallback_files,
                                &file,
                                outcome,
                            );
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        crate::sco_log!(
                            "[SCO/watch] replay watcher channel disconnected; stopping"
                        );
                        break;
                    }
                }
            }
        });
    }
}

impl TauriOverlayOps {
    fn spawn_game_launch_player_stats_task(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(4));

            let mut launch_detector = GameLaunchDetector::new(Instant::now());

            loop {
                thread::sleep(Duration::from_millis(500));

                let state = app.state::<BackendState>();
                let settings = state.read_settings_memory();
                let show_player_stats_popups = settings.show_player_winrates();
                if !show_player_stats_popups {
                    continue;
                }

                let replay_count = state.replay_count_for_launch_detector();
                let now = Instant::now();
                launch_detector.observe_replay_count(replay_count, now);

                let Some(payload) = LiveGameOps::fetch_sc2_live_game_payload() else {
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

                if LiveGameOps::extract_live_game_players(&payload) <= 2 {
                    launch_detector.observe_non_live_state();
                    continue;
                }
                if LiveGameOps::all_players_are_users(&payload) {
                    launch_detector.observe_non_live_state();
                    continue;
                }

                let display_time =
                    LiveGameOps::value_as_u64_lossy(payload.get("displayTime")).unwrap_or(0);
                match launch_detector.update_display_time_status(display_time) {
                    GameLaunchStatus::Started => {}
                    GameLaunchStatus::Unknown
                    | GameLaunchStatus::Idle
                    | GameLaunchStatus::Running
                    | GameLaunchStatus::Ended => continue,
                }

                if !launch_detector
                    .should_attempt_popup(state.stats_have_player_rows(), replay_count)
                {
                    continue;
                }
                if !launch_detector.replay_change_settled(now) {
                    continue;
                }

                let (main_names, main_handles) = state.build_launch_main_identity();
                let Some((other_player_handle, other_player_name)) =
                    LiveGameOps::choose_other_coop_player_stats(
                        &payload,
                        &main_names,
                        &main_handles,
                    )
                else {
                    continue;
                };

                let invalidation_generation =
                    state.invalidate_delayed_player_stats_popup_generation();
                crate::sco_log!(
                    "[SCO/launch] invalidated delayed player stats popups generation={}",
                    invalidation_generation
                );

                if overlay_info::OverlayInfoOps::show_player_stats_for_name(
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
}

impl TauriOverlayOps {
    fn spawn_protocol_store_warmup() {
        thread::spawn(|| {
            let started_at = Instant::now();
            match s2protocol_port::ProtocolStoreBuilder::build() {
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
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn ratio(numerator: u64, denominator: u64) -> f64 {
        if denominator == 0 {
            0.0
        } else {
            numerator as f64 / denominator as f64
        }
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn kill_fraction(main_kills: u64, ally_kills: u64) -> f64 {
        let total = main_kills + ally_kills;
        if total == 0 {
            0.0
        } else {
            main_kills as f64 / total as f64
        }
    }
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn normalized_commander_name(commander: &str, _fallback: &str) -> String {
        let trimmed = commander.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            TauriOverlayOps::normalize_known_commander_name(trimmed)
                .unwrap_or(trimmed)
                .to_string()
        }
    }
}

impl TauriOverlayOps {
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

        TauriOverlayOps::to_json_value(EmptyStatsPayload {
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
}

impl TauriOverlayOps {
    pub(crate) fn apply_rebuild_snapshot(
        stats: &mut StatsState,
        snapshot: StatsSnapshot,
        mode: AnalysisMode,
    ) {
        let (ready, games, main_players, main_handles, analysis, prestige_names, message) =
            snapshot.into_parts();
        stats.set_ready(ready);
        stats.set_games(games);
        stats.set_main_players(main_players);
        stats.set_main_handles(main_handles);
        stats.set_analysis(Some(analysis));
        stats.set_prestige_names(prestige_names);
        stats.set_message(message);

        stats.set_analysis_terminal_status(mode, "completed");
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowCloseAction {
    AllowClose,
    HidePerformance,
    HideWindow,
    ExitApp,
}

impl TauriOverlayOps {
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
}

impl TauriOverlayOps {
    fn request_clean_exit(app: &AppHandle<Wry>, exit_code: i32) {
        let state = app.state::<BackendState>();
        if !state.try_begin_exit() {
            return;
        }

        if let Ok(mut tray_icon) = app.state::<TrayState>().tray_icon.lock() {
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
}

#[tauri::command]
fn performance_start_drag(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    performance_overlay::PerformanceOverlayOps::start_drag(&app)
}

#[tauri::command]
async fn pick_folder(title: String, directory: Option<String>) -> Result<Option<String>, String> {
    let start_directory = TauriOverlayOps::folder_dialog_start_directory(directory);
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
    PathManagerOps::is_dev_env()
}

#[tauri::command]
fn save_overlay_screenshot(path: String, png_bytes: Vec<u8>) -> Result<(), String> {
    overlay_info::OverlayInfoOps::save_overlay_screenshot(Path::new(&path), &png_bytes)
}

#[tauri::command]
fn open_folder_path(path: String) -> Result<(), String> {
    overlay_info::OverlayInfoOps::open_folder_in_explorer(&path)
}

#[tauri::command]
async fn config_get(
    app: tauri::AppHandle<Wry>,
    state: State<'_, BackendState>,
) -> Result<ConfigPayload, String> {
    state.log_request("get", "/config", &None);
    Ok(ConfigPayload {
        status: "ok",
        settings: AppSettings::from_saved_file(),
        active_settings: state.read_settings_memory(),
        randomizer_catalog: state
            .dictionary_data()
            .map(|dictionary| {
                randomizer::RandomizerOps::catalog_payload_with_dictionary(&dictionary)
            })
            .unwrap_or_default(),
        monitor_catalog: monitor_settings::MonitorSettingsOps::available_monitor_catalog(&app),
    })
}

#[tauri::command]
async fn config_update(
    app: tauri::AppHandle<Wry>,
    settings: Value,
    persist: Option<bool>,
    state: State<'_, BackendState>,
) -> Result<ConfigPayload, String> {
    let body = Some(TauriOverlayOps::to_json_value(serde_json::json!({
        "settings": settings,
        "persist": persist.unwrap_or(true),
    })));
    state.log_request("post", "/config", &body);

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

    next_settings.set_performance_geometry(previous_settings.performance_geometry());

    if persist {
        state.write_settings_file(&next_settings)?;
    }
    TauriOverlayOps::apply_runtime_settings(&app, &previous_settings, &next_settings);

    Ok(ConfigPayload {
        status: "ok",
        settings: AppSettings::from_saved_file(),
        active_settings: state.read_settings_memory(),
        randomizer_catalog: state
            .dictionary_data()
            .map(|dictionary| {
                randomizer::RandomizerOps::catalog_payload_with_dictionary(&dictionary)
            })
            .unwrap_or_default(),
        monitor_catalog: monitor_settings::MonitorSettingsOps::available_monitor_catalog(&app),
    })
}

#[tauri::command]
async fn config_replays_get(
    _app: tauri::AppHandle<Wry>,
    limit: Option<usize>,
    state: State<'_, BackendState>,
) -> Result<ConfigReplaysPayload, String> {
    let path = format!("/config/replays?limit={}", limit.unwrap_or(300));
    state.log_request("get", &path, &None);
    let limit = TauriOverlayOps::parse_query_usize(&path, "limit", 300);
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
    let path = format!(
        "/config/players?limit={}",
        limit.unwrap_or(UNLIMITED_REPLAY_LIMIT)
    );
    state.log_request("get", &path, &None);
    let limit = TauriOverlayOps::parse_query_usize(&path, "limit", UNLIMITED_REPLAY_LIMIT);
    let replay_state = state.get_replay_state();
    let main_names = state.configured_main_names();
    let main_handles = state.configured_main_handles();
    let settings = state.read_settings_memory();
    let resources = state.replay_analysis_resources().ok();

    let (players, total_players) = tauri::async_runtime::spawn_blocking(move || {
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
        let mut players = ReplayAnalysis::rebuild_player_rows_fast(&replays);
        players.sort_by(|left, right| {
            right
                .last_seen
                .cmp(&left.last_seen)
                .then_with(|| left.handle.cmp(&right.handle))
        });
        let total_players = players.len();
        if limit > 0 && players.len() > limit {
            players.truncate(limit);
        }
        (players, total_players)
    })
    .await
    .map_err(|error| format!("Failed to load /config/players: {error}"))?;

    Ok(ConfigPlayersPayload {
        status: "ok",
        players,
        total_players,
        loading: false,
    })
}

#[tauri::command]
async fn config_weeklies_get(
    _app: tauri::AppHandle<Wry>,
    state: State<'_, BackendState>,
) -> Result<ConfigWeekliesPayload, String> {
    state.log_request("get", "/config/weeklies", &None);
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
    state.log_request("get", &path, &None);
    let stats = state.stats_handle();
    let replays = state
        .get_replay_state()
        .lock()
        .map(|replay_state| replay_state.replays_handle())
        .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new())));
    let stats_current_replay_files = state.stats_current_replay_files_handle();
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
}

#[tauri::command]
async fn config_replay_show(
    app: tauri::AppHandle<Wry>,
    file: Option<String>,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    let body = Some(TauriOverlayOps::to_json_value(
        serde_json::json!({ "file": file }),
    ));
    state.log_request("post", "/config/replays/show", &body);
    let requested = body
        .as_ref()
        .and_then(|payload| payload.get("file"))
        .and_then(Value::as_str);
    Ok(overlay_info::OverlayInfoOps::replay_show_for_window(
        &app, &state, requested,
    ))
}

#[tauri::command]
async fn config_replay_chat(
    _app: tauri::AppHandle<Wry>,
    file: String,
    state: State<'_, BackendState>,
) -> Result<ConfigChatPayload, String> {
    let body = Some(TauriOverlayOps::to_json_value(
        serde_json::json!({ "file": file }),
    ));
    state.log_request("post", "/config/replays/chat", &body);
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
        TauriOverlayOps::replay_chat_payload_from_slots(
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
    .map_err(|error| format!("Failed to load /config/replays/chat: {error}"))??;
    Ok(ConfigChatPayload { status: "ok", chat })
}

#[tauri::command]
async fn config_replay_visual(
    _app: tauri::AppHandle<Wry>,
    file: String,
    state: State<'_, BackendState>,
) -> Result<ConfigReplayVisualPayload, String> {
    let body = Some(TauriOverlayOps::to_json_value(
        serde_json::json!({ "file": file }),
    ));
    state.log_request("post", "/config/replays/visual", &body);
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
    let dictionary = state.dictionary_data()?;
    let resources = state.replay_analysis_resources()?;
    let visual = tauri::async_runtime::spawn_blocking(move || {
        TauriOverlayOps::replay_visual_payload_from_slots(
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
    .map_err(|error| format!("Failed to load /config/replays/visual: {error}"))??;
    Ok(ConfigReplayVisualPayload {
        status: "ok",
        visual,
    })
}

#[tauri::command]
async fn config_replay_move(
    app: tauri::AppHandle<Wry>,
    delta: i64,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    let body = Some(TauriOverlayOps::to_json_value(
        serde_json::json!({ "delta": delta }),
    ));
    state.log_request("post", "/config/replays/move", &body);
    let delta = body
        .as_ref()
        .and_then(|payload| payload.get("delta"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Ok(overlay_info::OverlayInfoOps::replay_move_window(
        &app, &state, delta,
    ))
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
        Some(TauriOverlayOps::to_json_value(
            serde_json::json!({ "action": action }),
        ))
    };
    state.log_request("post", "/config/action", &body);
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
            saved_settings.update_player_note(player_name, note_value)?;
            saved_settings.write_saved_settings_file()?;

            let mut active_settings = state.read_settings_memory();
            active_settings.update_player_note(player_name, note_value)?;
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
            if let Some(response) = overlay_info::OverlayInfoOps::perform_overlay_action(
                &app,
                &state,
                action,
                body.as_ref(),
            ) {
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
        Some(TauriOverlayOps::to_json_value(
            serde_json::json!({ "action": action }),
        ))
    };
    state.log_request("post", "/config/stats/action", &body);
    let action = body
        .as_ref()
        .and_then(|payload| payload.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if let Some(response) =
        overlay_info::OverlayInfoOps::perform_overlay_action(&app, &state, action, body.as_ref())
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
            TauriOverlayOps::request_startup_analysis(
                app.clone(),
                state.stats_handle(),
                state
                    .get_replay_state()
                    .lock()
                    .map(|replay_state| replay_state.replays_handle())
                    .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new()))),
                state.stats_current_replay_files_handle(),
                state.detailed_analysis_stop_controller_slot(),
                StartupAnalysisTrigger::FrontendReady,
            )?;
            let stats_handle = state.stats_handle();
            let stats = stats_handle
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
                message: stats.message().to_string(),
                stats: Some(stats.as_payload_typed(state.replay_scan_progress().as_payload())),
            });
        }
        "start_simple_analysis" | "run_detailed_analysis" => {
            let include_detailed = action == "run_detailed_analysis";
            let mode = TauriOverlayOps::analysis_mode(include_detailed);

            let limit = UNLIMITED_REPLAY_LIMIT;
            crate::sco_log!("[SCO/stats] {action} requested replay_limit={limit} on thread");
            TauriOverlayOps::spawn_analysis_task(
                app.clone(),
                state.stats_handle(),
                state
                    .get_replay_state()
                    .lock()
                    .map(|replay_state| replay_state.replays_handle())
                    .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new()))),
                state.stats_current_replay_files_handle(),
                state.detailed_analysis_stop_controller_slot(),
                include_detailed,
                limit,
            );
            let status = state
                .stats_handle()
                .lock()
                .ok()
                .and_then(|stats| {
                    if stats.message().is_empty() {
                        None
                    } else {
                        Some(stats.message().to_string())
                    }
                })
                .unwrap_or_else(|| TauriOverlayOps::analysis_started_message(mode));
            crate::sco_log!(
                "[SCO/stats/action] {} immediate response message={}",
                action,
                status
            );
            let stats_payload = state
                .stats_handle()
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

    let stats_handle = state.stats_handle();
    let mut stats = stats_handle
        .lock()
        .map_err(|error| format!("Failed to access stats state: {error}"))?;
    let request_started_at = Instant::now();
    crate::sco_log!("[SCO/stats/action] action={action}");

    match action {
        "stop_detailed_analysis" => {
            if !stats.analysis_running()
                || stats.analysis_running_mode() != Some(AnalysisMode::Detailed)
            {
                stats.set_message("Detailed analysis is not running.");
            } else if state.request_detailed_analysis_stop() {
                stats.set_detailed_analysis_status(TauriOverlayOps::analysis_status_text(
                    AnalysisMode::Detailed,
                    "stopping",
                ));
                stats.set_message("Detailed analysis will stop after the current work finishes.");
            } else {
                stats.set_message("Detailed analysis stop could not be requested.");
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

            let payload = TauriOverlayOps::to_json_value(DumpPayload {
                timestamp: TauriOverlayOps::format_date_from_system_time(SystemTime::now()),
                stats: stats.as_payload(state.replay_scan_progress().as_payload()),
            });
            match serde_json::to_string_pretty(&payload) {
                Ok(contents) => match std::fs::write(&dump_path, contents) {
                    Ok(_) => {
                        let path = dump_path.display();
                        stats.set_message(format!("Data dumped to {path}"));
                        crate::sco_log!("[SCO/stats] dump_data written to {path}");
                    }
                    Err(error) => {
                        let message = format!("Failed to write dump: {error}");
                        crate::sco_log!("[SCO/stats] {message}");
                        stats.set_message(message);
                    }
                },
                Err(error) => {
                    let message = format!("Failed to serialize dump: {error}");
                    crate::sco_log!("[SCO/stats] {message}");
                    stats.set_message(message);
                }
            }
            crate::sco_log!(
                "[SCO/stats] dump_data completed in {}ms",
                request_started_at.elapsed().as_millis()
            );
        }
        "delete_parsed_data" => {
            crate::sco_log!("[SCO/stats/action] delete_parsed_data requested");
            stats.set_ready(false);
            stats.set_startup_analysis_requested(false);
            stats.set_analysis(Some(TauriOverlayOps::empty_stats_payload()));
            stats.clear_prestige_names();
            stats.set_analysis_terminal_status(AnalysisMode::Simple, "not started");
            stats.set_analysis_terminal_status(AnalysisMode::Detailed, "not started");
            state.set_detailed_analysis_stop_controller(None);
            stats.set_message("No parsed statistics available yet.");
            state.clear_replay_cache_slots();
            state.clear_stats_current_replay_files();
            state.set_overlay_replay_data_active(false);
            TauriOverlayOps::clear_analysis_cache_files();
            crate::sco_log!(
                "[SCO/stats] delete_parsed_data completed in {}ms",
                request_started_at.elapsed().as_millis()
            );
        }
        "set_detailed_analysis_atstart" => {
            if let Some(payload) = body.as_ref() {
                if let Some(enabled) = payload.get("enabled").and_then(Value::as_bool) {
                    stats.set_detailed_analysis_atstart(enabled);
                    if let Err(error) =
                        state.persist_bool_setting("detailed_analysis_atstart", enabled)
                    {
                        crate::sco_log!(
                            "[SCO/settings] Failed to save detailed_analysis_atstart: {error}"
                        );
                    }
                    stats.set_message(TauriOverlayOps::analysis_at_start_message(enabled));
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
                stats.set_message("No replay file specified to reveal.");
            } else {
                match overlay_info::OverlayInfoOps::reveal_file_in_explorer(file) {
                    Ok(()) => stats.set_message(format!("Revealing file: {file}")),
                    Err(error) => {
                        let message = format!("Unable to reveal file: {error}");
                        crate::sco_log!("[SCO/stats] reveal_file failed: {error}");
                        stats.set_message(message);
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

impl TauriOverlayOps {
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
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = AppSettings::from_saved_file();

    let state = BackendState::new_with_settings(settings.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .manage(TrayState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_menu_event(|app, event| match event.id() {
            id if id == overlay_info::MENU_ITEM_SHOW_CONFIG => {
                overlay_info::OverlayInfoOps::show_config_window(app)
            }
            id if id == overlay_info::MENU_ITEM_SHOW_OVERLAY => {
                overlay_info::OverlayInfoOps::show_overlay_window(app);
                let _ = app.emit(
                    overlay_info::OVERLAY_SHOWSTATS_EVENT,
                    shared_types::EmptyPayload::default(),
                );
            }
            id if id == overlay_info::MENU_ITEM_QUIT => TauriOverlayOps::request_clean_exit(app, 0),
            _ => {}
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let state = window.app_handle().state::<BackendState>();
                let flags = state.runtime_flags();
                match TauriOverlayOps::window_close_action(
                    window.label(),
                    flags.minimize_to_tray(),
                    state.exit_in_progress(),
                ) {
                    WindowCloseAction::AllowClose => {}
                    WindowCloseAction::HidePerformance => {
                        api.prevent_close();
                        performance_overlay::PerformanceOverlayOps::hide_window(window.app_handle());
                    }
                    WindowCloseAction::HideWindow => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    WindowCloseAction::ExitApp => {
                        api.prevent_close();
                        TauriOverlayOps::request_clean_exit(window.app_handle(), 0);
                    }
                }
            }
            tauri::WindowEvent::Moved(_) => {
                if window.label() == "performance" {
                    if let Some(performance_window) =
                        window.app_handle().get_webview_window("performance")
                    {
                        performance_overlay::PerformanceOverlayOps::persist_geometry(&performance_window);
                    }
                }
            }
            tauri::WindowEvent::Resized(_) => {
                if window.label() == "overlay" {
                    if let Some(overlay_window) = window.app_handle().get_webview_window("overlay")
                    {
                        if let Err(error) = overlay_info::OverlayInfoOps::stabilize_overlay_bounds(&overlay_window)
                        {
                            crate::sco_log!(
                                "[SCO/overlay] Failed to stabilize overlay bounds after resize: {error}"
                            );
                        }
                    }
                }
                if window.label() == "performance" {
                    if let Some(performance_window) =
                        window.app_handle().get_webview_window("performance")
                    {
                        performance_overlay::PerformanceOverlayOps::persist_geometry(&performance_window);
                    }
                }
            }
            tauri::WindowEvent::ScaleFactorChanged { .. } => {
                if window.label() == "overlay" {
                    if let Some(overlay_window) = window.app_handle().get_webview_window("overlay")
                    {
                        if let Err(error) = overlay_info::OverlayInfoOps::stabilize_overlay_bounds(&overlay_window)
                        {
                            crate::sco_log!(
                                "[SCO/overlay] Failed to stabilize overlay bounds after scale change: {error}"
                            );
                        }
                    }
                }
            }
            _ => {}
        })
        .setup(|app| {
            TauriOverlayOps::spawn_protocol_store_warmup();
            TauriOverlayOps::spawn_replay_analysis_resource_warmup(app.handle().clone());

            let state = app.state::<BackendState>();
            let flags = state.runtime_flags();

            if flags.auto_update() {
                let handle = app.handle().clone();

                tauri::async_runtime::spawn(async move {
                    if let Err(error) = TauriOverlayOps::auto_update(handle).await {
                        crate::sco_log!("Auto update failed: {}", error);
                    }
                });
            }

            // Always start with overlay hidden; user can show it via hotkey/tray/actions.
            overlay_info::OverlayInfoOps::hide_overlay_window(app.app_handle());

            if flags.start_minimized() {
                if let Some(config_window) = app.get_webview_window("config") {
                    let _ = config_window.hide();
                }
            } else {
                overlay_info::OverlayInfoOps::show_config_window(app.app_handle());
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
                if let Err(error) = overlay_info::OverlayInfoOps::apply_overlay_placement(&window) {
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
                if let Err(error) = performance_overlay::PerformanceOverlayOps::apply_saved_geometry(&window) {
                    crate::sco_log!("Could not apply saved performance placement: {error}");
                }
            }

            if let Some(tray_menu) = overlay_info::OverlayInfoOps::build_tray_menu(app.app_handle()) {
                let mut tray_builder = TrayIconBuilder::new()
                    .menu(&tray_menu)
                    .show_menu_on_left_click(true)
                    .tooltip("SCO Overlay");

                if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                }

                if let Ok(tray) = tray_builder.build(app) {
                    if let Ok(mut tray_slot) = app.state::<TrayState>().tray_icon.lock() {
                        *tray_slot = Some(tray);
                    }
                } else {
                    crate::sco_log!("Failed to build system tray icon");
                }
            }

            let startup_settings = state.read_settings_memory();
            if let Err(error) = startup_settings.sync_start_with_windows_registration() {
                crate::sco_log!("[SCO/settings] Failed to initialize start_with_windows: {error}");
            }

            overlay_info::OverlayInfoOps::sync_overlay_runtime_settings(app.app_handle());
            performance_overlay::PerformanceOverlayOps::apply_settings(app.app_handle());

            if let Err(error) = overlay_info::OverlayInfoOps::register_overlay_hotkeys(app.app_handle()) {
                crate::sco_log!("[SCO/hotkey] {error}");
            }

            TauriOverlayOps::spawn_replay_creation_watcher(app.app_handle().clone());
            TauriOverlayOps::spawn_game_launch_player_stats_task(app.app_handle().clone());
            performance_overlay::PerformanceOverlayOps::spawn_monitor(app.app_handle().clone());
            let (stats, replays, stats_current_replay_files, detailed_stop_controller_slot) = {
                let state = app.state::<BackendState>();
                let replays = state
                    .get_replay_state()
                    .lock()
                    .map(|replay_state| replay_state.replays_handle())
                    .unwrap_or_else(|_| Arc::new(Mutex::new(HashMap::new())));
                (
                    state.stats_handle(),
                    replays,
                    state.stats_current_replay_files_handle(),
                    state.detailed_analysis_stop_controller_slot(),
                )
            };
            if let Err(error) = TauriOverlayOps::request_startup_analysis(
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
            config_replay_visual,
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
