use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use s2coop_analyzer::cache_overall_stats_generator::GenerateCacheStopController;
use serde::Serialize;
use serde_json::Value;
use tauri::{tray::TrayIcon, Wry};

use crate::shared_types::ReplayScanProgressPayload;
use crate::{
    apply_rebuild_snapshot, configured_main_handles_from_settings,
    configured_main_names_from_settings, overlay_info::ResolvedHotkeyBinding,
    replay_analysis::ReplayAnalysis, session_counter_delta,
    sync_detailed_analysis_status_from_replays, AnalysisMode, AppSettings, ReplayInfo, StatsState,
    UNLIMITED_REPLAY_LIMIT,
};

pub struct BackendState {
    pub tray_icon: Arc<Mutex<Option<TrayIcon<Wry>>>>,
    pub stats: Arc<Mutex<StatsState>>,
    pub stats_current_replay_files: Arc<Mutex<HashSet<String>>>,
    pub overlay_replay_data_active: AtomicBool,
    pub session_victories: AtomicU64,
    pub session_defeats: AtomicU64,
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
    replay_state: Arc<Mutex<ReplayState>>,
}

pub struct ReplayState {
    pub replays: Arc<Mutex<HashMap<String, ReplayInfo>>>,
    pub selected_replay_file: Arc<Mutex<Option<String>>>,
}

#[derive(Debug)]
pub struct ReplayScanProgress {
    pub total: AtomicU64,
    pub cache_hits: AtomicU64,
    pub to_parse: AtomicU64,
    pub newly_parsed: AtomicU64,
    pub completed: AtomicU64,
    pub failed: AtomicU64,
    pub parse_skipped: AtomicU64,
    pub started_at_ms: AtomicU64,
    pub elapsed_ms: AtomicU64,
    stage: Mutex<String>,
    status: Mutex<String>,
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
    pub fn reset(&self, stage: &str) {
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

    pub fn set_stage(&self, stage: &str) {
        if let Ok(mut value) = self.stage.lock() {
            *value = stage.to_string();
        }
    }

    pub fn set_status(&self, status: &str) {
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

    pub fn set_counts(&self, total: u64, completed: u64) {
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

    pub fn as_payload(&self) -> ReplayScanProgressPayload {
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
        ReplayScanProgressPayload {
            stage,
            status: status.clone(),
            parsing_status: status,
            total: effective_total,
            total_replay_files: effective_total,
            cache_hits,
            files_already_cached: cache_hits,
            to_parse,
            completed,
            newly_parsed,
            newly_parsed_files: newly_parsed,
            failed,
            parse_failed_files: failed,
            parse_skipped,
            parse_skipped_files: parse_skipped,
            elapsed_ms,
            total_time_taken_ms: elapsed_ms,
        }
    }
}

fn replay_cache_snapshot(cache: &HashMap<String, ReplayInfo>) -> Vec<ReplayInfo> {
    let mut replays = cache.values().cloned().collect::<Vec<_>>();
    ReplayInfo::sort_replays(&mut replays);
    replays
}

fn upsert_replay_map(
    cache: &mut HashMap<String, ReplayInfo>,
    replay_hash: &str,
    replay: &ReplayInfo,
) {
    if replay_hash.is_empty() {
        return;
    }

    cache.retain(|hash, entry| hash == replay_hash || entry.file != replay.file);

    match cache.get(replay_hash) {
        Some(existing)
            if ReplayInfo::should_keep_existing_detailed_variant(
                existing.is_detailed,
                replay.is_detailed,
            ) => {}
        Some(_) => {
            cache.insert(replay_hash.to_string(), replay.clone());
        }
        None => {
            cache.insert(replay_hash.to_string(), replay.clone());
        }
    }
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

impl BackendState {
    pub fn new() -> Self {
        Self::new_with_settings(AppSettings::from_saved_file())
    }

    pub fn new_with_settings(settings: AppSettings) -> Self {
        let file_logging_enabled = crate::logging_enabled_from_settings(&settings);
        Self {
            tray_icon: Arc::new(Mutex::new(None)),
            stats: Arc::new(Mutex::new(StatsState::from_settings(&settings))),
            stats_current_replay_files: Arc::new(Mutex::new(HashSet::new())),
            overlay_replay_data_active: AtomicBool::new(false),
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
            replay_state: Arc::new(Mutex::new(ReplayState {
                replays: Arc::new(Mutex::new(HashMap::new())),
                selected_replay_file: Arc::new(Mutex::new(None)),
            })),
        }
    }

    pub fn read_settings_memory(&self) -> AppSettings {
        self.active_settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_else(|_| AppSettings::from_saved_file())
    }

    pub fn replace_active_settings(&self, value: &AppSettings) -> AppSettings {
        let sanitized = AppSettings::merge_settings_with_defaults(value.to_value());

        if let Ok(mut cached_settings) = self.active_settings.lock() {
            *cached_settings = sanitized.clone();
        }

        self.file_logging_enabled.store(
            crate::logging_enabled_from_settings(&sanitized),
            Ordering::Release,
        );
        self.clear_main_identity_cache();
        sanitized
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
        let account_root = settings.account_folder.trim().to_string();

        if !account_root.is_empty() {
            if let Ok(cache) = self.discovered_main_names.lock() {
                if let Some(cached) = cache.get(&account_root) {
                    return cached.clone();
                }
            }
        }

        let names = configured_main_names_from_settings(&settings);

        if !account_root.is_empty() {
            if let Ok(mut cache) = self.discovered_main_names.lock() {
                cache.insert(account_root, names.clone());
            }
        }

        names
    }

    pub fn configured_main_handles(&self) -> HashSet<String> {
        let settings = self.read_settings_memory();
        let account_root = settings.account_folder.trim().to_string();

        if !account_root.is_empty() {
            if let Ok(cache) = self.discovered_main_handles.lock() {
                if let Some(cached) = cache.get(&account_root) {
                    return cached.clone();
                }
            }
        }

        let handles = configured_main_handles_from_settings(&settings);

        if !account_root.is_empty() {
            if let Ok(mut cache) = self.discovered_main_handles.lock() {
                cache.insert(account_root, handles.clone());
            }
        }

        handles
    }

    pub fn get_replay_state(&self) -> Arc<Mutex<ReplayState>> {
        self.replay_state.clone()
    }

    pub fn replay_cache_snapshot(&self) -> Vec<ReplayInfo> {
        self.replay_state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .replays
                    .lock()
                    .ok()
                    .map(|replays| replay_cache_snapshot(&replays))
            })
            .unwrap_or_default()
    }

    pub fn sync_replay_cache_slots(&self, limit: usize) -> Vec<ReplayInfo> {
        let main_names = self.configured_main_names();
        let main_handles = self.configured_main_handles();
        self.replay_state
            .lock()
            .map(|state| state.sync_replay_cache_slots(limit, &main_names, &main_handles))
            .unwrap_or_default()
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

    pub fn upsert_replay_cache_slot(&self, replay: &ReplayInfo) {
        if let Ok(replay_state) = self.replay_state.lock() {
            let replay_hash = s2coop_analyzer::detailed_replay_analysis::calculate_replay_hash(
                &std::path::PathBuf::from(&replay.file),
            );
            replay_state.upsert_replay_cache_slot(&replay_hash, replay);
        }
    }

    pub fn cached_replay_by_hash(&self, replay_hash: &str) -> Option<ReplayInfo> {
        self.replay_state
            .lock()
            .ok()
            .and_then(|state| state.cached_replay_by_hash(replay_hash))
    }

    pub fn clear_replay_cache_slots(&self) {
        if let Ok(replay_state) = self.replay_state.lock() {
            replay_state.clear_replay_cache_slots();
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
        let (victories, defeats) = session_counter_delta(result);
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
            crate::sco_log!("[SCO/players] background player scan started (limit={limit})");
            let replays = ReplayAnalysis::analyze_replays_with_identity(
                limit,
                &settings,
                &main_names,
                &main_handles,
                replay_scan_progress.as_ref(),
                replay_scan_in_flight.as_ref(),
            );
            let selected = replays.first().map(|replay| replay.file.clone());

            match replay_state.lock() {
                Ok(state) => {
                    if let Ok(mut cache) = state.replays.lock() {
                        cache.clear();
                        for replay in replays {
                            let replay_hash =
                                s2coop_analyzer::detailed_replay_analysis::calculate_replay_hash(
                                    &std::path::PathBuf::from(&replay.file),
                                );
                            upsert_replay_map(&mut cache, &replay_hash, &replay);
                        }
                    } else {
                        crate::sco_log!("[SCO/players] failed to update player replay cache");
                    }

                    if let Ok(mut selected_file) = state.selected_replay_file.lock() {
                        match selected_file.as_ref() {
                            Some(current)
                                if state.replays.lock().ok().is_some_and(|cache| {
                                    cache.values().any(|replay| &replay.file == current)
                                }) => {}
                            _ => {
                                *selected_file = selected;
                            }
                        }
                    }
                }
                Err(error) => {
                    crate::sco_log!("[SCO/players] failed to access replay state: {error}");
                }
            }

            players_scan_in_flight.store(false, Ordering::Release);
            crate::sco_log!("[SCO/players] background player scan completed");
        });
    }

    pub fn refresh_stats_snapshot_after_replay_upsert(&self) {
        let stats_replays = self.replay_cache_snapshot();

        let mut stats = match self.stats.lock() {
            Ok(stats) => stats,
            Err(_) => return,
        };

        if !stats.ready || stats.analysis_running {
            return;
        }

        let include_detailed = include_detailed_stats_for_cache(&stats, &stats_replays);
        let mode = AnalysisMode::from_include_detailed(include_detailed);
        let main_names = self.configured_main_names();
        let main_handles = self.configured_main_handles();
        let snapshot = ReplayAnalysis::build_rebuild_snapshot_with_identity(
            &stats_replays,
            include_detailed,
            &main_names,
            &main_handles,
        );
        apply_rebuild_snapshot(&mut stats, snapshot, mode);
        if !include_detailed {
            sync_detailed_analysis_status_from_replays(&mut stats, &stats_replays);
        }
    }

    pub fn upsert_replay_in_memory_cache(&self, replay_hash: &str, replay: &ReplayInfo) {
        let replay_state = self.get_replay_state();

        let _ = replay_state
            .lock()
            .map(|replay_state| replay_state.upsert_replay_cache_slot(replay_hash, replay));

        if let Ok(mut current_replay_files) = self.stats_current_replay_files.lock() {
            current_replay_files.insert(replay.file.clone());
        }

        let _ = replay_state
            .lock()
            .map(|replay_state| replay_state.set_current_replay_file(Some(&replay.file)));

        self.refresh_stats_snapshot_after_replay_upsert();
    }

    pub fn build_launch_main_identity(&self) -> (HashSet<String>, HashSet<String>) {
        let mut main_names = self.configured_main_names();
        let mut main_handles = self.configured_main_handles();

        if let Ok(stats) = self.stats.lock() {
            for name in &stats.main_players {
                let normalized = ReplayAnalysis::normalized_player_key(name);
                if !normalized.is_empty() {
                    main_names.insert(normalized);
                }
            }
        }

        let selected = self.get_current_replay_file();
        let replays = self.replay_cache_snapshot();
        let seed = selected
            .as_ref()
            .and_then(|file| replays.iter().find(|replay| &replay.file == file))
            .or_else(|| replays.first());
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
        self.stats
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

    pub fn replay_count_for_launch_detector(&self) -> usize {
        self.replay_state
            .lock()
            .ok()
            .and_then(|state| state.replays.lock().ok().map(|replays| replays.len()))
            .unwrap_or_default()
    }
}

impl ReplayState {
    pub fn sync_full_replay_cache_slots(
        &self,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let cached = self
            .replays
            .lock()
            .map(|replays| replay_cache_snapshot(&replays))
            .unwrap_or_default();

        let replays = if cached.is_empty() {
            let from_detailed_analysis = ReplayAnalysis::load_detailed_analysis_replays_snapshot(
                UNLIMITED_REPLAY_LIMIT,
                main_names,
                main_handles,
            );

            let loaded = if from_detailed_analysis.is_empty() {
                ReplayAnalysis::analyze_replays(UNLIMITED_REPLAY_LIMIT)
            } else {
                from_detailed_analysis
            };

            if let Ok(mut cache) = self.replays.lock() {
                cache.clear();
                for replay in &loaded {
                    let replay_hash =
                        s2coop_analyzer::detailed_replay_analysis::calculate_replay_hash(
                            &std::path::PathBuf::from(&replay.file),
                        );
                    upsert_replay_map(&mut cache, &replay_hash, replay);
                }
            }
            loaded
        } else {
            cached
        };

        let selected = replays.first().map(|replay| replay.file.clone());

        if let Ok(mut selected_file) = self.selected_replay_file.lock() {
            match selected_file.as_ref() {
                Some(current) if replays.iter().any(|replay| &replay.file == current) => {}
                _ => {
                    *selected_file = selected;
                }
            }
        }

        replays
    }

    pub fn sync_replay_cache_slots(
        &self,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let replays = self.sync_full_replay_cache_slots(main_names, main_handles);

        let mut limited = replays.clone();
        if limit > 0 {
            limited.truncate(limit);
        }

        limited
    }

    pub fn get_current_replay_file(&self) -> Option<String> {
        self.selected_replay_file
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    pub fn set_current_replay_file(&self, filename: Option<&str>) {
        if let Ok(mut selected_file) = self.selected_replay_file.lock() {
            *selected_file = filename.map(ToString::to_string);
        }
    }

    pub fn upsert_replay_cache_slot(&self, replay_hash: &str, replay: &ReplayInfo) {
        let _ = self
            .replays
            .lock()
            .map(|mut cache| upsert_replay_map(&mut cache, replay_hash, replay));
    }

    pub fn cached_replay_by_hash(&self, replay_hash: &str) -> Option<ReplayInfo> {
        self.replays
            .lock()
            .ok()
            .and_then(|cache| cache.get(replay_hash).cloned())
    }

    pub fn clear_replay_cache_slots(&self) {
        if let Ok(mut replays) = self.replays.lock() {
            replays.clear();
        }
        if let Ok(mut selected_replay_file) = self.selected_replay_file.lock() {
            *selected_replay_file = None;
        }
    }
}
