use sco_tauri_overlay::{
    merge_settings_with_defaults, overlay_info, read_settings_file, replace_active_settings,
    window_close_action, WindowCloseAction,
};
use serde_json::json;

#[test]
fn performance_close_hides_the_window() {
    assert_eq!(
        window_close_action("performance", false, false),
        WindowCloseAction::HidePerformance
    );
}

#[test]
fn overlay_close_hides_the_window() {
    assert_eq!(
        window_close_action("overlay", false, false),
        WindowCloseAction::HideWindow
    );
}

#[test]
fn config_close_hides_when_minimize_to_tray_is_enabled() {
    assert_eq!(
        window_close_action("config", true, false),
        WindowCloseAction::HideWindow
    );
}

#[test]
fn config_close_exits_when_minimize_to_tray_is_disabled() {
    assert_eq!(
        window_close_action("config", false, false),
        WindowCloseAction::ExitApp
    );
}

#[test]
fn shutdown_path_allows_windows_to_close() {
    for label in ["config", "overlay", "performance"] {
        assert_eq!(
            window_close_action(label, true, true),
            WindowCloseAction::AllowClose
        );
    }
}

#[test]
fn runtime_flags_follow_active_settings_before_save() {
    let previous_settings = read_settings_file();

    replace_active_settings(&merge_settings_with_defaults(json!({
        "start_minimized": true,
        "minimize_to_tray": false,
    })));
    let disabled_flags = overlay_info::parse_runtime_flags();
    assert!(!disabled_flags.start_minimized);
    assert!(!disabled_flags.minimize_to_tray);

    replace_active_settings(&merge_settings_with_defaults(json!({
        "start_minimized": false,
        "minimize_to_tray": true,
    })));
    let enabled_flags = overlay_info::parse_runtime_flags();
    assert!(!enabled_flags.start_minimized);
    assert!(enabled_flags.minimize_to_tray);

    replace_active_settings(&previous_settings);
}
