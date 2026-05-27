use sco_tauri_overlay::{AppSettings, TauriOverlayOps};
use serde_json::json;

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
fn tauri_autostart_registration_replaces_legacy_manual_registration() {
    assert_eq!(
        TauriOverlayOps::autostart_registration_name(),
        env!("CARGO_PKG_NAME")
    );
    assert_eq!(
        TauriOverlayOps::legacy_windows_startup_registration_name(),
        "SCO Overlay"
    );
    assert!(TauriOverlayOps::should_remove_legacy_windows_startup_registration());
}
