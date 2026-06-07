use super::*;

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
fn sqlite_games_page_query_supports_explicit_offsets() {
    let root = unique_temp_path("replay_cache_db_games_offset");
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
    let mut beta = sample_cache_entry(
        "beta.SC2Replay",
        "beta-hash",
        "2026-01-02 00:00:00",
        false,
        "Victory",
    );
    beta.map_name = "Beta Map".to_string();
    let mut gamma = sample_cache_entry(
        "gamma.SC2Replay",
        "gamma-hash",
        "2026-01-03 00:00:00",
        false,
        "Victory",
    );
    gamma.map_name = "Gamma Map".to_string();

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[gamma, beta, alpha])
        .expect("entries should write");

    let offset_page = database
        .load_summary_entries_page(&ReplayCacheGamesPageQuery::new(
            ReplayCachePage::from_offset(1, 1),
            String::new(),
            ReplayCacheGameSortKey::Map,
            ReplayCacheSortDirection::Asc,
            ReplayCacheDifficultyFilter::all(),
            true,
            true,
        ))
        .expect("offset games page should load");
    assert_eq!(offset_page.total_rows(), 3);
    assert_eq!(offset_page.rows()[0].hash, "beta-hash");

    let _ = std::fs::remove_dir_all(&root);
}
