#![cfg(not(windows))]

mod common;

use common::test_replay_path;
use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::{
    canonicalize_coop_map_id, sync_detailed_analysis_status_from_replays, BackendState, ReplayInfo,
    StatsState,
};
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;

#[test]
fn sync_replay_cache_slots_uses_cached_entries_and_sets_selected_file() {
    let replay_path = test_replay_path("example.SC2Replay");
    let state = BackendState::new();
    {
        let replay_state = state.get_replay_state();
        let replay_slots = replay_state
            .lock()
            .expect("replay state mutex should not be poisoned");
        let mut replays = replay_slots
            .replays
            .lock()
            .expect("replays mutex should not be poisoned");
        replays.insert(
            "example-hash".to_string(),
            ReplayInfo {
                file: replay_path.clone(),
                date: 123,
                result: "Victory".to_string(),
                ..ReplayInfo::default()
            },
        );
    }

    let replays = state.sync_replay_cache_slots(1);

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].file.as_str(), replay_path.as_str());
    assert_eq!(
        state.get_current_replay_file().as_deref(),
        Some(replay_path.as_str())
    );
}

#[test]
fn sync_detailed_analysis_status_from_replays_reports_cached_progress() {
    let mut stats = StatsState::default();
    let detailed_replay = ReplayInfo {
        file: test_replay_path("detailed.SC2Replay"),
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        main_units: json!({
            "Marine": [4, 1, 10, 0.5]
        }),
        ..ReplayInfo::default()
    };
    let simple_replay = ReplayInfo {
        file: test_replay_path("simple.SC2Replay"),
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        ..ReplayInfo::default()
    };

    sync_detailed_analysis_status_from_replays(&mut stats, &[detailed_replay, simple_replay]);

    assert_eq!(
        stats.detailed_analysis_status,
        "Detailed analysis: loaded from cache (1/2)."
    );
    assert!(!stats.analysis_running);
}

#[test]
fn should_include_detailed_stats_response_uses_cached_detailed_replays() {
    let response = json!({
        "analysis": {
            "UnitData": Value::Null
        }
    });
    let cached_replays = vec![ReplayInfo {
        file: test_replay_path("cached_detailed.SC2Replay"),
        main_units: json!({
            "Marine": [4, 1, 10, 0.5]
        }),
        ..ReplayInfo::default()
    }];

    assert!(ReplayAnalysis::should_include_detailed_stats_response(
        &response,
        &cached_replays
    ));
}

#[test]
fn backend_state_reuses_cached_dictionary_and_resources() {
    let state = BackendState::new();

    let dictionary_a = state
        .dictionary_data()
        .expect("dictionary data should load from backend state");
    let dictionary_b = state
        .dictionary_data()
        .expect("dictionary data should be cached in backend state");
    assert!(Arc::ptr_eq(&dictionary_a, &dictionary_b));

    let resources_a = state
        .replay_analysis_resources()
        .expect("replay analysis resources should load from backend state");
    let resources_b = state
        .replay_analysis_resources()
        .expect("replay analysis resources should be cached in backend state");
    assert!(Arc::ptr_eq(&resources_a, &resources_b));
}
