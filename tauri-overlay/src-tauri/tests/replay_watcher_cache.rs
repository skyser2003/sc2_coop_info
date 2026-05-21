#![cfg(not(windows))]

use s2coop_analyzer::cache_overall_stats_generator::{
    CacheNumericValue, CacheOverallStatsFile, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::TestHelperOps;
use sco_tauri_overlay::{
    BackendState, ReplayCacheDatabase, ReplayInfo, ReplayPlayerInfo, StatsState, TauriOverlayOps,
};
use serde_json::Value;
use serde_json::json;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_path(label: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sco_{label}_{timestamp}"))
}

fn sample_cache_entry(file: &str, hash: &str, date: &str, result: &str) -> CacheReplayEntry {
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
        detailed_analysis: true,
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

fn test_backend_state() -> BackendState {
    let state = BackendState::new();
    if let Ok(mut stats) = state.stats_handle().lock() {
        *stats = StatsState::default();
    }
    state
}

#[test]
fn record_replay_cache_update_updates_current_files_and_selection() {
    let state = test_backend_state();
    let mut existing_replay = ReplayInfo::default();
    existing_replay.set_file(TestHelperOps::test_replay_path("existing.SC2Replay"));
    existing_replay.set_date(100);
    existing_replay.set_result("Victory");
    let mut updated_replay = ReplayInfo::default();
    updated_replay.set_file(TestHelperOps::test_replay_path("new.SC2Replay"));
    updated_replay.set_date(200);
    updated_replay.set_result("Defeat");

    {
        let mut current_files = state
            .stats_current_replay_files_handle()
            .lock()
            .expect("current replay file mutex should not be poisoned");
        current_files.insert(existing_replay.file().to_string());
    }

    state.record_replay_cache_update(&updated_replay);

    let current_files = state
        .stats_current_replay_files_handle()
        .lock()
        .expect("current replay file mutex should not be poisoned")
        .clone();
    let selected_file = state.get_current_replay_file();

    assert!(current_files.contains(existing_replay.file()));
    assert!(current_files.contains(updated_replay.file()));
    assert_eq!(selected_file.as_deref(), Some(updated_replay.file()));
}

#[test]
fn record_replay_cache_update_refreshes_ready_stats_with_detailed_data() {
    let state = test_backend_state();
    let mut updated_replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("Updated Main")
            .with_handle("1-S2-1-333")
            .with_commander("Fenix")
            .with_units(json!({
                "Adept": [6, 1, 23, 0.5]
            })),
        ReplayPlayerInfo::default()
            .with_name("Updated Ally")
            .with_handle("1-S2-1-444")
            .with_commander("Karax"),
        0,
    );
    updated_replay.set_file(TestHelperOps::test_replay_path("new_detailed.SC2Replay"));
    updated_replay.set_date(200);
    updated_replay
        .set_map(TestHelperOps::canonicalize_map_id("Void Launch").expect("map id should resolve"));
    updated_replay.set_result("Victory");

    {
        let mut stats = state
            .stats_handle()
            .lock()
            .expect("stats mutex should not be poisoned");
        stats.set_ready(true);
        stats.set_analysis(Some(json!({
            "MapData": {},
            "CommanderData": {},
            "AllyCommanderData": {},
            "DifficultyData": {},
            "RegionData": {},
            "UnitData": Value::Null,
            "AmonData": {},
            "PlayerData": {},
        })));
        stats.set_message("Scanned 1 replay file(s).");
    }
    state.record_replay_cache_update(&updated_replay);

    let stats = state
        .stats_handle()
        .lock()
        .expect("stats mutex should not be poisoned");
    let analysis = stats
        .analysis_cloned()
        .expect("analysis should be present after refresh");

    assert_eq!(stats.games(), 1);
    assert_eq!(stats.message(), "Scanned 1 replay file(s).");
    assert!(
        analysis
            .get("UnitData")
            .is_some_and(|value| !value.is_null())
    );
}

#[test]
fn persist_detailed_cache_entry_to_path_writes_and_replaces_entry() {
    let root = unique_temp_path("persist_detailed_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let pretty_path = CacheOverallStatsFile::pretty_output_path(&cache_path);
    let replay_file = TestHelperOps::test_replay_path("persisted.SC2Replay");

    let original = sample_cache_entry(&replay_file, "same-hash", "2025-01-01 00:00:00", "Defeat");
    let updated = sample_cache_entry(&replay_file, "same-hash", "2026-01-01 00:00:00", "Victory");
    let payload = serde_json::to_vec(&vec![original]).expect("cache payload should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");

    TauriOverlayOps::persist_detailed_cache_entry_to_path(&cache_path, &updated)
        .expect("cache entry should persist");

    let database =
        ReplayCacheDatabase::open_for_cache_path(&cache_path).expect("database should open");
    let persisted_entries = database
        .load_entries(sco_tauri_overlay::ReplayCacheEntryQuery::all(0))
        .expect("persisted cache should load");

    assert_eq!(persisted_entries.len(), 1);
    assert_eq!(persisted_entries[0].file, replay_file);
    assert_eq!(persisted_entries[0].hash, "same-hash");
    assert_eq!(persisted_entries[0].date, "2026-01-01 00:00:00");
    assert_eq!(persisted_entries[0].result, "Victory");
    assert!(!pretty_path.exists());

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_dir_all(&root);
}
