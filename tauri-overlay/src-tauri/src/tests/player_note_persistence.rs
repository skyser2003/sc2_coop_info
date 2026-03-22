use super::*;
use serde_json::json;

#[test]
fn update_settings_player_note_only_changes_player_notes_branch() {
    let mut settings = json!({
        "show_charts": true,
        "player_notes": {
            "1-S2-1-111": "old note"
        },
        "other_setting": 42
    });

    update_settings_player_note(&mut settings, "1-S2-1-111", "new note")
        .expect("player note update should succeed");

    assert_eq!(settings["show_charts"], json!(true));
    assert_eq!(settings["other_setting"], json!(42));
    assert_eq!(settings["player_notes"]["1-S2-1-111"], json!("new note"));
}

#[test]
fn update_settings_player_note_removes_case_insensitive_match_when_cleared() {
    let mut settings = json!({
        "player_notes": {
            "1-S2-1-111": "old note",
            "1-S2-1-222": "keep me"
        }
    });

    update_settings_player_note(&mut settings, "1-s2-1-111", "")
        .expect("clearing player note should succeed");

    let notes = settings["player_notes"]
        .as_object()
        .expect("player_notes should stay an object");
    assert!(!notes.contains_key("1-S2-1-111"));
    assert_eq!(notes.get("1-S2-1-222"), Some(&json!("keep me")));
}
