mod common;

use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, GenerateCacheConfig, GenerateCacheRuntimeOptions,
};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn full_simple_analysis_writes_cache_for_empty_valid_set() {
    let resources = common::load_replay_resources();
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");
    fs::create_dir_all(&account_dir).expect("failed to create account directory");
    fs::write(account_dir.join("invalid.SC2Replay"), b"not a replay")
        .expect("failed to write invalid replay placeholder");

    let output_file = temp_dir.path().join("cache_overall_stats");
    let config = GenerateCacheConfig::new(account_dir, output_file.clone());
    let runtime = GenerateCacheRuntimeOptions::default();
    let summary = DetailedReplayAnalyzer::analyze_full_simple(&config, &resources, None, &runtime)
        .expect("simple analysis should succeed for invalid replay placeholders");

    assert_eq!(summary.scanned_replays(), 0);
    assert!(summary.completed());
    assert!(summary.cache_entries().is_empty());
    assert_eq!(summary.timing_report().total_replay_files(), 1);
    assert_eq!(summary.timing_report().candidate_count(), 1);
    assert_eq!(summary.timing_report().pending_candidate_count(), 1);
    assert_eq!(summary.timing_report().reused_candidate_count(), 0);
    assert_eq!(
        summary.timing_report().replay_analysis_parse_detailed(),
        Duration::ZERO
    );
    assert_eq!(
        summary
            .timing_report()
            .replay_analysis_parse_basic_fallback(),
        Duration::ZERO
    );
    assert!(
        !output_file.exists(),
        "legacy cache json should not be written"
    );
}
