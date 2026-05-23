use sco_tauri_overlay::{
    AppSettings, BackendState, ReplayInfo, StartupAnalysisRequestOutcome, StartupAnalysisTrigger,
    StatsState, TauriOverlayOps,
};

#[test]
fn parse_detailed_analysis_progress_counts_reads_running_line() {
    assert_eq!(
        TauriOverlayOps::parse_detailed_analysis_progress_counts("Running... 12/34 (35%)"),
        Some((12, 34))
    );
    assert_eq!(
        TauriOverlayOps::parse_detailed_analysis_progress_counts(
            "Estimated remaining time: 01:02:03\nRunning... 56/78 (71%)"
        ),
        Some((56, 78))
    );
}

#[test]
fn parse_detailed_analysis_progress_counts_reads_completion_line() {
    assert_eq!(
        TauriOverlayOps::parse_detailed_analysis_progress_counts(
            "Detailed analysis completed! 90/90 | 100%"
        ),
        Some((90, 90))
    );
}

#[test]
fn prepare_startup_analysis_request_marks_once_and_preserves_existing_status() {
    let mut stats = StatsState::default().with_detailed_analysis_atstart(true);

    let first = TauriOverlayOps::prepare_startup_analysis_request(
        &mut stats,
        StartupAnalysisTrigger::Setup,
    );

    assert_eq!(first, StartupAnalysisRequestOutcome::new(true, true));
    assert!(stats.startup_analysis_requested());
    assert_eq!(
        stats.message(),
        "Detailed analysis: startup requested while the frontend loads."
    );

    stats.set_message("Detailed analysis: generating cache.");

    let second = TauriOverlayOps::prepare_startup_analysis_request(
        &mut stats,
        StartupAnalysisTrigger::FrontendReady,
    );

    assert_eq!(second, StartupAnalysisRequestOutcome::new(true, false));
    assert_eq!(stats.message(), "Detailed analysis: generating cache.");
}

#[test]
fn backend_state_flags_are_instance_local() {
    let disabled_logging = AppSettings::default().with_enable_logging(false);

    let first = BackendState::new_with_settings(disabled_logging);
    let second = BackendState::new();

    assert!(!first.file_logging_enabled());
    assert!(second.file_logging_enabled());
    assert!(!first.performance_edit_mode());
    assert!(!second.performance_edit_mode());

    first.set_performance_edit_mode(true);

    assert!(first.performance_edit_mode());
    assert!(!second.performance_edit_mode());
}

#[test]
fn replace_active_settings_updates_file_logging_flag() {
    let settings = AppSettings::default().with_enable_logging(false);

    let state = BackendState::new_with_settings(settings);
    assert!(!state.file_logging_enabled());

    let mut next = state.read_settings_memory();
    next.set_enable_logging(true);
    state.replace_active_settings(&next);

    assert!(state.file_logging_enabled());
}

#[test]
fn record_replay_cache_update_if_persistable_skips_nonpersistable_replays() {
    let state = BackendState::new();
    let mut replay = ReplayInfo::default();
    replay.set_file("invalid.SC2Replay");
    replay.set_date(100);
    replay.set_result("Victory");

    let stored = state.record_replay_cache_update_if_persistable(&replay, false);

    assert!(!stored);
    assert_eq!(state.get_current_replay_file(), None);
}

#[test]
fn record_replay_cache_update_if_persistable_selects_persistable_replay() {
    let state = BackendState::new();
    let mut replay = ReplayInfo::default();
    replay.set_file("valid.SC2Replay");
    replay.set_date(100);
    replay.set_result("Victory");

    let stored = state.record_replay_cache_update_if_persistable(&replay, true);

    assert!(stored);
    assert_eq!(
        state.get_current_replay_file().as_deref(),
        Some("valid.SC2Replay")
    );
}
