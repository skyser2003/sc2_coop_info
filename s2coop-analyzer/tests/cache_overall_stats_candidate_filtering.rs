mod common;

use s2coop_analyzer::cache_overall_stats_generator::pretty_output_path;
use s2coop_analyzer::detailed_replay_analysis::{
    analyze_full_detailed, GenerateCacheConfig, GenerateCacheRuntimeOptions,
};
use std::fs;
use tempfile::TempDir;

#[test]
fn generate_cache_skips_invalid_replay_candidates() {
    let resources = common::load_replay_resources();
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");
    fs::create_dir_all(&account_dir).expect("failed to create account directory");
    fs::write(account_dir.join("invalid.SC2Replay"), b"not a replay")
        .expect("failed to write invalid replay placeholder");

    let output_file = temp_dir.path().join("cache_overall_stats");
    let config = GenerateCacheConfig::new(account_dir, output_file.clone());
    let runtime = GenerateCacheRuntimeOptions::default();
    let summary = analyze_full_detailed(&config, &resources, None, &runtime)
        .expect("cache generation should succeed for invalid replay placeholders");

    assert_eq!(summary.scanned_replays(), 0);
    assert!(output_file.is_file(), "cache output should be written");
    assert_eq!(
        fs::read_to_string(&output_file).expect("cache output should be readable"),
        "[]"
    );

    let pretty_output = pretty_output_path(&output_file);
    assert!(
        pretty_output.is_file(),
        "pretty cache output should be written"
    );
    assert_eq!(
        fs::read_to_string(pretty_output).expect("pretty cache output should be readable"),
        "[]\n"
    );
}
