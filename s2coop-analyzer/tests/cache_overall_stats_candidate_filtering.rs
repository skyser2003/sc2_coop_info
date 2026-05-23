mod common;

use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, GenerateCacheConfig, GenerateCacheRuntimeOptions,
    ReplayCacheFileIdentity, ReplayFileIdentity,
};
use std::collections::HashMap;
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
    let summary =
        DetailedReplayAnalyzer::analyze_full_detailed(&config, &resources, None, &runtime)
            .expect("cache generation should succeed for invalid replay placeholders");

    assert_eq!(summary.scanned_replays(), 0);
    assert!(summary.cache_entries().is_empty());
    assert!(
        !output_file.exists(),
        "legacy cache json should not be written"
    );
}

#[test]
fn detailed_cache_reuse_uses_hash_and_modified_time_without_file_path() {
    let resources = common::load_replay_resources();
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");
    fs::create_dir_all(&account_dir).expect("failed to create account directory");
    let replay_path = account_dir.join("renamed-invalid.SC2Replay");
    fs::write(&replay_path, b"not a replay").expect("failed to write invalid replay placeholder");

    let hash = ReplayFileIdentity::calculate_hash(&replay_path);
    let modified_seconds =
        ReplayFileIdentity::modified_seconds(&replay_path).expect("mtime should load");
    let mut identities_by_hash = HashMap::new();
    identities_by_hash.insert(
        hash.clone(),
        ReplayCacheFileIdentity::new(hash, modified_seconds),
    );

    let output_file = temp_dir.path().join("cache_overall_stats");
    let config = GenerateCacheConfig::new(account_dir, output_file.clone());
    let runtime = GenerateCacheRuntimeOptions::default()
        .with_existing_detailed_cache_identities_by_hash(identities_by_hash);
    let summary =
        DetailedReplayAnalyzer::analyze_full_detailed(&config, &resources, None, &runtime)
            .expect("cache generation should reuse existing checked identity");

    assert_eq!(summary.timing_report().candidate_count(), 1);
    assert_eq!(summary.timing_report().reused_candidate_count(), 1);
    assert_eq!(summary.timing_report().pending_candidate_count(), 0);
    assert_eq!(summary.scanned_replays(), 0);
    assert!(summary.cache_entries().is_empty());
    assert!(
        !output_file.exists(),
        "legacy cache json should not be written"
    );
}
