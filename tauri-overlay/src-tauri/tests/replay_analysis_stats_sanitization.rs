use sco_tauri_overlay::{ReplayAnalysis, TestHelperOps};
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};
use serde_json::{Value, json};

fn sanitized_stats_replay() -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("<b>Main Player</b>")
            .with_handle("1-S2-1-111")
            .with_apm(150)
            .with_kills(30)
            .with_commander("<b>Raynor</b>")
            .with_commander_level(15)
            .with_mastery_level(90)
            .with_masteries(vec![30, 60, 30, 60, 30, 60]),
        ReplayPlayerInfo::default()
            .with_name("<i>Ally Player</i>")
            .with_handle("2-S2-1-222")
            .with_apm(120)
            .with_kills(10)
            .with_commander("<i>Karax</i>")
            .with_commander_level(15)
            .with_mastery_level(90)
            .with_masteries(vec![60, 30, 60, 30, 60, 30]),
        0,
    );
    replay.set_file("fixtures/replays/example.SC2Replay");
    replay.set_date(1_741_510_400);
    replay
        .set_map(TestHelperOps::canonicalize_map_id("Void Launch").expect("map id should resolve"));
    replay.set_result("Victory");
    replay.set_difficulty("<b>Brutal</b>");
    replay.set_enemy("<span>Zerg</span>");
    replay.set_accurate_length(600.0);
    replay.set_weekly(true);
    replay.set_weekly_name(Some("<b>Mutation #1</b>".to_string()));
    replay
}

#[test]
fn rebuild_analysis_payload_sanitizes_output_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let payload = TestHelperOps::rebuild_analysis_payload(&[replay], false);
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
    assert!(
        rows.iter()
            .any(|row| { row.player == "Main Player" && row.commander == "Raynor" })
    );
    assert!(
        !rows
            .iter()
            .any(|row| { row.player == "<b>Main Player</b>" || row.commander == "<b>Raynor</b>" })
    );
}

#[test]
fn rebuild_weeklies_rows_sanitizes_fields_without_full_replay_clone() {
    let replay = sanitized_stats_replay();

    let rows = TestHelperOps::rebuild_weeklies_rows(&[replay]);
    let row = rows
        .iter()
        .find(|row| row.mutation == "Mutation #1")
        .expect("sanitized weekly mutation row should exist");

    assert_eq!(row.mutation, "Mutation #1");
    assert_eq!(row.difficulty, "Brutal");
}
