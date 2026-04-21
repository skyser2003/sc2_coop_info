use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CachePlayer, CacheReplayEntry, CacheUnitStats,
    ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::test_helper::{
    canonicalize_map_id, load_detailed_analysis_replays_snapshot_from_path_with_identity,
    stats_replays_for_response_from_path_with_identity,
};
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo, UNLIMITED_REPLAY_LIMIT};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_map_id(raw: &str) -> String {
    canonicalize_map_id(raw).expect("map id should resolve")
}

fn player(name: &str, handle: &str, commander: &str) -> ReplayPlayerInfo {
    ReplayPlayerInfo::default()
        .with_name(name)
        .with_handle(handle)
        .with_commander(commander)
}

fn sample_replay(file: &str, main: ReplayPlayerInfo, ally: ReplayPlayerInfo) -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(main, ally, 0);
    replay.set_file(file);
    replay.set_map(test_map_id("Void Launch"));
    replay.set_result("Victory");
    replay.set_difficulty("Brutal");
    replay
}

fn unique_temp_path(label: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sco_{label}_{timestamp}"))
}

fn sample_cache_player(
    pid: u8,
    name: &str,
    handle: &str,
    commander: &str,
    units: &[(&str, CacheUnitStats)],
) -> CachePlayer {
    let mut unit_map = BTreeMap::new();
    for (unit_name, unit_stats) in units {
        unit_map.insert((*unit_name).to_string(), unit_stats.clone());
    }

    CachePlayer {
        pid,
        apm: Some(150),
        commander: Some(commander.to_string()),
        commander_level: Some(15),
        commander_mastery_level: Some(90),
        handle: Some(handle.to_string()),
        icons: Some(BTreeMap::new()),
        kills: Some(25),
        masteries: Some([30, 60, 30, 60, 30, 60]),
        name: Some(name.to_string()),
        observer: None,
        prestige: Some(1),
        prestige_name: Some("P1".to_string()),
        race: Some("Terran".to_string()),
        result: Some("Victory".to_string()),
        units: Some(unit_map),
    }
}

fn sample_cache_entry(file: &Path) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Integer(600),
        amon_units: None,
        bonus: Some(vec!["First".to_string()]),
        brutal_plus: 0,
        build: ReplayBuildInfo::new(1, ProtocolBuildValue::Int(1)),
        comp: Some("Terran".to_string()),
        date: "2026-03-09 12:00:00".to_string(),
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: "Brutal".to_string(),
        extension: false,
        file: file.display().to_string(),
        form_alength: "10:00".to_string(),
        detailed_analysis: true,
        hash: format!("hash_{}", file.display()),
        length: 600,
        map_name: "Void Launch".to_string(),
        messages: Vec::new(),
        mutators: Vec::new(),
        player_stats: None,
        players: vec![
            CachePlayer {
                pid: 0,
                apm: None,
                commander: None,
                commander_level: None,
                commander_mastery_level: None,
                handle: None,
                icons: None,
                kills: None,
                masteries: None,
                name: None,
                observer: None,
                prestige: None,
                prestige_name: None,
                race: None,
                result: None,
                units: None,
            },
            sample_cache_player(
                1,
                "Main Player",
                "1-S2-1-111",
                "Raynor",
                &[(
                    "Marine",
                    CacheUnitStats(
                        CacheCountValue::Count(8),
                        CacheCountValue::Count(2),
                        99,
                        0.75,
                    ),
                )],
            ),
            sample_cache_player(
                2,
                "Ally Player",
                "1-S2-1-222",
                "Karax",
                &[(
                    "Zealot",
                    CacheUnitStats(CacheCountValue::Count(2), CacheCountValue::Count(0), 4, 0.1),
                )],
            ),
        ],
        region: "NA".to_string(),
        result: "Victory".to_string(),
        weekly: false,
    }
}

fn merge_cached_detailed_replays_from_path(
    replays: &[ReplayInfo],
    cache_path: &Path,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> Vec<ReplayInfo> {
    if replays.is_empty() {
        return Vec::new();
    }

    let detailed_replays = load_detailed_analysis_replays_snapshot_from_path_with_identity(
        cache_path,
        UNLIMITED_REPLAY_LIMIT,
        main_names,
        main_handles,
    );
    if detailed_replays.is_empty() {
        return replays.to_vec();
    }

    let detailed_by_file: HashMap<String, ReplayInfo> = detailed_replays
        .into_iter()
        .map(|replay| (replay.file().to_string(), replay))
        .collect();

    replays
        .iter()
        .map(|replay| {
            detailed_by_file
                .get(replay.file())
                .cloned()
                .unwrap_or_else(|| replay.clone())
        })
        .collect()
}

#[test]
fn stats_response_prefers_detailed_analysis_cache_when_unit_data_is_enabled() {
    let root = unique_temp_path("stats_source");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let replay_path = root.join("example.SC2Replay");
    let cache_path = root.join("cache_overall_stats.json");
    std::fs::write(&replay_path, []).expect("replay file should be created");

    let payload = serde_json::to_vec(&vec![sample_cache_entry(&replay_path)])
        .expect("cache should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");

    let stale_replay = sample_replay(
        &replay_path.display().to_string(),
        player("Stale Main", "1-S2-1-111", "Dehaka").with_units(json!({
            "Primal Hydralisk": [1, 0, 1, 1.0]
        })),
        player("Stale Ally", "1-S2-1-222", "Abathur").with_units(json!({})),
    );

    let stale_replays = [stale_replay];
    let replays = stats_replays_for_response_from_path_with_identity(
        true,
        &stale_replays,
        &cache_path,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].main_commander(), "Raynor");
    assert_eq!(replays[0].ally_commander(), "Karax");
    assert_eq!(replays[0].main_units()["Marine"], json!([8, 2, 99, 0.75]));
    assert!(replays[0].main_units().get("Primal Hydralisk").is_none());

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn stats_replays_for_response_prefers_in_memory_stats_cache() {
    let resident_replay = sample_replay(
        "fixtures/replays/resident.SC2Replay",
        player("Resident Main", "1-S2-1-111", "Fenix").with_units(json!({
            "Adept": [6, 1, 23, 0.5]
        })),
        player("Resident Ally", "1-S2-1-222", "Karax").with_units(json!({})),
    );

    let resident_replays = [resident_replay];
    let replays = ReplayAnalysis::stats_replays_for_response(true, &resident_replays);

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].main_commander(), "Fenix");
    assert_eq!(replays[0].main_units()["Adept"], json!([6, 1, 23, 0.5]));
}

#[test]
fn merge_cached_detailed_replays_replaces_matching_simple_entries() {
    let root = unique_temp_path("merge_detailed_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let replay_path = root.join("example.SC2Replay");
    let cache_path = root.join("cache_overall_stats");
    std::fs::write(&replay_path, []).expect("replay file should be created");

    let payload = serde_json::to_vec(&vec![sample_cache_entry(&replay_path)])
        .expect("cache should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");

    let simple_replay = sample_replay(
        &replay_path.display().to_string(),
        player("Simple Main", "", "Artanis").with_units(json!({})),
        player("Simple Ally", "", "Swann").with_units(json!({})),
    );

    let merged = merge_cached_detailed_replays_from_path(
        &[simple_replay],
        &cache_path,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].main_commander(), "Raynor");
    assert_eq!(merged[0].ally_commander(), "Karax");
    assert_eq!(merged[0].main_units()["Marine"], json!([8, 2, 99, 0.75]));

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn stats_source_replays_for_response_matches_wx_show_all_behavior() {
    let mut current_replay = ReplayInfo::default();
    current_replay.set_file("fixtures/replays/current.SC2Replay");
    let mut historic_replay = ReplayInfo::default();
    historic_replay.set_file("fixtures/replays/historic.SC2Replay");
    let current_files = HashSet::from([current_replay.file().to_string()]);

    let selected_replays = [current_replay.clone(), historic_replay.clone()];
    let current_only = ReplayAnalysis::stats_source_replays_for_response(
        "/config/stats?show_all=0",
        &selected_replays,
        &current_files,
    );
    assert_eq!(current_only.len(), 1);
    assert_eq!(current_only[0].file(), current_replay.file());

    let all_replays = [current_replay, historic_replay];
    let show_all = ReplayAnalysis::stats_source_replays_for_response(
        "/config/stats?show_all=1",
        &all_replays,
        &current_files,
    );
    assert_eq!(show_all.len(), 2);
}

#[test]
fn detailed_stats_counts_only_replays_with_unit_payloads() {
    let detailed_replay = sample_replay(
        "fixtures/replays/detailed.SC2Replay",
        ReplayPlayerInfo::default().with_units(json!({
            "Marine": [6, 1, 9, 0.5]
        })),
        ReplayPlayerInfo::default(),
    );
    let mut simple_replay = ReplayInfo::default();
    simple_replay.set_file("fixtures/replays/simple.SC2Replay");
    let mut amon_only_replay = ReplayInfo::default();
    amon_only_replay.set_file("fixtures/replays/amon.SC2Replay");
    amon_only_replay.set_amon_units(json!({
        "Zergling": [20, 20, 3, 0.1]
    }));

    let filtered_replays = vec![&detailed_replay, &simple_replay, &amon_only_replay];
    let (detailed_parsed_count, total_valid_files) =
        ReplayAnalysis::detailed_stats_counts(&filtered_replays);

    assert_eq!(detailed_parsed_count, 2);
    assert_eq!(total_valid_files, 3);
}
