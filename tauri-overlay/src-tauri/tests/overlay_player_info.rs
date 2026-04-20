mod common;

use common::test_replay_path;
use sco_tauri_overlay::overlay_info::{
    overlay_payload_from_replay, player_note_from_settings_value,
};
use sco_tauri_overlay::shared_types::OverlayReplayPayload;
use sco_tauri_overlay::{AppSettings, BackendState, ReplayInfo, ReplayPlayerInfo};
use serde_json::json;

fn sample_replay() -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo {
            name: "MainPlayer".to_string(),
            commander: "Abathur".to_string(),
            prestige: 1,
            ..ReplayPlayerInfo::default()
        },
        ReplayPlayerInfo {
            name: "AllyPlayer".to_string(),
            commander: "Swann".to_string(),
            prestige: 2,
            ..ReplayPlayerInfo::default()
        },
        0,
    );
    replay.file = test_replay_path("example.SC2Replay");
    replay.result = "Victory".to_string();
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
    let note = player_note_from_settings_value(
        &AppSettings::merge_settings_with_defaults(json!({
            "player_notes": {
                "allyplayer": "  Expand early.  "
            }
        })),
        "allyplayer",
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
