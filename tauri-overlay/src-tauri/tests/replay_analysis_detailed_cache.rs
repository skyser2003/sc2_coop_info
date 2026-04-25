use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CacheOverallStatsFile, CachePlayer, CacheReplayEntry,
    CacheUnitStats, ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::test_helper::TestHelperOps;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_map_id(raw: &str) -> String {
    TestHelperOps::canonicalize_map_id(raw).expect("map id should resolve")
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
    for (unit, stats) in units {
        unit_map.insert((*unit).to_string(), stats.clone());
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

fn sample_cache_entry(file: &Path, detailed_analysis: bool) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Float(610.25),
        amon_units: None,
        bonus: Some(vec!["First".to_string(), "Second".to_string()]),
        brutal_plus: 0,
        build: ReplayBuildInfo::new(1, ProtocolBuildValue::Int(1)),
        comp: Some("Terran".to_string()),
        date: "2026-03-09 12:00:00".to_string(),
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: "Brutal".to_string(),
        extension: false,
        file: file.display().to_string(),
        form_alength: "10:10".to_string(),
        detailed_analysis,
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
                "Player One",
                "1-S2-1-111",
                "Raynor",
                &[(
                    "Marine",
                    CacheUnitStats(
                        CacheCountValue::Count(3),
                        CacheCountValue::Count(1),
                        12,
                        0.48,
                    ),
                )],
            ),
            sample_cache_player(
                2,
                "Player Two",
                "1-S2-1-222",
                "Karax",
                &[(
                    "Zealot",
                    CacheUnitStats(
                        CacheCountValue::Count(2),
                        CacheCountValue::Count(0),
                        7,
                        0.28,
                    ),
                )],
            ),
        ],
        region: "NA".to_string(),
        result: "Victory".to_string(),
        weekly: false,
    }
}

#[test]
fn load_detailed_analysis_replays_snapshot_from_path_uses_cache_entries() {
    let root = unique_temp_path("full_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let replay_path = root.join("example.SC2Replay");
    let ignored_replay_path = root.join("ignored.SC2Replay");
    let cache_path = root.join("cache_overall_stats.json");

    std::fs::write(&replay_path, []).expect("replay file should be created");
    std::fs::write(&ignored_replay_path, []).expect("ignored replay file should be created");

    let entries = vec![
        sample_cache_entry(&replay_path, true),
        sample_cache_entry(&ignored_replay_path, false),
    ];
    let payload = serde_json::to_vec(&entries).expect("cache payload should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");

    let replays = TestHelperOps::load_detailed_analysis_replays_snapshot_from_path(&cache_path, 0);

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].file(), replay_path.display().to_string());
    assert_eq!(replays[0].map(), test_map_id("Void Launch"));
    assert_eq!(replays[0].difficulty(), "Brutal");
    assert_eq!(replays[0].length(), 610);
    assert_eq!(replays[0].main_commander(), "Raynor");
    assert_eq!(replays[0].ally_commander(), "Karax");
    assert_eq!(replays[0].main_units()["Marine"], json!([3, 1, 12, 0.48]));
    assert_eq!(replays[0].ally_units()["Zealot"], json!([2, 0, 7, 0.28]));
    assert_eq!(replays[0].bonus(), vec![1, 1]);
    assert_eq!(
        replays[0].bonus_total(),
        TestHelperOps::bonus_objective_total_for_map_id(&test_map_id("Void Launch"))
    );

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_file(&ignored_replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn load_detailed_analysis_replays_snapshot_from_path_recovers_temp_cache_entries() {
    let root = unique_temp_path("recover_temp_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let existing_replay_path = root.join("existing.SC2Replay");
    let recovered_replay_path = root.join("recovered.SC2Replay");
    let cache_path = root.join("cache_overall_stats.json");
    let temp_path = cache_path.with_extension("temp.jsonl");
    let pretty_path = CacheOverallStatsFile::pretty_output_path(&cache_path);

    std::fs::write(&existing_replay_path, []).expect("existing replay file should be created");
    std::fs::write(&recovered_replay_path, []).expect("recovered replay file should be created");

    let existing_entry = sample_cache_entry(&existing_replay_path, true);
    let recovered_entry = sample_cache_entry(&recovered_replay_path, true);

    let payload =
        serde_json::to_vec(&vec![existing_entry.clone()]).expect("cache payload should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");
    std::fs::write(
        &temp_path,
        format!(
            "{}\n",
            serde_json::to_string(&recovered_entry).expect("temp cache entry should serialize")
        ),
    )
    .expect("temp cache file should be written");

    let replays = TestHelperOps::load_detailed_analysis_replays_snapshot_from_path(&cache_path, 0);

    assert_eq!(replays.len(), 2);
    assert!(replays
        .iter()
        .any(|replay| replay.file() == existing_replay_path.display().to_string()));
    assert!(replays
        .iter()
        .any(|replay| replay.file() == recovered_replay_path.display().to_string()));
    assert!(
        !temp_path.exists(),
        "temp cache should be removed after recovery"
    );
    assert!(
        pretty_path.exists(),
        "pretty cache should be regenerated after recovery"
    );

    let persisted_payload = std::fs::read(&cache_path).expect("cache file should exist");
    let persisted_entries = serde_json::from_slice::<Vec<CacheReplayEntry>>(&persisted_payload)
        .expect("persisted cache should parse");
    assert_eq!(persisted_entries.len(), 2);
    assert!(persisted_entries
        .iter()
        .any(|entry| entry.file == existing_entry.file));
    assert!(persisted_entries
        .iter()
        .any(|entry| entry.file == recovered_entry.file));

    let _ = std::fs::remove_file(&pretty_path);
    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&existing_replay_path);
    let _ = std::fs::remove_file(&recovered_replay_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn load_detailed_analysis_replays_snapshot_from_path_persists_simple_temp_entry_to_cache_file() {
    let root = unique_temp_path("recover_simple_temp_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let existing_replay_path = root.join("existing.SC2Replay");
    let recovered_replay_path = root.join("recovered_simple.SC2Replay");
    let cache_path = root.join("cache_overall_stats.json");
    let temp_path = cache_path.with_extension("temp.jsonl");
    let pretty_path = CacheOverallStatsFile::pretty_output_path(&cache_path);

    std::fs::write(&existing_replay_path, []).expect("existing replay file should be created");
    std::fs::write(&recovered_replay_path, []).expect("recovered replay file should be created");

    let existing_entry = sample_cache_entry(&existing_replay_path, true);
    let recovered_entry = sample_cache_entry(&recovered_replay_path, false);

    let payload =
        serde_json::to_vec(&vec![existing_entry.clone()]).expect("cache payload should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");
    std::fs::write(
        &temp_path,
        format!(
            "{}\n",
            serde_json::to_string(&recovered_entry).expect("temp cache entry should serialize")
        ),
    )
    .expect("temp cache file should be written");

    let replays = TestHelperOps::load_detailed_analysis_replays_snapshot_from_path(&cache_path, 0);

    assert_eq!(replays.len(), 1);
    assert_eq!(
        replays[0].file(),
        existing_replay_path.display().to_string()
    );
    assert!(
        !temp_path.exists(),
        "temp cache should be removed after recovery"
    );
    assert!(
        pretty_path.exists(),
        "pretty cache should be regenerated after recovery"
    );

    let persisted_payload = std::fs::read(&cache_path).expect("cache file should exist");
    let persisted_entries = serde_json::from_slice::<Vec<CacheReplayEntry>>(&persisted_payload)
        .expect("persisted cache should parse");
    assert_eq!(persisted_entries.len(), 2);
    assert!(persisted_entries
        .iter()
        .any(|entry| entry.file == existing_entry.file && entry.detailed_analysis));
    assert!(persisted_entries
        .iter()
        .any(|entry| entry.file == recovered_entry.file && !entry.detailed_analysis));

    let _ = std::fs::remove_file(&pretty_path);
    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&existing_replay_path);
    let _ = std::fs::remove_file(&recovered_replay_path);
    let _ = std::fs::remove_dir(&root);
}
