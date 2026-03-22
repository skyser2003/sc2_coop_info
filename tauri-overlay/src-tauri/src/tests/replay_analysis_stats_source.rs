use super::*;
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CachePlayer, CacheReplayEntry, ProtocolBuildValue,
    ReplayBuildInfo,
};
use std::collections::{BTreeMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

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
        build: ReplayBuildInfo {
            replay_build: 1,
            protocol_build: ProtocolBuildValue::Int(1),
        },
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

    let detailed_replays = ReplayAnalysis::load_detailed_analysis_replays_snapshot_from_path(
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
        .map(|replay| (replay.file.clone(), replay))
        .collect();

    replays
        .iter()
        .map(|replay| {
            detailed_by_file
                .get(&replay.file)
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

    let stale_replay = ReplayInfo {
        file: replay_path.display().to_string(),
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        difficulty: "Brutal".to_string(),
        p1: "Stale Main".to_string(),
        p2: "Stale Ally".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-222".to_string(),
        main_commander: "Dehaka".to_string(),
        ally_commander: "Abathur".to_string(),
        main_units: json!({
            "Primal Hydralisk": [1, 0, 1, 1.0]
        }),
        ally_units: json!({}),
        ..ReplayInfo::default()
    };

    let stale_replays = [stale_replay];
    let replays = ReplayAnalysis::stats_replays_for_response_from_path(
        true,
        &stale_replays,
        &cache_path,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].main_commander, "Raynor");
    assert_eq!(replays[0].ally_commander, "Karax");
    assert_eq!(replays[0].main_units["Marine"], json!([8, 2, 99, 0.75]));
    assert!(replays[0].main_units.get("Primal Hydralisk").is_none());

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn stats_replays_for_response_prefers_in_memory_stats_cache() {
    let resident_replay = ReplayInfo {
        file: "fixtures/replays/resident.SC2Replay".to_string(),
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        difficulty: "Brutal".to_string(),
        p1: "Resident Main".to_string(),
        p2: "Resident Ally".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-222".to_string(),
        main_commander: "Fenix".to_string(),
        ally_commander: "Karax".to_string(),
        main_units: json!({
            "Adept": [6, 1, 23, 0.5]
        }),
        ally_units: json!({}),
        ..ReplayInfo::default()
    };

    let resident_replays = [resident_replay];
    let replays = ReplayAnalysis::stats_replays_for_response(true, &resident_replays);

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].main_commander, "Fenix");
    assert_eq!(replays[0].main_units["Adept"], json!([6, 1, 23, 0.5]));
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

    let simple_replay = ReplayInfo {
        file: replay_path.display().to_string(),
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        difficulty: "Brutal".to_string(),
        p1: "Simple Main".to_string(),
        p2: "Simple Ally".to_string(),
        main_commander: "Artanis".to_string(),
        ally_commander: "Swann".to_string(),
        main_units: json!({}),
        ally_units: json!({}),
        ..ReplayInfo::default()
    };

    let merged = merge_cached_detailed_replays_from_path(
        &[simple_replay],
        &cache_path,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].main_commander, "Raynor");
    assert_eq!(merged[0].ally_commander, "Karax");
    assert_eq!(merged[0].main_units["Marine"], json!([8, 2, 99, 0.75]));

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn stats_source_replays_for_response_matches_wx_show_all_behavior() {
    let current_replay = ReplayInfo {
        file: "fixtures/replays/current.SC2Replay".to_string(),
        ..ReplayInfo::default()
    };
    let historic_replay = ReplayInfo {
        file: "fixtures/replays/historic.SC2Replay".to_string(),
        ..ReplayInfo::default()
    };
    let current_files = HashSet::from([current_replay.file.clone()]);

    let selected_replays = [current_replay.clone(), historic_replay.clone()];
    let current_only = ReplayAnalysis::stats_source_replays_for_response(
        "/config/stats?show_all=0",
        &selected_replays,
        &current_files,
    );
    assert_eq!(current_only.len(), 1);
    assert_eq!(current_only[0].file, current_replay.file);

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
    let detailed_replay = ReplayInfo {
        file: "fixtures/replays/detailed.SC2Replay".to_string(),
        main_units: json!({
            "Marine": [6, 1, 9, 0.5]
        }),
        ..ReplayInfo::default()
    };
    let simple_replay = ReplayInfo {
        file: "fixtures/replays/simple.SC2Replay".to_string(),
        ..ReplayInfo::default()
    };
    let amon_only_replay = ReplayInfo {
        file: "fixtures/replays/amon.SC2Replay".to_string(),
        amon_units: json!({
            "Zergling": [20, 20, 3, 0.1]
        }),
        ..ReplayInfo::default()
    };

    let filtered_replays = vec![&detailed_replay, &simple_replay, &amon_only_replay];
    let (detailed_parsed_count, total_valid_files) =
        ReplayAnalysis::detailed_stats_counts(&filtered_replays);

    assert_eq!(detailed_parsed_count, 2);
    assert_eq!(total_valid_files, 3);
}
