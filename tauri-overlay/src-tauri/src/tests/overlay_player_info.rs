use super::*;
use crate::ReplayInfo;
use serde_json::json;

fn sample_replay() -> ReplayInfo {
    ReplayInfo {
        file: crate::test_replay_path("example.SC2Replay"),
        p1: "MainPlayer".to_string(),
        p2: "AllyPlayer".to_string(),
        main_commander: "Abathur".to_string(),
        ally_commander: "Swann".to_string(),
        main_prestige: 1,
        ally_prestige: 2,
        result: "Victory".to_string(),
        ..ReplayInfo::default()
    }
}

#[test]
fn overlay_payload_omits_session_counts_when_disabled() {
    let payload = overlay_payload_from_replay(&sample_replay(), true, false, 4, 1);

    assert_eq!(payload.victory, None);
    assert_eq!(payload.defeat, None);
    assert_eq!(payload.new_replay, Some(true));
}

#[test]
fn overlay_payload_includes_session_counts_when_enabled() {
    let payload = overlay_payload_from_replay(&sample_replay(), false, true, 4, 1);

    assert_eq!(payload.victory, Some(4));
    assert_eq!(payload.defeat, Some(1));
    assert_eq!(payload.new_replay, None);
}

#[test]
fn player_note_lookup_matches_case_insensitive_names() {
    let note = player_note_from_settings_value(
        &json!({
            "player_notes": {
                "allyplayer": "  Expand early.  "
            }
        }),
        "AllyPlayer",
    );

    assert_eq!(note.as_deref(), Some("Expand early."));
}

#[test]
fn overlay_prestige_text_uses_selected_language() {
    assert_eq!(
        OverlayReplayPayload::localized_prestige_text("Abathur", 1, "en"),
        "Essence Hoarder"
    );
    assert_eq!(
        OverlayReplayPayload::localized_prestige_text("Abathur", 1, "ko"),
        "정수 축적가"
    );
    assert_eq!(
        OverlayReplayPayload::localized_prestige_text("Swann", 2, "ko"),
        "노련한 기계공"
    );
}
