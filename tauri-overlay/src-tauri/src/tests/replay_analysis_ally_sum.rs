use super::*;
use serde_json::json;

#[test]
fn ally_commander_data_includes_sum_row() {
    let replays = vec![
        ReplayInfo {
            map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            main_apm: 120,
            ally_apm: 90,
            main_kills: 20,
            ally_kills: 10,
            ..ReplayInfo::default()
        },
        ReplayInfo {
            map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
            result: "Defeat".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Stukov".to_string(),
            main_apm: 80,
            ally_apm: 70,
            main_kills: 8,
            ally_kills: 12,
            ..ReplayInfo::default()
        },
    ];

    let snapshot = ReplayAnalysis::build_rebuild_snapshot(&replays, false);
    let ally_commander_data = snapshot
        .analysis
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
        ReplayInfo {
            map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "2-S2-1-222".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            ..ReplayInfo::default()
        },
        ReplayInfo {
            map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "2-S2-1-223".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Stukov".to_string(),
            ..ReplayInfo::default()
        },
        ReplayInfo {
            map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "2-S2-1-224".to_string(),
            main_commander: "Karax".to_string(),
            ally_commander: "Stukov".to_string(),
            ..ReplayInfo::default()
        },
    ];

    let snapshot = ReplayAnalysis::build_rebuild_snapshot(&replays, false);
    let ally_commander_data = snapshot
        .analysis
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
