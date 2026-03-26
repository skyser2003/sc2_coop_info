use sco_tauri_overlay::{start_with_windows_enabled, windows_startup_command_value};
use serde_json::json;
use std::path::Path;

#[test]
fn start_with_windows_defaults_to_disabled() {
    assert!(!start_with_windows_enabled(&json!({})));
    assert!(!start_with_windows_enabled(&json!({
        "start_with_windows": "yes",
    })));
}

#[test]
fn start_with_windows_reads_boolean_setting() {
    assert!(start_with_windows_enabled(&json!({
        "start_with_windows": true,
    })));
    assert!(!start_with_windows_enabled(&json!({
        "start_with_windows": false,
    })));
}

#[test]
fn windows_startup_command_value_quotes_executable_path() {
    let value = windows_startup_command_value(Path::new(r"fixtures\apps\SCO Overlay.exe"));

    assert_eq!(value, r#""fixtures\apps\SCO Overlay.exe""#);
}
