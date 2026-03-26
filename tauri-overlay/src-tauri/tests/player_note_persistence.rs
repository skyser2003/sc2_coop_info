use sco_tauri_overlay::{merge_settings_with_defaults, update_settings_player_note};
use serde_json::json;

#[test]
fn update_settings_player_note_only_changes_player_notes_branch() {
    let mut settings = merge_settings_with_defaults(json!({
        "show_charts": true,
        "player_notes": {
            "1-S2-1-111": "old note"
        },
    }));

    update_settings_player_note(&mut settings, "1-S2-1-111", "new note")
        .expect("player note update should succeed");

    assert!(settings.show_charts);
    assert_eq!(
        settings.player_notes.get("1-S2-1-111"),
        Some(&"new note".to_string())
    );
}

#[test]
fn update_settings_player_note_removes_case_insensitive_match_when_cleared() {
    let mut settings = merge_settings_with_defaults(json!({
        "player_notes": {
            "1-S2-1-111": "old note",
            "1-S2-1-222": "keep me"
        }
    }));

    update_settings_player_note(&mut settings, "1-s2-1-111", "")
        .expect("clearing player note should succeed");

    let notes = &settings.player_notes;
    assert!(!notes.contains_key("1-S2-1-111"));
    assert_eq!(notes.get("1-S2-1-222"), Some(&"keep me".to_string()));
}
