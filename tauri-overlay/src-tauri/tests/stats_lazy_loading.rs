use sco_tauri_overlay::{
    AnalysisMode, ReplayScanProgressPayload, StatsSnapshot, StatsState, TauriOverlayOps,
};

#[test]
fn cache_ready_snapshot_keeps_statistics_payload_empty_until_statistics_request() {
    let mut stats = StatsState::default();
    let snapshot = StatsSnapshot::new(
        true,
        0,
        Vec::new(),
        Vec::new(),
        TauriOverlayOps::empty_stats_payload(),
        Default::default(),
        "Detailed analysis cache generation completed.",
    );

    TauriOverlayOps::apply_rebuild_snapshot(&mut stats, snapshot, AnalysisMode::Detailed);

    let payload = stats.as_payload_typed(ReplayScanProgressPayload::default());
    assert!(payload.ready);
    assert_eq!(payload.games, 0);
    let analysis = payload
        .analysis
        .expect("empty analysis payload should exist");
    assert!(analysis.map_data.is_empty());
    assert!(!payload.analysis_running);
    assert!(!stats.should_start_lazy_statistics_analysis());
}

#[test]
fn lazy_statistics_analysis_starts_only_when_statistics_are_not_ready_or_running() {
    let mut ready = StatsState::default();
    ready.set_ready(true);

    let mut running = StatsState::default();
    running.start_analysis(AnalysisMode::Simple);

    assert!(StatsState::default().should_start_lazy_statistics_analysis());
    assert!(!ready.should_start_lazy_statistics_analysis());
    assert!(!running.should_start_lazy_statistics_analysis());
}
