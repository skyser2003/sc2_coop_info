use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{Manager, Wry};

use crate::{
    ActiveWindowDetector, ActiveWindowListener, AppSettings, BackendState,
    FirstWinBonusDisplayMode, FirstWinBonusTimerPayload, PathManagerOps, ReplayAnalysis,
    ReplayAnalysisOps, ReplayCacheDatabase, Sc2GameState, Sc2GameStateTransition, Sc2Server,
    TauriOverlayOps, overlay_info, today_win_bonus,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FirstWinBonusReplayContext {
    server: Sc2Server,
    modified_time_seconds: u64,
}

impl FirstWinBonusReplayContext {
    fn new(server: Sc2Server, modified_time_seconds: u64) -> Option<Self> {
        if modified_time_seconds == 0 {
            return None;
        }
        Some(Self {
            server,
            modified_time_seconds,
        })
    }

    fn server(&self) -> Sc2Server {
        self.server
    }

    fn modified_time_seconds(&self) -> u64 {
        self.modified_time_seconds
    }
}

struct TodayWinBonusPersistResult {
    saved_time: Option<String>,
    error: Option<String>,
}

impl TodayWinBonusPersistResult {
    fn new(saved_time: Option<String>, error: Option<String>) -> Self {
        Self { saved_time, error }
    }

    fn saved(saved_time: String) -> Self {
        Self::new(Some(saved_time), None)
    }

    fn failed(error: String) -> Self {
        Self::new(None, Some(error))
    }

    fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    fn into_parts(self) -> (Option<String>, Option<String>) {
        (self.saved_time, self.error)
    }
}

impl TauriOverlayOps {
    pub fn spawn_today_win_bonus_scan(
        app: tauri::AppHandle<Wry>,
        replay_file: String,
        replay_server: Option<Sc2Server>,
    ) {
        thread::spawn(move || {
            const SCAN_DURATION: Duration = Duration::from_secs(45);

            let scan_started_at = Instant::now();
            let scan_deadline = scan_started_at + SCAN_DURATION;
            let mut today_win_bonus_capture = today_win_bonus::TodayWinBonusWindowCapture::new();
            let replay_file_modified_time_seconds = {
                let state = app.state::<BackendState>();
                match state.update_latest_replay_file_modified_time(Path::new(&replay_file)) {
                    Ok(modified_time_seconds) => modified_time_seconds,
                    Err(error) => {
                        crate::sco_warn!(
                            "[SCO/today-win-bonus] failed to read replay file modified time replay='{}' error='{}'",
                            replay_file,
                            error
                        );
                        None
                    }
                }
            };
            let replay_context = replay_server.and_then(|server| {
                replay_file_modified_time_seconds
                    .and_then(|seconds| FirstWinBonusReplayContext::new(server, seconds))
            });
            crate::sco_info!(
                "[SCO/today-win-bonus] scan started replay='{}' replay_file_modified_time_seconds='{}' duration_secs={} interval_secs=1 initial_capture_method='{}' fallback_after_failures={}",
                replay_file,
                replay_file_modified_time_seconds
                    .map(|seconds| seconds.to_string())
                    .unwrap_or_default(),
                SCAN_DURATION.as_secs(),
                today_win_bonus::TodayWinBonusWindowCapture::initial_capture_method(),
                today_win_bonus::WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK
            );
            let mut attempts = 0u32;
            let mut captured = 0u32;
            let mut not_detected = 0u32;
            let mut skipped_not_focused = 0u32;
            let mut errors = 0u32;
            let mut last_error = None::<String>;
            let mut saved_time = None::<String>;
            let mut save_error = None::<String>;
            let mut ended_reason = "expired";

            while Instant::now() < scan_deadline {
                let state = app.state::<BackendState>();
                attempts = attempts.saturating_add(1);
                match today_win_bonus_capture.capture_focused_sc2_window_detection() {
                    Ok(Some(detection)) if detection.found_today_win_bonus() => {
                        captured = captured.saturating_add(1);
                        let persist_result = TauriOverlayOps::persist_today_win_bonus_detected_time(
                            &state,
                            replay_context,
                            Some(&replay_file),
                        );
                        (saved_time, save_error) = persist_result.into_parts();
                        ended_reason = "detected";
                        break;
                    }
                    Ok(Some(_detection)) => {
                        captured = captured.saturating_add(1);
                        not_detected = not_detected.saturating_add(1);
                    }
                    Ok(None) => {
                        skipped_not_focused = skipped_not_focused.saturating_add(1);
                    }
                    Err(error) => {
                        errors = errors.saturating_add(1);
                        last_error = Some(error);
                    }
                }

                thread::sleep(Duration::from_secs(1));
            }

            let capture_fallback_state = today_win_bonus_capture.fallback_state();
            crate::sco_info!(
                "[SCO/today-win-bonus] scan summary replay='{}' replay_file_modified_time_seconds='{}' reason={} attempts={} captured={} not_detected={} skipped_not_focused={} errors={} window_capture_failures={} fallback_happened={} selected_fallback_method='{}' active_capture_method='{}' elapsed_ms={} saved_time='{}' save_error='{}' last_error='{}'",
                replay_file,
                replay_file_modified_time_seconds
                    .map(|seconds| seconds.to_string())
                    .unwrap_or_default(),
                ended_reason,
                attempts,
                captured,
                not_detected,
                skipped_not_focused,
                errors,
                capture_fallback_state.consecutive_window_capture_failures(),
                capture_fallback_state.region_capture_fallback(),
                today_win_bonus_capture.selected_fallback_method(),
                today_win_bonus_capture.active_capture_method(),
                scan_started_at.elapsed().as_millis(),
                saved_time.as_deref().unwrap_or(""),
                save_error.as_deref().unwrap_or(""),
                last_error.as_deref().unwrap_or("")
            );
        });
    }

    pub fn sc2_game_ended_display_duration(settings: &AppSettings) -> Duration {
        Duration::from_secs(u64::from(settings.duration().max(1)))
    }

    fn persist_today_win_bonus_detected_time(
        state: &BackendState,
        replay_context: Option<FirstWinBonusReplayContext>,
        replay_file: Option<&str>,
    ) -> TodayWinBonusPersistResult {
        let replay_context = replay_context.or_else(|| {
            TauriOverlayOps::cached_replay_context_for_today_win_bonus(replay_file)
                .map_err(|error| {
                    crate::sco_warn!(
                        "[SCO/today-win-bonus] failed to resolve replay server and time: {error}"
                    );
                    error
                })
                .ok()
                .flatten()
        });
        let Some(replay_context) = replay_context else {
            return TodayWinBonusPersistResult::failed(
                "No replay server and time were available for today's win bonus".to_string(),
            );
        };
        let Some(saved_time) =
            today_win_bonus::FirstWinBonusAcquiredTime::from_replay_file_modified_time_seconds(
                replay_context.modified_time_seconds(),
            )
            .and_then(|time| time.to_rfc3339())
        else {
            return TodayWinBonusPersistResult::failed(
                "The replay time for today's win bonus was invalid".to_string(),
            );
        };

        match TauriOverlayOps::persist_first_win_bonus_time(
            state,
            replay_context.server(),
            &saved_time,
        ) {
            Ok(()) => TodayWinBonusPersistResult::saved(saved_time),
            Err(error) => TodayWinBonusPersistResult::saved(saved_time).with_error(error),
        }
    }

    fn cached_replay_context_for_today_win_bonus(
        replay_file: Option<&str>,
    ) -> Result<Option<FirstWinBonusReplayContext>, String> {
        let cache_path = PathManagerOps::get_cache_path();
        let database = ReplayCacheDatabase::open_for_cache_path(&cache_path)
            .map_err(|error| error.to_string())?;
        let entry = match replay_file {
            Some(file) => database.load_entry_by_file(file),
            None => database.load_latest_entry(),
        }
        .map_err(|error| error.to_string())?;
        let Some(entry) = entry else {
            return Ok(None);
        };
        let Some(server) = Sc2Server::from_region_code(&entry.region) else {
            return Ok(None);
        };
        let modified_time_seconds =
            today_win_bonus::FirstWinBonusAcquiredTime::from_replay_file_modified_time(Path::new(
                &entry.file,
            ))
            .ok()
            .flatten()
            .map(|time| time.replay_file_modified_time_seconds())
            .or_else(|| ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date));

        Ok(modified_time_seconds
            .and_then(|seconds| FirstWinBonusReplayContext::new(server, seconds)))
    }

    pub fn persist_first_win_bonus_time(
        state: &BackendState,
        server: Sc2Server,
        saved_time: &str,
    ) -> Result<(), String> {
        let mut saved_settings = AppSettings::from_saved_file();
        saved_settings.set_first_win_bonus_time(server, saved_time.to_string());
        saved_settings.write_saved_settings_file()?;

        let mut active_settings = state.read_settings_memory();
        active_settings.set_first_win_bonus_time(server, saved_time.to_string());
        state.replace_active_settings(&active_settings);
        Ok(())
    }

    fn latest_replay_server_for_legacy_migration(
        state: &BackendState,
        settings: &AppSettings,
    ) -> Result<Option<Sc2Server>, String> {
        if let Some(root) = settings.resolve_replay_root()
            && let Some(path) = ReplayAnalysis::collect_replay_paths(&root, 1)
                .into_iter()
                .next()
            && let Ok(resources) = state.replay_analysis_resources()
            && let Some(entry) = CacheReplayEntry::parse_basic_with_resources(&path, &resources)
            && let Some(server) = Sc2Server::from_region_code(&entry.region)
        {
            return Ok(Some(server));
        }

        let database = ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
            .map_err(|error| error.to_string())?;
        database
            .load_latest_entry()
            .map_err(|error| error.to_string())
            .map(|entry| entry.and_then(|entry| Sc2Server::from_region_code(&entry.region)))
    }

    fn migrate_legacy_first_win_bonus_time(state: &BackendState) -> Result<bool, String> {
        let mut settings = AppSettings::from_saved_file();
        if settings.latest_today_win_bonus_time().is_none() {
            return Ok(false);
        }
        let Some(server) =
            TauriOverlayOps::latest_replay_server_for_legacy_migration(state, &settings)?
        else {
            return Ok(false);
        };
        if !settings.migrate_legacy_first_win_bonus_time(server) {
            return Ok(false);
        }
        state.write_settings_file(&settings)?;
        crate::sco_info!(
            "[SCO/today-win-bonus] migrated legacy first win bonus timer to server={server:?}"
        );
        Ok(true)
    }

    pub fn log_sc2_game_state_transition(transition: Option<Sc2GameStateTransition>, reason: &str) {
        if let Some(transition) = transition {
            crate::sco_info!(
                "[SCO/game-state] {:?} -> {:?} reason={}",
                transition.previous(),
                transition.current(),
                reason
            );
        }
    }

    fn try_today_win_bonus_focus_scan(app: &tauri::AppHandle<Wry>, sc2_game_state: Sc2GameState) {
        let scan_started_at = Instant::now();
        let state = app.state::<BackendState>();
        let mut today_win_bonus_capture = today_win_bonus::TodayWinBonusWindowCapture::new();
        crate::sco_info!(
            "[SCO/today-win-bonus] focus scan started sc2_game_state={:?} initial_capture_method='{}'",
            sc2_game_state,
            today_win_bonus::TodayWinBonusWindowCapture::initial_capture_method()
        );

        let mut saved_time = None::<String>;
        let mut save_error = None::<String>;
        let mut last_error = None::<String>;
        let mut result = "not_detected";

        match today_win_bonus_capture.capture_focused_sc2_window_detection() {
            Ok(Some(detection)) if detection.found_today_win_bonus() => {
                let persist_result =
                    TauriOverlayOps::persist_today_win_bonus_detected_time(&state, None, None);
                (saved_time, save_error) = persist_result.into_parts();
                result = if saved_time.is_some() {
                    "detected"
                } else {
                    "detected_without_replay_time"
                };
            }
            Ok(Some(_detection)) => {}
            Ok(None) => {
                result = "skipped_not_focused";
            }
            Err(error) => {
                last_error = Some(error);
                result = "error";
            }
        }

        let capture_fallback_state = today_win_bonus_capture.fallback_state();
        crate::sco_info!(
            "[SCO/today-win-bonus] focus scan summary sc2_game_state={:?} result={} window_capture_failures={} fallback_happened={} selected_fallback_method='{}' active_capture_method='{}' elapsed_ms={} saved_time='{}' save_error='{}' last_error='{}'",
            sc2_game_state,
            result,
            capture_fallback_state.consecutive_window_capture_failures(),
            capture_fallback_state.region_capture_fallback(),
            today_win_bonus_capture.selected_fallback_method(),
            today_win_bonus_capture.active_capture_method(),
            scan_started_at.elapsed().as_millis(),
            saved_time.as_deref().unwrap_or(""),
            save_error.as_deref().unwrap_or(""),
            last_error.as_deref().unwrap_or("")
        );
    }

    fn first_win_bonus_timer_payload(
        settings: &AppSettings,
        visible: bool,
    ) -> FirstWinBonusTimerPayload {
        today_win_bonus::FirstWinBonusTimerStatus::payload_for_settings(
            settings,
            chrono::Utc::now(),
            visible,
        )
    }

    fn first_win_bonus_timer_should_be_visible(
        settings: &AppSettings,
        sc2_focused: bool,
        sc2_game_state: Sc2GameState,
    ) -> bool {
        if !sc2_focused {
            return false;
        }
        if matches!(
            sc2_game_state,
            Sc2GameState::GameStarting | Sc2GameState::GamePlaying
        ) {
            return false;
        }

        match settings.first_win_bonus_display_mode() {
            FirstWinBonusDisplayMode::Hidden => false,
            FirstWinBonusDisplayMode::AvailableOnly => {
                today_win_bonus::FirstWinBonusTimerStatus::any_selected_server_available(
                    settings,
                    chrono::Utc::now(),
                )
            }
            FirstWinBonusDisplayMode::Always => true,
        }
    }

    fn spawn_first_win_bonus_focus_listener(
        app: tauri::AppHandle<Wry>,
        focus_sender: mpsc::Sender<bool>,
        initial_sc2_focused: bool,
    ) -> Option<ActiveWindowListener> {
        let previous_sc2_focused = Arc::new(AtomicBool::new(initial_sc2_focused));
        match ActiveWindowDetector::spawn_focus_listener(move |sc2_focused| {
            let previous = previous_sc2_focused.swap(sc2_focused, Ordering::AcqRel);
            if previous == sc2_focused {
                return;
            }

            let _ = focus_sender.send(sc2_focused);
            if sc2_focused {
                TauriOverlayOps::spawn_today_win_bonus_focus_activation(app.clone());
            }
        }) {
            Ok(listener) => Some(listener),
            Err(error) => {
                crate::sco_warn!("[SCO/today-win-bonus] active window listener failed: {error}");
                None
            }
        }
    }

    fn spawn_today_win_bonus_focus_activation(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            let sc2_game_state = app.state::<BackendState>().sc2_game_state();
            TauriOverlayOps::try_today_win_bonus_focus_scan(&app, sc2_game_state);
            let sc2_focused = ActiveWindowDetector::focused_window_is_sc2().unwrap_or(false);
            TauriOverlayOps::sync_first_win_bonus_timer_overlay(&app, sc2_focused, !sc2_focused);
        });
    }

    fn sync_first_win_bonus_timer_overlay(
        app: &tauri::AppHandle<Wry>,
        sc2_focused: bool,
        delay_hide: bool,
    ) {
        let state = app.state::<BackendState>();
        let timer_visible = state.first_win_bonus_timer_visible();
        let settings = state.read_settings_memory();
        let sc2_game_state = state.sc2_game_state();
        let visible = TauriOverlayOps::first_win_bonus_timer_should_be_visible(
            &settings,
            sc2_focused,
            sc2_game_state,
        );

        if visible {
            let payload = TauriOverlayOps::first_win_bonus_timer_payload(&settings, true);
            overlay_info::OverlayInfoOps::emit_first_win_bonus_timer(app, payload);
        } else if timer_visible {
            if delay_hide {
                state.start_first_win_bonus_timer_hide_delay_if_needed(
                    today_win_bonus::FIRST_WIN_BONUS_TIMER_POLL_INTERVAL,
                );
                if !state.first_win_bonus_timer_hide_delay_elapsed() {
                    return;
                }
            }

            let payload = TauriOverlayOps::first_win_bonus_timer_payload(&settings, false);
            overlay_info::OverlayInfoOps::emit_first_win_bonus_timer(app, payload);
        }
    }

    fn wait_first_win_bonus_focus_event(
        focus_receiver: &Receiver<bool>,
        current_sc2_focused: bool,
        timeout: Duration,
    ) -> bool {
        let mut sc2_focused = match focus_receiver.recv_timeout(timeout) {
            Ok(next_sc2_focused) => next_sc2_focused,
            Err(RecvTimeoutError::Timeout) => return current_sc2_focused,
            Err(RecvTimeoutError::Disconnected) => return current_sc2_focused,
        };

        loop {
            match focus_receiver.try_recv() {
                Ok(next_sc2_focused) => sc2_focused = next_sc2_focused,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return sc2_focused,
            }
        }
    }

    pub fn spawn_first_win_bonus_timer_task(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            if let Err(error) =
                TauriOverlayOps::migrate_legacy_first_win_bonus_time(&app.state::<BackendState>())
            {
                crate::sco_warn!("[SCO/today-win-bonus] legacy timer migration failed: {error}");
            }
            thread::sleep(Duration::from_secs(4));

            let mut sc2_focused = ActiveWindowDetector::focused_window_is_sc2().unwrap_or(false);
            let (focus_sender, focus_receiver) = mpsc::channel();
            let focus_listener = TauriOverlayOps::spawn_first_win_bonus_focus_listener(
                app.clone(),
                focus_sender,
                sc2_focused,
            );
            let use_focus_poll_fallback = focus_listener.is_none();
            let _focus_listener = focus_listener;
            loop {
                sc2_focused = TauriOverlayOps::wait_first_win_bonus_focus_event(
                    &focus_receiver,
                    sc2_focused,
                    today_win_bonus::FIRST_WIN_BONUS_TIMER_POLL_INTERVAL,
                );
                if use_focus_poll_fallback {
                    sc2_focused =
                        ActiveWindowDetector::focused_window_is_sc2().unwrap_or(sc2_focused);
                }

                let state = app.state::<BackendState>();
                let settings = state.read_settings_memory();
                let game_ended_duration =
                    TauriOverlayOps::sc2_game_ended_display_duration(&settings);
                TauriOverlayOps::log_sc2_game_state_transition(
                    state.advance_sc2_game_state_timers(
                        Instant::now(),
                        crate::SC2_GAME_STARTING_DISPLAY_DURATION,
                        game_ended_duration,
                    ),
                    "timer_tick",
                );
                TauriOverlayOps::sync_first_win_bonus_timer_overlay(&app, sc2_focused, true);
            }
        });
    }
}
