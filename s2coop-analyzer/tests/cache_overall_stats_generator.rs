use s2coop_analyzer::cache_overall_stats_generator::{
    CacheNumericValue, CachePlayer, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
    ReplayMessage,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_build_info() -> ReplayBuildInfo {
    ReplayBuildInfo::new(12345, ProtocolBuildValue::Int(12345))
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
fn load_existing_detailed_cache_entries_only_keep_detailed_entries() {
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

    let loaded_cache = CacheReplayEntry::load_existing_detailed_cache_entries(&cache_path, None);
    assert_eq!(loaded_cache.len(), 1);
    assert!(loaded_cache.contains_key("reuse-hash"));
    assert!(!loaded_cache.contains_key("pending-hash"));

    let _ = fs::remove_file(&cache_path);
    let _ = fs::remove_dir_all(&cache_dir);
}

#[test]
fn persist_simple_cache_entries_preserve_existing_simple_entries() {
    let cache_path = unique_temp_path("cache_overall_stats.json");
    let cache_dir = cache_path
        .parent()
        .expect("temp cache path should have a parent")
        .to_path_buf();
    fs::create_dir_all(&cache_dir).expect("failed to create temp cache dir");

    let existing_detailed = sample_cached_entry("detailed-hash", "detailed.SC2Replay", true);
    let existing_simple = sample_cached_entry("simple-existing", "existing.SC2Replay", false);
    let payload = serde_json::to_vec(&vec![existing_detailed.clone(), existing_simple.clone()])
        .expect("failed to serialize existing cache entries");
    fs::write(&cache_path, payload).expect("failed to write cache file");

    let new_simple = sample_cached_entry("simple-new", "new.SC2Replay", false);
    CacheReplayEntry::persist_simple_cache_entries(std::slice::from_ref(&new_simple), &cache_path)
        .expect("simple cache persistence should succeed");

    let persisted_payload = fs::read(&cache_path).expect("cache file should exist");
    let persisted_entries = serde_json::from_slice::<Vec<CacheReplayEntry>>(&persisted_payload)
        .expect("persisted cache should deserialize");

    assert_eq!(persisted_entries.len(), 3);
    assert!(
        persisted_entries
            .iter()
            .any(|entry| entry.hash == existing_detailed.hash && entry.detailed_analysis)
    );
    assert!(
        persisted_entries
            .iter()
            .any(|entry| entry.hash == existing_simple.hash && !entry.detailed_analysis)
    );
    assert!(
        persisted_entries
            .iter()
            .any(|entry| entry.hash == new_simple.hash && !entry.detailed_analysis)
    );

    let _ = fs::remove_file(&cache_path);
    let _ = fs::remove_dir_all(&cache_dir);
}

#[test]
fn serialize_entries_preserves_input_order_after_parallel_canonicalization() {
    let entries = vec![
        sample_cached_entry("hash-03", "third.SC2Replay", true),
        sample_cached_entry("hash-01", "first.SC2Replay", true),
        sample_cached_entry("hash-02", "second.SC2Replay", false),
    ];

    let payload = CacheReplayEntry::serialize_entries(&entries).expect("entries should serialize");
    let serialized =
        serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).expect("entries should parse");
    let hashes = serialized
        .iter()
        .map(|entry| entry.hash.as_str())
        .collect::<Vec<&str>>();

    assert_eq!(hashes, vec!["hash-03", "hash-01", "hash-02"]);
}
