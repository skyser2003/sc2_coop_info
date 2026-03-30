mod common;

use common::test_replay_path;
use sco_tauri_overlay::{orient_replay_for_main_names, ReplayInfo, ReplayPlayerInfo};
use serde_json::json;
use std::collections::HashSet;

#[test]
fn games_rows_keep_true_slot_order_when_main_player_is_slot_two() {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo {
            name: "Teammate".to_string(),
            handle: "1-S2-1-111".to_string(),
            commander: "Swann".to_string(),
            ..ReplayPlayerInfo::default()
        },
        ReplayPlayerInfo {
            name: "Main".to_string(),
            handle: "1-S2-1-222".to_string(),
            commander: "Abathur".to_string(),
            ..ReplayPlayerInfo::default()
        },
        0,
    );
    replay.file = test_replay_path("example.SC2Replay");
    let main_names = HashSet::new();
    let main_handles = HashSet::from(["1-s2-1-222".to_string()]);

    let oriented = orient_replay_for_main_names(replay, &main_names, &main_handles);

    assert_eq!(oriented.main().name, "Main");
    assert_eq!(oriented.ally().name, "Teammate");

    let row = oriented.as_games_row();

    assert_eq!(row.get("p1"), Some(&json!("Teammate")));
    assert_eq!(row.get("p2"), Some(&json!("Main")));
    assert_eq!(row.get("main_commander"), Some(&json!("Abathur")));
    assert_eq!(row.get("ally_commander"), Some(&json!("Swann")));
}
