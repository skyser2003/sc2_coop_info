use super::BackendState;
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
};

use s2coop_analyzer::detailed_replay_analysis::GenerateCacheStopController;

use crate::{TauriOverlayOps, replay_analysis::ReplayAnalysis};

impl BackendState {
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
}
