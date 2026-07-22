use sco_tauri_overlay::{AnalysisStatusPayload, ReplayScanProgressPayload, StatsState};

#[test]
fn analysis_status_payload_excludes_statistics_data_and_messages() {
    let mut stats = StatsState::default();
    stats.set_detailed_analysis_progress(7, 9);
    let payload = AnalysisStatusPayload::new(&stats, ReplayScanProgressPayload::default());
    let value = serde_json::to_value(payload).expect("analysis status should serialize");
    let fields = value
        .as_object()
        .expect("analysis status should serialize as an object");

    assert!(fields.contains_key("detailed_analysis_status"));
    assert_eq!(
        fields.get("current_status"),
        Some(&"Detailed analysis: not started.".into())
    );
    assert!(fields.contains_key("scan_progress"));
    assert_eq!(fields.get("detailed_parsed_count"), Some(&7.into()));
    assert_eq!(fields.get("total_valid_files"), Some(&9.into()));
    assert!(!fields.contains_key("analysis"));
    assert!(!fields.contains_key("message"));
    assert!(!fields.contains_key("games"));
}
