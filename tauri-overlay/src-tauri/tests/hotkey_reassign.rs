use sco_tauri_overlay::{overlay_info, AppSettings};
use serde_json::json;

#[test]
fn normalize_hotkey_accepts_shifted_symbol_variants() {
    assert_eq!(
        overlay_info::normalize_hotkey("Ctrl+Shift+&"),
        Some("control+shift+7".to_string())
    );
    assert_eq!(
        overlay_info::normalize_hotkey("Meta+P"),
        Some("super+p".to_string())
    );
    assert_eq!(
        overlay_info::normalize_hotkey("Ctrl+Shift+?"),
        Some("control+shift+/".to_string())
    );
}

#[test]
fn reassign_end_uses_cached_binding_when_current_settings_do_not_resolve_path() {
    let fallback = overlay_info::ResolvedHotkeyBinding::new(
        "performance_hotkey",
        "performance_show_hide",
        "control+shift+p",
        "control+shift+p",
    );

    let resolved = AppSettings::merge_settings_with_defaults(json!({
        "hotkey_show/hide": "Ctrl+Shift+*"
    }))
    .hotkey_binding_for_reassign_end("performance_hotkey", Some(&fallback))
    .expect("cached binding should be reused when the path cannot be resolved");

    assert_eq!(resolved.path(), "performance_hotkey");
    assert_eq!(resolved.action(), "performance_show_hide");
    assert_eq!(resolved.shortcut(), "control+shift+p");
    assert_eq!(resolved.canonical(), "control+shift+p");
}

#[test]
fn reassign_end_reuses_cached_binding_when_hotkey_is_null() {
    let fallback = overlay_info::ResolvedHotkeyBinding::new(
        "performance_hotkey",
        "performance_show_hide",
        "control+shift+p",
        "control+shift+p",
    );

    let resolved = AppSettings::merge_settings_with_defaults(json!({
        "performance_hotkey": null
    }))
    .hotkey_binding_for_reassign_end("performance_hotkey", Some(&fallback));

    let resolved = resolved.expect("null hotkey should be treated as not set");
    assert_eq!(resolved.path(), "performance_hotkey");
    assert_eq!(resolved.shortcut(), "control+shift+p");
}

#[test]
fn reassign_end_does_not_restore_explicitly_cleared_hotkey() {
    let fallback = overlay_info::ResolvedHotkeyBinding::new(
        "performance_hotkey",
        "performance_show_hide",
        "control+shift+p",
        "control+shift+p",
    );

    let resolved = AppSettings::merge_settings_with_defaults(json!({
        "performance_hotkey": ""
    }))
    .hotkey_binding_for_reassign_end("performance_hotkey", Some(&fallback));

    assert!(resolved.is_none());
}

#[test]
fn reassign_end_uses_builtin_default_when_overlay_hotkey_is_null() {
    let resolved = AppSettings::merge_settings_with_defaults(json!({
        "hotkey_show/hide": null
    }))
    .hotkey_binding_for_reassign_end("hotkey_show/hide", None)
    .expect("null overlay hotkey should use builtin default");

    assert_eq!(resolved.path(), "hotkey_show/hide");
    assert_eq!(resolved.action(), "overlay_show_hide");
    assert_eq!(resolved.shortcut(), "control+shift+8");
    assert_eq!(resolved.canonical(), "shift+control+digit8");
}
