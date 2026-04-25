use sco_tauri_overlay::test_helper::TestHelperOps;
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};
use serde_json::json;
use std::collections::HashSet;

#[test]
fn games_rows_keep_true_slot_order_when_main_player_is_slot_two() {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("Teammate")
            .with_handle("1-S2-1-111")
            .with_commander("Swann"),
        ReplayPlayerInfo::default()
            .with_name("Main")
            .with_handle("1-S2-1-222")
            .with_commander("Abathur"),
        0,
    );
    replay.set_file(TestHelperOps::test_replay_path("example.SC2Replay"));
    let main_names = HashSet::new();
    let main_handles = HashSet::from(["1-s2-1-222".to_string()]);

    let oriented = replay.oriented_for_main_identity(&main_names, &main_handles);

    assert_eq!(oriented.main().name(), "Main");
    assert_eq!(oriented.ally().name(), "Teammate");

    let row = oriented.as_games_row();

    assert_eq!(row.get("p1"), Some(&json!("Teammate")));
    assert_eq!(row.get("p2"), Some(&json!("Main")));
    assert_eq!(row.get("slot1_commander"), Some(&json!("Swann")));
    assert_eq!(row.get("slot2_commander"), Some(&json!("Abathur")));
    assert_eq!(row.get("main_commander"), Some(&json!("Abathur")));
    assert_eq!(row.get("ally_commander"), Some(&json!("Swann")));
}
