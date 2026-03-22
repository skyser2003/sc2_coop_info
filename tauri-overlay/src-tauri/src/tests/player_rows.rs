use super::*;

fn replay_for_player_rows(
    player_name: &str,
    commander: &str,
    result: &str,
    apm: u64,
    date: u64,
) -> ReplayInfo {
    replay_for_player_rows_with_handle(player_name, commander, result, apm, date, "1-S2-1-111")
}

fn replay_for_player_rows_with_handle(
    player_name: &str,
    commander: &str,
    result: &str,
    apm: u64,
    date: u64,
    handle: &str,
) -> ReplayInfo {
    ReplayInfo {
        p1: player_name.to_string(),
        p2: "Teammate".to_string(),
        p1_handle: handle.to_string(),
        p2_handle: "1-S2-1-999".to_string(),
        main_commander: commander.to_string(),
        ally_commander: "Abathur".to_string(),
        main_apm: apm,
        ally_apm: 50,
        main_kills: 8,
        ally_kills: 2,
        result: result.to_string(),
        date,
        map: "Void Launch".to_string(),
        ..ReplayInfo::default()
    }
}

#[test]
fn rebuild_player_rows_fast_restores_winrate_and_dominant_commander_frequency() {
    let replays = vec![
        replay_for_player_rows("MemoTarget", "Tychus", "Victory", 120, 100),
        replay_for_player_rows("MemoTarget", "Tychus", "Defeat", 80, 300),
        replay_for_player_rows("MemoTarget", "Raynor", "Victory", 100, 200),
    ];

    let rows = ReplayAnalysis::rebuild_player_rows_fast(&replays);
    let row = rows
        .iter()
        .find(|row| row.get("handle").and_then(serde_json::Value::as_str) == Some("1-S2-1-111"))
        .expect("expected MemoTarget row");

    assert_eq!(row.get("wins").and_then(serde_json::Value::as_u64), Some(2));
    assert_eq!(
        row.get("losses").and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        row.get("commander").and_then(serde_json::Value::as_str),
        Some("Tychus")
    );
    assert_eq!(
        row.get("last_seen").and_then(serde_json::Value::as_u64),
        Some(300)
    );

    let winrate = row
        .get("winrate")
        .and_then(serde_json::Value::as_f64)
        .expect("row winrate should be numeric");
    let frequency = row
        .get("frequency")
        .and_then(serde_json::Value::as_f64)
        .expect("row frequency should be numeric");
    assert!((winrate - (2.0 / 3.0)).abs() < 1e-9);
    assert!((frequency - (2.0 / 3.0)).abs() < 1e-9);

    let player_names = row
        .get("player_names")
        .and_then(serde_json::Value::as_array)
        .expect("row player_names should be an array");
    assert_eq!(player_names, &vec![json!("MemoTarget")]);
}

#[test]
fn analysis_player_data_uses_dominant_commander_frequency_for_overlay_player_rows() {
    let replays = vec![
        replay_for_player_rows("MemoTarget", "Tychus", "Victory", 120, 100),
        replay_for_player_rows("MemoTarget", "Tychus", "Defeat", 80, 300),
        replay_for_player_rows("MemoTarget", "Raynor", "Victory", 100, 200),
    ];

    let payload = ReplayAnalysis::rebuild_analysis_payload(&replays, false);
    let row = payload["analysis"]["PlayerData"]["MemoTarget"]
        .as_object()
        .expect("expected player data row");

    assert_eq!(
        row.get("commander").and_then(serde_json::Value::as_str),
        Some("Tychus")
    );

    let winrate = row
        .get("winrate")
        .and_then(serde_json::Value::as_f64)
        .expect("player data winrate should be numeric");
    let frequency = row
        .get("frequency")
        .and_then(serde_json::Value::as_f64)
        .expect("player data frequency should be numeric");
    assert!((winrate - (2.0 / 3.0)).abs() < 1e-9);
    assert!((frequency - (2.0 / 3.0)).abs() < 1e-9);
}

#[test]
fn rebuild_player_rows_fast_groups_usernames_by_handle_and_prefers_most_recent_name() {
    let replays = vec![
        replay_for_player_rows_with_handle(
            "MemoTarget",
            "Tychus",
            "Victory",
            120,
            100,
            "1-S2-1-111",
        ),
        replay_for_player_rows_with_handle(
            "OtherName",
            "Raynor",
            "Victory",
            110,
            200,
            "1-S2-1-111",
        ),
        replay_for_player_rows_with_handle("MemoTarget", "Raynor", "Defeat", 90, 300, "1-S2-1-111"),
    ];

    let rows = ReplayAnalysis::rebuild_player_rows_fast(&replays);
    let row = rows
        .iter()
        .find(|entry| entry.get("handle").and_then(serde_json::Value::as_str) == Some("1-S2-1-111"))
        .expect("expected MemoTarget row");

    assert_eq!(
        row.get("player").and_then(serde_json::Value::as_str),
        Some("MemoTarget")
    );

    let player_names = row
        .get("player_names")
        .and_then(serde_json::Value::as_array)
        .expect("row player_names should be an array");
    assert_eq!(player_names, &vec![json!("MemoTarget"), json!("OtherName")]);
}
