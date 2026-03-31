use sco_tauri_overlay::AppSettings;
use serde_json::json;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_settings_path() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("sco-overlay-settings-{unique}"))
        .join("settings.json")
}

#[test]
fn merge_settings_with_defaults_uses_requested_overlay_defaults() {
    let merged = AppSettings::merge_settings_with_defaults(json!({}));

    assert!(!merged.start_with_windows);
    assert!(merged.minimize_to_tray);
    assert!(!merged.start_minimized);
    assert_eq!(merged.duration, 30);
    assert!(merged.show_player_winrates);
    assert!(merged.show_replay_info_after_game);
    assert!(merged.show_session);
    assert!(merged.show_charts);
    assert_eq!(merged.hotkey_show_hide, Some("Ctrl+Shift+8".to_string()));
    assert_eq!(merged.hotkey_show, None);
    assert_eq!(merged.hotkey_hide, None);
    assert_eq!(merged.hotkey_newer, Some("Ctrl+Alt+/".to_string()));
    assert_eq!(merged.hotkey_older, Some("Ctrl+Alt+8".to_string()));
    assert_eq!(merged.hotkey_winrates, Some("Ctrl+Alt+-".to_string()));
    assert_eq!(merged.performance_hotkey, None);
}

#[test]
fn merge_settings_with_defaults_preserves_existing_values() {
    let merged = AppSettings::merge_settings_with_defaults(json!({
        "duration": 45,
        "show_session": false,
        "show_charts": false,
    }));

    assert_eq!(merged.duration, 45);
    assert!(!merged.show_session);
    assert!(!merged.show_charts);
    assert!(merged.show_replay_info_after_game);
}

#[test]
fn read_saved_settings_file_from_path_creates_defaults_when_missing() {
    let settings_path = unique_temp_settings_path();
    let parent = settings_path
        .parent()
        .expect("settings path should have a parent")
        .to_path_buf();

    let settings = AppSettings::read_saved_settings_file_from_path(&settings_path, true);
    let written = std::fs::read_to_string(&settings_path)
        .expect("settings file should be created when missing");
    let parsed: Value =
        serde_json::from_str(&written).expect("created settings file should contain valid json");

    let mut expected = AppSettings::default();
    let mut actual_settings = settings;
    let mut parsed_settings = AppSettings::merge_settings_with_defaults(parsed);
    actual_settings.present_keys.clear();
    parsed_settings.present_keys.clear();
    expected.present_keys.clear();

    assert_eq!(actual_settings, expected);
    assert_eq!(parsed_settings, expected);

    let _ = std::fs::remove_file(&settings_path);
    let _ = std::fs::remove_dir(&parent);
}

#[test]
fn merge_settings_with_defaults_initializes_null_overlay_hotkeys_to_defaults() {
    let merged = AppSettings::merge_settings_with_defaults(json!({
        "hotkey_show/hide": null,
        "hotkey_newer": null,
        "hotkey_older": null,
        "hotkey_winrates": null,
    }));

    assert_eq!(merged.hotkey_show_hide, Some("Ctrl+Shift+8".to_string()));
    assert_eq!(merged.hotkey_newer, Some("Ctrl+Alt+/".to_string()));
    assert_eq!(merged.hotkey_older, Some("Ctrl+Alt+8".to_string()));
    assert_eq!(merged.hotkey_winrates, Some("Ctrl+Alt+-".to_string()));
}
