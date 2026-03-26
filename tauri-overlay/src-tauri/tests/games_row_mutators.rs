mod common;

use common::test_replay_path;
use sco_tauri_overlay::*;
use serde_json::json;

#[test]
fn games_rows_include_mutators_and_mutation_flag() {
    let replay = ReplayInfo {
        file: test_replay_path("mutation.SC2Replay"),
        difficulty: "Brutal".to_string(),
        mutators: vec!["Barrier".to_string()],
        weekly: true,
        ..ReplayInfo::default()
    };

    let row = replay.as_games_row();

    assert_eq!(row.get("is_mutation"), Some(&json!(true)));
    assert_eq!(row.get("weekly"), Some(&json!(true)));
    assert_eq!(row.pointer("/mutators/0/name"), Some(&json!("Barrier")));
    assert_eq!(row.pointer("/mutators/0/iconName"), Some(&json!("Barrier")));
}
