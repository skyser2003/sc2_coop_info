use sco_tauri_overlay::{
    AppSettings, OVERLAY_HOTKEY_SETTING_KEYS, OVERLAY_PLACEMENT_SETTING_KEYS,
    OVERLAY_RUNTIME_SETTING_KEYS, OverlayInfoOps, OverlayMonitorGeometry, OverlayWindowBoundsInput,
    OverlayWindowOffsets, OverlayWindowScale,
};
use serde_json::json;

fn bounds_input(
    geometry: (i32, i32, u32, u32),
    scale: (f64, f64),
    offsets: (i32, i32, i32),
) -> OverlayWindowBoundsInput {
    OverlayWindowBoundsInput::new(
        OverlayMonitorGeometry::new(geometry.0, geometry.1, geometry.2, geometry.3),
        OverlayWindowScale::new(scale.0, scale.1),
        OverlayWindowOffsets::new(offsets.0, offsets.1, offsets.2),
    )
}

#[test]
fn runtime_setting_diff_ignores_color_updates_for_hotkeys_and_placement() {
    let previous = AppSettings::merge_settings_with_defaults(json!({
        "color_player1": "#0080F8",
        "hotkey_show/hide": "Ctrl+Shift+*",
        "monitor": 1,
    }));
    let next = AppSettings::merge_settings_with_defaults(json!({
        "color_player1": "#FF0000",
        "hotkey_show/hide": "Ctrl+Shift+*",
        "monitor": 1,
    }));

    assert!(AppSettings::any_setting_changed(
        &previous,
        &next,
        &OVERLAY_RUNTIME_SETTING_KEYS,
    ));
    assert!(!AppSettings::any_setting_changed(
        &previous,
        &next,
        &OVERLAY_HOTKEY_SETTING_KEYS,
    ));
    assert!(!AppSettings::any_setting_changed(
        &previous,
        &next,
        &OVERLAY_PLACEMENT_SETTING_KEYS,
    ));
}

#[test]
fn runtime_setting_diff_detects_hotkey_and_placement_changes() {
    let previous = AppSettings::merge_settings_with_defaults(json!({
        "hotkey_show/hide": "Ctrl+Shift+*",
        "monitor": 1,
    }));
    let next = AppSettings::merge_settings_with_defaults(json!({
        "hotkey_show/hide": "Ctrl+Shift+P",
        "monitor": 2,
    }));

    assert!(AppSettings::any_setting_changed(
        &previous,
        &next,
        &OVERLAY_HOTKEY_SETTING_KEYS,
    ));
    assert!(AppSettings::any_setting_changed(
        &previous,
        &next,
        &OVERLAY_PLACEMENT_SETTING_KEYS,
    ));
}

#[test]
fn overlay_window_bounds_use_target_size_for_right_alignment() {
    let (size, position) = OverlayInfoOps::overlay_window_bounds_for_monitor(bounds_input(
        (100, 200, 1920, 1080),
        (0.7, 1.0),
        (12, -24, 1),
    ));

    assert_eq!(size.width, 1344);
    assert_eq!(size.height, 1079);
    assert_eq!(position.x, 652);
    assert_eq!(position.y, 212);
}

#[test]
fn overlay_window_bounds_clamp_to_monitor_dimensions() {
    let (size, position) = OverlayInfoOps::overlay_window_bounds_for_monitor(bounds_input(
        (-1920, 0, 1920, 1080),
        (2.0, 2.0),
        (0, 0, -500),
    ));

    assert_eq!(size.width, 1920);
    assert_eq!(size.height, 1080);
    assert_eq!(position.x, -1920);
    assert_eq!(position.y, 0);
}

#[test]
fn overlay_window_position_uses_actual_applied_width_for_right_alignment() {
    let requested = OverlayInfoOps::overlay_window_bounds_for_monitor(bounds_input(
        (-1080, 0, 1080, 1920),
        (0.7, 1.0),
        (0, 0, 1),
    ));
    let actual_position =
        OverlayInfoOps::overlay_window_position_for_monitor(-1080, 0, 1080, 492, 0, 0);

    assert_eq!(requested.0.width, 1080);
    assert_eq!(requested.0.height, 1919);
    assert_eq!(requested.1.x, -1080);
    assert_eq!(actual_position.x, -492);
    assert_eq!(actual_position.y, 0);
}

#[test]
fn overlay_window_size_match_detects_runtime_monitor_switch_shrink() {
    let requested = OverlayInfoOps::overlay_window_bounds_for_monitor(bounds_input(
        (-1080, 0, 1080, 1920),
        (0.7, 1.0),
        (0, 0, 1),
    ));

    assert!(!OverlayInfoOps::overlay_window_size_matches_target(
        tauri::PhysicalSize {
            width: 492,
            height: 1919,
        },
        requested.0,
    ));
    assert!(OverlayInfoOps::overlay_window_size_matches_target(
        tauri::PhysicalSize {
            width: 1080,
            height: 1919,
        },
        requested.0,
    ));
}

#[test]
fn overlay_window_bounds_use_full_width_for_portrait_monitors() {
    let (size, position) = OverlayInfoOps::overlay_window_bounds_for_monitor(bounds_input(
        (300, 100, 1080, 1920),
        (0.7, 1.0),
        (5, -12, 1),
    ));

    assert_eq!(size.width, 1080);
    assert_eq!(size.height, 1919);
    assert_eq!(position.x, 288);
    assert_eq!(position.y, 105);
}
