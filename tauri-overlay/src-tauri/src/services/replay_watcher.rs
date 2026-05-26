use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{ReplayAnalysisResources, ReplayFileIdentity};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{Manager, Wry};

use crate::{
    BackendState, PathManagerOps, ReplayAnalysis, ReplayCacheDatabase, ReplayCacheWriteQueue,
    ReplayInfo, Sc2GameState, TauriOverlayOps, overlay_info,
};

pub enum ReplayWatcherMessage {
    Event(notify::Result<notify::Event>),
    RefreshRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayProcessOutcome {
    Processed,
    RetryLater,
    AlreadyHandled,
    Ignored,
}

impl TauriOverlayOps {
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
                crate::sco_warn!(
                    "[SCO/watch] parse abort file='{}' reason=replay_analysis_resources_unavailable",
                    path.to_string_lossy()
                );
                return None;
            }
        };
        let file = path.to_string_lossy().to_string();
        crate::sco_debug!(
            "[SCO/watch] parse start file='{}' max_attempts={} retry_ms={}",
            file,
            MAX_ATTEMPTS,
            RETRY_DELAY.as_millis()
        );
        let mut previous_size: Option<u64> = None;

        for attempt in 0..MAX_ATTEMPTS {
            let attempt_num = attempt + 1;
            if !path.exists() {
                crate::sco_debug!(
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
            crate::sco_debug!(
                "[SCO/watch] parse attempt file='{}' attempt={}/{} size={} modified={}",
                file,
                attempt_num,
                MAX_ATTEMPTS,
                size_bytes,
                modified
            );

            if size_bytes < MIN_REPLAY_SIZE_BYTES {
                crate::sco_debug!(
                    "[SCO/watch] parse wait file='{}' attempt={}/{} reason=size_below_min min={} current={}",
                    file,
                    attempt_num,
                    MAX_ATTEMPTS,
                    MIN_REPLAY_SIZE_BYTES,
                    size_bytes
                );
                previous_size = Some(size_bytes);
                if attempt + 1 < MAX_ATTEMPTS {
                    thread::sleep(RETRY_DELAY);
                }
                continue;
            }

            match previous_size {
                None => {
                    crate::sco_debug!(
                        "[SCO/watch] parse wait file='{}' attempt={}/{} reason=awaiting_size_stability size={}",
                        file,
                        attempt_num,
                        MAX_ATTEMPTS,
                        size_bytes
                    );
                    previous_size = Some(size_bytes);
                    if attempt + 1 < MAX_ATTEMPTS {
                        thread::sleep(RETRY_DELAY);
                    }
                    continue;
                }
                Some(previous) if previous != size_bytes => {
                    crate::sco_debug!(
                        "[SCO/watch] parse wait file='{}' attempt={}/{} reason=size_changed previous={} current={}",
                        file,
                        attempt_num,
                        MAX_ATTEMPTS,
                        previous,
                        size_bytes
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
                    crate::sco_error!(
                        "[SCO/watch] parse panic file='{}' attempt={}/{} message='{}'",
                        file,
                        attempt_num,
                        MAX_ATTEMPTS,
                        panic_message
                    );
                    if attempt + 1 < MAX_ATTEMPTS {
                        crate::sco_debug!(
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
                    crate::sco_debug!(
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
                crate::sco_info!(
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
            crate::sco_debug!(
                "[SCO/watch] parse pending file='{}' attempt={}/{} result='Unparsed'",
                file,
                attempt_num,
                MAX_ATTEMPTS
            );

            if attempt + 1 < MAX_ATTEMPTS {
                crate::sco_debug!(
                    "[SCO/watch] parse retry scheduled file='{}' next_attempt={} wait_ms={}",
                    file,
                    attempt_num + 1,
                    RETRY_DELAY.as_millis()
                );
                thread::sleep(RETRY_DELAY);
            }
        }
        crate::sco_warn!(
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
        TauriOverlayOps::persist_detailed_cache_entry_to_path_with_lock(
            cache_path,
            entry,
            &local_lock,
        )
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

        let write_result = ReplayCacheWriteQueue::write_entries_to_path(
            cache_path.to_path_buf(),
            std::slice::from_ref(entry),
        )
        .map_err(|error| error.to_string())?;
        if write_result.failed_batches() > 0 {
            return Err(format!(
                "Failed to persist {} detailed cache writer batch(es)",
                write_result.failed_batches()
            ));
        }
        Ok(())
    }

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
                crate::sco_warn!(
                    "[SCO/{log_prefix}] failed to persist detailed cache entry for '{}': {error}",
                    replay_file
                );
                return;
            }

            crate::sco_debug!(
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
                if meta.is_file() && TauriOverlayOps::path_is_sc2_replay(&path) {
                    out.push(path);
                }
            }
        }
        out
    }

    pub fn replay_watch_roots_match(current_root: Option<&Path>, next_root: Option<&Path>) -> bool {
        match (current_root, next_root) {
            (Some(current_root), Some(next_root)) => current_root == next_root,
            (None, None) => true,
            _ => false,
        }
    }

    fn seed_handled_replay_files_for_watch_root(
        replay_root: &Path,
        handled_files: &mut HashSet<String>,
    ) {
        handled_files.clear();
        for path in TauriOverlayOps::collect_sc2_replay_files(replay_root) {
            let key = path.to_string_lossy().to_string();
            if !key.is_empty() {
                handled_files.insert(key);
            }
        }
    }

    fn process_new_replay_path(
        app: &tauri::AppHandle<Wry>,
        path: &Path,
        handled_files: &mut HashSet<String>,
    ) -> ReplayProcessOutcome {
        if !TauriOverlayOps::path_is_sc2_replay(path) {
            return ReplayProcessOutcome::Ignored;
        }
        if !path.exists() {
            crate::sco_debug!(
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
            crate::sco_debug!("[SCO/watch] skip file='{}' reason=already_handled", file);
            return ReplayProcessOutcome::AlreadyHandled;
        }
        crate::sco_info!("[SCO/watch] processing new replay file='{}'", file);

        let state = app.state::<BackendState>();
        let resources = state.replay_analysis_resources().ok();
        let Some((parsed, cache_entry)) =
            TauriOverlayOps::parse_new_replay_with_retries(path, resources.as_deref())
        else {
            crate::sco_warn!("[SCO/watch] failed to parse new replay '{}'", file);
            return ReplayProcessOutcome::RetryLater;
        };

        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let replay = parsed.oriented_for_main_identity(&main_names, &main_handles);
        if replay.main_commander().trim().is_empty() && replay.ally_commander().trim().is_empty() {
            crate::sco_warn!(
                "[SCO/watch] parsed replay ignored file='{}' reason=missing_commanders main='{}' ally='{}'",
                replay.file,
                replay.main_commander(),
                replay.ally_commander()
            );
            handled_files.insert(file);
            return ReplayProcessOutcome::Ignored;
        }

        handled_files.insert(file);
        crate::sco_info!(
            "[SCO/watch] replay accepted file='{}' date={} result='{}' main='{}' ally='{}' main_comm='{}' ally_comm='{}'",
            replay.file,
            replay.date,
            replay.result,
            replay.main().name,
            replay.ally().name,
            replay.main_commander(),
            replay.ally_commander()
        );
        let replay_cached = if cache_entry.is_some() {
            state.record_replay_cache_update_if_persistable(&replay, true)
        } else {
            false
        };
        if replay_cached {
            state.record_session_result(&replay.result);
        } else {
            crate::sco_debug!(
                "[SCO/watch] replay displayed without cache file='{}' reason=not_cache_persistable",
                replay.file
            );
        }
        let settings = state.read_settings_memory();
        let game_state_transition =
            state.transition_sc2_game_state(Sc2GameState::GameEnded, Instant::now());
        let entered_game_end_state = game_state_transition
            .is_some_and(|transition| transition.current() == Sc2GameState::GameEnded);
        TauriOverlayOps::log_sc2_game_state_transition(game_state_transition, "replay_processed");
        if entered_game_end_state {
            TauriOverlayOps::spawn_today_win_bonus_scan(app.clone(), replay.file.clone());
        } else {
            crate::sco_debug!(
                "[SCO/today-win-bonus] scan suppressed replay='{}' reason=not_game_end_transition map='{}' result='{}' main_comm='{}' ally_comm='{}'",
                replay.file,
                replay.map(),
                replay.result(),
                replay.main_commander(),
                replay.ally_commander()
            );
        }
        let show_replay_info_after_game = settings.show_replay_info_after_game();

        if show_replay_info_after_game {
            crate::sco_debug!(
                "[SCO/watch] emitting replay to overlay file='{}'",
                replay.file
            );
            overlay_info::OverlayInfoOps::emit_replay_to_overlay_from_replay(app, &replay, true);
            state.set_overlay_replay_data_active(true);
        } else {
            crate::sco_debug!(
                "[SCO/watch] replay overlay suppressed by settings file='{}'",
                replay.file
            );
            state.set_overlay_replay_data_active(false);
        }

        if let Some(cache_entry) = cache_entry {
            TauriOverlayOps::spawn_detailed_cache_persist(&state, cache_entry, "watch");
        }

        let invalidation_generation = state.invalidate_delayed_player_stats_popup_generation();
        crate::sco_debug!(
            "[SCO/watch] invalidated delayed player stats popups generation={} replay='{}'",
            invalidation_generation,
            replay.file
        );

        ReplayProcessOutcome::Processed
    }

    fn active_replay_watch_root(app: &tauri::AppHandle<Wry>) -> Option<PathBuf> {
        app.state::<BackendState>()
            .read_settings_memory()
            .replay_watch_root()
    }

    fn path_belongs_to_watch_root(path: &Path, watch_root: Option<&Path>) -> bool {
        watch_root
            .map(|root| path.starts_with(root))
            .unwrap_or(false)
    }

    fn apply_replay_watch_root(
        watcher: &mut RecommendedWatcher,
        watched_root: &mut Option<PathBuf>,
        handled_files: &mut HashSet<String>,
        pending_fallback_files: &mut HashSet<String>,
        next_root: Option<PathBuf>,
    ) {
        if TauriOverlayOps::replay_watch_roots_match(watched_root.as_deref(), next_root.as_deref())
        {
            return;
        }

        if let Some(previous_root) = watched_root.take()
            && let Err(error) = watcher.unwatch(&previous_root)
        {
            crate::sco_warn!(
                "[SCO/watch] failed to stop watching replay root '{}': {error}",
                previous_root.display()
            );
        }

        handled_files.clear();
        pending_fallback_files.clear();

        let Some(replay_root) = next_root else {
            return;
        };

        if let Err(error) = watcher.watch(&replay_root, RecursiveMode::Recursive) {
            crate::sco_error!(
                "[SCO/watch] failed to watch replay root '{}': {error}",
                replay_root.display()
            );
            return;
        }

        TauriOverlayOps::seed_handled_replay_files_for_watch_root(&replay_root, handled_files);
        crate::sco_info!(
            "[SCO/watch] replay watcher active on {}",
            replay_root.display()
        );
        *watched_root = Some(replay_root);
    }

    pub fn process_replay_detailed(
        state: &BackendState,
        path: &Path,
    ) -> (ReplayProcessOutcome, Option<ReplayInfo>) {
        if !TauriOverlayOps::path_is_sc2_replay(path) {
            return (ReplayProcessOutcome::Ignored, None);
        }

        if !path.exists() {
            crate::sco_debug!(
                "[SCO/show] skip path='{}' reason=missing",
                path.to_string_lossy()
            );
            return (ReplayProcessOutcome::RetryLater, None);
        }

        let file = path.to_string_lossy().to_string();

        if file.is_empty() {
            return (ReplayProcessOutcome::Ignored, None);
        }

        crate::sco_info!("[SCO/show] processing existing replay file='{}'", file);

        let replay_hash = ReplayFileIdentity::calculate_hash(path);
        if let Some(existing) = state.cached_replay_by_hash(&replay_hash)
            && existing.is_detailed
        {
            return (ReplayProcessOutcome::Processed, Some(existing));
        }

        match ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
            .and_then(|database| database.load_entry_by_hash(&replay_hash))
        {
            Ok(Some(entry)) if entry.detailed_analysis => {
                let replay = TauriOverlayOps::replay_info_from_cache_entry_for_state(state, &entry);
                state.record_replay_cache_update_if_persistable(&replay, true);
                return (ReplayProcessOutcome::Processed, Some(replay));
            }
            Ok(_) => {}
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache-db] replay show cache lookup failed for '{}': {error}",
                    file
                );
            }
        }

        let resources = state.replay_analysis_resources().ok();
        let Some((parsed, cache_entry)) =
            TauriOverlayOps::parse_new_replay_with_retries(path, resources.as_deref())
        else {
            crate::sco_warn!("[SCO/show] failed to parse existing replay '{}'", file);
            return (ReplayProcessOutcome::RetryLater, None);
        };

        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let replay = parsed.oriented_for_main_identity(&main_names, &main_handles);

        crate::sco_info!(
            "[SCO/show] replay accepted file='{}' date={} result='{}' main='{}' ally='{}' main_comm='{}' ally_comm='{}'",
            replay.file,
            replay.date,
            replay.result,
            replay.main().name,
            replay.ally().name,
            replay.main_commander(),
            replay.ally_commander()
        );

        let replay_cached = if cache_entry.is_some() {
            state.record_replay_cache_update_if_persistable(&replay, true)
        } else {
            false
        };
        if !replay_cached {
            crate::sco_debug!(
                "[SCO/show] replay displayed without cache file='{}' reason=not_cache_persistable",
                replay.file
            );
        }
        if let Some(cache_entry) = cache_entry {
            TauriOverlayOps::spawn_detailed_cache_persist(state, cache_entry, "show");
        }

        (ReplayProcessOutcome::Processed, Some(replay))
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
                    crate::sco_debug!("[SCO/watch] fallback queued file='{}'", file);
                }
                if should_log_start && !pending_fallback_files.is_empty() {
                    crate::sco_debug!(
                        "[SCO/watch] fallback polling started pending={}",
                        pending_fallback_files.len()
                    );
                }
            }
            ReplayProcessOutcome::Processed
            | ReplayProcessOutcome::AlreadyHandled
            | ReplayProcessOutcome::Ignored => {
                if pending_fallback_files.remove(file) {
                    crate::sco_debug!("[SCO/watch] fallback cleared file='{}'", file);
                    if pending_fallback_files.is_empty() {
                        crate::sco_debug!("[SCO/watch] fallback polling stopped");
                    }
                }
            }
        }
    }

    pub fn spawn_replay_creation_watcher(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel::<ReplayWatcherMessage>();
            let watcher_tx = tx.clone();
            let mut watcher = match RecommendedWatcher::new(
                move |event_result| {
                    let _ = watcher_tx.send(ReplayWatcherMessage::Event(event_result));
                },
                NotifyConfig::default(),
            ) {
                Ok(watcher) => watcher,
                Err(error) => {
                    crate::sco_error!("[SCO/watch] failed to initialize replay watcher: {error}");
                    return;
                }
            };
            app.state::<BackendState>()
                .set_replay_watcher_sender(Some(tx));

            let mut handled_files = HashSet::<String>::new();
            let mut pending_fallback_files = HashSet::<String>::new();
            let mut watched_root = None::<PathBuf>;
            let mut missing_root_logged_at = None::<Instant>;

            loop {
                let next_root = TauriOverlayOps::active_replay_watch_root(&app);
                let next_root_available = next_root.is_some();
                TauriOverlayOps::apply_replay_watch_root(
                    &mut watcher,
                    &mut watched_root,
                    &mut handled_files,
                    &mut pending_fallback_files,
                    next_root,
                );
                if watched_root.is_some() {
                    missing_root_logged_at = None;
                } else if !next_root_available {
                    let now = Instant::now();
                    let should_log = missing_root_logged_at
                        .map(|last| now.duration_since(last) >= Duration::from_secs(5))
                        .unwrap_or(true);
                    if should_log {
                        crate::sco_warn!(
                            "[SCO/watch] account_folder replay root unavailable, retrying in 5s"
                        );
                        missing_root_logged_at = Some(now);
                    }
                }

                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(ReplayWatcherMessage::RefreshRoot) => {
                        continue;
                    }
                    Ok(ReplayWatcherMessage::Event(Ok(event))) => {
                        let current_root = watched_root.clone();
                        if !TauriOverlayOps::is_replay_creation_event(&event.kind) {
                            continue;
                        }
                        crate::sco_debug!(
                            "[SCO/watch] notify event kind={:?} paths={}",
                            event.kind,
                            event.paths.len()
                        );

                        for path in event.paths {
                            if !TauriOverlayOps::path_belongs_to_watch_root(
                                &path,
                                current_root.as_deref(),
                            ) {
                                continue;
                            }
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
                    Ok(ReplayWatcherMessage::Event(Err(error))) => {
                        crate::sco_warn!("[SCO/watch] watcher event error: {error}");
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if pending_fallback_files.is_empty() || watched_root.is_none() {
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
                        app.state::<BackendState>().set_replay_watcher_sender(None);
                        crate::sco_warn!(
                            "[SCO/watch] replay watcher channel disconnected; stopping"
                        );
                        break;
                    }
                }
            }
        });
    }
}
