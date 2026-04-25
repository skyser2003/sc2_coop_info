use sco_tauri_overlay::{AppSettings, BackendState, ReplayInfo, ReplayPlayerInfo};
use sco_tauri_overlay::{OverlayInfoOps, TestHelperOps};
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
    replay.set_file(TestHelperOps::test_replay_path("example.SC2Replay"));
    replay.set_result("Victory");
    replay
}

fn cached_orientation_replay_with_reversed_player_stats() -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("AllyPlayer")
            .with_commander("Swann"),
        ReplayPlayerInfo::default()
            .with_name("MainPlayer")
            .with_commander("Abathur"),
        1,
    );
    replay.set_file("cached-replay.SC2Replay");
    replay.set_result("Victory");
    replay.set_player_stats(json!({
        "1": {
            "name": "AllyPlayer",
            "army": [11.0],
            "supply": [12.0],
            "killed": [13.0],
            "mining": [14.0]
        },
        "2": {
            "name": "MainPlayer",
            "army": [21.0],
            "supply": [22.0],
            "killed": [23.0],
            "mining": [24.0]
        }
    }));
    replay
}

#[test]
fn overlay_payload_omits_session_counts_when_disabled() {
    let state = BackendState::new();
    let payload =
        OverlayInfoOps::overlay_payload_from_replay(&state, &sample_replay(), true, false, 4, 1);

    assert_eq!(payload.victory, None);
    assert_eq!(payload.defeat, None);
    assert_eq!(payload.new_replay, Some(true));
}

#[test]
fn overlay_payload_includes_session_counts_when_enabled() {
    let state = BackendState::new();
    let payload =
        OverlayInfoOps::overlay_payload_from_replay(&state, &sample_replay(), false, true, 4, 1);

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
        TestHelperOps::localized_prestige_text("Abathur", 1, "en"),
        "Essence Hoarder"
    );
    assert_eq!(
        TestHelperOps::localized_prestige_text("Abathur", 1, "ko"),
        "정수 축적가"
    );
    assert_eq!(
        TestHelperOps::localized_prestige_text("Swann", 2, "ko"),
        "노련한 기계공"
    );
}

#[test]
fn overlay_payload_exposes_semantic_player_stats_for_main_and_ally() {
    let state = BackendState::new();
    let payload = OverlayInfoOps::overlay_payload_from_replay(
        &state,
        &cached_orientation_replay_with_reversed_player_stats(),
        false,
        false,
        0,
        0,
    );

    assert_eq!(payload.main, "MainPlayer");
    assert_eq!(payload.ally, "AllyPlayer");
    assert_eq!(
        payload
            .main_player_stats
            .as_ref()
            .map(|stats| stats.name.as_str()),
        Some("MainPlayer")
    );
    assert_eq!(
        payload
            .main_player_stats
            .as_ref()
            .map(|stats| stats.army.clone()),
        Some(vec![21.0])
    );
    assert_eq!(
        payload
            .ally_player_stats
            .as_ref()
            .map(|stats| stats.name.as_str()),
        Some("AllyPlayer")
    );
    assert_eq!(
        payload
            .ally_player_stats
            .as_ref()
            .map(|stats| stats.army.clone()),
        Some(vec![11.0])
    );
}
