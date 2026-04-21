use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::test_helper::rebuild_analysis_payload;
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};

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
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo {
            name: player_name.to_string(),
            handle: handle.to_string(),
            commander: commander.to_string(),
            apm,
            kills: 8,
            ..ReplayPlayerInfo::default()
        },
        ReplayPlayerInfo {
            name: "Teammate".to_string(),
            handle: "1-S2-1-999".to_string(),
            commander: "Abathur".to_string(),
            apm: 50,
            kills: 2,
            ..ReplayPlayerInfo::default()
        },
        0,
    );
    replay.result = result.to_string();
    replay.date = date;
    replay.map = "Void Launch".to_string();
    replay
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
        .find(|row| row.handle == "1-S2-1-111")
        .expect("expected MemoTarget row");

    assert_eq!(row.wins, 2);
    assert_eq!(row.losses, 1);
    assert_eq!(row.commander, "Tychus");
    assert_eq!(row.last_seen, 300);

    let winrate = row.winrate;
    let frequency = row.frequency;
    assert!((winrate - (2.0 / 3.0)).abs() < 1e-9);
    assert!((frequency - (2.0 / 3.0)).abs() < 1e-9);

    assert_eq!(row.player_names, vec!["MemoTarget".to_string()]);
}

#[test]
fn analysis_player_data_uses_dominant_commander_frequency_for_overlay_player_rows() {
    let replays = vec![
        replay_for_player_rows("MemoTarget", "Tychus", "Victory", 120, 100),
        replay_for_player_rows("MemoTarget", "Tychus", "Defeat", 80, 300),
        replay_for_player_rows("MemoTarget", "Raynor", "Victory", 100, 200),
    ];

    let payload = rebuild_analysis_payload(&replays, false);
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
        .find(|entry| entry.handle == "1-S2-1-111")
        .expect("expected MemoTarget row");

    assert_eq!(row.player, "MemoTarget");
    assert_eq!(
        row.player_names,
        vec!["MemoTarget".to_string(), "OtherName".to_string()]
    );
}
