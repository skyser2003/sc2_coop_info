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

fn write_sized_replay_file(path: &Path, size_bytes: usize) {
    fs::create_dir_all(
        path.parent()
            .expect("test replay path must have parent directory"),
    )
    .expect("failed to create replay directory");
    fs::write(path, vec![0_u8; size_bytes]).expect("failed to write replay file");
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
    assert_eq!(first_summary.timing_report().total_replay_files(), 24);
    assert!(first_summary.timing_report().total() > std::time::Duration::ZERO);
    assert!(first_summary.timing_report().serial_wall_fraction() >= 0.0);
    assert_eq!(
        first_summary.timing_report().canonicalize_worker_count(),
        first_summary.timing_report().worker_count()
    );
    let timing_summary = first_summary.timing_report().format_amdahl_summary();
    assert!(timing_summary.contains("parse_detailed parts"));
    assert!(timing_summary.contains("parse_detailed parts decode"));

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

#[test]
fn detailed_analysis_priority_schedules_largest_replays_first() {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let replay_dir = temp_dir.path().join("Accounts").join("1-S2-1-42");
    let small = replay_dir.join("small.SC2Replay");
    let medium = replay_dir.join("medium.SC2Replay");
    let large = replay_dir.join("large.SC2Replay");

    write_sized_replay_file(&small, 10);
    write_sized_replay_file(&medium, 20);
    write_sized_replay_file(&large, 30);

    let mut replay_paths = vec![small, large, medium];
    DetailedReplayAnalyzer::sort_replay_paths_by_detailed_analysis_priority(&mut replay_paths);

    let replay_names = replay_paths
        .iter()
        .map(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .expect("replay path should include valid file name")
                .to_string()
        })
        .collect::<Vec<String>>();

    assert_eq!(
        replay_names,
        vec![
            "large.SC2Replay".to_string(),
            "medium.SC2Replay".to_string(),
            "small.SC2Replay".to_string()
        ]
    );
}
