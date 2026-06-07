use chrono::{Local, LocalResult, TimeZone, Utc};
use rusqlite::{Connection, params};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheIconValue, CacheNumericValue, CachePlayer, CachePlayerStatsSeries,
    CacheReplayEntry, CacheStatValue, CacheUnitStats, ProtocolBuildValue, ReplayBuildInfo,
    ReplayMessage,
};
use s2coop_analyzer::detailed_replay_analysis::{CacheEntrySink, CacheReplayCheck};
use sco_tauri_overlay::{
    PathManagerOps, QueuedReplayCacheEntrySink, ReplayCacheDatabase, ReplayCacheDbError,
    ReplayCacheDifficultyFilter, ReplayCacheEntryQuery, ReplayCacheGameSortKey,
    ReplayCacheGamesPageQuery, ReplayCachePage, ReplayCachePlayerNote, ReplayCachePlayerSortKey,
    ReplayCachePlayersPageQuery, ReplayCacheReadScope, ReplayCacheSortDirection,
    ReplayCacheStatsDifficultyExclusion, ReplayCacheStatsQuery, ReplayCacheWriteQueue,
    SqliteReplayCacheEntrySink,
};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

mod replay_cache_database_query_tests;
mod replay_cache_database_stats_tests;

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

fn sqlite_index_names(db_path: &Path, table_name: &str) -> Vec<String> {
    let connection = Connection::open(db_path).expect("sqlite database should open");
    let sql = format!("PRAGMA index_list({table_name})");
    let mut statement = connection
        .prepare(&sql)
        .expect("index list statement should prepare");
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("index list should query")
        .map(|row| row.expect("index name should load"))
        .collect()
}

fn sqlite_index_columns(db_path: &Path, index_name: &str) -> Vec<String> {
    let connection = Connection::open(db_path).expect("sqlite database should open");
    let sql = format!("PRAGMA index_info({index_name})");
    let mut statement = connection
        .prepare(&sql)
        .expect("index info statement should prepare");
    statement
        .query_map([], |row| row.get::<_, String>(2))
        .expect("index info should query")
        .map(|row| row.expect("index column should load"))
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

fn utc_seconds(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> u64 {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .expect("test datetime should be valid")
        .timestamp()
        .try_into()
        .expect("test datetime should be positive")
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
fn path_manager_uses_sqlite_as_primary_cache_file() {
    assert_eq!(
        PathManagerOps::get_cache_path()
            .extension()
            .and_then(|value| value.to_str()),
        Some("sqlite3")
    );
    assert_eq!(
        PathManagerOps::get_cache_db_path(),
        PathManagerOps::get_cache_path()
    );
    assert_eq!(
        PathManagerOps::get_legacy_cache_json_path()
            .extension()
            .and_then(|value| value.to_str()),
        Some("json")
    );
}

#[test]
fn database_related_paths_include_sqlite_sidecars() {
    let cache_path = PathBuf::from("generated").join("cache_overall_stats.sqlite3");
    let paths = ReplayCacheDatabase::db_related_paths_for_cache_path(&cache_path);

    assert_eq!(paths.len(), 4);
    assert_eq!(paths[0], cache_path);
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

    let legacy_path = PathBuf::from("generated").join("cache_overall_stats.json");
    let legacy_paths = ReplayCacheDatabase::db_related_paths_for_cache_path(&legacy_path);
    assert_eq!(legacy_paths[0], legacy_path.with_extension("sqlite3"));
}

#[test]
fn sqlite_cache_schema_stores_typed_columns_without_payload_json() {
    let root = unique_temp_path("replay_cache_db_typed_schema");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
    assert!(!columns.contains(&"detailed_analysis_attempted".to_string()));
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
    assert!(player_columns.contains(&"player_handle".to_string()));
    assert!(player_columns.contains(&"mastery_values".to_string()));
    assert!(!player_columns.contains(&"handle".to_string()));
    assert!(!player_columns.contains(&"name".to_string()));
    assert!(!player_columns.contains(&"replay_hash".to_string()));
    let player_info_columns = sqlite_table_columns(&db_path, "replay_player_infos");
    assert!(player_info_columns.contains(&"handle".to_string()));
    assert!(player_info_columns.contains(&"wins".to_string()));
    assert!(player_info_columns.contains(&"losses".to_string()));
    assert!(player_info_columns.contains(&"average_apm".to_string()));
    assert!(player_info_columns.contains(&"latest_commander".to_string()));
    assert!(player_info_columns.contains(&"kill_ratio".to_string()));
    assert!(player_info_columns.contains(&"latest_played_time".to_string()));
    assert!(!player_info_columns.contains(&"name".to_string()));
    let weekly_columns = sqlite_table_columns(&db_path, "replay_cache_weeklies");
    assert!(weekly_columns.contains(&"replay_id".to_string()));
    assert!(weekly_columns.contains(&"difficulty".to_string()));
    assert!(weekly_columns.contains(&"mutator_values".to_string()));
    let icon_columns = sqlite_table_columns(&db_path, "replay_cache_player_icons");
    assert!(!icon_columns.contains(&"order_values".to_string()));
    let icon_order_columns = sqlite_table_columns(&db_path, "replay_cache_player_icon_orders");
    assert!(icon_order_columns.contains(&"order_values".to_string()));
    let stats_player_columns = sqlite_table_columns(&db_path, "replay_cache_stats_players");
    assert!(stats_player_columns.contains(&"player_handle_key".to_string()));
    assert!(stats_player_columns.contains(&"commander".to_string()));
    assert!(!stats_player_columns.contains(&"player_handle".to_string()));
    assert!(!stats_player_columns.contains(&"player_kills".to_string()));
    let stats_unit_columns = sqlite_table_columns(&db_path, "replay_cache_stats_player_units");
    assert!(stats_unit_columns.contains(&"player_handle_key".to_string()));
    assert!(stats_unit_columns.contains(&"player_kills".to_string()));
    assert!(!stats_unit_columns.contains(&"player_handle".to_string()));
    let amon_unit_index_names = sqlite_index_names(&db_path, "replay_cache_amon_units");
    assert!(amon_unit_index_names.contains(&"idx_replay_cache_amon_units_rollup".to_string()));
    assert!(!amon_unit_index_names.contains(&"idx_replay_cache_amon_units_unit".to_string()));
    assert!(
        !sqlite_index_names(&db_path, "replay_cache_stats_players")
            .contains(&"idx_replay_cache_stats_players_handle".to_string())
    );
    assert!(
        !sqlite_index_names(&db_path, "replay_cache_stats_player_units")
            .contains(&"idx_replay_cache_stats_player_units_unit".to_string())
    );
    assert_eq!(
        sqlite_index_columns(&db_path, "idx_replay_cache_amon_units_rollup"),
        vec![
            "unit_name".to_string(),
            "replay_id".to_string(),
            "created_kind".to_string(),
            "created_count".to_string(),
            "lost_kind".to_string(),
            "lost_count".to_string(),
            "kills".to_string(),
        ]
    );
    assert!(columns.contains(&"bonus_values".to_string()));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_bonus"));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_metadata"));
    assert!(!sqlite_table_exists(&db_path, "replay_cache_mutators"));
    assert!(sqlite_table_exists(
        &db_path,
        "replay_cache_unsaved_replay_checks"
    ));
    assert!(!sqlite_table_exists(
        &db_path,
        "replay_cache_player_masteries"
    ));
    assert_eq!(sqlite_user_version(&db_path), 2);

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
fn sqlite_cache_skips_mm_entries_and_checks() {
    let root = unique_temp_path("replay_cache_db_skip_mm");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let normal = sample_cache_entry(
        "normal.SC2Replay",
        "normal-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let mm = sample_cache_entry(
        "[MM] Custom.SC2Replay",
        "mm-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[normal, mm])
        .expect("entries should write");
    let cached_files = database
        .load_cached_files()
        .expect("cached files should load");
    database
        .upsert_unsaved_replay_checks(&[
            CacheReplayCheck::new("normal-check", "normal-check.SC2Replay", 1_766_643_840),
            CacheReplayCheck::new("mm-check", "[MM] Check.SC2Replay", 1_766_643_840),
        ])
        .expect("checks should write");
    drop(database);

    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_entries"), 1);
    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_unsaved_replay_checks"),
        1
    );
    assert!(cached_files.contains("normal.SC2Replay"));
    assert!(!cached_files.contains("[MM] Custom.SC2Replay"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_stores_player_identity_by_handle() {
    let root = unique_temp_path("replay_cache_db_player_infos");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut first = sample_cache_entry(
        "player-info-first.SC2Replay",
        "player-info-first-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    first.players = vec![sample_player(1, "Player One")];
    let mut second = sample_cache_entry(
        "player-info-second.SC2Replay",
        "player-info-second-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    let mut renamed_player = sample_player(1, "Player Renamed");
    renamed_player.handle = Some("1-S2-1-1".to_string());
    second.players = vec![renamed_player];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[first, second])
        .expect("entries should write");
    drop(database);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let player_info = connection
        .query_row(
            "
            SELECT COUNT(*), MAX(wins), MAX(losses), MAX(average_apm), MAX(latest_commander),
                MAX(kill_ratio), MAX(latest_played_time)
            FROM replay_player_infos
            WHERE handle = ?1
            ",
            params!["1-S2-1-1"],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .expect("player info should load");
    assert_eq!(player_info.0, 1);
    assert_eq!(player_info.1, 2);
    assert_eq!(player_info.2, 0);
    assert!((player_info.3 - 120.0).abs() < 1e-9);
    assert_eq!(player_info.4, "Raynor");
    assert!((player_info.5 - 1.0).abs() < 1e-9);
    assert!(player_info.6 > 0);
    let replay_player_names = sqlite_table_columns(&db_path, "replay_cache_players");
    assert!(replay_player_names.contains(&"player_name".to_string()));
    assert!(!replay_player_names.contains(&"name".to_string()));
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let loaded = database
        .load_entry_by_hash("player-info-second-hash")
        .expect("entry should load")
        .expect("entry should exist");
    assert_eq!(loaded.players[0].handle.as_deref(), Some("1-S2-1-1"));
    assert_eq!(loaded.players[0].name.as_deref(), Some("Player Renamed"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_loads_overlay_player_stats_row_from_fresh_player_info() {
    let root = unique_temp_path("replay_cache_db_overlay_player_stats");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let mut entry = sample_cache_entry(
        "overlay-player-stats.SC2Replay",
        "overlay-player-stats-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    let main_player = sample_player(1, "Main Player");
    let mut ally_player = sample_player(2, "Fresh Ally");
    ally_player.handle = Some("1-S2-1-222".to_string());
    ally_player.commander = Some("Karax".to_string());
    ally_player.apm = Some(88);
    ally_player.kills = Some(30);
    entry.players = vec![main_player, ally_player];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[entry])
        .expect("entry should write");

    assert!(
        database
            .has_player_info_rows()
            .expect("player info presence should query")
    );

    let row_by_handle = database
        .load_overlay_player_stats_row("1-s2-1-222", "unused name")
        .expect("overlay player stats row should query")
        .expect("fresh player info should be available by handle");
    assert_eq!(row_by_handle.handle, "1-S2-1-222");
    assert_eq!(row_by_handle.player, "Fresh Ally");
    assert_eq!(row_by_handle.wins, 1);
    assert_eq!(row_by_handle.losses, 0);
    assert!((row_by_handle.apm - 88.0).abs() < 1e-9);
    assert_eq!(row_by_handle.commander, "Karax");
    assert!((row_by_handle.kills - 0.75).abs() < 1e-9);
    assert!(row_by_handle.last_seen > 0);

    let row_by_name = database
        .load_overlay_player_stats_row("1-S2-1-999", "fresh ally")
        .expect("overlay player stats row should query by name")
        .expect("fresh player info should fall back to latest matching name");
    assert_eq!(row_by_name.handle, "1-S2-1-222");
    assert_eq!(row_by_name.player, "Fresh Ally");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_detailed_override_updates_player_kill_ratio_only() {
    fn player(
        pid: u8,
        handle: &str,
        name: &str,
        commander: &str,
        apm: u32,
        kills: u64,
    ) -> CachePlayer {
        let mut player = sample_player(pid, name);
        player.handle = Some(handle.to_string());
        player.commander = Some(commander.to_string());
        player.apm = Some(apm);
        player.kills = Some(kills);
        player
    }

    let root = unique_temp_path("replay_cache_db_player_info_detailed_override");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut simple = sample_cache_entry(
        "override.SC2Replay",
        "override-hash",
        "2026-01-01 00:00:00",
        false,
        "Victory",
    );
    simple.players = vec![
        player(1, "1-S2-1-1", "Player One", "Raynor", 100, 10),
        player(2, "1-S2-1-2", "Player Two", "Kerrigan", 50, 30),
    ];
    let mut detailed = simple.clone();
    detailed.detailed_analysis = true;
    detailed.result = "Defeat".to_string();
    detailed.players = vec![
        player(1, "1-S2-1-1", "Player One", "Swann", 300, 30),
        player(2, "1-S2-1-2", "Player Two", "Kerrigan", 50, 10),
    ];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&simple))
        .expect("simple entry should write");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&detailed))
        .expect("detailed entry should update");
    drop(database);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let aggregate = connection
        .query_row(
            "
            SELECT wins, losses, average_apm, latest_commander, kill_ratio
            FROM replay_player_infos
            WHERE handle = ?1
            ",
            params!["1-S2-1-1"],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            },
        )
        .expect("player aggregate should load");
    assert_eq!(aggregate.0, 1);
    assert_eq!(aggregate.1, 0);
    assert!((aggregate.2 - 100.0).abs() < 1e-9);
    assert_eq!(aggregate.3, "Raynor");
    assert!((aggregate.4 - 0.75).abs() < 1e-9);

    let replay_player = connection
        .query_row(
            "
            SELECT result, commander, apm, kills
            FROM replay_cache_players
            WHERE player_handle = ?1
            ",
            params!["1-S2-1-1"],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .expect("replay player row should load");
    assert_eq!(replay_player.0.as_deref(), Some("Victory"));
    assert_eq!(replay_player.1.as_deref(), Some("Swann"));
    assert_eq!(replay_player.2, Some(300));
    assert_eq!(replay_player.3, Some(30));
    drop(connection);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_batches_player_info_and_stats_temp_table_inputs() {
    const ENTRY_COUNT: usize = 925;

    let root = unique_temp_path("replay_cache_db_batched_inputs");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let mut entries = Vec::with_capacity(ENTRY_COUNT);
    let mut main_handles = HashSet::with_capacity(ENTRY_COUNT);

    for index in 0..ENTRY_COUNT {
        let main_handle = format!("1-S2-1-{}", 10_000 + index);
        let ally_handle = format!("1-S2-1-{}", 20_000 + index);
        main_handles.insert(main_handle.to_ascii_lowercase());

        let mut main_player = sample_player(1, &format!("Main Player {index}"));
        main_player.handle = Some(main_handle.clone());
        main_player.commander = Some("Raynor".to_string());
        main_player.kills = Some(30);
        main_player.units = Some(BTreeMap::from([(
            "Marine".to_string(),
            CacheUnitStats(
                CacheCountValue::Count(1),
                CacheCountValue::Count(0),
                30,
                0.75,
            ),
        )]));

        let mut ally_player = sample_player(2, &format!("Ally Player {index}"));
        ally_player.handle = Some(ally_handle);
        ally_player.commander = Some("Karax".to_string());
        ally_player.kills = Some(10);
        ally_player.units = Some(BTreeMap::from([(
            "Sentinel".to_string(),
            CacheUnitStats(
                CacheCountValue::Count(1),
                CacheCountValue::Count(0),
                10,
                0.25,
            ),
        )]));

        let mut entry = sample_cache_entry(
            &format!("batch-{index}.SC2Replay"),
            &format!("batch-hash-{index}"),
            "2026-01-01 00:00:00",
            true,
            "Victory",
        );
        entry.players = vec![main_player, ally_player];
        entries.push(entry);
    }

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(&entries)
        .expect("batched entries should write");

    let player_info_count = database
        .connection
        .query_row("SELECT COUNT(*) FROM replay_player_infos", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("player info count should load");
    assert_eq!(player_info_count, (ENTRY_COUNT * 2) as i64);

    let sample_info = database
        .connection
        .query_row(
            "
            SELECT wins, losses, kill_ratio
            FROM replay_player_infos
            WHERE handle = ?1
            ",
            params!["1-S2-1-10000"],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .expect("sample player info should load");
    assert_eq!(sample_info.0, 1);
    assert_eq!(sample_info.1, 0);
    assert!((sample_info.2 - 0.75).abs() < 1e-9);

    let dictionary = sco_tauri_overlay::TestHelperOps::load_dictionary();
    let payload = database
        .load_statistics_payload(
            &ReplayCacheStatsQuery::new(ReplayCacheReadScope::DetailedOnly, 0),
            &HashSet::new(),
            &main_handles,
            &dictionary,
        )
        .expect("statistics payload should load through batched temp tables");
    assert_eq!(payload.games(), ENTRY_COUNT as u64);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_stores_player_masteries_and_icon_orders_as_json_arrays() {
    let root = unique_temp_path("replay_cache_db_player_json_arrays");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
