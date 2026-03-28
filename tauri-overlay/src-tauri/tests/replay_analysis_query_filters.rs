use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::ReplayInfo;

fn replay_for_checkbox_filter(
    file_name: &str,
    difficulty: &str,
    brutal_plus: u64,
    p1_handle: &str,
) -> ReplayInfo {
    ReplayInfo {
        file: format!("fixtures/replays/{file_name}.SC2Replay"),
        map: "Void Launch".to_string(),
        result: "Victory".to_string(),
        difficulty: difficulty.to_string(),
        p1: "Main".to_string(),
        p2: "Ally".to_string(),
        p1_handle: p1_handle.to_string(),
        p2_handle: "1-S2-1-999".to_string(),
        main_commander: "Raynor".to_string(),
        ally_commander: "Karax".to_string(),
        main_commander_level: 15,
        ally_commander_level: 15,
        brutal_plus,
        ..ReplayInfo::default()
    }
}

fn replay_for_mastery_filter(
    file_name: &str,
    main_mastery_points: u64,
    ally_mastery_points: u64,
) -> ReplayInfo {
    fn mastery_distribution(total_points: u64) -> Vec<u64> {
        let first = total_points.min(30);
        let second = total_points.saturating_sub(first).min(30);
        let third = total_points.saturating_sub(first + second);
        vec![first, 0, second, 0, third, 0]
    }

    ReplayInfo {
        file: format!("fixtures/replays/{file_name}.SC2Replay"),
        map: "Void Launch".to_string(),
        result: "Victory".to_string(),
        difficulty: "Brutal".to_string(),
        p1: "Main".to_string(),
        p2: "Ally".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-999".to_string(),
        main_commander: "Raynor".to_string(),
        ally_commander: "Karax".to_string(),
        main_commander_level: 15,
        ally_commander_level: 15,
        main_mastery_level: main_mastery_points,
        ally_mastery_level: ally_mastery_points,
        main_masteries: mastery_distribution(main_mastery_points),
        ally_masteries: mastery_distribution(ally_mastery_points),
        ..ReplayInfo::default()
    }
}

fn replay_for_result_filter(file_name: &str, result: &str) -> ReplayInfo {
    ReplayInfo {
        file: format!("fixtures/replays/{file_name}.SC2Replay"),
        map: "Void Launch".to_string(),
        result: result.to_string(),
        difficulty: "Brutal".to_string(),
        p1: "Main".to_string(),
        p2: "Ally".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-999".to_string(),
        main_commander: "Raynor".to_string(),
        ally_commander: "Karax".to_string(),
        main_commander_level: 15,
        ally_commander_level: 15,
        ..ReplayInfo::default()
    }
}

fn replay_for_ally_level_filter(file_name: &str, ally_commander_level: u64) -> ReplayInfo {
    ReplayInfo {
        file: format!("fixtures/replays/{file_name}.SC2Replay"),
        map: "Void Launch".to_string(),
        result: "Victory".to_string(),
        difficulty: "Brutal".to_string(),
        p1: "Main".to_string(),
        p2: "Ally".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-999".to_string(),
        main_commander: "Raynor".to_string(),
        ally_commander: "Karax".to_string(),
        main_commander_level: 15,
        ally_commander_level,
        ..ReplayInfo::default()
    }
}

#[test]
fn filter_replays_for_stats_decodes_checkbox_filter_lists_from_query_string() {
    let replays = vec![
        replay_for_checkbox_filter("na_brutal", "Brutal", 0, "1-S2-1-111"),
        replay_for_checkbox_filter("eu_normal", "Normal", 0, "2-S2-1-222"),
        replay_for_checkbox_filter("kr_bplus3", "Brutal", 3, "3-S2-1-333"),
    ];

    let filtered = ReplayAnalysis::filter_replays_for_stats(
        "/config/stats?difficulty_filter=Normal%2C3&region_filter=EU%2CKR",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file, "fixtures/replays/na_brutal.SC2Replay");
}

#[test]
fn filter_replays_for_stats_decodes_brutal_plus_checkbox_values() {
    let replays = vec![
        replay_for_checkbox_filter("plain_brutal", "Brutal", 0, "1-S2-1-111"),
        replay_for_checkbox_filter("bplus1", "Brutal", 1, "1-S2-1-222"),
    ];

    let filtered = ReplayAnalysis::filter_replays_for_stats(
        "/config/stats?difficulty_filter=1%2C2%2C3%2C4%2C5%2C6",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file, "fixtures/replays/plain_brutal.SC2Replay");
}

#[test]
fn filter_replays_for_stats_can_limit_results_to_main_normal_mastery_games() {
    let replays = vec![
        replay_for_mastery_filter("main_normal_90", 90, 200),
        replay_for_mastery_filter("main_abnormal_91", 91, 0),
        replay_for_mastery_filter("main_normal_60", 60, 150),
    ];

    let filtered =
        ReplayAnalysis::filter_replays_for_stats("/config/stats?main_abnormal_mastery=0", &replays);

    assert_eq!(filtered.len(), 2);
    assert_eq!(
        filtered[0].file,
        "fixtures/replays/main_normal_90.SC2Replay"
    );
    assert_eq!(
        filtered[1].file,
        "fixtures/replays/main_normal_60.SC2Replay"
    );
}

#[test]
fn filter_replays_for_stats_can_limit_results_to_ally_abnormal_mastery_games() {
    let replays = vec![
        replay_for_mastery_filter("ally_normal_90", 0, 90),
        replay_for_mastery_filter("ally_abnormal_91", 10, 91),
        replay_for_mastery_filter("ally_abnormal_120", 20, 120),
    ];

    let filtered =
        ReplayAnalysis::filter_replays_for_stats("/config/stats?ally_normal_mastery=0", &replays);

    assert_eq!(filtered.len(), 2);
    assert_eq!(
        filtered[0].file,
        "fixtures/replays/ally_abnormal_91.SC2Replay"
    );
    assert_eq!(
        filtered[1].file,
        "fixtures/replays/ally_abnormal_120.SC2Replay"
    );
}

#[test]
fn filter_replays_for_stats_can_limit_results_to_losses() {
    let replays = vec![
        replay_for_result_filter("win", "Victory"),
        replay_for_result_filter("loss", "Defeat"),
    ];

    let filtered =
        ReplayAnalysis::filter_replays_for_stats("/config/stats?include_wins=0", &replays);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file, "fixtures/replays/loss.SC2Replay");
}

#[test]
fn filter_replays_for_stats_can_limit_results_to_ally_levels_1_14() {
    let replays = vec![
        replay_for_ally_level_filter("ally_low", 10),
        replay_for_ally_level_filter("ally_high", 15),
    ];

    let filtered =
        ReplayAnalysis::filter_replays_for_stats("/config/stats?ally_over_15=0", &replays);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file, "fixtures/replays/ally_low.SC2Replay");
}

#[test]
fn filter_replays_for_stats_uses_or_logic_within_main_level_group() {
    let replays = vec![
        ReplayInfo {
            file: "fixtures/replays/main_group_level_match.SC2Replay".to_string(),
            map: "Void Launch".to_string(),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "1-S2-1-999".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            main_commander_level: 10,
            ally_commander_level: 15,
            main_masteries: vec![30, 30, 30, 0, 0, 0],
            ally_masteries: vec![0, 0, 0, 0, 0, 0],
            ..ReplayInfo::default()
        },
        ReplayInfo {
            file: "fixtures/replays/main_group_high_level_match.SC2Replay".to_string(),
            map: "Void Launch".to_string(),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "1-S2-1-999".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            main_commander_level: 15,
            ally_commander_level: 15,
            main_masteries: vec![30, 30, 30, 0, 0, 0],
            ally_masteries: vec![0, 0, 0, 0, 0, 0],
            ..ReplayInfo::default()
        },
        ReplayInfo {
            file: "fixtures/replays/main_group_no_match.SC2Replay".to_string(),
            map: "Void Launch".to_string(),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "1-S2-1-999".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            main_commander_level: 15,
            ally_commander_level: 15,
            main_masteries: vec![91, 91, 91, 0, 0, 0],
            ally_masteries: vec![0, 0, 0, 0, 0, 0],
            ..ReplayInfo::default()
        },
    ];

    let filtered = ReplayAnalysis::filter_replays_for_stats(
        "/config/stats?sub_15=1&over_15=1&main_abnormal_mastery=0",
        &replays,
    );

    assert_eq!(filtered.len(), 2);
    assert_eq!(
        filtered[0].file,
        "fixtures/replays/main_group_level_match.SC2Replay"
    );
    assert_eq!(
        filtered[1].file,
        "fixtures/replays/main_group_high_level_match.SC2Replay"
    );
}

#[test]
fn abnormal_main_mastery_filter_updates_fastest_map_payload() {
    let replays = vec![
        ReplayInfo {
            file: "fixtures/replays/excluded_fastest.SC2Replay".to_string(),
            map: "Void Launch".to_string(),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Main".to_string(),
            p2: "Ally".to_string(),
            p1_handle: "1-S2-1-111".to_string(),
            p2_handle: "1-S2-1-999".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            main_commander_level: 15,
            ally_commander_level: 15,
            main_mastery_level: 90,
            ally_mastery_level: 200,
            main_masteries: vec![30, 0, 30, 0, 30, 0],
            ally_masteries: vec![30, 0, 30, 0, 140, 0],
            accurate_length: 500.0,
            ..ReplayInfo::default()
        },
        ReplayInfo {
            file: "fixtures/replays/included_fastest.SC2Replay".to_string(),
            map: "Void Launch".to_string(),
            result: "Victory".to_string(),
            difficulty: "Brutal".to_string(),
            p1: "Legacy Main".to_string(),
            p2: "Legacy Ally".to_string(),
            p1_handle: "1-S2-1-222".to_string(),
            p2_handle: "1-S2-1-333".to_string(),
            main_commander: "Raynor".to_string(),
            ally_commander: "Karax".to_string(),
            main_commander_level: 15,
            ally_commander_level: 15,
            main_mastery_level: 91,
            ally_mastery_level: 0,
            main_masteries: vec![30, 0, 30, 0, 31, 0],
            ally_masteries: vec![0, 0, 0, 0, 0, 0],
            accurate_length: 600.0,
            ..ReplayInfo::default()
        },
    ];

    let filtered =
        ReplayAnalysis::filter_replays_for_stats("/config/stats?main_normal_mastery=0", &replays);
    let snapshot = ReplayAnalysis::build_rebuild_snapshot(&filtered, false);
    let fastest = snapshot
        .analysis
        .get("MapData")
        .and_then(serde_json::Value::as_object)
        .and_then(|maps| maps.get("Void Launch"))
        .and_then(serde_json::Value::as_object)
        .and_then(|map| map.get("Fastest"))
        .and_then(serde_json::Value::as_object)
        .expect("fastest payload should exist");

    assert_eq!(
        fastest.get("file"),
        Some(&serde_json::Value::String(
            "fixtures/replays/included_fastest.SC2Replay".to_string()
        ))
    );
    assert_eq!(fastest.get("length"), Some(&serde_json::Value::from(600.0)));
}
