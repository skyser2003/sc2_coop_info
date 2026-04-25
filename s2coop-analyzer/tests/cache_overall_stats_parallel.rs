mod common;

use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, GenerateCacheConfig, GenerateCacheRuntimeOptions,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write_replay_file(path: &Path) {
    fs::create_dir_all(
        path.parent()
            .expect("test replay path must have parent directory"),
    )
    .expect("failed to create replay directory");
    fs::write(path, b"SC2ReplayTestData").expect("failed to write replay file");
}

#[test]
fn generate_cache_parallel_runs_are_deterministic() {
    let resources = common::load_replay_resources();
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");

    for index in 0..24 {
        let account_id = if index % 2 == 0 {
            "2-S2-1-111"
        } else {
            "1-S2-1-42"
        };
        let replay_name = format!("Replay_{index:02}.SC2Replay");
        write_replay_file(&account_dir.join(account_id).join(replay_name));
    }

    let first_output = temp_dir.path().join("cache_overall_stats_first");
    let second_output = temp_dir.path().join("cache_overall_stats_second");

    let runtime = GenerateCacheRuntimeOptions::default();
    let first_config = GenerateCacheConfig::new(account_dir.clone(), first_output.clone());
    let first_summary =
        DetailedReplayAnalyzer::analyze_full_detailed(&first_config, &resources, None, &runtime)
            .expect("first cache generation should succeed");
    let second_config = GenerateCacheConfig::new(account_dir, second_output.clone());
    let second_summary =
        DetailedReplayAnalyzer::analyze_full_detailed(&second_config, &resources, None, &runtime)
            .expect("second cache generation should succeed");

    assert_eq!(first_summary.scanned_replays(), 0);
    assert_eq!(second_summary.scanned_replays(), 0);

    let first_entries: Vec<CacheReplayEntry> = serde_json::from_str(
        &fs::read_to_string(first_output).expect("first cache file should exist"),
    )
    .expect("first cache should deserialize");
    let second_entries: Vec<CacheReplayEntry> = serde_json::from_str(
        &fs::read_to_string(second_output).expect("second cache file should exist"),
    )
    .expect("second cache should deserialize");

    assert_eq!(first_entries, second_entries);
    assert!(first_entries.is_empty());
}
