use sco_tauri_overlay::AppSettings;
use serde_json::json;

#[test]
fn simple_analysis_worker_threads_always_uses_half_of_logical_cores() {
    let logical_cores = AppSettings::logical_core_count();
    let settings = AppSettings::merge_settings_with_defaults(json!({
        "analysis_worker_threads": logical_cores,
    }));

    assert_eq!(settings.normalized_analysis_worker_threads(), logical_cores);
    assert_eq!(
        AppSettings::simple_analysis_worker_threads(),
        (logical_cores / 2).max(1)
    );
    assert_eq!(
        AppSettings::simple_analysis_worker_threads(),
        AppSettings::default_analysis_worker_threads()
    );
}
