use super::*;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_settings_path() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("sco-overlay-settings-{unique}"))
        .join("Settings.json")
}

#[test]
fn merge_settings_with_defaults_uses_requested_overlay_defaults() {
    let merged = merge_settings_with_defaults(json!({}));

    assert_eq!(merged["start_with_windows"], json!(false));
    assert_eq!(merged["minimize_to_tray"], json!(true));
    assert_eq!(merged["start_minimized"], json!(false));
    assert_eq!(merged["duration"], json!(30));
    assert_eq!(merged["show_player_winrates"], json!(true));
    assert_eq!(merged["show_replay_info_after_game"], json!(true));
    assert_eq!(merged["show_session"], json!(true));
    assert_eq!(merged["show_charts"], json!(true));
}

#[test]
fn merge_settings_with_defaults_preserves_existing_values() {
    let merged = merge_settings_with_defaults(json!({
        "duration": 45,
        "show_session": false,
        "show_charts": false,
        "custom_setting": "keep",
    }));

    assert_eq!(merged["duration"], json!(45));
    assert_eq!(merged["show_session"], json!(false));
    assert_eq!(merged["show_charts"], json!(false));
    assert_eq!(merged["custom_setting"], json!("keep"));
    assert_eq!(merged["show_replay_info_after_game"], json!(true));
}

#[test]
fn read_saved_settings_file_from_path_creates_defaults_when_missing() {
    let settings_path = unique_temp_settings_path();
    let parent = settings_path
        .parent()
        .expect("settings path should have a parent")
        .to_path_buf();

    let settings = read_saved_settings_file_from_path(&settings_path, true);
    let written = std::fs::read_to_string(&settings_path)
        .expect("settings file should be created when missing");
    let parsed: Value =
        serde_json::from_str(&written).expect("created settings file should contain valid json");

    assert_eq!(settings, default_settings_value());
    assert_eq!(parsed, default_settings_value());

    let _ = std::fs::remove_file(&settings_path);
    let _ = std::fs::remove_dir(&parent);
}
