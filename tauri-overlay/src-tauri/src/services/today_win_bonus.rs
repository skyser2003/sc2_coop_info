use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{Manager, Wry};

use crate::{
    AppSettings, BackendState, FirstWinBonusDisplayMode, FirstWinBonusTimerPayload, Sc2GameState,
    Sc2GameStateTransition, TauriOverlayOps, overlay_info, today_win_bonus,
};

impl TauriOverlayOps {
    pub fn spawn_today_win_bonus_scan(app: tauri::AppHandle<Wry>, replay_file: String) {
        thread::spawn(move || {
            const SCAN_DURATION: Duration = Duration::from_secs(45);

            let scan_started_at = Instant::now();
            let scan_deadline = scan_started_at + SCAN_DURATION;
            let mut today_win_bonus_capture = today_win_bonus::TodayWinBonusWindowCapture::new();
            crate::sco_log!(
                "[SCO/today-win-bonus] scan started replay='{}' duration_secs={} interval_secs=1 initial_capture_method='{}' fallback_after_failures={}",
                replay_file,
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
            let mut detected_at = None::<String>;
            let mut save_error = None::<String>;
            let mut ended_reason = "expired";

            while Instant::now() < scan_deadline {
                let state = app.state::<BackendState>();
                attempts = attempts.saturating_add(1);
                match today_win_bonus_capture.capture_focused_sc2_window_detection() {
                    Ok(Some(detection)) if detection.found_today_win_bonus() => {
                        captured = captured.saturating_add(1);
                        let detected_time =
                            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                        save_error = TauriOverlayOps::persist_today_win_bonus_detected_time(
                            &state,
                            detected_time.clone(),
                        );
                        detected_at = Some(detected_time);
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
            crate::sco_log!(
                "[SCO/today-win-bonus] scan summary replay='{}' reason={} attempts={} captured={} not_detected={} skipped_not_focused={} errors={} window_capture_failures={} fallback_happened={} selected_fallback_method='{}' active_capture_method='{}' elapsed_ms={} detected_at='{}' save_error='{}' last_error='{}'",
                replay_file,
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
                detected_at.as_deref().unwrap_or(""),
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
        detected_time: String,
    ) -> Option<String> {
        match state.persist_single_setting_value(
            today_win_bonus::TODAY_WIN_BONUS_SETTINGS_KEY,
            Value::String(detected_time),
        ) {
            Ok(()) => None,
            Err(error) => Some(error.to_string()),
        }
    }

    pub fn log_sc2_game_state_transition(transition: Option<Sc2GameStateTransition>, reason: &str) {
        if let Some(transition) = transition {
            crate::sco_log!(
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
        crate::sco_log!(
            "[SCO/today-win-bonus] focus scan started sc2_game_state={:?} initial_capture_method='{}'",
            sc2_game_state,
            today_win_bonus::TodayWinBonusWindowCapture::initial_capture_method()
        );

        let mut detected_at = None::<String>;
        let mut save_error = None::<String>;
        let mut last_error = None::<String>;
        let mut result = "not_detected";

        match today_win_bonus_capture.capture_focused_sc2_window_detection() {
            Ok(Some(detection)) if detection.found_today_win_bonus() => {
                let detected_time =
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                save_error = TauriOverlayOps::persist_today_win_bonus_detected_time(
                    &state,
                    detected_time.clone(),
                );
                detected_at = Some(detected_time);
                result = "detected";
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
        crate::sco_log!(
            "[SCO/today-win-bonus] focus scan summary sc2_game_state={:?} result={} window_capture_failures={} fallback_happened={} selected_fallback_method='{}' active_capture_method='{}' elapsed_ms={} detected_at='{}' save_error='{}' last_error='{}'",
            sc2_game_state,
            result,
            capture_fallback_state.consecutive_window_capture_failures(),
            capture_fallback_state.region_capture_fallback(),
            today_win_bonus_capture.selected_fallback_method(),
            today_win_bonus_capture.active_capture_method(),
            scan_started_at.elapsed().as_millis(),
            detected_at.as_deref().unwrap_or(""),
            save_error.as_deref().unwrap_or(""),
            last_error.as_deref().unwrap_or("")
        );
    }

    fn spawn_today_win_bonus_focus_scan(app: tauri::AppHandle<Wry>, sc2_game_state: Sc2GameState) {
        thread::spawn(move || {
            TauriOverlayOps::try_today_win_bonus_focus_scan(&app, sc2_game_state);
        });
    }

    fn first_win_bonus_timer_payload(
        settings: &AppSettings,
        visible: bool,
    ) -> FirstWinBonusTimerPayload {
        today_win_bonus::FirstWinBonusTimerStatus::from_latest_acquired_time(
            settings.latest_today_win_bonus_time(),
            chrono::Utc::now(),
        )
        .into_payload(visible)
    }

    fn first_win_bonus_timer_should_be_visible(
        settings: &AppSettings,
        sc2_focused: bool,
        sc2_game_state: Sc2GameState,
    ) -> bool {
        if !sc2_focused {
            return false;
        }
        if sc2_game_state != Sc2GameState::Lobby {
            return false;
        }

        let status = today_win_bonus::FirstWinBonusTimerStatus::from_latest_acquired_time(
            settings.latest_today_win_bonus_time(),
            chrono::Utc::now(),
        );
        match settings.first_win_bonus_display_mode() {
            FirstWinBonusDisplayMode::Hidden => false,
            FirstWinBonusDisplayMode::AvailableOnly => status.available(),
            FirstWinBonusDisplayMode::Always => true,
        }
    }

    pub fn spawn_first_win_bonus_timer_task(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(4));

            let mut previous_sc2_focused = None::<bool>;
            loop {
                thread::sleep(today_win_bonus::FIRST_WIN_BONUS_TIMER_POLL_INTERVAL);

                let state = app.state::<BackendState>();
                let timer_visible = state.first_win_bonus_timer_visible();
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
                let sc2_game_state = state.sc2_game_state();
                let sc2_focused =
                    today_win_bonus::TodayWinBonusDetector::focused_sc2_window_active()
                        .unwrap_or(false);
                let sc2_focus_regained = previous_sc2_focused == Some(false) && sc2_focused;
                previous_sc2_focused = Some(sc2_focused);
                let visible = TauriOverlayOps::first_win_bonus_timer_should_be_visible(
                    &settings,
                    sc2_focused,
                    sc2_game_state,
                );

                if visible {
                    let payload = TauriOverlayOps::first_win_bonus_timer_payload(&settings, true);
                    overlay_info::OverlayInfoOps::emit_first_win_bonus_timer(&app, payload);
                } else if timer_visible {
                    state.start_first_win_bonus_timer_hide_delay_if_needed(
                        today_win_bonus::FIRST_WIN_BONUS_TIMER_POLL_INTERVAL,
                    );
                    if state.first_win_bonus_timer_hide_delay_elapsed() {
                        let payload =
                            TauriOverlayOps::first_win_bonus_timer_payload(&settings, false);
                        overlay_info::OverlayInfoOps::emit_first_win_bonus_timer(&app, payload);
                    }
                }

                if sc2_focus_regained
                    && matches!(
                        sc2_game_state,
                        Sc2GameState::Lobby | Sc2GameState::GameEnded
                    )
                {
                    TauriOverlayOps::spawn_today_win_bonus_focus_scan(app.clone(), sc2_game_state);
                }
            }
        });
    }
}
