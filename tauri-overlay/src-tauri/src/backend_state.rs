use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};

use serde_json::Value;
use tauri::{tray::TrayIcon, Wry};

use crate::{
    apply_rebuild_snapshot, configured_main_handles, configured_main_names,
    replay_analysis::ReplayAnalysis, scan_replays, session_counter_delta,
    sync_detailed_analysis_status_from_replays, AnalysisMode, ReplayInfo, StatsState,
    UNLIMITED_REPLAY_LIMIT,
};

pub struct BackendState {
    pub tray_icon: Arc<Mutex<Option<TrayIcon<Wry>>>>,
    pub stats: Arc<Mutex<StatsState>>,
    pub stats_replays: Arc<Mutex<Vec<ReplayInfo>>>,
    pub stats_current_replay_files: Arc<Mutex<HashSet<String>>>,
    pub overlay_replay_data_active: AtomicBool,
    pub session_victories: AtomicU64,
    pub session_defeats: AtomicU64,
    replay_state: Arc<Mutex<ReplayState>>,
}

pub struct ReplayState {
    pub replays: Arc<Mutex<Vec<ReplayInfo>>>,
    pub selected_replay_file: Arc<Mutex<Option<String>>>,
}

static PLAYERS_SCAN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

fn upsert_replay_list(cache: &mut Vec<ReplayInfo>, replay: &ReplayInfo) {
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

impl BackendState {
    pub fn new() -> Self {
        Self {
            tray_icon: Arc::new(Mutex::new(None)),
            stats: Arc::new(Mutex::new(StatsState::from_settings())),
            stats_replays: Arc::new(Mutex::new(Vec::new())),
            stats_current_replay_files: Arc::new(Mutex::new(HashSet::new())),
            overlay_replay_data_active: AtomicBool::new(false),
            session_victories: AtomicU64::new(0),
            session_defeats: AtomicU64::new(0),
            replay_state: Arc::new(Mutex::new(ReplayState {
                replays: Arc::new(Mutex::new(Vec::new())),
                selected_replay_file: Arc::new(Mutex::new(None)),
            })),
        }
    }

    pub fn get_replay_state(&self) -> Arc<Mutex<ReplayState>> {
        self.replay_state.clone()
    }

    pub fn replay_cache_snapshot(&self) -> Vec<ReplayInfo> {
        self.replay_state
            .lock()
            .ok()
            .and_then(|state| state.replays.lock().ok().map(|replays| replays.clone()))
            .unwrap_or_default()
    }

    pub fn sync_replay_cache_slots(&self, limit: usize) -> Vec<ReplayInfo> {
        self.replay_state
            .lock()
            .map(|state| state.sync_replay_cache_slots(limit))
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
            replay_state.upsert_replay_cache_slot(replay);
        }
    }

    pub fn clear_replay_cache_slots(&self) {
        if let Ok(replay_state) = self.replay_state.lock() {
            replay_state.clear_replay_cache_slots();
        }
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

            match replay_state.lock() {
                Ok(state) => {
                    if let Ok(mut cache) = state.replays.lock() {
                        *cache = replays;
                    } else {
                        crate::sco_log!("[SCO/players] failed to update player replay cache");
                    }

                    if let Ok(mut selected_file) = state.selected_replay_file.lock() {
                        match selected_file.as_ref() {
                            Some(current)
                                if state.replays.lock().ok().is_some_and(|cache| {
                                    cache.iter().any(|replay| &replay.file == current)
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

            PLAYERS_SCAN_IN_FLIGHT.store(false, Ordering::Release);
            crate::sco_log!("[SCO/players] background player scan completed");
        });
    }

    pub fn refresh_stats_snapshot_after_replay_upsert(&self) {
        let stats_replays = match self.stats_replays.lock() {
            Ok(replays) => replays.clone(),
            Err(_) => return,
        };

        let mut stats = match self.stats.lock() {
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

    pub fn upsert_replay_in_memory_cache(&self, replay: &ReplayInfo) {
        let replay_state = self.get_replay_state();

        let _ = replay_state
            .lock()
            .map(|replay_state| replay_state.upsert_replay_cache_slot(replay));

        if let Ok(mut stats_replays) = self.stats_replays.lock() {
            upsert_replay_list(&mut stats_replays, replay);
        }

        if let Ok(mut current_replay_files) = self.stats_current_replay_files.lock() {
            current_replay_files.insert(replay.file.clone());
        }

        let _ = replay_state
            .lock()
            .map(|replay_state| replay_state.set_current_replay_file(Some(&replay.file)));

        self.refresh_stats_snapshot_after_replay_upsert();
    }

    pub fn build_launch_main_identity(&self) -> (HashSet<String>, HashSet<String>) {
        let mut main_names = configured_main_names();
        let mut main_handles = configured_main_handles();

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
            let normalized_name = ReplayAnalysis::normalized_player_key(&seed.p1);
            if !normalized_name.is_empty() {
                main_names.insert(normalized_name);
            }
            let normalized_handle = ReplayAnalysis::normalized_handle_key(&seed.p1_handle);
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
        self.replay_cache_snapshot().len()
    }
}

impl ReplayState {
    pub fn sync_full_replay_cache_slots(&self) -> Vec<ReplayInfo> {
        let cached = self
            .replays
            .lock()
            .map(|replays| replays.clone())
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

            if let Ok(mut cache) = self.replays.lock() {
                *cache = loaded.clone();
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

    pub fn sync_replay_cache_slots(&self, limit: usize) -> Vec<ReplayInfo> {
        let replays = self.sync_full_replay_cache_slots();

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

    pub fn upsert_replay_cache_slot(&self, replay: &ReplayInfo) {
        let _ = self
            .replays
            .lock()
            .map(|mut cache| upsert_replay_list(&mut cache, replay));
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
