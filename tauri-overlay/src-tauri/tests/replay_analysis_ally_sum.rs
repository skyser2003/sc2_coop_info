use sco_tauri_overlay::test_helper::{build_rebuild_snapshot, canonicalize_map_id};
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};
use serde_json::json;
use serde_json::Value;

fn test_map_id(raw: &str) -> String {
    canonicalize_map_id(raw).expect("map id should resolve")
}

fn player(name: &str, handle: &str, commander: &str) -> ReplayPlayerInfo {
    ReplayPlayerInfo::default()
        .with_name(name)
        .with_handle(handle)
        .with_commander(commander)
}

fn replay_with_players(result: &str, main: ReplayPlayerInfo, ally: ReplayPlayerInfo) -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(main, ally, 0);
    replay.set_map(test_map_id("Void Launch"));
    replay.set_result(result);
    replay.set_difficulty("Brutal");
    replay
}

#[test]
fn ally_commander_data_includes_sum_row() {
    let replays = vec![
        replay_with_players(
            "Victory",
            player("Main", "", "Raynor").with_apm(120).with_kills(20),
            player("Ally", "", "Karax").with_apm(90).with_kills(10),
        ),
        replay_with_players(
            "Defeat",
            player("Main", "", "Raynor").with_apm(80).with_kills(8),
            player("Ally", "", "Stukov").with_apm(70).with_kills(12),
        ),
    ];

    let snapshot = build_rebuild_snapshot(&replays, false);
    let ally_commander_data = snapshot
        .analysis()
        .get("AllyCommanderData")
        .and_then(Value::as_object)
        .expect("ally commander data should exist");

    let sum = ally_commander_data
        .get("any")
        .and_then(Value::as_object)
        .expect("ally commander sum row should exist");

    assert_eq!(sum.get("Victory"), Some(&json!(1)));
    assert_eq!(sum.get("Defeat"), Some(&json!(1)));
    assert_eq!(sum.get("Frequency"), Some(&json!(1.0)));
}

#[test]
fn ally_commander_frequency_matches_wx_preference_correction_rule() {
    let replays = vec![
        replay_with_players(
            "Victory",
            player("Main", "1-S2-1-111", "Raynor"),
            player("Ally", "2-S2-1-222", "Karax"),
        ),
        replay_with_players(
            "Victory",
            player("Main", "1-S2-1-111", "Raynor"),
            player("Ally", "2-S2-1-223", "Stukov"),
        ),
        replay_with_players(
            "Victory",
            player("Main", "1-S2-1-111", "Karax"),
            player("Ally", "2-S2-1-224", "Stukov"),
        ),
    ];

    let snapshot = build_rebuild_snapshot(&replays, false);
    let ally_commander_data = snapshot
        .analysis()
        .get("AllyCommanderData")
        .and_then(Value::as_object)
        .expect("ally commander data should exist");

    let karax_frequency = ally_commander_data
        .get("Karax")
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("Frequency"))
        .and_then(Value::as_f64)
        .expect("Karax ally frequency should exist");
    let stukov_frequency = ally_commander_data
        .get("Stukov")
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("Frequency"))
        .and_then(Value::as_f64)
        .expect("Stukov ally frequency should exist");

    // wx rule:
    // main frequencies: Raynor=2/3, Karax=1/3
    // observed ally games: Karax=1, Stukov=2
    // corrected counts: Karax=1/(1-1/3)=1.5, Stukov=2/(1-0)=2
    // normalized: Karax=1.5/3.5, Stukov=2/3.5
    assert!((karax_frequency - (1.5 / 3.5)).abs() < 1e-9);
    assert!((stukov_frequency - (2.0 / 3.5)).abs() < 1e-9);
}
