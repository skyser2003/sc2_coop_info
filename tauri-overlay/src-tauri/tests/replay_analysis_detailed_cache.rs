use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CachePlayer, CacheReplayEntry, CacheUnitStats,
    ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::canonicalize_coop_map_id;
use sco_tauri_overlay::replay_analysis::{bonus_objective_total_for_map_id, ReplayAnalysis};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
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

    let replays = ReplayAnalysis::load_detailed_analysis_replays_snapshot_from_path(
        &cache_path,
        0,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].file, replay_path.display().to_string());
    assert_eq!(
        replays[0].map,
        canonicalize_coop_map_id("Void Launch").expect("map id should resolve")
    );
    assert_eq!(replays[0].difficulty, "Brutal");
    assert_eq!(replays[0].length, 610);
    assert_eq!(replays[0].main_commander, "Raynor");
    assert_eq!(replays[0].ally_commander, "Karax");
    assert_eq!(replays[0].main_units["Marine"], json!([3, 1, 12, 0.48]));
    assert_eq!(replays[0].ally_units["Zealot"], json!([2, 0, 7, 0.28]));
    assert_eq!(replays[0].bonus, vec![1, 1]);
    assert_eq!(
        replays[0].bonus_total,
        bonus_objective_total_for_map_id(
            &canonicalize_coop_map_id("Void Launch").expect("map id should resolve")
        )
    );

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_file(&replay_path);
    let _ = std::fs::remove_file(&ignored_replay_path);
    let _ = std::fs::remove_dir(&root);
}
