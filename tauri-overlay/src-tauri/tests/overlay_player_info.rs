use sco_tauri_overlay::overlay_info::overlay_payload_from_replay;
use sco_tauri_overlay::test_helper::{localized_prestige_text, test_replay_path};
use sco_tauri_overlay::{AppSettings, BackendState, ReplayInfo, ReplayPlayerInfo};
use serde_json::json;

fn sample_replay() -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("MainPlayer")
            .with_commander("Abathur")
            .with_prestige(1),
        ReplayPlayerInfo::default()
            .with_name("AllyPlayer")
            .with_commander("Swann")
            .with_prestige(2),
        0,
    );
    replay.set_file(test_replay_path("example.SC2Replay"));
    replay.set_result("Victory");
    replay
}

#[test]
fn overlay_payload_omits_session_counts_when_disabled() {
    let state = BackendState::new();
    let payload = overlay_payload_from_replay(&state, &sample_replay(), true, false, 4, 1);

    assert_eq!(payload.victory, None);
    assert_eq!(payload.defeat, None);
    assert_eq!(payload.new_replay, Some(true));
}

#[test]
fn overlay_payload_includes_session_counts_when_enabled() {
    let state = BackendState::new();
    let payload = overlay_payload_from_replay(&state, &sample_replay(), false, true, 4, 1);

    assert_eq!(payload.victory, Some(4));
    assert_eq!(payload.defeat, Some(1));
    assert_eq!(payload.new_replay, None);
}

#[test]
fn player_note_lookup_matches_case_insensitive_names() {
    let note = AppSettings::merge_settings_with_defaults(json!({
        "player_notes": {
            "allyplayer": "  Expand early.  "
        }
    }))
    .player_note("allyplayer");

    assert_eq!(note.as_deref(), Some("Expand early."));
}

#[test]
fn overlay_prestige_text_uses_selected_language() {
    assert_eq!(
        localized_prestige_text("Abathur", 1, "en"),
        "Essence Hoarder"
    );
    assert_eq!(localized_prestige_text("Abathur", 1, "ko"), "정수 축적가");
    assert_eq!(localized_prestige_text("Swann", 2, "ko"), "노련한 기계공");
}
