use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent, Wry, menu::MenuEvent};

use crate::{
    BackendState, StartupAnalysisTrigger, TauriOverlayOps, overlay_info, performance_overlay,
    shared_types,
};

pub struct AppLifecycleService;

impl AppLifecycleService {
    fn request_startup_analysis(app: &tauri::App<Wry>) {
        let (stats, stats_current_replay_files, detailed_stop_controller_slot) = {
            let state = app.state::<BackendState>();
            (
                state.stats_handle(),
                state.stats_current_replay_files_handle(),
                state.detailed_analysis_stop_controller_slot(),
            )
        };
        if let Err(error) = TauriOverlayOps::request_startup_analysis(
            app.app_handle().clone(),
            stats,
            stats_current_replay_files,
            detailed_stop_controller_slot,
            StartupAnalysisTrigger::Setup,
        ) {
            crate::sco_warn!(
                "[SCO/stats] failed to request startup analysis during setup: {error}"
            );
        }
    }

    pub fn handle_menu_event(app: &AppHandle<Wry>, event: MenuEvent) {
        match event.id() {
            id if id == overlay_info::MENU_ITEM_SHOW_CONFIG => {
                overlay_info::OverlayInfoOps::show_config_window(app)
            }
            id if id == overlay_info::MENU_ITEM_SHOW_OVERLAY => {
                overlay_info::OverlayInfoOps::show_overlay_window(app);
                let _ = app.emit(
                    overlay_info::OVERLAY_SHOWSTATS_EVENT,
                    shared_types::EmptyPayload::default(),
                );
            }
            id if id == overlay_info::MENU_ITEM_QUIT => TauriOverlayOps::request_clean_exit(app, 0),
            _ => {}
        }
    }

    pub fn handle_window_event(window: &Window<Wry>, event: &WindowEvent) {
        match event {
            WindowEvent::CloseRequested { api, .. } => {
                let state = window.app_handle().state::<BackendState>();
                let flags = state.runtime_flags();
                match TauriOverlayOps::window_close_action(
                    window.label(),
                    flags.minimize_to_tray(),
                    state.exit_in_progress(),
                ) {
                    crate::WindowCloseAction::AllowClose => {}
                    crate::WindowCloseAction::HidePerformance => {
                        api.prevent_close();
                        performance_overlay::PerformanceOverlayOps::hide_window(
                            window.app_handle(),
                        );
                    }
                    crate::WindowCloseAction::HideWindow => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    crate::WindowCloseAction::ExitApp => {
                        api.prevent_close();
                        TauriOverlayOps::request_clean_exit(window.app_handle(), 0);
                    }
                }
            }
            WindowEvent::Moved(_) => {
                if window.label() == overlay_info::PERFORMANCE_WINDOW_LABEL
                    && let Some(performance_window) = window
                        .app_handle()
                        .get_webview_window(overlay_info::PERFORMANCE_WINDOW_LABEL)
                {
                    performance_overlay::PerformanceOverlayOps::persist_geometry(
                        &performance_window,
                    );
                }
            }
            WindowEvent::Resized(_) => {
                if window.label() == overlay_info::OVERLAY_WINDOW_LABEL
                    && let Some(overlay_window) = window
                        .app_handle()
                        .get_webview_window(overlay_info::OVERLAY_WINDOW_LABEL)
                    && let Err(error) =
                        overlay_info::OverlayInfoOps::stabilize_overlay_bounds(&overlay_window)
                {
                    crate::sco_warn!(
                        "[SCO/overlay] Failed to stabilize overlay bounds after resize: {error}"
                    );
                }
                if window.label() == overlay_info::PERFORMANCE_WINDOW_LABEL
                    && let Some(performance_window) = window
                        .app_handle()
                        .get_webview_window(overlay_info::PERFORMANCE_WINDOW_LABEL)
                {
                    performance_overlay::PerformanceOverlayOps::persist_geometry(
                        &performance_window,
                    );
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if window.label() == overlay_info::OVERLAY_WINDOW_LABEL
                    && let Some(overlay_window) = window
                        .app_handle()
                        .get_webview_window(overlay_info::OVERLAY_WINDOW_LABEL)
                    && let Err(error) =
                        overlay_info::OverlayInfoOps::stabilize_overlay_bounds(&overlay_window)
                {
                    crate::sco_warn!(
                        "[SCO/overlay] Failed to stabilize overlay bounds after scale change: {error}"
                    );
                }
            }
            _ => {}
        }
    }

    pub fn setup_app(app: &mut tauri::App<Wry>) -> Result<(), Box<dyn std::error::Error>> {
        TauriOverlayOps::spawn_protocol_store_warmup();
        TauriOverlayOps::spawn_replay_analysis_resource_warmup(app.handle().clone());

        let state = app.state::<BackendState>();
        let flags = state.runtime_flags();
        Self::request_startup_analysis(app);

        if flags.auto_update() {
            let handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                if let Err(error) = TauriOverlayOps::auto_update(handle).await {
                    crate::sco_warn!("Auto update failed: {}", error);
                }
            });
        }

        TauriOverlayOps::setup_startup_windows(app);

        TauriOverlayOps::setup_tray_icon(app);

        let startup_settings = state.read_settings_memory();
        if let Err(error) = TauriOverlayOps::sync_start_with_windows_registration(
            app.app_handle(),
            &startup_settings,
        ) {
            crate::sco_warn!("[SCO/settings] Failed to initialize start_with_windows: {error}");
        }

        overlay_info::OverlayInfoOps::sync_overlay_runtime_settings(app.app_handle());
        performance_overlay::PerformanceOverlayOps::apply_settings(app.app_handle());

        if let Err(error) = overlay_info::OverlayInfoOps::register_overlay_hotkeys(app.app_handle())
        {
            crate::sco_warn!("[SCO/hotkey] {error}");
        }

        TauriOverlayOps::spawn_replay_creation_watcher(app.app_handle().clone());
        TauriOverlayOps::spawn_game_launch_player_stats_task(app.app_handle().clone());
        TauriOverlayOps::spawn_first_win_bonus_timer_task(app.app_handle().clone());
        overlay_info::OverlayInfoOps::spawn_sc2_overlay_window_tracker(app.app_handle().clone());
        performance_overlay::PerformanceOverlayOps::spawn_monitor(app.app_handle().clone());

        Ok(())
    }
}
