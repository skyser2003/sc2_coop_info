use sco_tauri_overlay::ReplayAnalysis;
use sco_tauri_overlay::StatsState;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[test]
fn build_stats_response_returns_raw_stats_payload_shape() {
    let stats = Arc::new(Mutex::new(StatsState::default()));
    let current_replay_files = Arc::new(Mutex::new(HashSet::<String>::new()));

    let payload = ReplayAnalysis::build_stats_response(
        "/config/stats?show_all=1",
        &stats,
        &current_replay_files,
    )
    .expect("stats response should build");

    assert!(!payload.ready);
    assert!(!payload.message.is_empty(), "message must be top-level");
    assert_eq!(payload.query.as_deref(), Some("show_all=1"));
}
