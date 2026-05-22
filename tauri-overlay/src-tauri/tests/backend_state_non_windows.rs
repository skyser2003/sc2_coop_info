#![cfg(not(windows))]

use sco_tauri_overlay::{BackendState, ReplayInfo, ReplayPlayerInfo, StatsState};
use sco_tauri_overlay::{ReplayAnalysis, TestHelperOps};
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;

#[test]
fn replay_cache_slot_update_sets_selected_file_without_shared_cache() {
    let replay_path = TestHelperOps::test_replay_path("example.SC2Replay");
    let state = BackendState::new();
    let mut replay = ReplayInfo::default();
    replay.set_file(replay_path.clone());
    replay.set_date(123);
    replay.set_result("Victory");

    state.upsert_replay_cache_slot(&replay);

    assert_eq!(
        state.get_current_replay_file().as_deref(),
        Some(replay_path.as_str())
    );
}

#[test]
fn sync_detailed_analysis_status_from_replays_reports_cached_progress() {
    let mut stats = StatsState::default();
    let mut detailed_replay = ReplayInfo::default();
    detailed_replay.set_file(TestHelperOps::test_replay_path("detailed.SC2Replay"));
    detailed_replay
        .set_map(TestHelperOps::canonicalize_map_id("Void Launch").expect("map id should resolve"));
    detailed_replay.set_result("Victory");
    detailed_replay.set_player_stats(
        vec![
            ReplayPlayerInfo::default().with_units(json!({
                "Marine": [4, 1, 10, 0.5]
            })),
            ReplayPlayerInfo::default(),
        ],
        0,
    );
    let mut simple_replay = ReplayInfo::default();
    simple_replay.set_file(TestHelperOps::test_replay_path("simple.SC2Replay"));
    simple_replay
        .set_map(TestHelperOps::canonicalize_map_id("Void Launch").expect("map id should resolve"));
    simple_replay.set_result("Victory");

    stats.sync_detailed_analysis_status_from_replays(&[detailed_replay, simple_replay]);

    assert_eq!(
        stats.detailed_analysis_status(),
        "Detailed analysis: loaded from cache (1/2)."
    );
    assert!(!stats.analysis_running());
}

#[test]
fn stats_response_has_detailed_analysis_reads_unit_payload() {
    let response = json!({
        "analysis": {
            "UnitData": {
                "main": {}
            }
        }
    });

    assert!(ReplayAnalysis::stats_response_has_detailed_analysis(
        &response
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
