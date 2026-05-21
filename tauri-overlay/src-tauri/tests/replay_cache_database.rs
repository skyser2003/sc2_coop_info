use rusqlite::Connection;
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheNumericValue, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
};
use s2coop_analyzer::detailed_replay_analysis::CacheEntrySink;
use sco_tauri_overlay::{
    ReplayCacheDatabase, ReplayCacheDbError, ReplayCacheEntryQuery, SqliteReplayCacheEntrySink,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_path(label: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sco_{label}_{timestamp}"))
}

fn sample_cache_entry(
    file: &str,
    hash: &str,
    date: &str,
    detailed_analysis: bool,
    result: &str,
) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Integer(600),
        amon_units: None,
        bonus: None,
        brutal_plus: 0,
        build: ReplayBuildInfo::new(1, ProtocolBuildValue::Int(1)),
        comp: Some("Terran".to_string()),
        date: date.to_string(),
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: "Brutal".to_string(),
        extension: false,
        file: file.to_string(),
        form_alength: "10:00".to_string(),
        detailed_analysis,
        hash: hash.to_string(),
        length: 600,
        map_name: "Void Launch".to_string(),
        messages: Vec::new(),
        mutators: Vec::new(),
        player_stats: None,
        players: Vec::new(),
        region: "NA".to_string(),
        result: result.to_string(),
        weekly: false,
    }
}

fn write_legacy_cache(cache_path: &Path, entries: &[CacheReplayEntry]) {
    let payload = serde_json::to_vec(entries).expect("legacy cache should serialize");
    std::fs::write(cache_path, payload).expect("legacy cache should be written");
}

#[test]
fn database_related_paths_include_sqlite_sidecars() {
    let cache_path = PathBuf::from("generated").join("cache_overall_stats.json");
    let paths = ReplayCacheDatabase::db_related_paths_for_cache_path(&cache_path);

    assert_eq!(paths.len(), 4);
    assert_eq!(paths[0], cache_path.with_extension("sqlite3"));
    assert_eq!(
        paths[1].file_name().and_then(|value| value.to_str()),
        Some("cache_overall_stats.sqlite3-wal")
    );
    assert_eq!(
        paths[2].file_name().and_then(|value| value.to_str()),
        Some("cache_overall_stats.sqlite3-shm")
    );
    assert_eq!(
        paths[3].file_name().and_then(|value| value.to_str()),
        Some("cache_overall_stats.sqlite3-journal")
    );
}

#[test]
fn sqlite_cache_entry_sink_writes_entries_to_database() {
    let root = unique_temp_path("replay_cache_db_sink");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let entry = sample_cache_entry(
        "sink.SC2Replay",
        "sink-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let sink = SqliteReplayCacheEntrySink::new(cache_path.clone());

    let changed = sink
        .write_entries(std::slice::from_ref(&entry))
        .expect("sink should write entry");

    assert_eq!(changed, 1);
    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    let persisted = database
        .load_entry_by_hash("sink-hash")
        .expect("entry query should succeed")
        .expect("sink entry should persist");
    assert_eq!(persisted.file, entry.file);
    assert!(persisted.detailed_analysis);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn imports_legacy_cache_file_into_sqlite_database() {
    let root = unique_temp_path("replay_cache_db_import");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let older = sample_cache_entry(
        "older.SC2Replay",
        "older-hash",
        "2025-01-01 00:00:00",
        true,
        "Defeat",
    );
    let newer = sample_cache_entry(
        "newer.SC2Replay",
        "newer-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    write_legacy_cache(&cache_path, &[older, newer]);

    let database = ReplayCacheDatabase::open_for_cache_path(&cache_path)
        .expect("database should import legacy cache");
    let entries = database
        .load_entries(ReplayCacheEntryQuery::all(0))
        .expect("entries should load from sqlite");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].hash, "newer-hash");
    assert_eq!(entries[1].hash, "older-hash");
    let cached_files = database
        .load_cached_files()
        .expect("cached file set should load");
    assert!(cached_files.contains("older.SC2Replay"));
    assert!(cached_files.contains("newer.SC2Replay"));
    assert!(ReplayCacheDatabase::db_path_for_cache_path(&cache_path).exists());
    assert!(
        !cache_path.exists(),
        "legacy cache JSON should be deleted after successful SQLite import"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn preserving_upsert_keeps_existing_detailed_entry_over_simple_entry() {
    let root = unique_temp_path("replay_cache_db_preserve");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let replay_file = root
        .join("persisted.SC2Replay")
        .to_string_lossy()
        .to_string();
    let detailed = sample_cache_entry(
        &replay_file,
        "same-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let simple = sample_cache_entry(
        &replay_file,
        "same-hash",
        "2026-01-02 00:00:00",
        false,
        "Defeat",
    );

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&detailed))
        .expect("detailed entry should insert");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&simple))
        .expect("simple entry should not replace detailed entry");
    let persisted = database
        .load_entry_by_hash("same-hash")
        .expect("entry should load")
        .expect("entry should exist");

    assert!(persisted.detailed_analysis);
    assert_eq!(persisted.result, "Victory");
    assert_eq!(persisted.date, "2026-01-01 00:00:00");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn opening_future_schema_version_returns_typed_error() {
    let root = unique_temp_path("replay_cache_db_future_schema");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let connection = Connection::open(&db_path).expect("sqlite file should be created");
    connection
        .pragma_update(None, "user_version", 99i32)
        .expect("user_version should update");
    drop(connection);

    match ReplayCacheDatabase::open_for_cache_path(&cache_path) {
        Err(ReplayCacheDbError::UnsupportedSchema {
            version, supported, ..
        }) => {
            assert_eq!(version, 99);
            assert_eq!(supported, 1);
        }
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("future schema should not open"),
    }

    let _ = std::fs::remove_dir_all(&root);
}
