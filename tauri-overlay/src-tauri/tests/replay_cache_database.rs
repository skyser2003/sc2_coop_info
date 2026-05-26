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

#[test]
fn sqlite_cache_reconstructs_hidden_unit_counts_without_hidden_text_columns() {
    let root = unique_temp_path("replay_cache_db_unit_hidden_schema");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
    entry.amon_units = Some(BTreeMap::from([(
        "Amon's Top Bar".to_string(),
        CacheUnitStats(
            CacheCountValue::Hidden("-".to_string()),
            CacheCountValue::Hidden("-".to_string()),
            20,
            0.75,
        ),
    )]));

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

    let amon_columns = sqlite_table_columns(&db_path, "replay_cache_amon_units");
    assert!(amon_columns.contains(&"created_kind".to_string()));
    assert!(amon_columns.contains(&"lost_kind".to_string()));
    assert!(!amon_columns.contains(&"created_hidden".to_string()));
    assert!(!amon_columns.contains(&"lost_hidden".to_string()));

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
    let loaded_amon_units = loaded.amon_units.as_ref().expect("Amon units should load");
    assert_eq!(
        loaded_amon_units["Amon's Top Bar"].0,
        CacheCountValue::Hidden("-".to_string())
    );
    assert_eq!(
        loaded_amon_units["Amon's Top Bar"].1,
        CacheCountValue::Hidden("-".to_string())
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_cleans_obsolete_statistics_schema_parts() {
    let root = unique_temp_path("replay_cache_db_stats_schema_cleanup");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    connection
        .execute_batch(
            "
            PRAGMA user_version = 1;

            CREATE TABLE replay_cache_stats_players (
                replay_id INTEGER NOT NULL,
                pid INTEGER NOT NULL CHECK(pid > 0),
                player_handle TEXT NOT NULL,
                player_handle_key TEXT NOT NULL,
                commander TEXT NOT NULL,
                player_kills INTEGER NOT NULL,
                PRIMARY KEY (replay_id, pid)
            );
            CREATE TABLE replay_cache_stats_player_units (
                replay_id INTEGER NOT NULL,
                pid INTEGER NOT NULL CHECK(pid > 0),
                player_handle TEXT NOT NULL,
                player_handle_key TEXT NOT NULL,
                commander TEXT NOT NULL,
                player_kills INTEGER NOT NULL,
                unit_name TEXT NOT NULL,
                created_hidden INTEGER NOT NULL,
                created_count INTEGER NOT NULL,
                lost_hidden INTEGER NOT NULL,
                lost_count INTEGER NOT NULL,
                kills INTEGER NOT NULL,
                PRIMARY KEY (replay_id, pid, unit_name)
            );
            CREATE TABLE replay_cache_amon_units (
                replay_id INTEGER NOT NULL,
                unit_name TEXT NOT NULL,
                created_kind TEXT NOT NULL CHECK(created_kind IN ('count', 'hidden')),
                created_count INTEGER,
                created_hidden TEXT,
                lost_kind TEXT NOT NULL CHECK(lost_kind IN ('count', 'hidden')),
                lost_count INTEGER,
                lost_hidden TEXT,
                kills INTEGER NOT NULL,
                fraction REAL NOT NULL,
                PRIMARY KEY (replay_id, unit_name)
            );

            CREATE INDEX idx_replay_cache_stats_players_handle
                ON replay_cache_stats_players(player_handle_key, replay_id);
            CREATE INDEX idx_replay_cache_stats_player_units_unit
                ON replay_cache_stats_player_units(unit_name, replay_id);
            CREATE INDEX idx_replay_cache_amon_units_unit
                ON replay_cache_amon_units(unit_name, replay_id);

            INSERT INTO replay_cache_stats_players (
                replay_id, pid, player_handle, player_handle_key, commander, player_kills
            ) VALUES (1, 1, '1-S2-1-1', '1-s2-1-1', 'Raynor', 42);
            INSERT INTO replay_cache_stats_player_units (
                replay_id, pid, player_handle, player_handle_key, commander, player_kills,
                unit_name, created_hidden, created_count, lost_hidden, lost_count, kills
            ) VALUES (1, 1, '1-S2-1-1', '1-s2-1-1', 'Raynor', 42,
                'Marine', 0, 12, 0, 3, 30);
            INSERT INTO replay_cache_amon_units (
                replay_id, unit_name, created_kind, created_count, created_hidden,
                lost_kind, lost_count, lost_hidden, kills, fraction
            ) VALUES (1, 'Zergling', 'count', 20, NULL, 'hidden', NULL, '-', 15, 0.5);
            ",
        )
        .expect("old sqlite schema should be created");
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should migrate");
    drop(database);

    let stats_player_columns = sqlite_table_columns(&db_path, "replay_cache_stats_players");
    assert!(!stats_player_columns.contains(&"player_handle".to_string()));
    assert!(!stats_player_columns.contains(&"player_kills".to_string()));
    assert!(stats_player_columns.contains(&"player_handle_key".to_string()));
    assert!(stats_player_columns.contains(&"commander".to_string()));

    let stats_unit_columns = sqlite_table_columns(&db_path, "replay_cache_stats_player_units");
    assert!(!stats_unit_columns.contains(&"player_handle".to_string()));
    assert!(stats_unit_columns.contains(&"player_kills".to_string()));

    let amon_columns = sqlite_table_columns(&db_path, "replay_cache_amon_units");
    assert!(!amon_columns.contains(&"created_hidden".to_string()));
    assert!(!amon_columns.contains(&"lost_hidden".to_string()));

    assert!(
        !sqlite_index_names(&db_path, "replay_cache_stats_players")
            .contains(&"idx_replay_cache_stats_players_handle".to_string())
    );
    assert!(
        !sqlite_index_names(&db_path, "replay_cache_stats_player_units")
            .contains(&"idx_replay_cache_stats_player_units_unit".to_string())
    );
    let amon_index_names = sqlite_index_names(&db_path, "replay_cache_amon_units");
    assert!(!amon_index_names.contains(&"idx_replay_cache_amon_units_unit".to_string()));
    assert!(amon_index_names.contains(&"idx_replay_cache_amon_units_rollup".to_string()));
    assert_eq!(sqlite_user_version(&db_path), 2);

    let connection = Connection::open(&db_path).expect("sqlite database should reopen");
    let stored_commander: String = connection
        .query_row(
            "SELECT commander FROM replay_cache_stats_players WHERE replay_id = 1 AND pid = 1",
            [],
            |row| row.get(0),
        )
        .expect("migrated stats player row should exist");
    assert_eq!(stored_commander, "Raynor");
    let stored_kills: i64 = connection
        .query_row(
            "
            SELECT player_kills
            FROM replay_cache_stats_player_units
            WHERE replay_id = 1 AND pid = 1 AND unit_name = 'Marine'
            ",
            [],
            |row| row.get(0),
        )
        .expect("migrated stats unit row should exist");
    assert_eq!(stored_kills, 42);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_backfills_missing_statistics_unit_facts_when_player_facts_exist() {
    let root = unique_temp_path("replay_cache_db_stats_unit_backfill");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "stats-unit-backfill.SC2Replay",
        "stats-unit-backfill-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let mut player = sample_player(1, "Player One");
    player.units = Some(BTreeMap::from([(
        "Marine".to_string(),
        CacheUnitStats(
            CacheCountValue::Count(12),
            CacheCountValue::Count(3),
            30,
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

    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_stats_players"),
        1
    );
    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_stats_player_units"),
        1
    );

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    connection
        .execute("DELETE FROM replay_cache_stats_player_units", [])
        .expect("statistics unit facts should delete");
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    drop(database);

    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_stats_players"),
        1
    );
    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_stats_player_units"),
        1
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_does_not_persist_dummy_pid_zero_players() {
    let root = unique_temp_path("replay_cache_db_no_dummy_pid");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut entry = sample_cache_entry(
        "stat-arrays.SC2Replay",
        "stat-arrays-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    entry.players = vec![sample_player(1, "Player One")];
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
    assert!(columns.contains(&"player_handle".to_string()));
    assert!(!columns.contains(&"name".to_string()));
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
fn sqlite_cache_stores_weekly_rows_for_weekly_tab_queries() {
    let root = unique_temp_path("replay_cache_db_weeklies");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut weekly = sample_cache_entry(
        "weekly.SC2Replay",
        "weekly-hash",
        "2026-01-01 00:00:00",
        false,
        "Victory",
    );
    weekly.weekly = true;
    weekly.ext_difficulty = "Brutal".to_string();
    weekly.brutal_plus = 2;
    weekly.mutators = vec!["Void Rifts".to_string(), "Avenger".to_string()];
    let mut normal = sample_cache_entry(
        "normal.SC2Replay",
        "normal-hash",
        "2026-01-02 00:00:00",
        false,
        "Victory",
    );
    normal.weekly = false;

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[weekly.clone(), normal])
        .expect("entries should write");
    drop(database);

    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_weeklies"), 1);
    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let weekly_row = connection
        .query_row(
            "
            SELECT difficulty, brutal_plus, json_array_length(mutator_values)
            FROM replay_cache_weeklies
            ",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .expect("weekly row should load");
    assert_eq!(weekly_row, ("Brutal".to_string(), 2, 2));
    drop(connection);

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let weekly_replays = database
        .load_weekly_replays()
        .expect("weekly replays should load");
    assert_eq!(weekly_replays.len(), 1);
    assert!(weekly_replays[0].weekly());
    assert_eq!(weekly_replays[0].difficulty(), "Brutal");
    assert_eq!(weekly_replays[0].brutal_plus(), 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_detailed_override_keeps_aggregate_tables_deduplicated() {
    fn player(pid: u8, handle: &str, name: &str, kills: u64) -> CachePlayer {
        let mut player = sample_player(pid, name);
        player.handle = Some(handle.to_string());
        player.kills = Some(kills);
        player
    }

    let root = unique_temp_path("replay_cache_db_override_aggregate_dedup");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut simple = sample_cache_entry(
        "weekly-override.SC2Replay",
        "weekly-override-hash",
        "2026-01-01 00:00:00",
        false,
        "Victory",
    );
    simple.weekly = true;
    simple.brutal_plus = 1;
    simple.mutators = vec!["Void Rifts".to_string()];
    simple.players = vec![
        player(1, "1-S2-1-100", "Player One", 10),
        player(2, "1-S2-1-200", "Player Two", 30),
    ];
    let mut detailed = simple.clone();
    detailed.detailed_analysis = true;
    detailed.brutal_plus = 3;
    detailed.mutators = vec!["Void Rifts".to_string(), "Avenger".to_string()];
    detailed.players = vec![
        player(1, "1-S2-1-100", "Player One", 30),
        player(2, "1-S2-1-200", "Player Two", 10),
    ];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&simple))
        .expect("simple entry should write");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&detailed))
        .expect("detailed entry should override simple entry");
    drop(database);

    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_entries"), 1);
    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_players"), 2);
    assert_eq!(sqlite_table_row_count(&db_path, "replay_player_infos"), 2);
    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_weeklies"), 1);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let weekly_row = connection
        .query_row(
            "
            SELECT brutal_plus, json_array_length(mutator_values)
            FROM replay_cache_weeklies
            ",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .expect("weekly row should load");
    assert_eq!(weekly_row, (3, 2));
    let player_info = connection
        .query_row(
            "
            SELECT wins, losses, kill_ratio
            FROM replay_player_infos
            WHERE handle = ?1
            ",
            params!["1-S2-1-100"],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .expect("player aggregate should load");
    assert_eq!(player_info.0, 1);
    assert_eq!(player_info.1, 0);
    assert!((player_info.2 - 0.75).abs() < 1e-9);
    drop(connection);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_same_file_replacement_removes_stale_aggregate_rows() {
    let root = unique_temp_path("replay_cache_db_same_file_replacement_dedup");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let mut old_entry = sample_cache_entry(
        "same-file.SC2Replay",
        "same-file-old-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    old_entry.weekly = true;
    old_entry.players = vec![sample_player(1, "Old Player")];
    let mut new_entry = sample_cache_entry(
        "same-file.SC2Replay",
        "same-file-new-hash",
        "2026-01-02 00:00:00",
        true,
        "Defeat",
    );
    let mut new_player = sample_player(1, "New Player");
    new_player.handle = Some("1-S2-1-999".to_string());
    new_entry.weekly = true;
    new_entry.brutal_plus = 4;
    new_entry.players = vec![new_player];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&old_entry))
        .expect("old entry should write");
    database
        .upsert_entries_preserving_detailed(std::slice::from_ref(&new_entry))
        .expect("new same-file entry should replace old entry");
    drop(database);

    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_entries"), 1);
    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_players"), 1);
    assert_eq!(sqlite_table_row_count(&db_path, "replay_player_infos"), 1);
    assert_eq!(sqlite_table_row_count(&db_path, "replay_cache_weeklies"), 1);

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let stored = connection
        .query_row(
            "
            SELECT e.hash, p.player_handle, info.handle, weekly.brutal_plus
            FROM replay_cache_entries e
            INNER JOIN replay_cache_players p ON p.replay_id = e.id
            INNER JOIN replay_player_infos info ON info.handle = p.player_handle
            INNER JOIN replay_cache_weeklies weekly ON weekly.replay_id = e.id
            ",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .expect("replacement row should load");
    assert_eq!(stored.0, "same-file-new-hash");
    assert_eq!(stored.1, "1-S2-1-999");
    assert_eq!(stored.2, "1-S2-1-999");
    assert_eq!(stored.3, 4);
    drop(connection);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_summary_entries_skip_heavy_child_payloads() {
    let root = unique_temp_path("replay_cache_db_summary_entries");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let mut entry = sample_cache_entry(
        "summary.SC2Replay",
        "summary-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );
    let mut player = sample_player(1, "Player One");
    player.masteries = Some([1, 2, 3, 4, 5, 6]);
    player.icons = Some(BTreeMap::from([(
        "Top Units".to_string(),
        CacheIconValue::Order(vec!["Marine".to_string()]),
    )]));
    player.units = Some(BTreeMap::from([(
        "Marine".to_string(),
        CacheUnitStats(
            CacheCountValue::Count(10),
            CacheCountValue::Count(2),
            20,
            0.75,
        ),
    )]));
    entry.players = vec![player];
    entry.messages = vec![ReplayMessage {
        player: 1,
        text: "hello".to_string(),
        time: 1.5,
    }];
    entry.amon_units = Some(BTreeMap::from([(
        "Zergling".to_string(),
        CacheUnitStats(
            CacheCountValue::Count(30),
            CacheCountValue::Count(30),
            0,
            0.0,
        ),
    )]));
    entry.player_stats = Some(BTreeMap::from([(1, sample_player_stats("Player One"))]));

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(std::slice::from_ref(&entry))
        .expect("entry should write");

    let summary = database
        .load_summary_entries(ReplayCacheEntryQuery::all(0))
        .expect("summary entries should load")
        .pop()
        .expect("summary entry should exist");
    assert_eq!(summary.players.len(), 1);
    assert_eq!(summary.players[0].masteries, Some([1, 2, 3, 4, 5, 6]));
    assert!(summary.players[0].icons.is_none());
    assert!(summary.players[0].units.is_none());
    assert!(summary.messages.is_empty());
    assert!(summary.amon_units.is_none());
    assert!(summary.player_stats.is_none());

    let full = database
        .load_entries(ReplayCacheEntryQuery::all(0))
        .expect("full entries should load")
        .pop()
        .expect("full entry should exist");
    assert!(full.players[0].icons.is_some());
    assert!(full.players[0].units.is_some());
    assert!(!full.messages.is_empty());
    assert!(full.amon_units.is_some());
    assert!(full.player_stats.is_some());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_games_page_query_filters_sorts_and_offsets_in_database() {
    let root = unique_temp_path("replay_cache_db_games_page");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let mut alpha = sample_cache_entry(
        "alpha.SC2Replay",
        "alpha-hash",
        "2026-01-01 00:00:00",
        false,
        "Victory",
    );
    alpha.map_name = "Alpha Map".to_string();
    alpha.players = vec![sample_player(1, "Alpha P1"), sample_player(2, "Alpha P2")];
    let mut beta = sample_cache_entry(
        "beta.SC2Replay",
        "beta-hash",
        "2026-01-02 00:00:00",
        false,
        "Victory",
    );
    beta.map_name = "Beta Map".to_string();
    beta.players = vec![sample_player(1, "Beta P1"), sample_player(2, "Beta P2")];
    let mut rifts = sample_cache_entry(
        "rifts.SC2Replay",
        "rifts-hash",
        "2026-01-03 00:00:00",
        false,
        "Victory",
    );
    rifts.map_name = "Gamma Map".to_string();
    rifts.weekly = true;
    rifts.mutators = vec!["Void Rifts".to_string()];
    rifts.players = vec![sample_player(1, "Rifts P1"), sample_player(2, "Rifts P2")];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[beta, rifts.clone(), alpha])
        .expect("entries should write");

    let second_map_page = database
        .load_summary_entries_page(&ReplayCacheGamesPageQuery::new(
            ReplayCachePage::new(2, 1),
            String::new(),
            ReplayCacheGameSortKey::Map,
            ReplayCacheSortDirection::Asc,
            ReplayCacheDifficultyFilter::all(),
            true,
            true,
        ))
        .expect("games page should load");
    assert_eq!(second_map_page.total_rows(), 3);
    assert_eq!(second_map_page.rows()[0].hash, "beta-hash");

    let mutation_search_page = database
        .load_summary_entries_page(&ReplayCacheGamesPageQuery::new(
            ReplayCachePage::new(1, 20),
            "rifts".to_string(),
            ReplayCacheGameSortKey::Time,
            ReplayCacheSortDirection::Desc,
            ReplayCacheDifficultyFilter::all(),
            false,
            true,
        ))
        .expect("filtered games page should load");
    assert_eq!(mutation_search_page.total_rows(), 1);
    assert_eq!(mutation_search_page.rows()[0].hash, rifts.hash);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_statistics_query_prefilters_replays_in_database() {
    let root = unique_temp_path("replay_cache_db_stats_filter");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");

    let mut included = sample_cache_entry(
        "stats-included.SC2Replay",
        "stats-included-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    included.accurate_length = CacheNumericValue::Integer(900);
    let mut included_main = sample_player(1, "Alice");
    included_main.masteries = Some([31, 30, 30, 0, 0, 0]);
    included.players = vec![included_main, sample_player(2, "Partner")];

    let mut defeated = sample_cache_entry(
        "stats-defeat.SC2Replay",
        "stats-defeat-hash",
        "2026-01-02 00:00:00",
        true,
        "Defeat",
    );
    defeated.accurate_length = CacheNumericValue::Integer(900);
    defeated.players = vec![sample_player(1, "Defeated"), sample_player(2, "Partner")];

    let mut mutation = sample_cache_entry(
        "stats-mutation.SC2Replay",
        "stats-mutation-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    mutation.extension = true;
    mutation.accurate_length = CacheNumericValue::Integer(900);
    mutation.players = vec![sample_player(1, "Mutation"), sample_player(2, "Partner")];

    let mut brutal_plus = sample_cache_entry(
        "stats-bplus.SC2Replay",
        "stats-bplus-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    brutal_plus.brutal_plus = 3;
    brutal_plus.accurate_length = CacheNumericValue::Integer(900);
    brutal_plus.players = vec![sample_player(1, "Plus"), sample_player(2, "Partner")];

    let mut too_short = sample_cache_entry(
        "stats-short.SC2Replay",
        "stats-short-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    too_short.accurate_length = CacheNumericValue::Integer(300);
    too_short.players = vec![sample_player(1, "Short"), sample_player(2, "Partner")];

    let mut too_late = sample_cache_entry(
        "stats-late.SC2Replay",
        "stats-late-hash",
        "2026-01-05 00:00:00",
        true,
        "Victory",
    );
    too_late.accurate_length = CacheNumericValue::Integer(900);
    too_late.players = vec![sample_player(1, "Late"), sample_player(2, "Partner")];

    let mut eu_region = sample_cache_entry(
        "stats-eu.SC2Replay",
        "stats-eu-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    eu_region.region = "EU".to_string();
    eu_region.accurate_length = CacheNumericValue::Integer(900);
    let mut eu_main = sample_player(1, "Euro");
    eu_main.masteries = Some([31, 30, 30, 0, 0, 0]);
    eu_region.players = vec![eu_main, sample_player(2, "Partner")];

    let mut low_level = sample_cache_entry(
        "stats-low-level.SC2Replay",
        "stats-low-level-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    low_level.accurate_length = CacheNumericValue::Integer(900);
    let mut low_main = sample_player(1, "Low");
    low_main.commander_level = Some(1);
    low_main.masteries = Some([0, 0, 0, 0, 0, 0]);
    let mut low_ally = sample_player(2, "Low Partner");
    low_ally.commander_level = Some(1);
    low_ally.masteries = Some([0, 0, 0, 0, 0, 0]);
    low_level.players = vec![low_main, low_ally];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[
            included,
            defeated,
            mutation,
            brutal_plus,
            too_short,
            too_late,
            eu_region,
            low_level,
        ])
        .expect("entries should write");

    let query = ReplayCacheStatsQuery::new(ReplayCacheReadScope::DetailedOnly, 0)
        .with_mutation_filters(false, true)
        .with_result_filters(true, false)
        .with_length_seconds(600, 1_200)
        .with_date_seconds(
            Some(utc_seconds(2026, 1, 1, 0, 0, 0)),
            Some(utc_seconds(2026, 1, 4, 0, 0, 0)),
        )
        .with_player_filter("A*".to_string())
        .with_difficulty_exclusions(vec![ReplayCacheStatsDifficultyExclusion::BrutalPlus3]);

    let matching_count = database
        .count_entries_for_stats(&query)
        .expect("filtered stats entries should count");
    assert_eq!(matching_count, 1);

    let current_file_query = query
        .clone()
        .with_current_replay_files(vec!["stats-included.SC2Replay".to_string()]);
    assert!(
        database
            .has_detailed_entries_for_stats(&current_file_query)
            .expect("detailed stats existence should query")
    );
    let current_file_count = database
        .count_entries_for_stats(&current_file_query)
        .expect("current-file filtered stats entries should count");
    assert_eq!(current_file_count, 1);

    let empty_current_file_query = query.clone().with_current_replay_files(Vec::new());
    assert!(
        !database
            .has_detailed_entries_for_stats(&empty_current_file_query)
            .expect("empty current-file detailed stats existence should query")
    );
    assert!(
        database
            .count_entries_for_stats(&empty_current_file_query)
            .expect("empty current-file filtered stats entries should count")
            == 0
    );

    let level_query = ReplayCacheStatsQuery::new(ReplayCacheReadScope::DetailedOnly, 0)
        .with_result_filters(true, false)
        .with_commander_level_filters(false, true, true, true);
    let level_count = database
        .count_entries_for_stats(&level_query)
        .expect("level-filtered stats entries should count");
    assert_eq!(level_count, 6);

    let mastery_query = ReplayCacheStatsQuery::new(ReplayCacheReadScope::DetailedOnly, 0)
        .with_result_filters(true, false)
        .with_mastery_filters(false, true, true, true);
    let mastery_count = database
        .count_entries_for_stats(&mastery_query)
        .expect("mastery-filtered stats entries should count");
    assert_eq!(mastery_count, 2);

    let region_query = ReplayCacheStatsQuery::new(ReplayCacheReadScope::DetailedOnly, 0)
        .with_result_filters(true, false)
        .with_mastery_filters(false, true, true, true)
        .with_region_exclusions(vec!["EU".to_string()]);
    let region_count = database
        .count_entries_for_stats(&region_query)
        .expect("region-filtered stats entries should count");
    assert_eq!(region_count, 1);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_players_page_query_aggregates_searches_notes_and_offsets_in_database() {
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

    let root = unique_temp_path("replay_cache_db_players_page");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let mut first = sample_cache_entry(
        "players-first.SC2Replay",
        "players-first-hash",
        "2026-01-01 00:00:00",
        false,
        "Victory",
    );
    first.players = vec![
        player(1, "1-S2-1-10", "Alice", "Raynor", 100, 30),
        player(2, "1-S2-1-20", "Bob", "Kerrigan", 50, 10),
    ];
    let mut second = sample_cache_entry(
        "players-second.SC2Replay",
        "players-second-hash",
        "2026-01-02 00:00:00",
        false,
        "Defeat",
    );
    second.players = vec![
        player(1, "1-S2-1-10", "Alice Prime", "Raynor", 200, 20),
        player(2, "1-S2-1-30", "Charlie", "Artanis", 80, 20),
    ];
    let mut third = sample_cache_entry(
        "players-third.SC2Replay",
        "players-third-hash",
        "2026-01-03 00:00:00",
        false,
        "Victory",
    );
    third.players = vec![
        player(1, "1-S2-1-20", "Bob", "Kerrigan", 70, 40),
        player(2, "1-S2-1-40", "Delta", "Swann", 60, 10),
    ];

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[first, second, third])
        .expect("entries should write");

    let wins_page = database
        .load_player_rows_page(&ReplayCachePlayersPageQuery::new(
            ReplayCachePage::new(1, 1),
            String::new(),
            ReplayCachePlayerSortKey::Wins,
            ReplayCacheSortDirection::Desc,
            Vec::new(),
        ))
        .expect("players page should load");
    assert_eq!(wins_page.total_rows(), 4);
    assert_eq!(wins_page.rows()[0].handle, "1-S2-1-20");
    assert_eq!(wins_page.rows()[0].wins, 2);

    let fourth_last_seen_page = database
        .load_player_rows_page(&ReplayCachePlayersPageQuery::new(
            ReplayCachePage::new(4, 1),
            String::new(),
            ReplayCachePlayerSortKey::LastSeen,
            ReplayCacheSortDirection::Desc,
            Vec::new(),
        ))
        .expect("fourth players page should load");
    assert_eq!(fourth_last_seen_page.total_rows(), 4);
    assert_eq!(fourth_last_seen_page.rows()[0].handle, "1-S2-1-30");

    let note_search_page = database
        .load_player_rows_page(&ReplayCachePlayersPageQuery::new(
            ReplayCachePage::new(1, 20),
            "favorite".to_string(),
            ReplayCachePlayerSortKey::LastSeen,
            ReplayCacheSortDirection::Desc,
            vec![ReplayCachePlayerNote::new(
                "1-S2-1-30".to_string(),
                "favorite ally".to_string(),
            )],
        ))
        .expect("note search players page should load");
    assert_eq!(note_search_page.total_rows(), 1);
    assert_eq!(note_search_page.rows()[0].handle, "1-S2-1-30");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sqlite_cache_entry_sink_writes_entries_to_database() {
    let root = unique_temp_path("replay_cache_db_sink");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
fn concurrent_worker_batches_wait_for_sqlite_writer_lock() {
    let root = unique_temp_path("replay_cache_db_concurrent_batches");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let worker_count = 8usize;
    let batches_per_worker = 6usize;
    let start_barrier = Arc::new(Barrier::new(worker_count));

    let handles = (0..worker_count)
        .map(|worker_index| {
            let cache_path = cache_path.clone();
            let start_barrier = Arc::clone(&start_barrier);
            thread::spawn(move || {
                start_barrier.wait();
                let mut changed = 0usize;
                for batch_index in 0..batches_per_worker {
                    let replay_index = worker_index * batches_per_worker + batch_index;
                    let mut entry = sample_cache_entry(
                        &format!("concurrent-{replay_index}.SC2Replay"),
                        &format!("concurrent-hash-{replay_index}"),
                        &format!("2026:01:01:00:00:{:02}", replay_index % 60),
                        false,
                        "Victory",
                    );
                    entry.players = vec![
                        sample_player(1, "Concurrent One"),
                        sample_player(2, "Concurrent Two"),
                    ];
                    let mut database = ReplayCacheDatabase::open_for_cache_path(&cache_path)
                        .expect("worker database should open");
                    changed = changed.saturating_add(
                        database
                            .upsert_entries_preserving_detailed(&[entry])
                            .expect("worker batch should persist"),
                    );
                }
                changed
            })
        })
        .collect::<Vec<_>>();

    let changed = handles
        .into_iter()
        .map(|handle| handle.join().expect("worker should finish"))
        .sum::<usize>();
    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let expected_entries = worker_count * batches_per_worker;

    assert_eq!(changed, expected_entries);
    assert_eq!(
        database
            .count_entries()
            .expect("cache entries should count"),
        expected_entries
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn cache_write_queue_serializes_parallel_worker_batches() {
    let root = unique_temp_path("replay_cache_db_write_queue");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let worker_count = 8usize;
    let batches_per_worker = 6usize;
    let start_barrier = Arc::new(Barrier::new(worker_count));
    let write_queue = ReplayCacheWriteQueue::start(cache_path.clone());
    let sender = write_queue.sender();

    let handles = (0..worker_count)
        .map(|worker_index| {
            let sender = sender.clone();
            let start_barrier = Arc::clone(&start_barrier);
            thread::spawn(move || {
                start_barrier.wait();
                for batch_index in 0..batches_per_worker {
                    let replay_index = worker_index * batches_per_worker + batch_index;
                    let mut entry = sample_cache_entry(
                        &format!("queued-{replay_index}.SC2Replay"),
                        &format!("queued-hash-{replay_index}"),
                        &format!("2026:01:01:00:01:{:02}", replay_index % 60),
                        false,
                        "Victory",
                    );
                    entry.players = vec![
                        sample_player(1, "Queued One"),
                        sample_player(2, "Queued Two"),
                    ];
                    sender
                        .write_entries(vec![entry])
                        .expect("worker batch should queue");
                }
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("worker should finish");
    }
    drop(sender);
    let write_result = write_queue.finish();
    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let expected_entries = worker_count * batches_per_worker;

    assert_eq!(write_result.persisted_entries(), expected_entries);
    assert_eq!(write_result.failed_batches(), 0);
    assert_eq!(
        database
            .count_entries()
            .expect("cache entries should count"),
        expected_entries
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn queued_cache_entry_sink_uses_writer_queue_for_detailed_batches() {
    let root = unique_temp_path("replay_cache_db_queued_sink");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let write_queue = ReplayCacheWriteQueue::start(cache_path.clone());
    let sink = QueuedReplayCacheEntrySink::new(write_queue.sender());
    let mut first = sample_cache_entry(
        "queued-sink-first.SC2Replay",
        "queued-sink-first-hash",
        "2026:01:01:00:02:00",
        true,
        "Victory",
    );
    first.players = vec![sample_player(1, "Queued Sink One")];
    let mut second = sample_cache_entry(
        "queued-sink-second.SC2Replay",
        "queued-sink-second-hash",
        "2026:01:01:00:03:00",
        true,
        "Victory",
    );
    second.players = vec![sample_player(2, "Queued Sink Two")];

    let queued = sink
        .write_entries(&[first, second])
        .expect("queued sink should accept entries");
    drop(sink);
    let write_result = write_queue.finish();
    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");

    assert_eq!(queued, 2);
    assert_eq!(write_result.persisted_entries(), 2);
    assert_eq!(write_result.failed_batches(), 0);
    assert_eq!(database.count_entries().expect("entries should count"), 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn queued_detailed_cache_sink_persists_checked_replay_identities() {
    let root = unique_temp_path("replay_cache_db_queued_checks");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let db_path = ReplayCacheDatabase::db_path_for_cache_path(&cache_path);
    let write_queue = ReplayCacheWriteQueue::start_detailed_analysis(cache_path.clone());
    let sink = QueuedReplayCacheEntrySink::new(write_queue.sender());
    let mut basic = sample_cache_entry(
        "queued-check-basic.SC2Replay",
        "queued-check-basic-hash",
        "2026:01:01:00:04:00",
        false,
        "Victory",
    );
    basic.players = vec![sample_player(1, "Queued Check Basic")];
    let queued_entries = sink
        .write_entries(std::slice::from_ref(&basic))
        .expect("basic checked replay entry should queue");
    let queued_checks = sink
        .write_checks(&[CacheReplayCheck::new(
            "queued-check-invalid-hash",
            "queued-check-invalid.SC2Replay",
            1_766_643_840,
        )])
        .expect("unsaved replay identity should queue");
    drop(sink);
    let write_result = write_queue.finish();
    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should reopen");
    let files_by_hash = database
        .load_detailed_cache_files_by_hash()
        .expect("detailed cache identities should load");

    assert_eq!(queued_entries, 1);
    assert_eq!(queued_checks, 1);
    assert_eq!(write_result.failed_batches(), 0);
    assert_eq!(files_by_hash.get("queued-check-basic-hash"), None);
    assert_eq!(
        files_by_hash.get("queued-check-invalid-hash"),
        Some(&"queued-check-invalid.SC2Replay".to_string())
    );
    let identities_by_hash = database
        .load_detailed_cache_identities_by_hash()
        .expect("detailed cache file identities should load");
    assert_eq!(
        identities_by_hash
            .get("queued-check-invalid-hash")
            .map(|identity| identity.modified_seconds()),
        Some(1_766_643_840)
    );

    let connection = Connection::open(&db_path).expect("sqlite database should open");
    let detailed_analysis = connection
        .query_row(
            "
            SELECT detailed_analysis
            FROM replay_cache_entries
            WHERE hash = ?1
            ",
            params!["queued-check-basic-hash"],
            |row| row.get::<_, i64>(0),
        )
        .expect("basic row should load");
    assert_eq!(detailed_analysis, 0);
    assert_eq!(
        sqlite_table_row_count(&db_path, "replay_cache_unsaved_replay_checks"),
        1
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn imports_legacy_json_cache_file_into_database() {
    let root = unique_temp_path("replay_cache_db_import");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let legacy_json_path = ReplayCacheDatabase::legacy_json_path_for_cache_path(&cache_path);
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
    write_legacy_cache(&legacy_json_path, &[older, newer]);

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
        !legacy_json_path.exists(),
        "legacy cache JSON should be deleted after successful SQLite import"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn legacy_cache_import_saves_replay_dates_as_utc() {
    let root = unique_temp_path("replay_cache_db_import_utc_date");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let legacy_json_path = ReplayCacheDatabase::legacy_json_path_for_cache_path(&cache_path);
    let entry = sample_cache_entry(
        "legacy-local-date.SC2Replay",
        "legacy-local-date-hash",
        "2026:01:01:00:00:00",
        true,
        "Victory",
    );
    write_legacy_cache(&legacy_json_path, std::slice::from_ref(&entry));

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
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
fn sqlite_navigation_candidates_load_adjacent_replays_without_full_cache_scan() {
    let root = unique_temp_path("replay_cache_db_navigation_candidates");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let newest = sample_cache_entry(
        "newest.SC2Replay",
        "newest-hash",
        "2026-01-03 00:00:00",
        true,
        "Victory",
    );
    let middle = sample_cache_entry(
        "middle.SC2Replay",
        "middle-hash",
        "2026-01-02 00:00:00",
        true,
        "Victory",
    );
    let oldest = sample_cache_entry(
        "oldest.SC2Replay",
        "oldest-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[oldest.clone(), middle.clone(), newest.clone()])
        .expect("entries should write");

    let inactive = database
        .load_navigation_candidates(Some(&oldest.file), -1, false, 0, 1)
        .expect("inactive navigation should load latest replay");
    assert_eq!(
        inactive.first().map(|entry| entry.hash.as_str()),
        Some("newest-hash")
    );

    let newer = database
        .load_navigation_candidates(Some(&middle.file), 1, true, 0, 1)
        .expect("newer navigation should load adjacent replay");
    assert_eq!(
        newer.first().map(|entry| entry.hash.as_str()),
        Some("newest-hash")
    );

    let older = database
        .load_navigation_candidates(Some(&middle.file), -1, true, 0, 1)
        .expect("older navigation should load adjacent replay");
    assert_eq!(
        older.first().map(|entry| entry.hash.as_str()),
        Some("oldest-hash")
    );

    let past_latest = database
        .load_navigation_candidates(Some(&newest.file), 1, true, 0, 1)
        .expect("latest replay should query successfully");
    assert!(past_latest.is_empty());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn opening_future_schema_version_returns_typed_error() {
    let root = unique_temp_path("replay_cache_db_future_schema");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
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
            assert_eq!(supported, 2);
        }
        Err(error) => panic!("unexpected error: {error}"),
        Ok(_) => panic!("future schema should not open"),
    }

    let _ = std::fs::remove_dir_all(&root);
}
