use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{
    GenerateCacheStopController, ReplayAnalysisResources,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::Serialize;
use serde_json::Value;

use crate::replay_scan_progress::ReplayScanProgress;
use crate::replay_state::ReplayState;
use crate::services::replay_watcher::ReplayWatcherMessage;
use crate::shared_types::{OverlayPlayerStatsPayload, OverlayPlayerStatsRow};
use crate::{
    AppSettings, FirstWinBonusAcquiredTime, PathManagerOps, PlayerRowPayload, ReplayCacheDatabase,
    ReplayInfo, Sc2GameState, Sc2GameStateTracker, Sc2GameStateTransition, StatsState,
    TauriOverlayOps,
    overlay_info::{ResolvedHotkeyBinding, RuntimeFlags},
    replay_analysis::ReplayAnalysis,
};

#[derive(Default)]
enum CachedLoad<T> {
    #[default]
    Uninitialized,
    Loaded(Arc<T>),
    Failed(String),
}

struct BackendStateOps;

impl BackendStateOps {
    fn load_cached_state<T, F>(slot: &Mutex<CachedLoad<T>>, loader: F) -> Result<Arc<T>, String>
    where
        F: FnOnce() -> Result<Arc<T>, String>,
    {
        let mut slot = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match &*slot {
            CachedLoad::Loaded(value) => Ok(value.clone()),
            CachedLoad::Failed(error) => Err(error.clone()),
            CachedLoad::Uninitialized => {
                let loaded = loader();
                match loaded {
                    Ok(value) => {
                        *slot = CachedLoad::Loaded(value.clone());
                        Ok(value)
                    }
                    Err(error) => {
                        *slot = CachedLoad::Failed(error.clone());
                        Err(error)
                    }
                }
            }
        }
    }
}

pub struct BackendState {
    stats: Arc<Mutex<StatsState>>,
    stats_current_replay_files: Arc<Mutex<HashSet<String>>>,
    overlay_replay_data_active: AtomicBool,
    sc2_overlay_keep_visible_until_millis: AtomicU64,
    first_win_bonus_timer_visible: AtomicBool,
    first_win_bonus_timer_hide_after_millis: AtomicU64,
    latest_replay_file_modified_time_seconds: AtomicU64,
    session_victories: AtomicU64,
    session_defeats: AtomicU64,
    active_settings: Arc<Mutex<AppSettings>>,
    detailed_cache_persist_lock: Arc<Mutex<()>>,
    discovered_main_names: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    discovered_main_handles: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    replay_scan_in_flight: Arc<AtomicBool>,
    players_scan_in_flight: Arc<AtomicBool>,
    app_exit_in_progress: Arc<AtomicBool>,
    replay_scan_progress: Arc<ReplayScanProgress>,
    delayed_player_stats_popup_generation: Arc<AtomicU64>,
    hotkey_action_inflight: Arc<AtomicBool>,
    active_hotkey_reassign_path: Arc<Mutex<Option<String>>>,
    active_hotkey_reassign_binding: Arc<Mutex<Option<ResolvedHotkeyBinding>>>,
    detailed_analysis_stop_controller: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
    performance_edit_mode: Arc<AtomicBool>,
    file_logging_enabled: Arc<AtomicBool>,
    replay_watcher_sender: Arc<Mutex<Option<mpsc::Sender<ReplayWatcherMessage>>>>,
    replay_state: Arc<Mutex<ReplayState>>,
    sc2_game_state: Arc<Mutex<Sc2GameStateTracker>>,
    analyzer_data_dir: PathBuf,
    dictionary_data: Arc<Mutex<CachedLoad<Sc2DictionaryData>>>,
    replay_analysis_resources: Arc<Mutex<CachedLoad<ReplayAnalysisResources>>>,
}

impl BackendStateOps {
    fn as_u32(value: u64) -> u32 {
        u32::try_from(value).unwrap_or(u32::MAX)
    }

    fn system_time_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

impl BackendStateOps {
    fn select_other_player_for_stats(
        replay: &ReplayInfo,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Option<(String, String)> {
        let p1 = replay.main().name.trim();
        let p2 = replay.ally().name.trim();

        if p1.is_empty() && p2.is_empty() {
            return None;
        }

        let p1_handle = replay.main().handle.clone();
        let p2_handle = replay.ally().handle.clone();

        let p1_is_main = ReplayAnalysis::is_main_player_identity(
            &replay.main().name,
            &replay.main().handle,
            main_names,
            main_handles,
        );
        let p2_is_main = ReplayAnalysis::is_main_player_identity(
            &replay.ally().name,
            &replay.ally().handle,
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
}

impl BackendStateOps {
    fn player_note_for_identity(
        settings: &AppSettings,
        player_handle: &str,
        player_name: &str,
    ) -> Option<String> {
        settings
            .player_note(player_handle)
            .or_else(|| settings.player_note(player_name))
    }

    fn overlay_stats_row_from_player_row(
        settings: &AppSettings,
        requested_player_handle: &str,
        requested_player_name: &str,
        row: PlayerRowPayload,
    ) -> (String, OverlayPlayerStatsRow) {
        let display_name = TauriOverlayOps::sanitize_replay_text(&row.player);
        let display_name = if display_name.trim().is_empty() {
            requested_player_name.to_string()
        } else {
            display_name
        };
        let note =
            Self::player_note_for_identity(settings, &row.handle, &display_name).or_else(|| {
                Self::player_note_for_identity(settings, requested_player_handle, &display_name)
            });

        (
            display_name,
            OverlayPlayerStatsRow::Stats {
                wins: BackendStateOps::as_u32(row.wins),
                losses: BackendStateOps::as_u32(row.losses),
                apm: BackendStateOps::as_u32(row.apm.round() as u64),
                commander: TauriOverlayOps::sanitize_replay_text(&row.commander),
                frequency: row.frequency,
                kills: row.kills,
                last_seen_relative: BackendStateOps::relative_last_seen_text(row.last_seen),
                note,
            },
        )
    }
}

impl BackendStateOps {
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
}

impl Default for BackendState {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendState {
    pub fn new() -> Self {
        Self::new_with_settings(AppSettings::default())
    }

    pub fn new_with_settings(settings: AppSettings) -> Self {
        let file_logging_enabled = settings.enable_logging();
        Self {
            stats: Arc::new(Mutex::new(StatsState::from_settings(&settings))),
            stats_current_replay_files: Arc::new(Mutex::new(HashSet::new())),
            overlay_replay_data_active: AtomicBool::new(false),
            sc2_overlay_keep_visible_until_millis: AtomicU64::new(0),
            first_win_bonus_timer_visible: AtomicBool::new(false),
            first_win_bonus_timer_hide_after_millis: AtomicU64::new(0),
            latest_replay_file_modified_time_seconds: AtomicU64::new(0),
            session_victories: AtomicU64::new(0),
            session_defeats: AtomicU64::new(0),
            active_settings: Arc::new(Mutex::new(settings)),
            detailed_cache_persist_lock: Arc::new(Mutex::new(())),
            discovered_main_names: Arc::new(Mutex::new(HashMap::new())),
            discovered_main_handles: Arc::new(Mutex::new(HashMap::new())),
            replay_scan_in_flight: Arc::new(AtomicBool::new(false)),
            players_scan_in_flight: Arc::new(AtomicBool::new(false)),
            app_exit_in_progress: Arc::new(AtomicBool::new(false)),
            replay_scan_progress: Arc::new(ReplayScanProgress::default()),
            delayed_player_stats_popup_generation: Arc::new(AtomicU64::new(0)),
            hotkey_action_inflight: Arc::new(AtomicBool::new(false)),
            active_hotkey_reassign_path: Arc::new(Mutex::new(None)),
            active_hotkey_reassign_binding: Arc::new(Mutex::new(None)),
            detailed_analysis_stop_controller: Arc::new(Mutex::new(None)),
            performance_edit_mode: Arc::new(AtomicBool::new(false)),
            file_logging_enabled: Arc::new(AtomicBool::new(file_logging_enabled)),
            replay_watcher_sender: Arc::new(Mutex::new(None)),
            replay_state: Arc::new(Mutex::new(ReplayState::new())),
            sc2_game_state: Arc::new(Mutex::new(Sc2GameStateTracker::new(Instant::now()))),
            analyzer_data_dir: crate::path_manager::PathManagerOps::get_json_data_dir(),
            dictionary_data: Arc::new(Mutex::new(CachedLoad::Uninitialized)),
            replay_analysis_resources: Arc::new(Mutex::new(CachedLoad::Uninitialized)),
        }
    }

    pub fn analyzer_data_dir(&self) -> PathBuf {
        self.analyzer_data_dir.clone()
    }

    pub fn stats_handle(&self) -> Arc<Mutex<StatsState>> {
        self.stats.clone()
    }

    pub fn stats_current_replay_files_handle(&self) -> Arc<Mutex<HashSet<String>>> {
        self.stats_current_replay_files.clone()
    }

    pub fn overlay_replay_data_active(&self) -> bool {
        self.overlay_replay_data_active.load(Ordering::Acquire)
    }

    pub fn set_overlay_replay_data_active(&self, active: bool) {
        self.overlay_replay_data_active
            .store(active, Ordering::Release);
    }

    pub fn enter_player_stats_overlay_mode(&self) {
        self.set_overlay_replay_data_active(false);
    }

    pub fn keep_sc2_overlay_visible_for(&self, duration: Duration) {
        let duration_millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        let deadline = BackendStateOps::system_time_millis().saturating_add(duration_millis);
        self.sc2_overlay_keep_visible_until_millis
            .store(deadline, Ordering::Release);
    }

    pub fn sc2_overlay_keep_visible_active(&self) -> bool {
        let current_deadline = self
            .sc2_overlay_keep_visible_until_millis
            .load(Ordering::Acquire);
        current_deadline != 0 && BackendStateOps::system_time_millis() < current_deadline
    }

    pub fn clear_sc2_overlay_keep_visible(&self) {
        self.sc2_overlay_keep_visible_until_millis
            .store(0, Ordering::Release);
    }

    pub fn first_win_bonus_timer_visible(&self) -> bool {
        self.first_win_bonus_timer_visible.load(Ordering::Acquire)
    }

    pub fn set_first_win_bonus_timer_visible(&self, visible: bool) {
        self.first_win_bonus_timer_visible
            .store(visible, Ordering::Release);
    }

    pub fn start_first_win_bonus_timer_hide_delay_if_needed(&self, duration: Duration) {
        let current_deadline = self
            .first_win_bonus_timer_hide_after_millis
            .load(Ordering::Acquire);
        if current_deadline != 0 {
            return;
        }

        let duration_millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        let deadline = BackendStateOps::system_time_millis().saturating_add(duration_millis);
        let _ = self
            .first_win_bonus_timer_hide_after_millis
            .compare_exchange(0, deadline, Ordering::AcqRel, Ordering::Acquire);
    }

    pub fn first_win_bonus_timer_hide_delay_elapsed(&self) -> bool {
        let current_deadline = self
            .first_win_bonus_timer_hide_after_millis
            .load(Ordering::Acquire);
        current_deadline != 0 && BackendStateOps::system_time_millis() >= current_deadline
    }

    pub fn clear_first_win_bonus_timer_hide_delay(&self) {
        self.first_win_bonus_timer_hide_after_millis
            .store(0, Ordering::Release);
    }

    pub fn update_latest_replay_file_modified_time_seconds(
        &self,
        replay_file_modified_time_seconds: u64,
    ) {
        if replay_file_modified_time_seconds == 0 {
            return;
        }

        self.latest_replay_file_modified_time_seconds
            .fetch_max(replay_file_modified_time_seconds, Ordering::AcqRel);
    }

    pub fn update_latest_replay_file_modified_time(
        &self,
        replay_file: &Path,
    ) -> Result<Option<u64>, String> {
        let acquired_time = FirstWinBonusAcquiredTime::from_replay_file_modified_time(replay_file)?;
        if let Some(acquired_time) = acquired_time {
            let seconds = acquired_time.replay_file_modified_time_seconds();
            self.update_latest_replay_file_modified_time_seconds(seconds);
            Ok(Some(seconds))
        } else {
            Ok(None)
        }
    }

    pub fn latest_observed_replay_file_modified_time_seconds(&self) -> Option<u64> {
        match self
            .latest_replay_file_modified_time_seconds
            .load(Ordering::Acquire)
        {
            0 => None,
            seconds => Some(seconds),
        }
    }

    pub fn latest_replay_file_modified_time_seconds(&self) -> Option<u64> {
        self.latest_observed_replay_file_modified_time_seconds()
    }

    pub fn sc2_game_state(&self) -> Sc2GameState {
        self.sc2_game_state
            .lock()
            .map(|tracker| tracker.state())
            .unwrap_or(Sc2GameState::Lobby)
    }

    pub fn sc2_game_state_should_poll_live_game(&self) -> bool {
        self.sc2_game_state
            .lock()
            .map(|tracker| tracker.should_poll_live_game())
            .unwrap_or(true)
    }

    pub fn transition_sc2_game_state(
        &self,
        next_state: Sc2GameState,
        now: Instant,
    ) -> Option<Sc2GameStateTransition> {
        self.sc2_game_state
            .lock()
            .ok()
            .and_then(|mut tracker| tracker.transition_to(next_state, now))
    }

    pub fn advance_sc2_game_state_timers(
        &self,
        now: Instant,
        game_starting_duration: Duration,
        game_ended_duration: Duration,
    ) -> Option<Sc2GameStateTransition> {
        self.sc2_game_state.lock().ok().and_then(|mut tracker| {
            tracker.advance_timed_transitions(now, game_starting_duration, game_ended_duration)
        })
    }

    pub fn clear_stats_current_replay_files(&self) {
        if let Ok(mut current_replay_files) = self.stats_current_replay_files.lock() {
            current_replay_files.clear();
        }
    }

    pub fn dictionary_data(&self) -> Result<Arc<Sc2DictionaryData>, String> {
        BackendStateOps::load_cached_state(&self.dictionary_data, || {
            Sc2DictionaryData::load(Some(self.analyzer_data_dir()))
                .map(Arc::new)
                .map_err(|error| format!("Failed to load dictionary data: {error}"))
        })
    }

    pub fn replay_analysis_resources(&self) -> Result<Arc<ReplayAnalysisResources>, String> {
        BackendStateOps::load_cached_state(&self.replay_analysis_resources, || {
            let dictionary_data = self.dictionary_data()?;
            ReplayAnalysisResources::from_dictionary_data(dictionary_data)
                .map(Arc::new)
                .map_err(|error| format!("Failed to build replay analysis resources: {error}"))
        })
    }

    pub fn warm_dictionary_data(&self) {
        let _ = self.dictionary_data();
    }

    pub fn warm_replay_analysis_resources(&self) {
        let _ = self.replay_analysis_resources();
    }

    pub fn read_settings_memory(&self) -> AppSettings {
        self.active_settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_else(|_| AppSettings::from_saved_file())
    }

    pub fn runtime_flags(&self) -> RuntimeFlags {
        self.read_settings_memory().runtime_flags()
    }

    pub fn set_replay_watcher_sender(&self, sender: Option<mpsc::Sender<ReplayWatcherMessage>>) {
        if let Ok(mut cached_sender) = self.replay_watcher_sender.lock() {
            *cached_sender = sender;
        }
    }

    pub fn request_replay_watcher_root_refresh(&self) {
        let sender = self
            .replay_watcher_sender
            .lock()
            .ok()
            .and_then(|cached_sender| cached_sender.clone());
        if let Some(sender) = sender
            && sender.send(ReplayWatcherMessage::RefreshRoot).is_err()
        {
            crate::sco_warn!("[SCO/watch] failed to request replay watcher root refresh");
        }
    }

    pub fn append_log_line_if_enabled(&self, message: &str) {
        if !self.file_logging_enabled() {
            return;
        }

        if let Err(error) = crate::logging::LoggingOps::append_line(message) {
            eprintln!("[SCO/log] {error}");
        }
    }

    pub fn log_request(&self, method: &str, path: &str, body: &Option<Value>) {
        let serialized_body = body.as_ref().map(|payload| {
            serde_json::to_string(payload).unwrap_or_else(|_| "<invalid-json>".into())
        });
        if let Some(serialized_body) = serialized_body {
            crate::sco_debug!(
                "[SCO/request] method={} path={} body={}",
                method,
                path,
                serialized_body
            );
        } else {
            crate::sco_debug!("[SCO/request] method={} path={}", method, path);
        }
    }

    pub fn replace_active_settings(&self, value: &AppSettings) -> AppSettings {
        let sanitized = AppSettings::merge_settings_with_defaults(value.to_value());

        if let Ok(mut cached_settings) = self.active_settings.lock() {
            *cached_settings = sanitized.clone();
        }

        self.file_logging_enabled
            .store(sanitized.enable_logging(), Ordering::Release);
        self.clear_main_identity_cache();
        sanitized
    }

    pub fn write_settings_file(&self, value: &AppSettings) -> Result<AppSettings, String> {
        let sanitized = value.write_saved_settings_file()?;

        self.replace_active_settings(&sanitized);
        Ok(sanitized)
    }

    pub fn persist_single_setting_value(&self, key: &str, value: Value) -> Result<(), String> {
        let previous_settings = AppSettings::from_saved_file();
        let mut saved_map = match previous_settings.to_value() {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        saved_map.insert(key.to_string(), value.clone());

        let saved_settings = AppSettings::merge_settings_with_defaults(Value::Object(saved_map));
        saved_settings.write_saved_settings_file()?;

        let current_settings = self.read_settings_memory();
        let mut active_map = match current_settings.to_value() {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        active_map.insert(key.to_string(), value);

        let active_settings = AppSettings::merge_settings_with_defaults(Value::Object(active_map));
        self.replace_active_settings(&active_settings);
        Ok(())
    }

    pub fn persist_serialized_setting_value<T: Serialize>(
        &self,
        key: &str,
        value: &T,
    ) -> Result<(), String> {
        let json_value = serde_json::to_value(value)
            .map_err(|error| format!("Failed to serialize setting: {error}"))?;
        self.persist_single_setting_value(key, json_value)
    }

    pub fn persist_bool_setting(&self, key: &str, value: bool) -> Result<(), String> {
        self.persist_single_setting_value(key, Value::Bool(value))
    }

    pub fn detailed_cache_persist_lock(&self) -> Arc<Mutex<()>> {
        self.detailed_cache_persist_lock.clone()
    }

    pub fn replay_scan_progress(&self) -> Arc<ReplayScanProgress> {
        self.replay_scan_progress.clone()
    }

    pub fn replay_scan_in_flight(&self) -> Arc<AtomicBool> {
        self.replay_scan_in_flight.clone()
    }

    pub fn file_logging_enabled(&self) -> bool {
        self.file_logging_enabled.load(Ordering::Acquire)
    }

    pub fn resolved_overlay_hotkey_bindings(&self) -> Vec<ResolvedHotkeyBinding> {
        self.read_settings_memory()
            .resolved_overlay_hotkey_bindings()
    }

    pub fn performance_edit_mode(&self) -> bool {
        self.performance_edit_mode.load(Ordering::Acquire)
    }

    pub fn set_performance_edit_mode(&self, enabled: bool) {
        self.performance_edit_mode.store(enabled, Ordering::Release);
    }

    pub fn try_begin_exit(&self) -> bool {
        self.app_exit_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    pub fn exit_in_progress(&self) -> bool {
        self.app_exit_in_progress.load(Ordering::Acquire)
    }

    pub fn delayed_player_stats_popup_generation(&self) -> u64 {
        self.delayed_player_stats_popup_generation
            .load(Ordering::Acquire)
    }

    pub fn invalidate_delayed_player_stats_popup_generation(&self) -> u64 {
        self.delayed_player_stats_popup_generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1)
    }

    pub fn try_begin_hotkey_action(&self) -> bool {
        self.hotkey_action_inflight
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    pub fn finish_hotkey_action(&self) {
        self.hotkey_action_inflight.store(false, Ordering::Release);
    }

    pub fn active_hotkey_reassign_path(&self) -> Option<String> {
        self.active_hotkey_reassign_path
            .lock()
            .ok()
            .and_then(|path| path.clone())
    }

    pub fn set_active_hotkey_reassign_path(&self, path: Option<String>) {
        if let Ok(mut current) = self.active_hotkey_reassign_path.lock() {
            *current = path;
        }
    }

    pub fn active_hotkey_reassign_binding(&self) -> Option<ResolvedHotkeyBinding> {
        self.active_hotkey_reassign_binding
            .lock()
            .ok()
            .and_then(|binding| binding.clone())
    }

    pub fn set_active_hotkey_reassign_binding(&self, binding: Option<ResolvedHotkeyBinding>) {
        if let Ok(mut current) = self.active_hotkey_reassign_binding.lock() {
            *current = binding;
        }
    }

    fn clear_main_identity_cache(&self) {
        if let Ok(mut cache) = self.discovered_main_names.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.discovered_main_handles.lock() {
            cache.clear();
        }
    }

    pub fn configured_main_names(&self) -> HashSet<String> {
        let settings = self.read_settings_memory();
        let account_root = settings.account_folder().trim().to_string();

        if !account_root.is_empty()
            && let Ok(cache) = self.discovered_main_names.lock()
            && let Some(cached) = cache.get(&account_root)
        {
            return cached.clone();
        }

        let names = settings.configured_main_names();

        if !account_root.is_empty()
            && let Ok(mut cache) = self.discovered_main_names.lock()
        {
            cache.insert(account_root, names.clone());
        }

        names
    }

    pub fn configured_main_handles(&self) -> HashSet<String> {
        let settings = self.read_settings_memory();
        let account_root = settings.account_folder().trim().to_string();

        if !account_root.is_empty()
            && let Ok(cache) = self.discovered_main_handles.lock()
            && let Some(cached) = cache.get(&account_root)
        {
            return cached.clone();
        }

        let handles = settings.configured_main_handles();

        if !account_root.is_empty()
            && let Ok(mut cache) = self.discovered_main_handles.lock()
        {
            cache.insert(account_root, handles.clone());
        }

        handles
    }

    pub fn overlay_player_stats_payload(&self) -> OverlayPlayerStatsPayload {
        let selected_file = self.get_current_replay_file();
        let selected = self.cached_replay_by_file_or_latest(selected_file.as_deref());

        let Some(selected) = selected else {
            return OverlayPlayerStatsPayload::default();
        };

        let main_names = self.configured_main_names();
        let main_handles = self.configured_main_handles();
        let player_stats_target =
            BackendStateOps::select_other_player_for_stats(&selected, &main_names, &main_handles)
                .or_else(|| {
                    let ally = selected.ally().name.trim();
                    if !ally.is_empty() {
                        Some((selected.ally().handle.clone(), ally.to_string()))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let main = selected.main().name.trim();
                    if !main.is_empty() {
                        Some((selected.main().handle.clone(), main.to_string()))
                    } else {
                        None
                    }
                });

        let Some((player_handle, player_name)) = player_stats_target else {
            return OverlayPlayerStatsPayload::default();
        };

        self.overlay_player_stats_payload_for_player(&player_handle, &player_name)
    }

    pub fn overlay_player_stats_payload_for_player(
        &self,
        player_handle: &str,
        player_name: &str,
    ) -> OverlayPlayerStatsPayload {
        let settings = self.read_settings_memory();

        let input_name = TauriOverlayOps::sanitize_replay_text(player_name);
        let fallback_name = if input_name.trim().is_empty() {
            "Unknown".to_string()
        } else {
            input_name.trim().to_string()
        };

        let mut data = std::collections::BTreeMap::new();

        let database_row =
            ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
                .and_then(|database| {
                    database.load_overlay_player_stats_row(player_handle, &fallback_name)
                })
                .map_err(|error| {
                    crate::sco_warn!(
                        "[SCO/player-stats] failed to load player row from cache: {error}"
                    );
                    error
                })
                .ok()
                .flatten();
        if let Some(row) = database_row {
            let (display_name, value) = BackendStateOps::overlay_stats_row_from_player_row(
                &settings,
                player_handle,
                &fallback_name,
                row,
            );
            data.insert(display_name, value);
            return OverlayPlayerStatsPayload { data };
        }

        let note =
            BackendStateOps::player_note_for_identity(&settings, player_handle, &fallback_name);
        let (display_name, value) = (fallback_name, OverlayPlayerStatsRow::NoGames { note });

        data.insert(display_name, value);

        OverlayPlayerStatsPayload { data }
    }

    pub fn get_replay_state(&self) -> Arc<Mutex<ReplayState>> {
        self.replay_state.clone()
    }

    fn replay_info_from_cache_entry(&self, entry: &CacheReplayEntry) -> ReplayInfo {
        let main_names = self.configured_main_names();
        let main_handles = self.configured_main_handles();
        let dictionary = self.dictionary_data().ok();
        TauriOverlayOps::replay_info_from_cache_entry_for_identity(
            entry,
            &main_names,
            &main_handles,
            dictionary.as_deref(),
        )
    }

    fn cached_replay_by_file_or_latest(&self, file: Option<&str>) -> Option<ReplayInfo> {
        let cache_path = PathManagerOps::get_cache_path();
        let database = ReplayCacheDatabase::open_for_cache_path(&cache_path).map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] failed to open replay cache: {error}");
            error
        });
        let Ok(database) = database else {
            return None;
        };
        let entry = match file {
            Some(file) => database
                .load_entry_by_file(file)
                .and_then(|entry| match entry {
                    Some(entry) => Ok(Some(entry)),
                    None => database.load_latest_entry(),
                }),
            None => database.load_latest_entry(),
        }
        .map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] failed to load selected replay: {error}");
            error
        })
        .ok()
        .flatten()?;
        Some(self.replay_info_from_cache_entry(&entry))
    }

    pub fn get_current_replay_file(&self) -> Option<String> {
        self.replay_state
            .lock()
            .ok()
            .and_then(|state| state.get_current_replay_file())
    }

    pub fn set_current_replay_file(&self, filename: Option<&str>) {
        if let Ok(replay_state) = self.replay_state.lock() {
            replay_state.set_current_replay_file(filename);
        }
    }

    pub fn cached_replay_by_hash(&self, replay_hash: &str) -> Option<ReplayInfo> {
        let cache_path = PathManagerOps::get_cache_path();
        ReplayCacheDatabase::open_for_cache_path(&cache_path)
            .and_then(|database| database.load_entry_by_hash(replay_hash))
            .map_err(|error| {
                crate::sco_warn!("[SCO/cache-db] failed to load replay by hash: {error}");
                error
            })
            .ok()
            .flatten()
            .map(|entry| self.replay_info_from_cache_entry(&entry))
    }

    pub fn clear_current_replay_file(&self) {
        if let Ok(replay_state) = self.replay_state.lock() {
            replay_state.clear_selected_replay_file();
        }
    }

    pub fn set_detailed_analysis_stop_controller(
        &self,
        controller: Option<Arc<GenerateCacheStopController>>,
    ) {
        if let Ok(mut slot) = self.detailed_analysis_stop_controller.lock() {
            *slot = controller;
        }
    }

    pub fn request_detailed_analysis_stop(&self) -> bool {
        self.detailed_analysis_stop_controller
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().cloned())
            .map(|controller| {
                controller.request_stop();
                true
            })
            .unwrap_or(false)
    }

    pub fn detailed_analysis_stop_controller_slot(
        &self,
    ) -> Arc<Mutex<Option<Arc<GenerateCacheStopController>>>> {
        self.detailed_analysis_stop_controller.clone()
    }

    pub fn record_session_result(&self, result: &str) {
        let (victories, defeats) = TauriOverlayOps::session_counter_delta(result);
        if victories > 0 {
            self.session_victories
                .fetch_add(victories, Ordering::AcqRel);
        }
        if defeats > 0 {
            self.session_defeats.fetch_add(defeats, Ordering::AcqRel);
        }
    }

    pub fn session_counts(&self) -> (u64, u64) {
        (
            self.session_victories.load(Ordering::Acquire),
            self.session_defeats.load(Ordering::Acquire),
        )
    }

    pub fn spawn_players_scan_task(&self, limit: usize) {
        let replay_state = self.get_replay_state();
        let settings = self.read_settings_memory();
        let main_names = self.configured_main_names();
        let main_handles = self.configured_main_handles();
        let replay_analysis_resources = self.replay_analysis_resources();
        let replay_scan_progress = self.replay_scan_progress();
        let replay_scan_in_flight = self.replay_scan_in_flight();
        let players_scan_in_flight = self.players_scan_in_flight.clone();

        if players_scan_in_flight
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        thread::spawn(move || {
            crate::sco_info!("[SCO/players] background player scan started (limit={limit})");
            let replays = match replay_analysis_resources {
                Ok(resources) => ReplayAnalysis::analyze_replays_with_resources(
                    limit,
                    &settings,
                    &main_names,
                    &main_handles,
                    replay_scan_progress.as_ref(),
                    replay_scan_in_flight.as_ref(),
                    resources.as_ref(),
                ),
                Err(error) => {
                    crate::sco_warn!("[SCO/players] background player scan skipped: {error}");
                    Vec::new()
                }
            };
            let selected = replays.first().map(|replay| replay.file.clone());

            match replay_state.lock() {
                Ok(state) => {
                    state.set_current_replay_file_if_empty(selected);
                }
                Err(error) => {
                    crate::sco_warn!("[SCO/players] failed to access replay state: {error}");
                }
            }

            players_scan_in_flight.store(false, Ordering::Release);
            crate::sco_info!("[SCO/players] background player scan completed");
        });
    }

    pub fn record_replay_cache_update(&self, replay: &ReplayInfo) {
        if let Ok(mut current_replay_files) = self.stats_current_replay_files.lock() {
            current_replay_files.insert(replay.file.clone());
        }

        if let Err(error) = self.update_latest_replay_file_modified_time(Path::new(&replay.file)) {
            crate::sco_warn!(
                "[SCO/today-win-bonus] failed to record replay file modified time file='{}' error='{}'",
                replay.file,
                error
            );
        }
        self.set_current_replay_file(Some(&replay.file));
    }

    pub fn record_replay_cache_update_if_persistable(
        &self,
        replay: &ReplayInfo,
        cache_persistable: bool,
    ) -> bool {
        if !cache_persistable {
            return false;
        }

        self.record_replay_cache_update(replay);
        true
    }

    pub fn build_launch_main_identity(&self) -> (HashSet<String>, HashSet<String>) {
        let mut main_names = self.configured_main_names();
        let mut main_handles = self.configured_main_handles();

        if let Ok(stats) = self.stats.lock() {
            for name in stats.main_players() {
                let normalized = ReplayAnalysis::normalized_player_key(name);
                if !normalized.is_empty() {
                    main_names.insert(normalized);
                }
            }
        }

        let selected = self.get_current_replay_file();
        let seed = self.cached_replay_by_file_or_latest(selected.as_deref());
        if let Some(seed) = seed {
            let normalized_name = ReplayAnalysis::normalized_player_key(&seed.main().name);
            if !normalized_name.is_empty() {
                main_names.insert(normalized_name);
            }
            let normalized_handle = ReplayAnalysis::normalized_handle_key(&seed.main().handle);
            if !normalized_handle.is_empty() {
                main_handles.insert(normalized_handle);
            }
        }

        (main_names, main_handles)
    }

    pub fn stats_have_player_rows(&self) -> bool {
        ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
            .and_then(|database| database.has_player_info_rows())
            .map_err(|error| {
                crate::sco_warn!("[SCO/player-stats] failed to check cached player rows: {error}");
                error
            })
            .unwrap_or(false)
    }

    pub fn replay_count_for_launch_detector(&self) -> usize {
        ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
            .and_then(|database| database.count_entries())
            .unwrap_or_default()
    }
}
