use super::*;
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
    let fallback = overlay_info::ResolvedHotkeyBinding {
        path: "performance_hotkey",
        action: "performance_show_hide",
        shortcut: "control+shift+p".to_string(),
        canonical: "control+shift+p".to_string(),
    };

    let resolved = overlay_info::resolve_hotkey_binding_for_reassign_end(
        &json!({ "hotkey_show/hide": "Ctrl+Shift+*" }),
        "performance_hotkey",
        Some(&fallback),
    )
    .expect("cached binding should be reused when the path cannot be resolved");

    assert_eq!(resolved.path, "performance_hotkey");
    assert_eq!(resolved.action, "performance_show_hide");
    assert_eq!(resolved.shortcut, "control+shift+p");
    assert_eq!(resolved.canonical, "control+shift+p");
}

#[test]
fn reassign_end_does_not_restore_explicitly_cleared_hotkey() {
    let fallback = overlay_info::ResolvedHotkeyBinding {
        path: "performance_hotkey",
        action: "performance_show_hide",
        shortcut: "control+shift+p".to_string(),
        canonical: "control+shift+p".to_string(),
    };

    let resolved = overlay_info::resolve_hotkey_binding_for_reassign_end(
        &json!({ "performance_hotkey": null }),
        "performance_hotkey",
        Some(&fallback),
    );

    assert!(resolved.is_none());
}
