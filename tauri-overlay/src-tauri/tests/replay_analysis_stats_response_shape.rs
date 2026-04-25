use sco_tauri_overlay::ReplayAnalysis;
use sco_tauri_overlay::{ReplayInfo, StatsState};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[test]
fn build_stats_response_returns_raw_stats_payload_shape() {
    let stats = Arc::new(Mutex::new(StatsState::default()));
    let replays = Arc::new(Mutex::new(HashMap::<String, ReplayInfo>::new()));
    let current_replay_files = Arc::new(Mutex::new(HashSet::<String>::new()));

    let payload = ReplayAnalysis::build_stats_response(
        "/config/stats?show_all=1",
        &stats,
        &replays,
        &current_replay_files,
    )
    .expect("stats response should build");

    assert!(payload.get("ready").is_some(), "ready must be top-level");
    assert!(
        payload.get("message").is_some(),
        "message must be top-level"
    );
    assert_eq!(
        payload.get("query").and_then(|value| value.as_str()),
        Some("show_all=1")
    );
    assert!(
        payload.get("status").is_none(),
        "config_stats_get expects a raw stats payload, not a wrapped response"
    );
    assert!(
        payload.get("stats").is_none(),
        "config_stats_get expects fields like ready/message at the top level"
    );
}
