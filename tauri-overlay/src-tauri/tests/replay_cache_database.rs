use chrono::{Local, LocalResult, TimeZone, Utc};
use rusqlite::{Connection, params};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheIconValue, CacheNumericValue, CachePlayer, CachePlayerStatsSeries,
    CacheReplayEntry, CacheStatValue, CacheUnitStats, ProtocolBuildValue, ReplayBuildInfo,
};
use s2coop_analyzer::detailed_replay_analysis::CacheEntrySink;
use sco_tauri_overlay::{
    ReplayCacheDatabase, ReplayCacheDbError, ReplayCacheEntryQuery, SqliteReplayCacheEntrySink,
};
use std::collections::BTreeMap;
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

fn sample_player(pid: u8, name: &str) -> CachePlayer {
    CachePlayer {
        pid,
        apm: Some(120),
        commander: Some("Raynor".to_string()),
        commander_level: Some(15),
        commander_mastery_level: Some(90),
        handle: Some(format!("1-S2-1-{pid}")),
        icons: None,
        kills: Some(10),
        masteries: None,
        name: Some(name.to_string()),
        observer: None,
        prestige: Some(0),
        prestige_name: Some("P0".to_string()),
        race: Some("Terran".to_string()),
        result: Some("Victory".to_string()),
        units: None,
    }
}

fn sample_player_stats(name: &str) -> CachePlayerStatsSeries {
    CachePlayerStatsSeries {
        name: name.to_string(),
        supply: vec![1.0, 2.5, 3.0],
        mining: vec![4.0, 5.5],
        army: vec![CacheStatValue::Integer(6), CacheStatValue::Float(7.25)],
        killed: vec![8, 9, 10],
    }
}

fn sqlite_table_columns(db_path: &Path, table_name: &str) -> Vec<String> {
    let connection = Connection::open(db_path).expect("sqlite database should open");
    let sql = format!("PRAGMA table_info({table_name})");
    let mut statement = connection
        .prepare(&sql)
        .expect("table info statement should prepare");
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info should query")
        .map(|row| row.expect("table column should load"))
        .collect()
}

fn sqlite_table_exists(db_path: &Path, table_name: &str) -> bool {
    let connection = Connection::open(db_path).expect("sqlite database should open");
    connection
        .query_row(
            "
            SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name = ?1
            )
            ",
            params![table_name],
            |row| row.get::<_, i64>(0),
        )
        .expect("table existence query should load")
        != 0
}

fn sqlite_table_row_count(db_path: &Path, table_name: &str) -> i64 {
    let connection = Connection::open(db_path).expect("sqlite database should open");
    let sql = format!("SELECT COUNT(*) FROM {table_name}");
    connection
        .query_row(&sql, [], |row| row.get::<_, i64>(0))
        .expect("table count should load")
}

fn sqlite_user_version(db_path: &Path) -> i32 {
    let connection = Connection::open(db_path).expect("sqlite database should open");
    connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
        .expect("user_version should load")
}

fn local_timestamp_text_as_utc(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> String {
    let local_datetime = match Local.with_ymd_and_hms(year, month, day, hour, minute, second) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(earliest, _) => earliest,
        LocalResult::None => panic!("test timestamp should be valid in local timezone"),
    };
    local_datetime
        .with_timezone(&Utc)
        .format("%Y:%m:%d:%H:%M:%S")
        .to_string()
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
fn sqlite_cache_schema_stores_typed_columns_without_payload_json() {
    let root = unique_temp_path("replay_cache_db_typed_schema");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "typed.SC2Replay",
        "typed-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    entry.accurate_length = CacheNumericValue::Float(450.25);
    entry.length = 720;
    entry.difficulty = ("Brutal".to_string(), "Hard".to_string());
    entry.mutators = vec!["Void Rifts".to_string(), "Avenger".to_string()];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&entry))
        .expect("entry should write");
    drop(database);

    let columns = sqlite_table_columns(&db_path, "replay_cache_entries");
    assert!(columns.contains(&"id".to_string()));
    assert!(columns.contains(&"hash".to_string()));
    assert!(columns.contains(&"difficulty_p1".to_string()));
    assert!(columns.contains(&"difficulty_p2".to_string()));
    assert!(columns.contains(&"length_ingame_seconds".to_string()));
    assert!(columns.contains(&"length_realtime_kind".to_string()));
    assert!(columns.contains(&"length_realtime_int".to_string()));
    assert!(columns.contains(&"length_realtime_float".to_string()));
    assert!(columns.contains(&"mutator_values".to_string()));
    assert!(!columns.contains(&"payload_json".to_string()));
    assert!(!columns.contains(&"difficulty_left".to_string()));
    assert!(!columns.contains(&"difficulty_right".to_string()));
    let player_columns = sqlite_table_columns(&db_path, "replay_cache_players");
    assert!(player_columns.contains(&"replay_id".to_string()));
    assert!(player_columns.contains(&"mastery_values".to_string()));
    assert!(!player_columns.contains(&"replay_hash".to_string()));
    let icon_columns = sqlite_table_columns(&db_path, "replay_cache_player_icons");
    assert!(!icon_columns.contains(&"order_values".to_string()));
    let icon_order_columns = sqlite_table_columns(&db_path, "replay_cache_player_icon_orders");
    assert!(icon_order_columns.contains(&"order_values".to_string()));
    assert!(columns.contains(&"bonus_values".to_string()));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_bonus"));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_metadata"));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_mutators"));
    assert!(!sqlite_table_exists(
        &db_path,
        "replay_cache_player_masteries"
    ));
    assert_eq!(sqlite_user_version(&db_path), 1);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let stored = connection
        .query_row(
            "
            SELECT difficulty_p1, difficulty_p2, length_ingame_seconds,
                length_realtime_kind, length_realtime_float, json_valid(mutator_values)
            FROM replay_cache_entries
            WHERE hash = ?1
            ",
            params!["typed-hash"],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .expect("typed row should load");
    assert_eq!(stored.0, "Brutal");
    assert_eq!(stored.1, "Hard");
    assert_eq!(stored.2, 720);
    assert_eq!(stored.3, "float");
    assert_eq!(stored.4, Some(450.25));
    assert_eq!(stored.5, 1);
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("typed-hash")
        .expect("entry should load")
        .expect("entry should exist");
    assert_eq!(loaded.mutators, entry.mutators);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_stores_player_masteries_and_icon_orders_as_json_arrays() {
    let root = unique_temp_path("replay_cache_db_player_json_arrays");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "player-arrays.SC2Replay",
        "player-arrays-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let mut player = sample_player(1, "Player One");
    player.masteries = Some([1, 2, 3, 4, 5, 6]);
    player.icons = Some(BTreeMap::from([
        (
            "Buildings Constructed".to_string(),
            CacheIconValue::Count(12),
        ),
        (
            "Top Units".to_string(),
            CacheIconValue::Order(vec!["Marine".to_string(), "Marauder".to_string()]),
        ),
    ]));
    entry.players = vec![player];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&entry))
        .expect("entry should write");
    drop(database);

    assert!(!sqlite_table_exists(
        &db_path,
        "replay_cache_player_masteries"
    ));
    assert!(sqlite_table_exists(
        &db_path,
        "replay_cache_player_icon_orders"
    ));
    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_player_icons"),
        2
    );
    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let mastery_row = connection
        .query_row(
            "
            SELECT json_array_length(mastery_values), json_extract(mastery_values, '$[5]')
            FROM replay_cache_players
            WHERE pid = ?1
            ",
            params![1_i64],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .expect("mastery json should load");
    assert_eq!(mastery_row, (6, 6));
    let order_row = connection
        .query_row(
            "
            SELECT json_array_length(order_values), json_extract(order_values, '$[1]')
            FROM replay_cache_player_icon_orders
            WHERE icon_name = ?1
            ",
            params!["Top Units"],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .expect("icon order json should load");
    assert_eq!(order_row, (2, "Marauder".to_string()));
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("player-arrays-hash")
        .expect("entry should load")
        .expect("entry should exist");
    assert_eq!(loaded.players.len(), 1);
    assert_eq!(loaded.players[0].masteries, Some([1, 2, 3, 4, 5, 6]));
    assert_eq!(
        loaded.players[0]
            .icons
            .as_ref()
            .and_then(|icons| icons.get("Top Units")),
        Some(&CacheIconValue::Order(vec![
            "Marine".to_string(),
            "Marauder".to_string()
        ]))
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_stores_bonus_as_json_array() {
    let root = unique_temp_path("replay_cache_db_bonus_json_array");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "bonus-array.SC2Replay",
        "bonus-array-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    entry.bonus = Some(vec![
        "Void Thrashing".to_string(),
        "Shuttle Launch".to_string(),
    ]);

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&entry))
        .expect("entry should write");
    drop(database);

    let columns = sqlite_table_columns(&db_path, "replay_cache_entries");
    assert!(columns.contains(&"bonus_values".to_string()));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_bonus"));
    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let bonus_row = connection
        .query_row(
            "
            SELECT json_array_length(bonus_values), json_extract(bonus_values, '$[1]')
            FROM replay_cache_entries
            WHERE id = ?1
            ",
            params![1_i64],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .expect("bonus json should load");
    assert_eq!(bonus_row, (2, "Shuttle Launch".to_string()));
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("bonus-array-hash")
        .expect("entry should load")
        .expect("entry should exist");
    assert_eq!(loaded.bonus, entry.bonus);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_reconstructs_hidden_player_unit_counts_without_hidden_text_columns() {
    let root = unique_temp_path("replay_cache_db_unit_hidden_schema");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "unit-hidden.SC2Replay",
        "unit-hidden-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let mut player = sample_player(1, "Player One");
    player.units = Some(BTreeMap::from([(
        "Karax's Top Bar".to_string(),
        CacheUnitStats(
            CacheCountValue::Hidden("-".to_string()),
            CacheCountValue::Hidden("-".to_string()),
            10,
            0.5,
        ),
    )]));
    entry.players = vec![player];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&entry))
        .expect("entry should write");
    drop(database);

    let columns = sqlite_table_columns(&db_path, "replay_cache_player_units");
    assert!(columns.contains(&"created_kind".to_string()));
    assert!(columns.contains(&"lost_kind".to_string()));
    assert!(!columns.contains(&"created_hidden".to_string()));
    assert!(!columns.contains(&"lost_hidden".to_string()));

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("unit-hidden-hash")
        .expect("entry should load")
        .expect("entry should exist");
    let loaded_units = loaded.players[0]
        .units
        .as_ref()
        .expect("player units should load");
    assert_eq!(
        loaded_units["Karax's Top Bar"].0,
        CacheCountValue::Hidden("-".to_string())
    );
    assert_eq!(
        loaded_units["Karax's Top Bar"].1,
        CacheCountValue::Hidden("-".to_string())
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_does_not_persist_dummy_pid_zero_players() {
    let root = unique_temp_path("replay_cache_db_no_dummy_pid");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "dummy.SC2Replay",
        "dummy-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    entry.players = vec![sample_player(0, "Dummy"), sample_player(1, "Player One")];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&entry))
        .expect("entry should write");
    drop(database);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let dummy_count = connection
        .query_row(
            "SELECT COUNT(*) FROM replay_cache_players WHERE pid = 0",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("dummy count should load");
    let real_count = connection
        .query_row(
            "SELECT COUNT(*) FROM replay_cache_players WHERE pid = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("player count should load");
    assert_eq!(dummy_count, 0);
    assert_eq!(real_count, 1);
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("dummy-hash")
        .expect("entry should load")
        .expect("entry should exist");
    assert_eq!(loaded.players.len(), 1);
    assert_eq!(loaded.players[0].pid, 1);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_stores_player_stats_as_json_arrays() {
    let root = unique_temp_path("replay_cache_db_stat_arrays");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "stat-arrays.SC2Replay",
        "stat-arrays-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    entry.player_stats = Some(BTreeMap::from([
        (0, sample_player_stats("Dummy")),
        (1, sample_player_stats("Player One")),
    ]));

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&entry))
        .expect("entry should write");
    drop(database);

    assert!(!sqlite_table_exists(
        &db_path,
        "replay_cache_player_stat_points"
    ));
    let columns = sqlite_table_columns(&db_path, "replay_cache_player_stat_series");
    assert!(columns.contains(&"replay_id".to_string()));
    assert!(!columns.contains(&"replay_hash".to_string()));
    assert!(columns.contains(&"supply_values".to_string()));
    assert!(columns.contains(&"mining_values".to_string()));
    assert!(columns.contains(&"army_values".to_string()));
    assert!(columns.contains(&"killed_values".to_string()));

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let stat_row = connection
        .query_row(
            "
            SELECT COUNT(*), MIN(pid), SUM(json_valid(supply_values)), SUM(json_valid(army_values))
            FROM replay_cache_player_stat_series
            ",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .expect("stat arrays should load");
    assert_eq!(stat_row.0, 1);
    assert_eq!(stat_row.1, 1);
    assert_eq!(stat_row.2, 1);
    assert_eq!(stat_row.3, 1);
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("stat-arrays-hash")
        .expect("entry should load")
        .expect("entry should exist");
    let loaded_stats = loaded.player_stats.expect("player stats should load");
    assert_eq!(loaded_stats.len(), 1);
    assert_eq!(loaded_stats[&1], sample_player_stats("Player One"));

    let _ = std::fs::remove_dir_all(&root);
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
fn legacy_cache_import_saves_replay_dates_as_utc() {
    let root = unique_temp_path("replay_cache_db_import_utc_date");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let entry = sample_cache_entry(
        "legacy-local-date.SC2Replay",
        "legacy-local-date-hash",
        "2026:01:01:00:00:00",
        true,
        "Victory",
    );
    write_legacy_cache(&cache_path, std::slice::from_ref(&entry));

    let database = ReplayCacheDatabase::open_for_cache_path(&cache_path)
        .expect("database should import legacy cache");
    let loaded = database
        .load_entry_by_hash("legacy-local-date-hash")
        .expect("entry should load")
        .expect("entry should exist");

    assert_eq!(
        loaded.date,
        local_timestamp_text_as_utc(2026, 1, 1, 0, 0, 0)
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
