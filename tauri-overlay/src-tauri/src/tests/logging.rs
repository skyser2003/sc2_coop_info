use super::*;
use serde_json::json;
use std::path::Path;

#[test]
fn sanitize_settings_value_removes_deleted_overlay_settings() {
    let sanitized = sanitize_settings_value(json!({
        "enable_logging": true,
        "fast_expand": true,
        "force_hide_overlay": true,
        "show_session": true,
    }));

    assert_eq!(
        sanitized.get("enable_logging").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        sanitized.get("show_session").and_then(Value::as_bool),
        Some(true)
    );
    assert!(sanitized.get("fast_expand").is_none());
    assert!(sanitized.get("force_hide_overlay").is_none());
}

fn logs_file_path_from_settings_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("logs.txt")
}

#[test]
fn logging_enabled_from_settings_respects_boolean_flag() {
    assert!(logging_enabled_from_settings(&json!({
        "enable_logging": true,
    })));
    assert!(!logging_enabled_from_settings(&json!({
        "enable_logging": false,
    })));
    assert!(!logging_enabled_from_settings(&json!({})));
}

#[test]
fn logs_file_path_stays_next_to_settings_file() {
    let settings_path = crate::test_config_path("settings.json");
    let path = logs_file_path_from_settings_path(Path::new(&settings_path));

    assert_eq!(path, crate::test_config_path("logs.txt"));
}

#[test]
fn session_counter_delta_only_tracks_victory_and_defeat() {
    assert_eq!(session_counter_delta("Victory"), (1, 0));
    assert_eq!(session_counter_delta("defeat"), (0, 1));
    assert_eq!(session_counter_delta("Unknown"), (0, 0));
}
