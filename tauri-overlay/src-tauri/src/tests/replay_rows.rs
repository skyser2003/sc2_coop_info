use super::*;
use serde_json::json;
use std::collections::HashSet;

#[test]
fn games_rows_keep_true_slot_order_when_main_player_is_slot_two() {
    let replay = ReplayInfo {
        file: test_replay_path("example.SC2Replay"),
        p1: "Teammate".to_string(),
        p2: "Main".to_string(),
        slot1_name: "Teammate".to_string(),
        slot2_name: "Main".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-222".to_string(),
        slot1_handle: "1-S2-1-111".to_string(),
        slot2_handle: "1-S2-1-222".to_string(),
        main_commander: "Swann".to_string(),
        ally_commander: "Abathur".to_string(),
        slot1_commander: "Swann".to_string(),
        slot2_commander: "Abathur".to_string(),
        ..ReplayInfo::default()
    };
    let main_names = HashSet::new();
    let main_handles = HashSet::from(["1-s2-1-222".to_string()]);

    let oriented = orient_replay_for_main_names(replay, &main_names, &main_handles);

    assert_eq!(oriented.p1, "Main");
    assert_eq!(oriented.p2, "Teammate");

    let row = oriented.as_games_row();

    assert_eq!(row.get("p1"), Some(&json!("Teammate")));
    assert_eq!(row.get("p2"), Some(&json!("Main")));
    assert_eq!(row.get("main_commander"), Some(&json!("Swann")));
    assert_eq!(row.get("ally_commander"), Some(&json!("Abathur")));
}
