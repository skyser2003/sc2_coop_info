#![cfg(not(windows))]

mod common;

use common::test_replay_path;
use s2coop_analyzer::cache_overall_stats_generator::{
    pretty_output_path, CacheNumericValue, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::{
    canonicalize_coop_map_id, persist_detailed_cache_entry_to_path, BackendState, ReplayInfo,
    StatsState,
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
    if let Ok(mut stats) = state.stats.lock() {
        *stats = StatsState::default();
    }
    state
}

#[test]
fn upsert_replay_in_memory_cache_updates_stats_cache_and_current_files() {
    let state = test_backend_state();
    let existing_replay = ReplayInfo {
        file: test_replay_path("existing.SC2Replay"),
        date: 100,
        result: "Victory".to_string(),
        ..ReplayInfo::default()
    };
    let updated_replay = ReplayInfo {
        file: test_replay_path("new.SC2Replay"),
        date: 200,
        result: "Defeat".to_string(),
        ..ReplayInfo::default()
    };

    {
        let replay_state = state.get_replay_state();
        let replays_slot = replay_state
            .lock()
            .expect("replay state mutex should not be poisoned")
            .replays
            .clone();
        let mut replays = replays_slot
            .lock()
            .expect("replays mutex should not be poisoned");
        replays.push(existing_replay.clone());
    }
    {
        let mut stats_replays = state
            .stats_replays
            .lock()
            .expect("stats replay mutex should not be poisoned");
        stats_replays.push(existing_replay.clone());
        stats_replays.push(ReplayInfo {
            file: updated_replay.file.clone(),
            date: 50,
            result: "Victory".to_string(),
            ..ReplayInfo::default()
        });
    }
    {
        let mut current_files = state
            .stats_current_replay_files
            .lock()
            .expect("current replay file mutex should not be poisoned");
        current_files.insert(existing_replay.file.clone());
    }

    state.upsert_replay_in_memory_cache(&updated_replay);

    let replays = state.replay_cache_snapshot();
    let stats_replays = state
        .stats_replays
        .lock()
        .expect("stats replay mutex should not be poisoned")
        .clone();
    let current_files = state
        .stats_current_replay_files
        .lock()
        .expect("current replay file mutex should not be poisoned")
        .clone();
    let selected_file = state.get_current_replay_file();

    assert_eq!(replays.len(), 2);
    assert_eq!(stats_replays.len(), 2);
    assert_eq!(replays[0].file, updated_replay.file);
    assert_eq!(stats_replays[0].file, updated_replay.file);
    assert_eq!(replays[0].result, updated_replay.result);
    assert_eq!(stats_replays[0].result, updated_replay.result);
    assert!(current_files.contains(&existing_replay.file));
    assert!(current_files.contains(&updated_replay.file));
    assert_eq!(selected_file.as_deref(), Some(updated_replay.file.as_str()));
}

#[test]
fn upsert_replay_in_memory_cache_refreshes_ready_stats_with_detailed_data() {
    let state = test_backend_state();
    let existing_replay = ReplayInfo {
        file: test_replay_path("existing_detailed.SC2Replay"),
        date: 100,
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        p1: "Existing Main".to_string(),
        p2: "Existing Ally".to_string(),
        p1_handle: "1-S2-1-111".to_string(),
        p2_handle: "1-S2-1-222".to_string(),
        main_commander: "Raynor".to_string(),
        ally_commander: "Karax".to_string(),
        main_units: json!({
            "Marine": [3, 1, 9, 0.5]
        }),
        ..ReplayInfo::default()
    };
    let updated_replay = ReplayInfo {
        file: test_replay_path("new_detailed.SC2Replay"),
        date: 200,
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        p1: "Updated Main".to_string(),
        p2: "Updated Ally".to_string(),
        p1_handle: "1-S2-1-333".to_string(),
        p2_handle: "1-S2-1-444".to_string(),
        main_commander: "Fenix".to_string(),
        ally_commander: "Karax".to_string(),
        main_units: json!({
            "Adept": [6, 1, 23, 0.5]
        }),
        ..ReplayInfo::default()
    };

    {
        let mut stats = state
            .stats
            .lock()
            .expect("stats mutex should not be poisoned");
        stats.ready = true;
        stats.analysis = Some(json!({
            "MapData": {},
            "CommanderData": {},
            "AllyCommanderData": {},
            "DifficultyData": {},
            "RegionData": {},
            "UnitData": Value::Null,
            "AmonData": {},
            "PlayerData": {},
        }));
        stats.message = "Scanned 1 replay file(s).".to_string();
    }
    {
        let replay_state = state.get_replay_state();
        let replays_slot = replay_state
            .lock()
            .expect("replay state mutex should not be poisoned")
            .replays
            .clone();
        let mut replays = replays_slot
            .lock()
            .expect("replays mutex should not be poisoned");
        replays.push(existing_replay.clone());
    }
    {
        let mut stats_replays = state
            .stats_replays
            .lock()
            .expect("stats replay mutex should not be poisoned");
        stats_replays.push(existing_replay);
    }

    state.upsert_replay_in_memory_cache(&updated_replay);

    let stats = state
        .stats
        .lock()
        .expect("stats mutex should not be poisoned");
    let analysis = stats
        .analysis
        .clone()
        .expect("analysis should be present after refresh");

    assert_eq!(stats.games, 2);
    assert_eq!(stats.message, "Scanned 2 replay file(s).");
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
