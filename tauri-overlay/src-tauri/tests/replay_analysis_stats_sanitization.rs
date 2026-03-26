use sco_tauri_overlay::replay_analysis::*;
use sco_tauri_overlay::{canonicalize_coop_map_id, ReplayInfo};
use serde_json::{json, Value};

fn sanitized_stats_replay() -> ReplayInfo {
    ReplayInfo {
        file: "fixtures/replays/example.SC2Replay".to_string(),
        date: 1_741_510_400,
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        difficulty: "<b>Brutal</b>".to_string(),
        p1: "<b>Main Player</b>".to_string(),
        p2: "<i>Ally Player</i>".to_string(),
        enemy: "<span>Zerg</span>".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "2-S2-1-222".to_string(),
        accurate_length: 600.0,
        main_apm: 150,
        ally_apm: 120,
        main_kills: 30,
        ally_kills: 10,
        main_commander: "<b>Raynor</b>".to_string(),
        ally_commander: "<i>Karax</i>".to_string(),
        main_commander_level: 15,
        ally_commander_level: 15,
        main_mastery_level: 90,
        ally_mastery_level: 90,
        main_masteries: vec![30, 60, 30, 60, 30, 60],
        ally_masteries: vec![60, 30, 60, 30, 60, 30],
        weekly: true,
        weekly_name: Some("<b>Mutation #1</b>".to_string()),
        ..ReplayInfo::default()
    }
}

#[test]
fn rebuild_analysis_payload_sanitizes_output_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let payload = ReplayAnalysis::rebuild_analysis_payload(&[replay], false);
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
    assert!(rows.iter().any(|row| {
        row.get("player") == Some(&json!("Main Player"))
            && row.get("commander") == Some(&json!("Raynor"))
    }));
    assert!(!rows.iter().any(|row| {
        row.get("player") == Some(&json!("<b>Main Player</b>"))
            || row.get("commander") == Some(&json!("<b>Raynor</b>"))
    }));
}

#[test]
fn rebuild_weeklies_rows_sanitizes_fields_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let rows = ReplayAnalysis::rebuild_weeklies_rows(&[replay]);
    let row = rows
        .iter()
        .find(|row| row.get("mutation") == Some(&json!("Mutation #1")))
        .expect("sanitized weekly mutation row should exist");

    assert_eq!(row.get("mutation"), Some(&json!("Mutation #1")));
    assert_eq!(row.get("difficulty"), Some(&json!("Brutal")));
}
