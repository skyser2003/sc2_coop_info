use sco_tauri_overlay::{overlay_info, AppSettings};
use serde_json::json;
use serde_json::Value;

#[test]
fn overlay_runtime_settings_defaults_to_visible_charts() {
    let payload = overlay_info::overlay_runtime_settings_payload(
        &AppSettings::merge_settings_with_defaults(json!({})),
        0,
        0,
    );
    let colors = payload
        .get("colors")
        .and_then(Value::as_array)
        .expect("colors should always be present");

    assert_eq!(payload.get("duration").and_then(Value::as_u64), Some(30));
    assert_eq!(
        payload.get("show_charts").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload.get("show_session").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload.get("session_victory").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        payload.get("session_defeat").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(payload.get("language").and_then(Value::as_str), Some("en"));
    assert_eq!(colors.len(), 4);
    assert_eq!(colors.first().and_then(Value::as_str), Some("#0080F8"));
    assert_eq!(colors.get(1).and_then(Value::as_str), Some("#00D532"));
    assert_eq!(colors.get(2).and_then(Value::as_str), Some("#FF0000"));
    assert_eq!(colors.get(3).and_then(Value::as_str), Some("#FFDC87"));
}

#[test]
fn overlay_runtime_settings_preserve_saved_chart_visibility_and_colors() {
    let payload = overlay_info::overlay_runtime_settings_payload(
        &AppSettings::merge_settings_with_defaults(json!({
            "duration": 90,
            "show_session": true,
            "show_charts": false,
            "language": "ko",
            "color_player1": "#0080F8",
            "color_player2": "#00D532",
            "color_amon": "#FF0000",
            "color_mastery": "#FFDC87",
        })),
        4,
        1,
    );
    let colors = payload
        .get("colors")
        .and_then(Value::as_array)
        .expect("colors should always be present");

    assert_eq!(payload.get("duration").and_then(Value::as_u64), Some(90));
    assert_eq!(
        payload.get("show_session").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload.get("show_charts").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        payload.get("session_victory").and_then(Value::as_u64),
        Some(4)
    );
    assert_eq!(
        payload.get("session_defeat").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(payload.get("language").and_then(Value::as_str), Some("ko"));
    assert_eq!(colors.first().and_then(Value::as_str), Some("#0080F8"));
    assert_eq!(colors.get(1).and_then(Value::as_str), Some("#00D532"));
    assert_eq!(colors.get(2).and_then(Value::as_str), Some("#FF0000"));
    assert_eq!(colors.get(3).and_then(Value::as_str), Some("#FFDC87"));
}

#[test]
fn replay_overlay_after_game_setting_defaults_to_enabled() {
    assert!(AppSettings::merge_settings_with_defaults(json!({})).show_replay_info_after_game);
}

#[test]
fn replay_overlay_after_game_setting_uses_saved_value() {
    assert!(
        !AppSettings::merge_settings_with_defaults(json!({
            "show_replay_info_after_game": false,
        }))
        .show_replay_info_after_game
    );
    assert!(
        AppSettings::merge_settings_with_defaults(json!({
            "show_replay_info_after_game": true,
        }))
        .show_replay_info_after_game
    );
}
