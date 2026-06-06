use sco_tauri_overlay::{
    AnalysisMode, ReplayAnalysis, ReplayCacheStatisticsPayload, ReplayScanProgressPayload,
    StatsState, TauriOverlayOps,
};

#[test]
fn cached_statistics_payload_overlays_startup_running_state() {
    let mut stats = StatsState::default();
    stats.start_analysis(AnalysisMode::Simple);
    stats.set_message("Simple analysis: started in background.");

    let mut response = stats.as_payload_typed(ReplayScanProgressPayload::default());
    assert!(!response.ready);
    assert!(response.analysis_running);
    assert_eq!(response.games, 0);
    assert_eq!(response.detailed_parsed_count, 0);
    assert_eq!(response.total_valid_files, 0);

    let cached_payload = ReplayCacheStatisticsPayload::new(
        TauriOverlayOps::empty_stats_payload(),
        Default::default(),
        12,
        9,
        12,
        vec!["Main".to_string()],
        vec!["1-S2-1-123".to_string()],
    );

    ReplayAnalysis::apply_cached_statistics_payload(&mut response, &cached_payload)
        .expect("cached statistics payload should be valid");

    assert!(response.ready);
    assert!(response.analysis_running);
    assert_eq!(response.games, 12);
    assert_eq!(response.detailed_parsed_count, 9);
    assert_eq!(response.total_valid_files, 12);
    assert_eq!(response.main_players, vec!["Main".to_string()]);
    assert_eq!(response.main_handles, vec!["1-S2-1-123".to_string()]);
    assert_eq!(response.message, "Simple analysis: started in background.");
}
