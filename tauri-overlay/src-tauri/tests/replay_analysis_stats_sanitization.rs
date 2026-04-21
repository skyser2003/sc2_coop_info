use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::test_helper::{
    canonicalize_map_id, rebuild_analysis_payload, rebuild_weeklies_rows,
};
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};
use serde_json::{json, Value};

fn sanitized_stats_replay() -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo {
            name: "<b>Main Player</b>".to_string(),
            handle: "1-S2-1-111".to_string(),
            apm: 150,
            kills: 30,
            commander: "<b>Raynor</b>".to_string(),
            commander_level: 15,
            mastery_level: 90,
            masteries: vec![30, 60, 30, 60, 30, 60],
            ..ReplayPlayerInfo::default()
        },
        ReplayPlayerInfo {
            name: "<i>Ally Player</i>".to_string(),
            handle: "2-S2-1-222".to_string(),
            apm: 120,
            kills: 10,
            commander: "<i>Karax</i>".to_string(),
            commander_level: 15,
            mastery_level: 90,
            masteries: vec![60, 30, 60, 30, 60, 30],
            ..ReplayPlayerInfo::default()
        },
        0,
    );
    replay.file = "fixtures/replays/example.SC2Replay".to_string();
    replay.date = 1_741_510_400;
    replay.map = canonicalize_map_id("Void Launch").expect("map id should resolve");
    replay.result = "Victory".to_string();
    replay.difficulty = "<b>Brutal</b>".to_string();
    replay.enemy = "<span>Zerg</span>".to_string();
    replay.accurate_length = 600.0;
    replay.weekly = true;
    replay.weekly_name = Some("<b>Mutation #1</b>".to_string());
    replay
}

#[test]
fn rebuild_analysis_payload_sanitizes_output_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let payload = rebuild_analysis_payload(&[replay], false);
    let analysis = payload
        .get("analysis")
        .and_then(Value::as_object)
        .expect("analysis payload should be present");

    let commander_data = analysis
        .get("CommanderData")
        .and_then(Value::as_object)
        .expect("commander data should be present");
    assert!(commander_data.contains_key("Raynor"));
    assert!(!commander_data.contains_key("<b>Raynor</b>"));

    let player_data = analysis
        .get("PlayerData")
        .and_then(Value::as_object)
        .expect("player data should be present");
    assert!(player_data.contains_key("Main Player"));
    assert!(!player_data.contains_key("<b>Main Player</b>"));

    let region_data = analysis
        .get("RegionData")
        .and_then(Value::as_object)
        .and_then(|regions| regions.get("NA"))
        .and_then(Value::as_object)
        .expect("NA region data should be present");
    assert_eq!(region_data.get("max_com"), Some(&json!(["Raynor"])));
}

#[test]
fn rebuild_player_rows_fast_sanitizes_fields_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let rows = ReplayAnalysis::rebuild_player_rows_fast(&[replay]);

    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|row| { row.player == "Main Player" && row.commander == "Raynor" }));
    assert!(!rows
        .iter()
        .any(|row| { row.player == "<b>Main Player</b>" || row.commander == "<b>Raynor</b>" }));
}

#[test]
fn rebuild_weeklies_rows_sanitizes_fields_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let rows = rebuild_weeklies_rows(&[replay]);
    let row = rows
        .iter()
        .find(|row| row.mutation == "Mutation #1")
        .expect("sanitized weekly mutation row should exist");

    assert_eq!(row.mutation, "Mutation #1");
    assert_eq!(row.difficulty, "Brutal");
}
