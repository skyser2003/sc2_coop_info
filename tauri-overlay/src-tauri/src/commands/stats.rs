use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::TryLockError;
use std::time::{Instant, SystemTime};
use tauri::{State, Wry};

use crate::{
    AnalysisMode, BackendState, OverlayActionResult, ReplayAnalysis, StartupAnalysisTrigger,
    StatsActionPayload, StatsResponseBuildInput, StatsState, StatsStatePayload, TauriOverlayOps,
    UNLIMITED_REPLAY_LIMIT, overlay_info,
};

pub struct StatsCommands;

#[tauri::command]
pub async fn config_stats_get(
    app: tauri::AppHandle<Wry>,
    query: Option<String>,
    state: State<'_, BackendState>,
) -> Result<StatsStatePayload, String> {
    StatsCommands::config_stats_get(app, query, state).await
}

#[tauri::command]
pub async fn config_stats_action(
    app: tauri::AppHandle<Wry>,
    action: String,
    payload: Option<Value>,
    state: State<'_, BackendState>,
) -> Result<StatsActionPayload, String> {
    StatsCommands::config_stats_action(app, action, payload, state).await
}

impl StatsCommands {
    pub async fn config_stats_get(
        _app: tauri::AppHandle<Wry>,
        query: Option<String>,
        state: State<'_, BackendState>,
    ) -> Result<StatsStatePayload, String> {
        let total_started_at = Instant::now();
        let path = if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
            format!("/config/stats?{query}")
        } else {
            "/config/stats".to_string()
        };
        state.log_request("get", &path, &None);
        let snapshot_started_at = Instant::now();
        let stats = state.stats_handle();
        let stats_current_replay_files = state.stats_current_replay_files_handle();
        let state_snapshot = (
            state.configured_main_names(),
            state.configured_main_handles(),
            state.replay_scan_progress().as_payload(),
            state.dictionary_data().ok(),
        );
        crate::sco_log!(
            "[SCO/stats/e2e/backend] stage=config_stats_get_snapshot path_len={} elapsed_ms={:.3}",
            path.len(),
            snapshot_started_at.elapsed().as_secs_f64() * 1000.0
        );

        let path_for_worker = path.clone();
        let worker_wait_started_at = Instant::now();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let worker_started_at = Instant::now();
            let (main_names, main_handles, scan_progress, dictionary) = state_snapshot;
            let payload_started_at = Instant::now();
            let payload = match dictionary.as_deref() {
                Some(dictionary) => ReplayAnalysis::build_stats_response_with_dictionary(
                    StatsResponseBuildInput::new(
                        &path_for_worker,
                        &stats,
                        &stats_current_replay_files,
                        scan_progress.clone(),
                        &main_names,
                        &main_handles,
                    ),
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
            crate::sco_log!(
                "[SCO/stats/e2e/backend] stage=config_stats_get_build_payload path_len={} elapsed_ms={:.3}",
                path_for_worker.len(),
                payload_started_at.elapsed().as_secs_f64() * 1000.0
            );
            let typed_started_at = Instant::now();
            serde_json::from_value(payload)
                .map_err(|error| format!("Invalid stats payload: {error}"))
                .inspect(|payload: &StatsStatePayload| {
                    crate::sco_log!(
                        "[SCO/stats/e2e/backend] stage=config_stats_get_typed_payload games={} elapsed_ms={:.3}",
                        payload.games,
                        typed_started_at.elapsed().as_secs_f64() * 1000.0
                    );
                    crate::sco_log!(
                        "[SCO/stats/e2e/backend] stage=config_stats_get_worker_total elapsed_ms={:.3}",
                        worker_started_at.elapsed().as_secs_f64() * 1000.0
                    );
                })
        })
        .await
        .map_err(|error| format!("Failed to read /config/stats: {error}"))?;
        crate::sco_log!(
            "[SCO/stats/e2e/backend] stage=config_stats_get_blocking_await elapsed_ms={:.3}",
            worker_wait_started_at.elapsed().as_secs_f64() * 1000.0
        );
        if let Ok(payload) = result.as_ref() {
            crate::sco_log!(
                "[SCO/stats/e2e/backend] stage=config_stats_get_total games={} elapsed_ms={:.3}",
                payload.games,
                total_started_at.elapsed().as_secs_f64() * 1000.0
            );
        }
        result
    }

    pub async fn config_stats_action(
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

        if let Some(response) = overlay_info::OverlayInfoOps::perform_overlay_action(
            &app,
            &state,
            action,
            body.as_ref(),
        ) {
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
                let stats_payload =
                    state.stats_handle().lock().ok().map(|stats| {
                        stats.as_payload_typed(state.replay_scan_progress().as_payload())
                    });
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
                    stats.set_message(
                        "Detailed analysis will stop after the current work finishes.",
                    );
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
                if let Some(payload) = body.as_ref()
                    && let Some(enabled) = payload.get("enabled").and_then(Value::as_bool)
                {
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
}
