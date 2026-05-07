use sco_tauri_overlay::{AppSettings, TauriOverlayOps};
use serde_json::json;
use std::path::Path;

#[test]
fn start_with_windows_setting_defaults_to_disabled() {
    assert!(!AppSettings::merge_settings_with_defaults(json!({})).start_with_windows());
    assert!(
        !AppSettings::merge_settings_with_defaults(json!({
            "start_with_windows": "yes",
        }))
        .start_with_windows()
    );
}

#[test]
fn start_with_windows_setting_reads_boolean_value() {
    assert!(
        AppSettings::merge_settings_with_defaults(json!({
            "start_with_windows": true,
        }))
        .start_with_windows()
    );
    assert!(
        !AppSettings::merge_settings_with_defaults(json!({
            "start_with_windows": false,
        }))
        .start_with_windows()
    );
}

#[test]
fn windows_startup_command_value_quotes_executable_path() {
    let value =
        TauriOverlayOps::windows_startup_command_value(Path::new(r"fixtures\apps\SCO Overlay.exe"));

    assert_eq!(value, r#""fixtures\apps\SCO Overlay.exe""#);
}
