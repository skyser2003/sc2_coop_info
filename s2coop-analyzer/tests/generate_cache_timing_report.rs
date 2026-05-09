mod common;

use s2coop_analyzer::cache_overall_stats_detailed_analysis::CacheAnalysisPaths;
use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, GenerateCacheConfig, GenerateCacheRuntimeOptions,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use walkdir::WalkDir;

fn read_env_file_value(env_file: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(env_file).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((current_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if current_key.trim() != key {
            continue;
        }
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        return Some(value.to_string());
    }
    None
}

fn resolve_account_dir() -> Option<PathBuf> {
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    let env_path = CacheAnalysisPaths::repo_root().join(".env");
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Some(value) = read_env_file_value(&env_path, key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    None
}

fn find_replay(root: &Path, replay_name: &str) -> Option<PathBuf> {
    let mut matches = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|value| value == replay_name)
        })
        .map(|entry| entry.into_path())
        .collect::<Vec<PathBuf>>();
    matches.sort();
    matches.into_iter().next()
}

#[test]
fn detailed_report_timings_are_aggregated_when_enabled() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping detailed timing aggregation test: no SC2 account directory configured");
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping detailed timing aggregation test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let replay_dir = temp_dir.path().join("Accounts").join("1-S2-1-42");
    fs::create_dir_all(&replay_dir).expect("failed to create replay directory");
    fs::copy(&replay_path, replay_dir.join("fixture.SC2Replay"))
        .expect("failed to copy replay fixture");

    let resources = common::load_replay_resources();
    let output_file = temp_dir.path().join("cache_overall_stats.json");
    let config = GenerateCacheConfig::new(temp_dir.path().join("Accounts"), output_file)
        .with_recent_replay_count(Some(1));
    let runtime = GenerateCacheRuntimeOptions::default()
        .with_worker_count(1)
        .with_detailed_report_timings(true);

    let summary =
        DetailedReplayAnalyzer::analyze_full_detailed(&config, &resources, None, &runtime)
            .expect("cache generation should succeed");
    let detailed_timing = summary
        .timing_report()
        .replay_analysis_detailed_report_breakdown();
    let timing_report = summary.timing_report();

    assert_eq!(summary.scanned_replays(), 1);
    assert!(detailed_timing.has_timings());
    assert!(detailed_timing.total() > Duration::ZERO);
    assert!(detailed_timing.events_input_count() > 0);
    assert!(detailed_timing.events_total() > Duration::ZERO);
    assert!(
        timing_report.replay_events_decoded_len() >= timing_report.replay_events_retained_len()
    );
    assert!(timing_report.replay_events_retained_len() > 0);
    assert!(
        timing_report.replay_events_retained_capacity()
            >= timing_report.replay_events_retained_len()
    );
    assert!(
        timing_report.replay_events_retained_capacity()
            <= timing_report.replay_events_decoded_capacity()
    );

    let timing_summary = summary.timing_report().format_amdahl_summary();
    assert!(timing_summary.contains("hotspot hints"));
    assert!(timing_summary.contains("retained_event_capacity_eff"));
    assert!(timing_summary.contains("report_to_cache_entry"));
    assert!(timing_summary.contains("detailed_report parts"));
    assert!(timing_summary.contains("detailed_report conversion"));
    assert!(timing_summary.contains("detailed_report events"));
}
