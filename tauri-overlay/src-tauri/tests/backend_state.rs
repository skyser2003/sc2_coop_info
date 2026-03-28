use sco_tauri_overlay::{
    parse_detailed_analysis_progress_counts, prepare_startup_analysis_request,
    StartupAnalysisRequestOutcome, StartupAnalysisTrigger, StatsState,
};

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
