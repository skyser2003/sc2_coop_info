use sco_tauri_overlay::path_manager::get_cache_path;
use sco_tauri_overlay::replay_analysis::{
    parse_replay_timestamp_seconds, sanitize_hidden_unit_stats_with_dictionary, ReplayAnalysis,
};
use sco_tauri_overlay::test_helper::{
    build_rebuild_snapshot, collect_main_identity_lists, filter_replays_for_stats, load_dictionary,
    stats_replays_for_response_from_path_with_identity,
};
use sco_tauri_overlay::{sanitize_unit_map, AppSettings, ReplayInfo, ReplayPlayerInfo};
use serde_json::json;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

fn test_map_id(raw: &str) -> String {
    load_dictionary()
        .canonicalize_coop_map_id(raw)
        .expect("map id should resolve")
}

fn player(name: &str, handle: &str, commander: &str) -> ReplayPlayerInfo {
    ReplayPlayerInfo::default()
        .with_name(name)
        .with_handle(handle)
        .with_commander(commander)
}

fn sample_replay(
    file: &str,
    result: &str,
    difficulty: &str,
    main: ReplayPlayerInfo,
    ally: ReplayPlayerInfo,
) -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(main, ally, 0);
    replay.set_file(file);
    replay.set_map(test_map_id("Void Launch"));
    replay.set_result(result);
    replay.set_difficulty(difficulty);
    replay
}

fn replay_analysis_fixture_paths() -> Option<(PathBuf, AppSettings)> {
    let current_cache = get_cache_path();
    let settings_path = PathBuf::from("../settings.json");
    if !current_cache.is_file() || !settings_path.is_file() {
        return None;
    }

    let settings_text = std::fs::read_to_string(settings_path).ok()?;
    let settings = serde_json::from_str(&settings_text)
        .ok()
        .map(AppSettings::merge_settings_with_defaults)?;
    Some((current_cache, settings))
}

#[test]
fn normalized_player_key_trims_and_lowercases() {
    assert_eq!(
        ReplayAnalysis::normalized_player_key("  TeSt_User  "),
        "test_user"
    );
}

#[test]
fn normalized_handle_key_requires_marker() {
    assert_eq!(
        ReplayAnalysis::normalized_handle_key("1-S2-1-12345"),
        "1-s2-1-12345"
    );
    assert_eq!(ReplayAnalysis::normalized_handle_key("invalid"), "");
}

#[test]
fn collect_replay_paths_returns_empty_for_missing_root() {
    let missing = PathBuf::from("__definitely_missing_replay_root__");
    let replays = ReplayAnalysis::collect_replay_paths(&missing, 100);
    assert!(replays.is_empty());
}

#[test]
fn rebuild_snapshot_returns_empty_payload() {
    let snapshot = build_rebuild_snapshot(&[], true);
    assert!(snapshot.ready());
    assert_eq!(snapshot.games(), 0);
    assert!(snapshot.main_players().is_empty());
    assert!(snapshot.main_handles().is_empty());
    assert!(snapshot.analysis().get("MapData").is_some());
    assert_eq!(snapshot.message(), "No replay files found.");
}

#[test]
fn rebuild_snapshot_without_detailed_data_uses_null_unit_data() {
    let replays = vec![sample_replay(
        "simple.SC2Replay",
        "Victory",
        "Brutal",
        player("Main", "", "Raynor"),
        player("Ally", "", "Karax"),
    )];

    let snapshot = build_rebuild_snapshot(&replays, false);

    assert!(snapshot
        .analysis()
        .get("UnitData")
        .is_some_and(Value::is_null));
}

#[test]
fn filter_replays_for_stats_excludes_unparsed_replays() {
    let mut replay = ReplayInfo::default();
    replay.set_result("Unparsed");

    let filtered = filter_replays_for_stats("/config/stats", &[replay]);
    assert!(filtered.is_empty());
}

#[test]
fn sanitize_hidden_unit_stats_masks_created_and_lost_counts() {
    let dictionary = load_dictionary();
    let payload = json!({
        "Karax's Top Bar": [1, 2, 3, 0.25],
        "Zealot": [4, 5, 6, 0.5]
    });

    let sanitized = sanitize_hidden_unit_stats_with_dictionary(payload, &dictionary);

    assert_eq!(sanitized["Karax's Top Bar"], json!(["-", "-", 3, 0.25]));
    assert_eq!(sanitized["Zealot"], json!([4, 5, 6, 0.5]));
}

#[test]
fn sanitize_unit_map_preserves_negative_counts() {
    let payload = json!({
        "Primal Hydralisk": [70, -4, 0, 0.0]
    });

    let sanitized = sanitize_unit_map(&payload);

    assert_eq!(sanitized["Primal Hydralisk"], json!([70, -4, 0, 0.0]));
}

#[test]
fn map_times_use_accurate_length_like_wx_version() {
    let dictionary = load_dictionary();
    let replays = vec![
        {
            let mut replay = sample_replay(
                "a.SC2Replay",
                "Victory",
                "Brutal",
                player("Main", "", "Raynor"),
                player("Ally", "", "Karax"),
            );
            replay.set_length(590);
            replay.set_accurate_length(600.5);
            replay
        },
        {
            let mut replay = sample_replay(
                "b.SC2Replay",
                "Victory",
                "Brutal",
                player("Main", "", "Raynor"),
                player("Ally", "", "Karax"),
            );
            replay.set_length(600);
            replay.set_accurate_length(610.25);
            replay
        },
    ];

    let snapshot = ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
        &replays,
        false,
        &HashSet::new(),
        &HashSet::new(),
        &dictionary,
    );
    let map_data = snapshot
        .analysis()
        .get("MapData")
        .and_then(Value::as_object)
        .and_then(|maps| maps.get("Void Launch"))
        .and_then(Value::as_object)
        .expect("map data should include Void Launch");

    let average = map_data
        .get("average_victory_time")
        .and_then(Value::as_f64)
        .expect("average should be numeric");
    let fastest = map_data
        .get("Fastest")
        .and_then(Value::as_object)
        .and_then(|row| row.get("length"))
        .and_then(Value::as_f64)
        .expect("fastest length should be numeric");

    assert!((average - 605.375).abs() < f64::EPSILON);
    assert!((fastest - 600.5).abs() < f64::EPSILON);
}

#[test]
fn map_fastest_payload_includes_player_metadata() {
    let dictionary = load_dictionary();
    let fastest_date = 1_700_000_000;
    let replays = vec![{
        let mut replay = sample_replay(
            "fastest.SC2Replay",
            "Victory",
            "Brutal",
            player("Main", "1-S2-1-111", "Raynor")
                .with_apm(143)
                .with_mastery_level(90)
                .with_prestige(0)
                .with_masteries(vec![30, 0, 20, 10, 0, 30]),
            player("Ally", "1-S2-1-222", "Karax")
                .with_apm(98)
                .with_mastery_level(76)
                .with_prestige(2)
                .with_masteries(vec![15, 15, 0, 30, 20, 10]),
        );
        replay.set_date(fastest_date);
        replay.set_enemy("Zerg");
        replay.set_length(590);
        replay.set_accurate_length(600.5);
        replay
    }];

    let snapshot = ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
        &replays,
        false,
        &HashSet::new(),
        &HashSet::new(),
        &dictionary,
    );
    let fastest = snapshot
        .analysis()
        .get("MapData")
        .and_then(Value::as_object)
        .and_then(|maps| maps.get("Void Launch"))
        .and_then(Value::as_object)
        .and_then(|map| map.get("Fastest"))
        .and_then(Value::as_object)
        .expect("fastest payload should exist");

    assert_eq!(fastest.get("difficulty"), Some(&json!("Brutal")));
    assert_eq!(fastest.get("enemy_race"), Some(&json!("Zerg")));
    assert_eq!(fastest.get("date"), Some(&json!(fastest_date)));

    let players = fastest
        .get("players")
        .and_then(Value::as_array)
        .expect("fastest players should be an array");
    assert_eq!(players.len(), 2);

    assert_eq!(players[0]["name"], json!("Main"));
    assert_eq!(players[0]["handle"], json!("1-S2-1-111"));
    assert_eq!(players[0]["commander"], json!("Raynor"));
    assert_eq!(players[0]["apm"], json!(143));
    assert_eq!(players[0]["mastery_level"], json!(90));
    assert_eq!(players[0]["masteries"], json!([30, 0, 20, 10, 0, 30]));
    assert_eq!(players[0]["prestige"], json!(0));
    assert_eq!(
        players[0]["prestige_name"],
        json!(dictionary
            .prestige_name("Raynor", 0)
            .expect("Raynor prestige 0 should exist"))
    );

    assert_eq!(players[1]["name"], json!("Ally"));
    assert_eq!(players[1]["handle"], json!("1-S2-1-222"));
    assert_eq!(players[1]["commander"], json!("Karax"));
    assert_eq!(players[1]["apm"], json!(98));
    assert_eq!(players[1]["mastery_level"], json!(76));
    assert_eq!(players[1]["masteries"], json!([15, 15, 0, 30, 20, 10]));
    assert_eq!(players[1]["prestige"], json!(2));
    assert_eq!(
        players[1]["prestige_name"],
        json!(dictionary
            .prestige_name("Karax", 2)
            .expect("Karax prestige 2 should exist"))
    );
}

#[test]
fn map_fastest_prefers_oldest_replay_when_lengths_tie() {
    let dictionary = load_dictionary();
    let replays = vec![
        {
            let mut replay = sample_replay(
                "newer_fastest.SC2Replay",
                "Victory",
                "Brutal",
                player("Main", "1-S2-1-111", "Raynor").with_apm(143),
                player("Ally", "1-S2-1-222", "Karax").with_apm(98),
            );
            replay.set_date(
                parse_replay_timestamp_seconds("2020:01:02:03:04:05")
                    .expect("newer replay timestamp should parse"),
            );
            replay.set_enemy("Zerg");
            replay.set_accurate_length(600.5);
            replay
        },
        {
            let mut replay = sample_replay(
                "older_fastest.SC2Replay",
                "Victory",
                "Normal",
                player("Older Main", "1-S2-1-333", "Artanis").with_apm(77),
                player("Older Ally", "1-S2-1-444", "Swann").with_apm(66),
            );
            replay.set_date(
                parse_replay_timestamp_seconds("2019:01:02:03:04:05")
                    .expect("older replay timestamp should parse"),
            );
            replay.set_enemy("Terran");
            replay.set_accurate_length(600.5);
            replay
        },
    ];

    let snapshot = ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
        &replays,
        false,
        &HashSet::new(),
        &HashSet::new(),
        &dictionary,
    );
    let fastest = snapshot
        .analysis()
        .get("MapData")
        .and_then(Value::as_object)
        .and_then(|maps| maps.get("Void Launch"))
        .and_then(Value::as_object)
        .and_then(|map| map.get("Fastest"))
        .and_then(Value::as_object)
        .expect("fastest payload should exist");

    assert_eq!(
        fastest.get("date"),
        Some(&json!(parse_replay_timestamp_seconds(
            "2019:01:02:03:04:05"
        )
        .expect("older replay timestamp should parse")))
    );
    assert_eq!(fastest.get("difficulty"), Some(&json!("Normal")));
    assert_eq!(fastest.get("enemy_race"), Some(&json!("Terran")));

    let players = fastest
        .get("players")
        .and_then(Value::as_array)
        .expect("fastest players should be an array");
    assert_eq!(players[0]["name"], json!("Older Main"));
    assert_eq!(players[1]["name"], json!("Older Ally"));
}

#[test]
fn collect_main_identity_lists_tracks_p2_main_handle_for_fastest_maps() {
    let replays = vec![{
        let mut replay = ReplayInfo::with_players(
            player("Teammate", "1-S2-1-111", "Swann"),
            player("Main", "1-S2-1-222", "Abathur"),
            1,
        );
        replay.set_file("fastest.SC2Replay");
        replay.set_result("Victory");
        replay.set_difficulty("Normal");
        replay.set_map(test_map_id("Miner Evacuation"));
        replay.set_accurate_length(1041.75);
        replay
    }];
    let main_names = HashSet::new();
    let main_handles = HashSet::from(["1-s2-1-222".to_string()]);

    let (players, handles) = collect_main_identity_lists(&replays, &main_names, &main_handles);

    assert_eq!(players, vec!["Main".to_string()]);
    assert_eq!(handles, vec!["1-S2-1-222".to_string()]);
}

#[test]
fn miner_evacuation_fastest_payload_matches_reference_fastest_replay() {
    let Some((current_cache, settings)) = replay_analysis_fixture_paths() else {
        eprintln!(
            "skipping Miner Evacuation fastest payload test: required cache fixtures are missing"
        );
        return;
    };

    let main_names = settings.configured_main_names();
    let main_handles = settings.configured_main_handles();
    let replays = stats_replays_for_response_from_path_with_identity(
        true,
        &[],
        &current_cache,
        &main_names,
        &main_handles,
    );
    let snapshot = build_rebuild_snapshot(&replays, true);
    let Some(fastest) = snapshot
        .analysis()
        .get("MapData")
        .and_then(Value::as_object)
        .and_then(|maps| maps.get("Miner Evacuation"))
        .and_then(Value::as_object)
        .and_then(|map| map.get("Fastest"))
        .and_then(Value::as_object)
    else {
        eprintln!(
            "skipping Miner Evacuation fastest payload test: fixture cache has no Miner Evacuation entry"
        );
        return;
    };

    assert_eq!(fastest.get("length"), Some(&json!(1041.75)));
    assert_eq!(fastest.get("difficulty"), Some(&json!("Normal")));
    assert_eq!(fastest.get("enemy_race"), Some(&json!("테란")));
    assert_eq!(
        fastest.get("date"),
        Some(&json!(parse_replay_timestamp_seconds(
            "2018:09:30:22:12:24"
        )
        .expect("reference timestamp should parse")))
    );

    let players = fastest
        .get("players")
        .and_then(Value::as_array)
        .expect("fastest players should be an array");
    assert_eq!(players.len(), 2);

    assert_eq!(players[0]["name"], fastest["main"]);
    assert!(snapshot
        .main_handles()
        .iter()
        .any(|handle| Some(handle.as_str()) == players[0]["handle"].as_str()));
    assert_eq!(players[0]["commander"], json!("Abathur"));
    assert_eq!(players[0]["apm"], json!(123));
    assert_eq!(players[0]["mastery_level"], json!(0));
    assert_eq!(players[0]["masteries"], json!([0, 0, 0, 0, 0, 0]));
    assert_eq!(players[0]["prestige_name"], json!("Evolution Master"));

    assert_ne!(players[1]["name"], players[0]["name"]);
    assert_ne!(players[1]["handle"], players[0]["handle"]);
    assert_eq!(players[1]["commander"], json!("Swann"));
    assert_eq!(players[1]["apm"], json!(83));
    assert_eq!(players[1]["mastery_level"], json!(0));
    assert_eq!(players[1]["masteries"], json!([0, 0, 0, 0, 0, 0]));
    assert_eq!(players[1]["prestige_name"], json!("Chief Engineer"));

    assert!(snapshot
        .main_players()
        .iter()
        .any(|name| Some(name.as_str()) == players[0]["name"].as_str()));
    assert!(snapshot
        .main_handles()
        .iter()
        .any(|handle| Some(handle.as_str()) == players[0]["handle"].as_str()));
}

#[test]
fn mastery_sum_filters_partition_existing_cache_replays() {
    let Some((current_cache, settings)) = replay_analysis_fixture_paths() else {
        eprintln!("skipping exclusive stats partition test: required cache fixtures are missing");
        return;
    };

    let main_names = settings.configured_main_names();
    let main_handles = settings.configured_main_handles();
    let replays = stats_replays_for_response_from_path_with_identity(
        true,
        &[],
        &current_cache,
        &main_names,
        &main_handles,
    );

    let total = filter_replays_for_stats("/config/stats", &replays).len();
    let wins_only = filter_replays_for_stats("/config/stats?include_losses=0", &replays).len();
    let losses_only = filter_replays_for_stats("/config/stats?include_wins=0", &replays).len();
    let main_levels_1_14 = filter_replays_for_stats("/config/stats?over_15=0", &replays).len();
    let main_levels_15_plus = filter_replays_for_stats("/config/stats?sub_15=0", &replays).len();
    let ally_levels_1_14 = filter_replays_for_stats("/config/stats?ally_over_15=0", &replays).len();
    let ally_levels_15_plus =
        filter_replays_for_stats("/config/stats?ally_sub_15=0", &replays).len();
    assert_eq!(total, wins_only + losses_only);
    assert_eq!(total, main_levels_1_14 + main_levels_15_plus);
    assert_eq!(total, ally_levels_1_14 + ally_levels_15_plus);
    assert_eq!(
        total,
        filter_replays_for_stats("/config/stats?main_abnormal_mastery=0", &replays).len()
            + filter_replays_for_stats("/config/stats?main_normal_mastery=0", &replays).len()
    );
    assert_eq!(
        total,
        filter_replays_for_stats("/config/stats?ally_abnormal_mastery=0", &replays).len()
            + filter_replays_for_stats("/config/stats?ally_normal_mastery=0", &replays).len()
    );
}
