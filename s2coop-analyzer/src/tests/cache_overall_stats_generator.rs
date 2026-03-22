use super::{
    load_existing_detailed_analysis_cache, partition_cached_candidates, CacheNumericValue,
    CachePlayer, CacheReplayEntry, CandidateReplay, ParsedCacheReplay, ProtocolBuildValue,
    ReplayBuildInfo, ReplayMessage,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_build_info() -> ReplayBuildInfo {
    ReplayBuildInfo {
        replay_build: 12345,
        protocol_build: ProtocolBuildValue::Int(12345),
    }
}

fn sample_player(pid: u8) -> CachePlayer {
    CachePlayer {
        pid,
        apm: None,
        commander: Some(format!("Commander {pid}")),
        commander_level: None,
        commander_mastery_level: None,
        handle: Some(format!("{pid}-S2-1-2-3")),
        icons: None,
        kills: None,
        masteries: None,
        name: Some(format!("Player {pid}")),
        observer: None,
        prestige: None,
        prestige_name: None,
        race: Some("Terran".to_string()),
        result: Some("Victory".to_string()),
        units: None,
    }
}

fn sample_cached_entry(hash: &str, file: &str, detailed_analysis: bool) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Float(1750.0),
        amon_units: None,
        bonus: None,
        brutal_plus: 0,
        build: sample_build_info(),
        comp: if detailed_analysis {
            Some("Commander 1, Commander 2".to_string())
        } else {
            None
        },
        date: "2024-01-01 00:00:00".to_string(),
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: "Brutal".to_string(),
        extension: false,
        file: file.to_string(),
        form_alength: "20:50".to_string(),
        detailed_analysis,
        hash: hash.to_string(),
        length: 1250,
        map_name: "Dead of Night".to_string(),
        messages: vec![ReplayMessage {
            text: "gl hf".to_string(),
            player: 1,
            time: 1.0,
        }],
        mutators: vec!["Alien Incubation".to_string()],
        player_stats: None,
        players: vec![sample_player(1), sample_player(2)],
        region: "NA".to_string(),
        result: "Victory".to_string(),
        weekly: false,
    }
}

fn sample_candidate(hash: &str, file: &str) -> CandidateReplay {
    CandidateReplay {
        path: PathBuf::from(file),
        basic: ParsedCacheReplay {
            accurate_length: 1750.0,
            accurate_length_force_float: true,
            brutal_plus: 0,
            build: sample_build_info(),
            date: "2024-01-01 00:00:00".to_string(),
            difficulty: ("Brutal".to_string(), "Brutal".to_string()),
            enemy_race: Some("Zerg".to_string()),
            ext_difficulty: "Brutal".to_string(),
            extension: false,
            file: file.to_string(),
            form_alength: "20:50".to_string(),
            length: 1250,
            map_name: "Dead of Night".to_string(),
            messages: Vec::new(),
            mutators: Vec::new(),
            players: vec![sample_player(1), sample_player(2)],
            region: "NA".to_string(),
            result: "Victory".to_string(),
            weekly: false,
            hash: hash.to_string(),
        },
    }
}

fn unique_temp_path(file_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "s2coop-analyzer-cache-reuse-{}-{nanos}",
            std::process::id()
        ))
        .join(file_name)
}

#[test]
fn existing_detailed_analysis_cache_reuses_matching_hashes_only() {
    let cache_path = unique_temp_path("cache_overall_stats.json");
    let cache_dir = cache_path
        .parent()
        .expect("temp cache path should have a parent")
        .to_path_buf();
    fs::create_dir_all(&cache_dir).expect("failed to create temp cache dir");

    let cache_entries = vec![
        sample_cached_entry("reuse-hash", "old-path.SC2Replay", true),
        sample_cached_entry("pending-hash", "basic-path.SC2Replay", false),
    ];
    let payload =
        serde_json::to_vec(&cache_entries).expect("failed to serialize temp cache entries");
    fs::write(&cache_path, payload).expect("failed to write temp cache file");

    let loaded_cache = load_existing_detailed_analysis_cache(&cache_path, None);
    assert_eq!(loaded_cache.len(), 1);
    assert!(loaded_cache.contains_key("reuse-hash"));
    assert!(!loaded_cache.contains_key("pending-hash"));

    let (reused_entries, pending_candidates) = partition_cached_candidates(
        vec![
            sample_candidate("reuse-hash", "current-path.SC2Replay"),
            sample_candidate("pending-hash", "needs-analysis.SC2Replay"),
        ],
        &loaded_cache,
    );

    assert_eq!(reused_entries.len(), 1);
    assert_eq!(pending_candidates.len(), 1);
    assert_eq!(reused_entries[0].hash, "reuse-hash");
    assert_eq!(reused_entries[0].file, "current-path.SC2Replay");
    assert!(reused_entries[0].detailed_analysis);
    assert_eq!(pending_candidates[0].basic.hash, "pending-hash");
    assert_eq!(
        pending_candidates[0].path,
        PathBuf::from("needs-analysis.SC2Replay")
    );

    let _ = fs::remove_file(&cache_path);
    let _ = fs::remove_dir_all(&cache_dir);
}
