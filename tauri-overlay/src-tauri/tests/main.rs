mod common;

use common::test_replay_path;
use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::*;
use serde_json::json;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[test]
fn sync_replay_cache_slots_uses_cached_entries_and_sets_selected_file() {
    let replay_path = test_replay_path("example.SC2Replay");
    let replays_slot = Arc::new(Mutex::new(vec![ReplayInfo {
        file: replay_path.clone(),
        date: 123,
        result: "Victory".to_string(),
        ..ReplayInfo::default()
    }]));
    let selected_replay_file = Arc::new(Mutex::new(None));

    let replays = sync_replay_cache_slots(&replays_slot, &selected_replay_file, 1);

    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].file.as_str(), replay_path.as_str());
    assert_eq!(
        selected_replay_file
            .lock()
            .expect("selected replay mutex should not be poisoned")
            .as_deref(),
        Some(replay_path.as_str())
    );
}

#[test]
fn prepare_startup_analysis_request_marks_once_and_preserves_existing_status() {
    let mut stats = StatsState {
        detailed_analysis_atstart: true,
        ..StatsState::default()
    };

    let first = prepare_startup_analysis_request(&mut stats, StartupAnalysisTrigger::Setup);

    assert_eq!(
        first,
        StartupAnalysisRequestOutcome {
            include_detailed: true,
            started: true,
        }
    );
    assert!(stats.startup_analysis_requested);
    assert_eq!(
        stats.message,
        "Detailed analysis: startup requested while the frontend loads."
    );

    stats.message = "Detailed analysis: generating cache.".to_string();

    let second =
        prepare_startup_analysis_request(&mut stats, StartupAnalysisTrigger::FrontendReady);

    assert_eq!(
        second,
        StartupAnalysisRequestOutcome {
            include_detailed: true,
            started: false,
        }
    );
    assert_eq!(stats.message, "Detailed analysis: generating cache.");
}

#[test]
fn parse_detailed_analysis_progress_counts_reads_running_line() {
    assert_eq!(
        parse_detailed_analysis_progress_counts("Running... 12/34 (35%)"),
        Some((12, 34))
    );
    assert_eq!(
        parse_detailed_analysis_progress_counts(
            "Estimated remaining time: 01:02:03\nRunning... 56/78 (71%)"
        ),
        Some((56, 78))
    );
}

#[test]
fn parse_detailed_analysis_progress_counts_reads_completion_line() {
    assert_eq!(
        parse_detailed_analysis_progress_counts("Detailed analysis completed! 90/90 | 100%"),
        Some((90, 90))
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
    assert!(!stats.detailed_analysis_running);
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
