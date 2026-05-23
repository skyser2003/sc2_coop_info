use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{Manager, Wry};

use crate::{
    BackendState, GameLaunchDetector, GameLaunchStatus, Sc2GameState, TauriOverlayOps,
    live_game::LiveGameOps, overlay_info,
};

struct PendingPlayerStatsPopup {
    handle: String,
    name: String,
}

impl PendingPlayerStatsPopup {
    fn new(handle: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            handle: handle.into(),
            name: name.into(),
        }
    }

    fn handle(&self) -> &str {
        &self.handle
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl TauriOverlayOps {
    fn live_game_payload_is_coop_game(payload: &Value) -> bool {
        if payload
            .get("isReplay")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            return false;
        }

        if LiveGameOps::extract_live_game_players(payload) <= 2 {
            return false;
        }

        !LiveGameOps::all_players_are_users(payload)
    }

    fn pending_player_stats_popup_from_payload(
        state: &BackendState,
        payload: &Value,
    ) -> Option<PendingPlayerStatsPopup> {
        let (main_names, main_handles) = state.build_launch_main_identity();
        LiveGameOps::choose_other_coop_player_stats(payload, &main_names, &main_handles)
            .map(|(handle, name)| PendingPlayerStatsPopup::new(handle, name))
    }

    fn try_show_pending_player_stats_popup(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        launch_detector: &mut GameLaunchDetector,
        replay_count: usize,
        now: Instant,
        pending_popup: Option<&PendingPlayerStatsPopup>,
    ) -> bool {
        let Some(pending_popup) = pending_popup else {
            return false;
        };
        if !launch_detector.should_attempt_popup(state.stats_have_player_rows(), replay_count) {
            return false;
        }
        if !launch_detector.replay_change_settled(now) {
            return false;
        }

        let invalidation_generation = state.invalidate_delayed_player_stats_popup_generation();
        crate::sco_log!(
            "[SCO/launch] invalidated delayed player stats popups generation={}",
            invalidation_generation
        );

        if overlay_info::OverlayInfoOps::show_player_stats_for_name(
            app,
            state,
            pending_popup.handle(),
            pending_popup.name(),
        ) {
            launch_detector.record_popup_shown(replay_count);
            return true;
        }

        false
    }

    pub fn spawn_game_launch_player_stats_task(app: tauri::AppHandle<Wry>) {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(4));

            let mut launch_detector = GameLaunchDetector::new(Instant::now());
            let mut pending_player_stats_popup = None::<PendingPlayerStatsPopup>;

            loop {
                thread::sleep(Duration::from_millis(500));

                let state = app.state::<BackendState>();
                let settings = state.read_settings_memory();
                let replay_count = state.replay_count_for_launch_detector();
                let now = Instant::now();
                let game_ended_duration =
                    TauriOverlayOps::sc2_game_ended_display_duration(&settings);
                TauriOverlayOps::log_sc2_game_state_transition(
                    state.advance_sc2_game_state_timers(
                        now,
                        crate::SC2_GAME_STARTING_DISPLAY_DURATION,
                        game_ended_duration,
                    ),
                    "launch_tick",
                );
                launch_detector.observe_replay_count(replay_count, now);

                match state.sc2_game_state() {
                    Sc2GameState::Lobby => {
                        pending_player_stats_popup = None;
                        let Some(payload) = LiveGameOps::fetch_sc2_live_game_payload() else {
                            launch_detector.observe_non_live_state();
                            continue;
                        };
                        if !TauriOverlayOps::live_game_payload_is_coop_game(&payload) {
                            launch_detector.observe_non_live_state();
                            continue;
                        }

                        let display_time =
                            LiveGameOps::value_as_u64_lossy(payload.get("displayTime"))
                                .unwrap_or(0);
                        match launch_detector.update_display_time_status(display_time) {
                            GameLaunchStatus::Started => {
                                TauriOverlayOps::log_sc2_game_state_transition(
                                    state
                                        .transition_sc2_game_state(Sc2GameState::GameStarting, now),
                                    "live_game_started",
                                );
                                if settings.show_player_winrates() {
                                    pending_player_stats_popup =
                                        TauriOverlayOps::pending_player_stats_popup_from_payload(
                                            &state, &payload,
                                        );
                                    if TauriOverlayOps::try_show_pending_player_stats_popup(
                                        &app,
                                        &state,
                                        &mut launch_detector,
                                        replay_count,
                                        now,
                                        pending_player_stats_popup.as_ref(),
                                    ) {
                                        pending_player_stats_popup = None;
                                    }
                                }
                            }
                            GameLaunchStatus::Unknown
                            | GameLaunchStatus::Idle
                            | GameLaunchStatus::Running
                            | GameLaunchStatus::Ended => {}
                        }
                    }
                    Sc2GameState::GameStarting => {
                        if !settings.show_player_winrates() {
                            pending_player_stats_popup = None;
                            continue;
                        }
                        if TauriOverlayOps::try_show_pending_player_stats_popup(
                            &app,
                            &state,
                            &mut launch_detector,
                            replay_count,
                            now,
                            pending_player_stats_popup.as_ref(),
                        ) {
                            pending_player_stats_popup = None;
                        }
                    }
                    Sc2GameState::GamePlaying => {
                        pending_player_stats_popup = None;
                    }
                    Sc2GameState::GameEnded => {
                        pending_player_stats_popup = None;
                        launch_detector.observe_non_live_state();
                    }
                }
            }
        });
    }
}
