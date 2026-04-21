use sco_tauri_overlay::test_helper::test_replay_path;
use sco_tauri_overlay::ReplayInfo;
use serde_json::json;

#[test]
fn games_rows_include_mutators_and_mutation_flag() {
    let mut replay = ReplayInfo::default();
    replay.set_file(test_replay_path("mutation.SC2Replay"));
    replay.set_difficulty("Brutal");
    replay.set_mutators(vec!["Barrier".to_string()]);
    replay.set_weekly(true);

    let row = replay.as_games_row();

    assert_eq!(row.get("is_mutation"), Some(&json!(true)));
    assert_eq!(row.get("weekly"), Some(&json!(true)));
    assert_eq!(row.pointer("/mutators/0/name/en"), Some(&json!("Barrier")));
    assert_eq!(row.pointer("/mutators/0/iconName"), Some(&json!("Barrier")));
}
