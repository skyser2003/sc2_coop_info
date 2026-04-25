use sco_tauri_overlay::TestHelperOps;
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};

fn player(name: &str, handle: &str, commander: &str) -> ReplayPlayerInfo {
    ReplayPlayerInfo::default()
        .with_name(name)
        .with_handle(handle)
        .with_commander(commander)
}

fn replay_with_players(
    file_name: &str,
    result: &str,
    difficulty: &str,
    brutal_plus: u64,
    slot1: ReplayPlayerInfo,
    slot2: ReplayPlayerInfo,
) -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(slot1, slot2, 0);
    replay.set_file(format!("fixtures/replays/{file_name}.SC2Replay"));
    replay.set_map("Void Launch");
    replay.set_result(result);
    replay.set_difficulty(difficulty);
    replay.set_brutal_plus(brutal_plus);
    replay
}

fn replay_for_checkbox_filter(
    file_name: &str,
    difficulty: &str,
    brutal_plus: u64,
    p1_handle: &str,
) -> ReplayInfo {
    replay_with_players(
        file_name,
        "Victory",
        difficulty,
        brutal_plus,
        player("Main", p1_handle, "Raynor").with_commander_level(15),
        player("Ally", "1-S2-1-999", "Karax").with_commander_level(15),
    )
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

    replay_with_players(
        file_name,
        "Victory",
        "Brutal",
        0,
        player("Main", "1-S2-1-111", "Raynor")
            .with_commander_level(15)
            .with_mastery_level(main_mastery_points)
            .with_masteries(mastery_distribution(main_mastery_points)),
        player("Ally", "1-S2-1-999", "Karax")
            .with_commander_level(15)
            .with_mastery_level(ally_mastery_points)
            .with_masteries(mastery_distribution(ally_mastery_points)),
    )
}

fn replay_for_result_filter(file_name: &str, result: &str) -> ReplayInfo {
    replay_with_players(
        file_name,
        result,
        "Brutal",
        0,
        player("Main", "1-S2-1-111", "Raynor").with_commander_level(15),
        player("Ally", "1-S2-1-999", "Karax").with_commander_level(15),
    )
}

fn replay_for_ally_level_filter(file_name: &str, ally_commander_level: u64) -> ReplayInfo {
    replay_with_players(
        file_name,
        "Victory",
        "Brutal",
        0,
        player("Main", "1-S2-1-111", "Raynor").with_commander_level(15),
        player("Ally", "1-S2-1-999", "Karax").with_commander_level(ally_commander_level),
    )
}

#[test]
fn filter_replays_for_stats_decodes_checkbox_filter_lists_from_query_string() {
    let replays = vec![
        replay_for_checkbox_filter("na_brutal", "Brutal", 0, "1-S2-1-111"),
        replay_for_checkbox_filter("eu_normal", "Normal", 0, "2-S2-1-222"),
        replay_for_checkbox_filter("kr_bplus3", "Brutal", 3, "3-S2-1-333"),
    ];

    let filtered = TestHelperOps::filter_replays_for_stats(
        "/config/stats?difficulty_filter=Normal%2C3&region_filter=EU%2CKR",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file(), "fixtures/replays/na_brutal.SC2Replay");
}

#[test]
fn filter_replays_for_stats_decodes_brutal_plus_checkbox_values() {
    let replays = vec![
        replay_for_checkbox_filter("plain_brutal", "Brutal", 0, "1-S2-1-111"),
        replay_for_checkbox_filter("bplus1", "Brutal", 1, "1-S2-1-222"),
    ];

    let filtered = TestHelperOps::filter_replays_for_stats(
        "/config/stats?difficulty_filter=1%2C2%2C3%2C4%2C5%2C6",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(
        filtered[0].file(),
        "fixtures/replays/plain_brutal.SC2Replay"
    );
}

#[test]
fn filter_replays_for_stats_can_limit_results_to_main_normal_mastery_games() {
    let replays = vec![
        replay_for_mastery_filter("main_normal_90", 90, 200),
        replay_for_mastery_filter("main_abnormal_91", 91, 0),
        replay_for_mastery_filter("main_normal_60", 60, 150),
    ];

    let filtered =
        TestHelperOps::filter_replays_for_stats("/config/stats?main_abnormal_mastery=0", &replays);

    assert_eq!(filtered.len(), 2);
    assert_eq!(
        filtered[0].file(),
        "fixtures/replays/main_normal_90.SC2Replay"
    );
    assert_eq!(
        filtered[1].file(),
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
        TestHelperOps::filter_replays_for_stats("/config/stats?ally_normal_mastery=0", &replays);

    assert_eq!(filtered.len(), 2);
    assert_eq!(
        filtered[0].file(),
        "fixtures/replays/ally_abnormal_91.SC2Replay"
    );
    assert_eq!(
        filtered[1].file(),
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
        TestHelperOps::filter_replays_for_stats("/config/stats?include_wins=0", &replays);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file(), "fixtures/replays/loss.SC2Replay");
}

#[test]
fn filter_replays_for_stats_can_limit_results_to_ally_levels_1_14() {
    let replays = vec![
        replay_for_ally_level_filter("ally_low", 10),
        replay_for_ally_level_filter("ally_high", 15),
    ];

    let filtered =
        TestHelperOps::filter_replays_for_stats("/config/stats?ally_over_15=0", &replays);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file(), "fixtures/replays/ally_low.SC2Replay");
}

#[test]
fn filter_replays_for_stats_uses_or_logic_within_main_level_group() {
    let replays = vec![
        replay_with_players(
            "main_group_level_match",
            "Victory",
            "Brutal",
            0,
            player("Main", "1-S2-1-111", "Raynor")
                .with_commander_level(10)
                .with_masteries(vec![30, 30, 30, 0, 0, 0]),
            player("Ally", "1-S2-1-999", "Karax")
                .with_commander_level(15)
                .with_masteries(vec![0, 0, 0, 0, 0, 0]),
        ),
        replay_with_players(
            "main_group_high_level_match",
            "Victory",
            "Brutal",
            0,
            player("Main", "1-S2-1-111", "Raynor")
                .with_commander_level(15)
                .with_masteries(vec![30, 30, 30, 0, 0, 0]),
            player("Ally", "1-S2-1-999", "Karax")
                .with_commander_level(15)
                .with_masteries(vec![0, 0, 0, 0, 0, 0]),
        ),
        replay_with_players(
            "main_group_no_match",
            "Victory",
            "Brutal",
            0,
            player("Main", "1-S2-1-111", "Raynor")
                .with_commander_level(15)
                .with_masteries(vec![91, 91, 91, 0, 0, 0]),
            player("Ally", "1-S2-1-999", "Karax")
                .with_commander_level(15)
                .with_masteries(vec![0, 0, 0, 0, 0, 0]),
        ),
    ];

    let filtered = TestHelperOps::filter_replays_for_stats(
        "/config/stats?sub_15=1&over_15=1&main_abnormal_mastery=0",
        &replays,
    );

    assert_eq!(filtered.len(), 2);
    assert_eq!(
        filtered[0].file(),
        "fixtures/replays/main_group_level_match.SC2Replay"
    );
    assert_eq!(
        filtered[1].file(),
        "fixtures/replays/main_group_high_level_match.SC2Replay"
    );
}

#[test]
fn abnormal_main_mastery_filter_updates_fastest_map_payload() {
    let replays = vec![
        {
            let mut replay = replay_with_players(
                "excluded_fastest",
                "Victory",
                "Brutal",
                0,
                player("Main", "1-S2-1-111", "Raynor")
                    .with_commander_level(15)
                    .with_mastery_level(90)
                    .with_masteries(vec![30, 0, 30, 0, 30, 0]),
                player("Ally", "1-S2-1-999", "Karax")
                    .with_commander_level(15)
                    .with_mastery_level(200)
                    .with_masteries(vec![30, 0, 30, 0, 140, 0]),
            );
            replay.set_accurate_length(500.0);
            replay
        },
        {
            let mut replay = replay_with_players(
                "included_fastest",
                "Victory",
                "Brutal",
                0,
                player("Legacy Main", "1-S2-1-222", "Raynor")
                    .with_commander_level(15)
                    .with_mastery_level(91)
                    .with_masteries(vec![30, 0, 30, 0, 31, 0]),
                player("Legacy Ally", "1-S2-1-333", "Karax")
                    .with_commander_level(15)
                    .with_mastery_level(0)
                    .with_masteries(vec![0, 0, 0, 0, 0, 0]),
            );
            replay.set_accurate_length(600.0);
            replay
        },
    ];

    let filtered =
        TestHelperOps::filter_replays_for_stats("/config/stats?main_normal_mastery=0", &replays);
    let snapshot = TestHelperOps::build_rebuild_snapshot(&filtered, false);
    let fastest = snapshot
        .analysis()
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
