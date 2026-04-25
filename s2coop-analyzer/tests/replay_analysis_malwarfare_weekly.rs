mod common;

use s2coop_analyzer::detailed_replay_analysis::analyze_single_detailed;
use s2protocol_port::{build_protocol_store, parse_file_with_store, ReplayParseMode};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use s2coop_analyzer::cache_overall_stats_detailed_analysis::repo_root;

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

    let env_path = repo_root().join(".env");
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
fn malwarfare_weekly_replay_with_korean_filename_builds_detailed_report() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!(
            "skipping Malwarfare weekly regression test: no SC2 account directory configured"
        );
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping Malwarfare weekly regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let main_handles = HashSet::new();
    let resources = common::load_replay_resources();
    let store = build_protocol_store().expect("protocol store should build");
    let parsed = parse_file_with_store(&replay_path, &store, ReplayParseMode::Detailed)
        .expect("detailed replay parser should read the replay");
    assert!(!parsed.tracker_events().is_empty());

    let result = analyze_single_detailed(&replay_path, &main_handles, &resources)
        .unwrap_or_else(|error| panic!("replay analysis should succeed: {error}"));
    let report = result.report();

    assert!(report.replaydata);
    assert!(report.weekly);
    assert_eq!(report.map_name, "Malwarfare");
    let commanders = HashSet::from([
        report.main_commander.as_str(),
        report.ally_commander.as_str(),
    ]);
    assert_eq!(commanders, HashSet::from(["Abathur", "Stukov"]));
    assert!(!report.main_units.is_empty());
    assert!(!report.ally_units.is_empty());
    assert!(!report.player_stats.is_empty());
}
