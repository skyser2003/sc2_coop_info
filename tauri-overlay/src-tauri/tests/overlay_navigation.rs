use sco_tauri_overlay::ReplayInfo;
use sco_tauri_overlay::{OverlayInfoOps, TestHelperOps};

fn sample_replay(file: &str) -> ReplayInfo {
    let mut replay = ReplayInfo::default();
    replay.set_file(file);
    replay
}

#[test]
fn replay_move_target_index_resets_to_most_recent_when_overlay_has_no_replay_data() {
    let replays = vec![
        sample_replay(&TestHelperOps::test_replay_path("newest.SC2Replay")),
        sample_replay(&TestHelperOps::test_replay_path("middle.SC2Replay")),
        sample_replay(&TestHelperOps::test_replay_path("oldest.SC2Replay")),
    ];
    let selected = Some(TestHelperOps::test_replay_path("oldest.SC2Replay"));

    assert_eq!(
        OverlayInfoOps::replay_move_target_index(&replays, &selected, 1, false),
        0
    );
    assert_eq!(
        OverlayInfoOps::replay_move_target_index(&replays, &selected, -1, false),
        0
    );
}

#[test]
fn replay_move_target_index_moves_relative_to_selected_replay_when_data_is_active() {
    let replays = vec![
        sample_replay(&TestHelperOps::test_replay_path("newest.SC2Replay")),
        sample_replay(&TestHelperOps::test_replay_path("middle.SC2Replay")),
        sample_replay(&TestHelperOps::test_replay_path("oldest.SC2Replay")),
    ];
    let selected = Some(TestHelperOps::test_replay_path("middle.SC2Replay"));

    assert_eq!(
        OverlayInfoOps::replay_move_target_index(&replays, &selected, 1, true),
        0
    );
    assert_eq!(
        OverlayInfoOps::replay_move_target_index(&replays, &selected, -1, true),
        2
    );
}

#[test]
fn replay_move_is_ignored_when_latest_replay_is_already_showing() {
    assert!(OverlayInfoOps::replay_move_should_be_ignored(
        Some(0),
        0,
        true
    ));
    assert!(!OverlayInfoOps::replay_move_should_be_ignored(
        Some(1),
        0,
        true
    ));
    assert!(!OverlayInfoOps::replay_move_should_be_ignored(
        Some(0),
        0,
        false
    ));
}

#[test]
fn replay_for_display_falls_back_to_most_recent_cached_replay() {
    let replays = vec![
        sample_replay(&TestHelperOps::test_replay_path("newest.SC2Replay")),
        sample_replay(&TestHelperOps::test_replay_path("older.SC2Replay")),
    ];
    let newest_path = TestHelperOps::test_replay_path("newest.SC2Replay");

    assert_eq!(
        OverlayInfoOps::replay_for_display(&replays, Some("newest.SC2Replay"), &None)
            .map(|replay| replay.file()),
        Some(newest_path.as_str())
    );
    assert_eq!(
        OverlayInfoOps::replay_for_display(&replays, None, &None).map(|replay| replay.file()),
        Some(newest_path.as_str())
    );
}
