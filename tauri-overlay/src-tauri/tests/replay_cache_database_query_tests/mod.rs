use super::*;

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
fn sqlite_load_latest_entry_date_seconds_returns_latest_replay_time() {
    let root = unique_temp_path("replay_cache_db_latest_date_seconds");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.sqlite3");
    let newest = sample_cache_entry(
        "newest.SC2Replay",
        "newest-date-seconds-hash",
        "2026-01-03 01:02:03",
        true,
        "Victory",
    );
    let older = sample_cache_entry(
        "older.SC2Replay",
        "older-date-seconds-hash",
        "2026-01-01 00:00:00",
        true,
        "Victory",
    );

    let mut database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    database
        .replace_entries(&[older, newest])
        .expect("entries should write");

    assert_eq!(
        database
            .load_latest_entry_date_seconds()
            .expect("latest date seconds should load"),
        Some(utc_seconds(2026, 1, 3, 1, 2, 3))
    );

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
