mod common;

use common::test_replay_path;
use s2coop_analyzer::dictionary_data;
use sco_tauri_overlay::merge_settings_with_defaults;
use sco_tauri_overlay::overlay_info::{
    overlay_payload_from_replay, player_note_from_settings_value,
};
use sco_tauri_overlay::shared_types::OverlayReplayPayload;
use sco_tauri_overlay::ReplayInfo;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Once;

fn initialize_dictionary_data() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("s2coop-analyzer")
            .join("data");
        let _ = dictionary_data::shared_dictionary_data(Some(data_dir));
    });
}

fn sample_replay() -> ReplayInfo {
    ReplayInfo {
        file: test_replay_path("example.SC2Replay"),
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
        &merge_settings_with_defaults(json!({
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
    initialize_dictionary_data();

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
