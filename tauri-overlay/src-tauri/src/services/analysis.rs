use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{
    DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, DetailedReplayAnalyzer, GenerateCacheConfig,
    GenerateCacheRuntimeOptions, GenerateCacheStopController, GenerateCacheSummary,
    ReplayCacheFileIdentity,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::stats_state::AnalysisOutcome;
use crate::{
    AnalysisCompletedPayload, AnalysisMode, BackendState, PathManagerOps,
    QueuedReplayCacheEntrySink, ReplayAnalysis, ReplayCacheDatabase, StartupAnalysisRequestOutcome,
    StartupAnalysisTrigger, StatsSnapshot, StatsState, TauriOverlayOps, UNLIMITED_REPLAY_LIMIT,
};

const SCO_REPLAY_SCAN_PROGRESS_EVENT: &str = "sco://replay-scan-progress";
const SCO_ANALYSIS_COMPLETED_EVENT: &str = "sco://analysis-completed";

enum ProgressEmitterCommand {
    Stop,
}

impl TauriOverlayOps {
    fn emit_replay_scan_progress(app: &AppHandle<Wry>, log_event: bool) {
        let payload = app
            .state::<BackendState>()
            .replay_scan_progress()
            .as_payload();
        if log_event {
            crate::sco_debug!(
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
            crate::sco_warn!("[SCO/stats] failed to emit scan progress: {error}");
        }
    }

    pub fn emit_current_replay_scan_progress(app: &AppHandle<Wry>) {
        TauriOverlayOps::emit_replay_scan_progress(app, true);
    }

    fn emit_analysis_completed(app: &AppHandle<Wry>, mode: AnalysisMode, message: &str) {
        let payload = AnalysisCompletedPayload {
            mode: mode.key().to_string(),
            message: message.to_string(),
        };
        crate::sco_debug!(
            "[SCO/stats/event] emit {} mode={} message={}",
            SCO_ANALYSIS_COMPLETED_EVENT,
            payload.mode,
            payload.message
        );
        if let Err(error) = app.emit(SCO_ANALYSIS_COMPLETED_EVENT, payload) {
            crate::sco_warn!("[SCO/stats] failed to emit analysis completed event: {error}");
        }
    }

    pub fn clear_analysis_cache_files() {
        let cache_path = PathManagerOps::get_cache_path();
        let legacy_json_path = ReplayCacheDatabase::legacy_json_path_for_cache_path(&cache_path);
        let temp_path = PathBuf::from(format!("{}_temp", legacy_json_path.display()));
        let temp_jsonl_path =
            ReplayCacheDatabase::legacy_temp_jsonl_path_for_cache_path(&cache_path);
        let cache_db_paths = ReplayCacheDatabase::db_related_paths_for_cache_path(&cache_path);

        let paths = [legacy_json_path, temp_path, temp_jsonl_path]
            .into_iter()
            .chain(cache_db_paths);

        for path in paths {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                crate::sco_warn!(
                    "[SCO/cache] failed to delete analysis cache file '{}': {error}",
                    path.display()
                );
            }
        }
    }

    fn generate_detailed_analysis_cache(
        app: &AppHandle<Wry>,
        stats: &Arc<Mutex<StatsState>>,
        worker_count: usize,
        stop_controller: Arc<GenerateCacheStopController>,
        existing_detailed_cache_identities_by_hash: HashMap<String, ReplayCacheFileIdentity>,
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
                crate::sco_debug!("[SCO/stats] {normalized}");
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
        let cache_writer =
            crate::ReplayCacheWriteQueue::start_detailed_analysis(output_file.clone());
        let runtime = GenerateCacheRuntimeOptions::default()
            .with_worker_count(worker_count)
            .with_stop_controller(stop_controller)
            .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(
                cache_writer.sender(),
            )))
            .with_cache_entry_sink_batch_size(DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE)
            .with_existing_detailed_cache_identities_by_hash(
                existing_detailed_cache_identities_by_hash,
            );
        let analysis_start = Instant::now();
        let analysis_result = DetailedReplayAnalyzer::analyze_full_detailed(
            &config,
            resources.as_ref(),
            Some(&logger),
            &runtime,
        );
        let analysis_elapsed = analysis_start.elapsed();
        drop(runtime);
        let writer_finish_start = Instant::now();
        let write_result = cache_writer.finish();
        let writer_finish_elapsed = writer_finish_start.elapsed();
        crate::sco_debug!(
            concat!(
                "[SCO/stats] detailed sqlite writer batch_size={} ",
                "analyze_ms={} finish_wait_ms={} sqlite_open_ms={} sqlite_write_ms={} ",
                "batches={} attempted_entries={} persisted_entries={} failed_batches={}"
            ),
            DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE,
            analysis_elapsed.as_millis(),
            writer_finish_elapsed.as_millis(),
            write_result.database_open().as_millis(),
            write_result.sqlite_write().as_millis(),
            write_result.processed_batches(),
            write_result.attempted_entries(),
            write_result.persisted_entries(),
            write_result.failed_batches()
        );
        if let Ok(summary) = analysis_result.as_ref() {
            let timing = summary.timing_report();
            crate::sco_debug!(
                concat!(
                    "[SCO/stats] detailed analyzer phases total_ms={} ",
                    "collect_files_ms={} collect_candidates_ms={} replay_analysis_ms={} ",
                    "parse_detailed_ms={} detailed_report_ms={} queue_send_ms={} ",
                    "canonicalize_ms={}"
                ),
                timing.total().as_millis(),
                timing.collect_replay_files().as_millis(),
                timing.collect_candidates_parallel().as_millis(),
                timing.replay_analysis_parallel().as_millis(),
                timing.replay_analysis_parse_detailed().as_millis(),
                timing.replay_analysis_detailed_report().as_millis(),
                timing.replay_analysis_temp_entry_write().as_millis(),
                timing.canonicalize_entries().as_millis()
            );
        }
        if write_result.failed_batches() > 0 {
            return Err(format!(
                "Failed to persist {} detailed-analysis cache writer batch(es)",
                write_result.failed_batches()
            ));
        }
        analysis_result
            .map_err(|error| format!("Failed to generate '{}': {error}", output_file.display()))
    }

    fn startup_analysis_mode(include_detailed: bool) -> &'static str {
        TauriOverlayOps::analysis_mode(include_detailed).slug()
    }

    pub fn prepare_startup_analysis_request(
        stats: &mut StatsState,
    ) -> StartupAnalysisRequestOutcome {
        let include_detailed = stats.detailed_analysis_atstart();
        if stats.startup_analysis_requested() {
            return StartupAnalysisRequestOutcome::new(include_detailed, false);
        }

        stats.set_startup_analysis_requested(true);
        let mode = TauriOverlayOps::analysis_mode(include_detailed);
        stats.set_message(format!(
            "{}: startup requested while the frontend loads.",
            mode.display()
        ));

        StartupAnalysisRequestOutcome::new(include_detailed, true)
    }

    pub fn request_startup_analysis(
        app: AppHandle<Wry>,
        stats: Arc<Mutex<StatsState>>,
        stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
        detailed_stop_controller_slot: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
        trigger: StartupAnalysisTrigger,
    ) -> Result<StartupAnalysisRequestOutcome, String> {
        let outcome = {
            let mut guard = stats
                .lock()
                .map_err(|error| format!("Failed to access stats state: {error}"))?;
            TauriOverlayOps::prepare_startup_analysis_request(&mut guard)
        };

        if outcome.started() {
            crate::sco_info!(
                "[SCO/stats] startup analysis requested from {} mode={}",
                trigger.label(),
                TauriOverlayOps::startup_analysis_mode(outcome.include_detailed())
            );
            TauriOverlayOps::spawn_startup_analysis_task(
                app,
                stats,
                stats_current_replay_files_slot,
                detailed_stop_controller_slot,
                outcome.include_detailed(),
            );
        } else {
            crate::sco_debug!(
                "[SCO/stats] startup analysis already requested before {} mode={}",
                trigger.label(),
                TauriOverlayOps::startup_analysis_mode(outcome.include_detailed())
            );
        }

        Ok(outcome)
    }

    fn load_existing_detailed_cache_identities_by_hash() -> HashMap<String, ReplayCacheFileIdentity>
    {
        let cache_path = PathManagerOps::get_cache_path();
        let database = match ReplayCacheDatabase::open_for_cache_path(&cache_path) {
            Ok(database) => database,
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache-db] failed to open existing detailed cache identity database: {error}"
                );
                return HashMap::new();
            }
        };

        match database.load_detailed_cache_identities_by_hash() {
            Ok(identities_by_hash) => identities_by_hash,
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache-db] failed to load existing detailed cache identities for reuse: {error}"
                );
                HashMap::new()
            }
        }
    }

    pub fn merge_cache_entries(
        existing_by_hash: &HashMap<String, CacheReplayEntry>,
        mut new_entries: Vec<CacheReplayEntry>,
    ) -> Vec<CacheReplayEntry> {
        let mut merged = existing_by_hash.clone();

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
                    let should_replace = (!existing.detailed_analysis && entry.detailed_analysis)
                        || ((entry.detailed_analysis || !existing.detailed_analysis)
                            && entry.date > existing.date);
                    if should_replace {
                        merged.insert(hash, entry);
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

    fn run_analysis(
        app: &AppHandle<Wry>,
        analysis_state: &Arc<Mutex<StatsState>>,
        detailed_stop_controller_slot: &Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
        limit: usize,
        include_detailed: bool,
    ) -> Result<AnalysisOutcome, String> {
        let state = app.state::<BackendState>();
        if include_detailed {
            let load_existing_start = Instant::now();
            let existing_detailed_cache_identities_by_hash =
                TauriOverlayOps::load_existing_detailed_cache_identities_by_hash();
            crate::sco_debug!(
                "[SCO/stats] detailed existing-cache identity load entries={} elapsed={}ms",
                existing_detailed_cache_identities_by_hash.len(),
                load_existing_start.elapsed().as_millis()
            );
            let worker_count = state
                .read_settings_memory()
                .normalized_analysis_worker_threads();
            let stop_controller = Arc::new(GenerateCacheStopController::new());
            if let Ok(mut slot) = detailed_stop_controller_slot.lock() {
                *slot = Some(stop_controller.clone());
            }

            let generation_start = Instant::now();
            let generation_result = TauriOverlayOps::generate_detailed_analysis_cache(
                app,
                analysis_state,
                worker_count,
                stop_controller,
                existing_detailed_cache_identities_by_hash,
            );

            if let Ok(mut slot) = detailed_stop_controller_slot.lock() {
                slot.take();
            }

            let generation_summary = generation_result?;
            let completed = generation_summary.completed();
            crate::sco_info!(
                concat!(
                    "[SCO/stats] detailed scan generated '{}' with {} new replay entr{} ",
                    "completed={} elapsed={}ms"
                ),
                ReplayCacheDatabase::db_path_for_cache_path(&PathManagerOps::get_cache_path())
                    .display(),
                generation_summary.scanned_replays(),
                if generation_summary.scanned_replays() == 1 {
                    "y"
                } else {
                    "ies"
                },
                completed,
                generation_start.elapsed().as_millis()
            );

            let snapshot = StatsSnapshot::new(
                completed,
                0,
                Vec::new(),
                Vec::new(),
                TauriOverlayOps::empty_stats_payload(),
                Default::default(),
                TauriOverlayOps::cache_generation_completed_message(
                    AnalysisMode::Detailed,
                    generation_start.elapsed(),
                ),
            );

            Ok(AnalysisOutcome::with_snapshot(
                generation_summary.scanned_replays(),
                snapshot,
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
            Ok(AnalysisOutcome::new(replays.len(), replays, true))
        }
    }

    pub fn spawn_analysis_task(
        app: AppHandle<Wry>,
        stats: Arc<Mutex<StatsState>>,
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
                    crate::sco_warn!(
                        "[SCO/stats] failed to start background {} thread: {error}",
                        mode.display()
                    );
                    return;
                }
            };

            if guard.analysis_running() {
                let active_mode = guard.analysis_running_mode();
                if active_mode == Some(mode) {
                    crate::sco_info!("[SCO/stats] {} already running", mode.display());
                    guard.set_message(TauriOverlayOps::analysis_already_running_message(mode));
                } else {
                    crate::sco_info!(
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
            crate::sco_info!("[SCO/stats] {} thread started", mode.display());
            replay_scan_progress_for_thread.set_stage(if include_detailed {
                "detailed_analysis_running"
            } else {
                "scan_running"
            });
            replay_scan_progress_for_thread.set_status("Parsing");
            TauriOverlayOps::emit_replay_scan_progress(&app_for_progress, true);

            let (progress_tx, progress_rx) = mpsc::channel::<ProgressEmitterCommand>();
            let progress_handle = thread::spawn(move || {
                loop {
                    match progress_rx.recv_timeout(Duration::from_millis(150)) {
                        Ok(ProgressEmitterCommand::Stop) => {
                            TauriOverlayOps::emit_replay_scan_progress(
                                &app_for_progress_updates,
                                true,
                            );
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
                    crate::sco_warn!("[SCO/stats] {} failed: {message}", mode.display());
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

            if let Ok(mut guard) = analysis_state.lock()
                && include_detailed
            {
                let replay_count = analysis_outcome.reported_replay_count();
                if analysis_outcome.analysis_completed() {
                    guard.set_analysis_running_status(mode, "cache generation completed");
                    guard.set_message(format!(
                        "Generated '{}' with {} replay entr{}.",
                        ReplayCacheDatabase::db_path_for_cache_path(
                            &PathManagerOps::get_cache_path()
                        )
                        .display(),
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

            let (reported_replay_count, all_replays, detailed_completed, analysis_snapshot) =
                analysis_outcome.into_parts();
            let has_snapshot = analysis_snapshot.is_some();

            let all_replays = if has_snapshot {
                all_replays
            } else {
                let dedupe_start = Instant::now();
                let mut hashes = HashMap::new();

                let all_replays = all_replays
                    .into_iter()
                    .filter(|replay| {
                        let file_key = replay.file().to_string();
                        let is_detailed = hashes.get(&file_key);

                        if is_detailed.is_some() && (*is_detailed.unwrap() || !replay.is_detailed) {
                            false
                        } else {
                            hashes.insert(file_key, replay.is_detailed);
                            true
                        }
                    })
                    .collect::<Vec<_>>();
                crate::sco_debug!(
                    "[SCO/stats] {} dedupe replay summaries replays={} elapsed={}ms",
                    mode.display(),
                    all_replays.len(),
                    dedupe_start.elapsed().as_millis()
                );
                all_replays
            };

            let current_files_start = Instant::now();
            let current_replay_files =
                settings_for_thread.current_replay_files_snapshot(UNLIMITED_REPLAY_LIMIT);
            crate::sco_debug!(
                "[SCO/stats] {} current replay file snapshot files={} elapsed={}ms",
                mode.display(),
                current_replay_files.len(),
                current_files_start.elapsed().as_millis()
            );
            if include_detailed && !detailed_completed {
                replay_scan_progress_for_thread.set_total(current_replay_files.len() as u64);
            }
            match current_replay_files_slot.lock() {
                Ok(mut current_files) => {
                    *current_files = current_replay_files;
                }
                Err(_) => {
                    crate::sco_warn!(
                        "[SCO/stats] failed to update current replay file set after scan"
                    );
                }
            }

            if !has_snapshot {
                replay_scan_progress_for_thread.set_stage("building_statistics");
            }
            let snapshot = if let Some(snapshot) = analysis_snapshot {
                snapshot
            } else {
                let dictionary_start = Instant::now();
                let dictionary = app_for_analysis
                    .state::<BackendState>()
                    .dictionary_data()
                    .ok();
                crate::sco_debug!(
                    "[SCO/stats] {} rebuild dictionary access elapsed={}ms available={}",
                    mode.display(),
                    dictionary_start.elapsed().as_millis(),
                    dictionary.is_some()
                );
                let snapshot_start = Instant::now();
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
                crate::sco_debug!(
                    "[SCO/stats] {} build rebuild snapshot elapsed={}ms",
                    mode.display(),
                    snapshot_start.elapsed().as_millis()
                );
                snapshot
            };

            let mut guard = match analysis_state.lock() {
                Ok(guard) => guard,
                Err(error) => {
                    crate::sco_warn!(
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
            } else if !has_snapshot {
                guard.set_analysis_running_status(mode, "building statistics");
            }

            let apply_snapshot_start = Instant::now();
            TauriOverlayOps::apply_rebuild_snapshot(&mut guard, snapshot, mode);
            crate::sco_debug!(
                "[SCO/stats] {} apply analysis state elapsed={}ms prebuilt_snapshot={}",
                mode.display(),
                apply_snapshot_start.elapsed().as_millis(),
                has_snapshot
            );
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
            } else if include_detailed {
                guard.set_message(TauriOverlayOps::cache_generation_completed_message(
                    mode,
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
                let dictionary = app_for_analysis
                    .state::<BackendState>()
                    .dictionary_data()
                    .ok();
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

            crate::sco_info!(
                "[SCO/stats] {} finished in {}ms for {} replay(s) completed={}",
                mode.display(),
                started_at.elapsed().as_millis(),
                reported_replay_count,
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

    fn spawn_startup_analysis_task(
        app: AppHandle<Wry>,
        stats: Arc<Mutex<StatsState>>,
        stats_current_replay_files_slot: Arc<Mutex<HashSet<String>>>,
        detailed_stop_controller_slot: Arc<Mutex<Option<Arc<GenerateCacheStopController>>>>,
        include_detailed: bool,
    ) {
        crate::sco_info!(
            "[SCO/stats] startup analysis mode={}",
            TauriOverlayOps::startup_analysis_mode(include_detailed)
        );
        TauriOverlayOps::spawn_analysis_task(
            app,
            stats,
            stats_current_replay_files_slot,
            detailed_stop_controller_slot,
            include_detailed,
            UNLIMITED_REPLAY_LIMIT,
        );
    }
}
