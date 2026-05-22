use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CachePlayer, CacheReplayEntry, CacheUnitStats,
    ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::{ReplayAnalysis, ReplayAnalysisOps, ReplayCacheDatabase, TestHelperOps};
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo, StatsState, UNLIMITED_REPLAY_LIMIT};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_map_id(raw: &str) -> String {
    TestHelperOps::canonicalize_map_id(raw).expect("map id should resolve")
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

    let detailed_replays =
        TestHelperOps::load_detailed_analysis_replays_snapshot_from_path_with_identity(
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
fn sqlite_statistics_payload_uses_detailed_cache_unit_data() {
    let root = unique_temp_path("stats_source");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let replay_path = root.join("example.SC2Replay");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    std::fs::write(&replay_path, []).expect("replay file should be created");

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[sample_cache_entry(&replay_path)])
        .expect("cache entry should be written");
    drop(database);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let dictionary = TestHelperOps::load_dictionary();
    let main_handles = HashSet::from(["1-s2-1-111".to_string()]);
    let payload = database
        .load_statistics_payload(
            &sco_tauri_overlay::ReplayCacheStatsQuery::new(
                sco_tauri_overlay::ReplayCacheReadScope::DetailedOnly,
                0,
            ),
            &HashSet::new(),
            &main_handles,
            &dictionary,
        )
        .expect("statistics payload should load from sqlite");
    assert_eq!(payload.games(), 1);
    assert_eq!(
        payload.analysis()["UnitData"]["main"]["Raynor"]["Marine"]["created"],
        json!(8)
    );
    assert_eq!(
        payload.analysis()["UnitData"]["ally"]["Karax"]["Zealot"]["created"],
        json!(2)
    );
    assert!(payload.analysis()["UnitData"]["main"]["Dehaka"].is_null());

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn sqlite_statistics_payload_matches_rebuild_payload_for_same_cache_rows() {
    let root = unique_temp_path("stats_payload_match");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let replay_path = root.join("example.SC2Replay");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    std::fs::write(&replay_path, []).expect("replay file should be created");

    let entry = sample_cache_entry(&replay_path);
    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(std::slice::from_ref(&entry))
        .expect("cache entry should be written");

    let dictionary = TestHelperOps::load_dictionary();
    let main_names = HashSet::new();
    let main_handles = HashSet::from(["1-s2-1-111".to_string()]);
    let replay =
        ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(&entry, &dictionary)
            .oriented_for_main_identity(&main_names, &main_handles);
    let rebuild_payload = ReplayAnalysis::rebuild_analysis_payload_with_dictionary(
        &[replay],
        true,
        &main_names,
        &main_handles,
        &dictionary,
    );
    let database_payload = database
        .load_statistics_payload(
            &sco_tauri_overlay::ReplayCacheStatsQuery::new(
                sco_tauri_overlay::ReplayCacheReadScope::DetailedOnly,
                0,
            ),
            &main_names,
            &main_handles,
            &dictionary,
        )
        .expect("statistics payload should load from sqlite");

    assert_eq!(
        database_payload.analysis(),
        rebuild_payload
            .get("analysis")
            .expect("rebuild payload should contain analysis")
    );

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn merge_cached_detailed_replays_replaces_matching_simple_entries() {
    let root = unique_temp_path("merge_detailed_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let replay_path = root.join("example.SC2Replay");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    std::fs::write(&replay_path, []).expect("replay file should be created");

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[sample_cache_entry(&replay_path)])
        .expect("cache entry should be written");
    drop(database);

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
fn detailed_stats_counts_uses_cache_marker_without_unit_payloads() {
    let mut detailed_replay = sample_replay(
        "fixtures/replays/detailed.SC2Replay",
        ReplayPlayerInfo::default(),
        ReplayPlayerInfo::default(),
    );
    detailed_replay.set_is_detailed(true);
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

#[test]
fn stats_response_has_detailed_analysis_reads_unit_payload() {
    let response = json!({
        "analysis": {
            "UnitData": {
                "main": {}
            }
        }
    });

    assert!(ReplayAnalysis::stats_response_has_detailed_analysis(
        &response
    ));
}

#[test]
fn detailed_analysis_status_counts_cache_marker_without_unit_payloads() {
    let mut stats = StatsState::default();
    let mut detailed_replay = ReplayInfo::default();
    detailed_replay.set_file("fixtures/replays/detailed_marker.SC2Replay");
    detailed_replay.set_map(test_map_id("Void Launch"));
    detailed_replay.set_result("Victory");
    detailed_replay.set_is_detailed(true);
    let mut simple_replay = ReplayInfo::default();
    simple_replay.set_file("fixtures/replays/simple.SC2Replay");
    simple_replay.set_map(test_map_id("Void Launch"));
    simple_replay.set_result("Victory");

    stats.sync_detailed_analysis_status_from_replays(&[detailed_replay, simple_replay]);

    assert_eq!(
        stats.detailed_analysis_status(),
        "Detailed analysis: loaded from cache (1/2)."
    );
}
