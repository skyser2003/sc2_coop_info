#![cfg(not(windows))]

use s2coop_analyzer::cache_overall_stats_generator::{
    pretty_output_path, CacheNumericValue, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::test_helper::{canonicalize_map_id, test_replay_path};
use sco_tauri_overlay::{
    persist_detailed_cache_entry_to_path, BackendState, ReplayInfo, ReplayPlayerInfo, StatsState,
};
use serde_json::json;
use serde_json::Value;
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
        build: ReplayBuildInfo {
            replay_build: 1,
            protocol_build: ProtocolBuildValue::Int(1),
        },
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

fn seed_replay_cache(state: &BackendState, entries: &[(&str, ReplayInfo)]) {
    let replay_state = state.get_replay_state();
    let replays_slot = replay_state
        .lock()
        .expect("replay state mutex should not be poisoned")
        .replays_handle()
        .clone();
    let mut replays = replays_slot
        .lock()
        .expect("replays mutex should not be poisoned");
    for (hash, replay) in entries {
        replays.insert((*hash).to_string(), replay.clone());
    }
}

#[test]
fn upsert_replay_in_memory_cache_updates_replay_cache_and_current_files() {
    let state = test_backend_state();
    let mut existing_replay = ReplayInfo::default();
    existing_replay.set_file(test_replay_path("existing.SC2Replay"));
    existing_replay.set_date(100);
    existing_replay.set_result("Victory");
    let mut updated_replay = ReplayInfo::default();
    updated_replay.set_file(test_replay_path("new.SC2Replay"));
    updated_replay.set_date(200);
    updated_replay.set_result("Defeat");

    seed_replay_cache(
        &state,
        &[
            ("existing-hash", existing_replay.clone()),
            ({
                let mut previous_updated_replay = ReplayInfo::default();
                previous_updated_replay.set_file(updated_replay.file().to_string());
                previous_updated_replay.set_date(50);
                previous_updated_replay.set_result("Victory");
                ("updated-hash", previous_updated_replay)
            }),
        ],
    );
    {
        let mut current_files = state
            .stats_current_replay_files_handle()
            .lock()
            .expect("current replay file mutex should not be poisoned");
        current_files.insert(existing_replay.file().to_string());
    }

    state.upsert_replay_in_memory_cache("updated-hash", &updated_replay);

    let replays = state.replay_cache_snapshot();
    let current_files = state
        .stats_current_replay_files_handle()
        .lock()
        .expect("current replay file mutex should not be poisoned")
        .clone();
    let selected_file = state.get_current_replay_file();

    assert_eq!(replays.len(), 2);
    assert_eq!(replays[0].file(), updated_replay.file());
    assert_eq!(replays[0].result(), updated_replay.result());
    assert!(current_files.contains(existing_replay.file()));
    assert!(current_files.contains(updated_replay.file()));
    assert_eq!(selected_file.as_deref(), Some(updated_replay.file()));
}

#[test]
fn upsert_replay_in_memory_cache_refreshes_ready_stats_with_detailed_data() {
    let state = test_backend_state();
    let mut existing_replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("Existing Main")
            .with_handle("1-S2-1-111")
            .with_commander("Raynor")
            .with_units(json!({
                "Marine": [3, 1, 9, 0.5]
            })),
        ReplayPlayerInfo::default()
            .with_name("Existing Ally")
            .with_handle("1-S2-1-222")
            .with_commander("Karax"),
        0,
    );
    existing_replay.set_file(test_replay_path("existing_detailed.SC2Replay"));
    existing_replay.set_date(100);
    existing_replay.set_map(canonicalize_map_id("Void Launch").expect("map id should resolve"));
    existing_replay.set_result("Victory");
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
    updated_replay.set_file(test_replay_path("new_detailed.SC2Replay"));
    updated_replay.set_date(200);
    updated_replay.set_map(canonicalize_map_id("Void Launch").expect("map id should resolve"));
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
    seed_replay_cache(&state, &[("existing-detailed-hash", existing_replay)]);

    state.upsert_replay_in_memory_cache("updated-detailed-hash", &updated_replay);

    let stats = state
        .stats_handle()
        .lock()
        .expect("stats mutex should not be poisoned");
    let analysis = stats
        .analysis_cloned()
        .expect("analysis should be present after refresh");

    assert_eq!(stats.games(), 2);
    assert_eq!(stats.message(), "Scanned 2 replay file(s).");
    assert!(analysis
        .get("UnitData")
        .is_some_and(|value| !value.is_null()));
}

#[test]
fn persist_detailed_cache_entry_to_path_writes_and_replaces_entry() {
    let root = unique_temp_path("persist_detailed_cache");
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let cache_path = root.join("cache_overall_stats.json");
    let pretty_path = pretty_output_path(&cache_path);
    let replay_file = test_replay_path("persisted.SC2Replay");

    let original = sample_cache_entry(&replay_file, "same-hash", "2025-01-01 00:00:00", "Defeat");
    let updated = sample_cache_entry(&replay_file, "same-hash", "2026-01-01 00:00:00", "Victory");
    let payload = serde_json::to_vec(&vec![original]).expect("cache payload should serialize");
    std::fs::write(&cache_path, payload).expect("cache file should be written");

    persist_detailed_cache_entry_to_path(&cache_path, &updated)
        .expect("cache entry should persist");

    let persisted_payload = std::fs::read(&cache_path).expect("cache file should exist");
    let persisted_entries = serde_json::from_slice::<Vec<CacheReplayEntry>>(&persisted_payload)
        .expect("persisted cache should parse");

    assert_eq!(persisted_entries.len(), 1);
    assert_eq!(persisted_entries[0].file, replay_file);
    assert_eq!(persisted_entries[0].hash, "same-hash");
    assert_eq!(persisted_entries[0].date, "2026-01-01 00:00:00");
    assert_eq!(persisted_entries[0].result, "Victory");
    assert!(!pretty_path.exists());

    let _ = std::fs::remove_file(&cache_path);
    let _ = std::fs::remove_dir_all(&root);
}
