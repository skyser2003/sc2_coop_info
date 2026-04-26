mod common;

use s2coop_analyzer::cache_overall_stats_detailed_analysis::CacheAnalysisPaths;
use s2coop_analyzer::detailed_replay_analysis::DetailedReplayAnalyzer;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const PART_AND_PARCEL_REPLAY_NAME: &str = "핵심 부품 (144).SC2Replay";
const MAIN_PLAYER_HANDLE_ENV_KEY: &str = "MAIN_PLAYER_HANDLE";
const PARTS_ICON_KEY: &str = "parts";

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

fn resolve_env_file_value(key: &str) -> Option<String> {
    read_env_file_value(&CacheAnalysisPaths::repo_root().join(".env"), key)
}

fn resolve_main_handles() -> Option<HashSet<String>> {
    let value = std::env::var(MAIN_PLAYER_HANDLE_ENV_KEY)
        .ok()
        .or_else(|| resolve_env_file_value(MAIN_PLAYER_HANDLE_ENV_KEY))?;
    let handles = value
        .split([',', ';'])
        .map(str::trim)
        .filter(|handle| !handle.is_empty())
        .map(str::to_string)
        .collect::<HashSet<String>>();
    if handles.is_empty() {
        None
    } else {
        Some(handles)
    }
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

fn icon_count(icons: &BTreeMap<String, u64>, key: &str) -> u64 {
    icons.get(key).copied().unwrap_or_default()
}

#[test]
fn part_and_parcel_replay_shows_collected_parts_icon_per_player() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!(
            "skipping Part and Parcel parts regression test: no SC2 account directory configured"
        );
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, PART_AND_PARCEL_REPLAY_NAME) else {
        eprintln!(
            "skipping Part and Parcel parts regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };
    let Some(main_handles) = resolve_main_handles() else {
        eprintln!(
            "skipping Part and Parcel parts regression test: {MAIN_PLAYER_HANDLE_ENV_KEY} is not configured"
        );
        return;
    };

    let resources = common::load_replay_resources();
    let result =
        DetailedReplayAnalyzer::analyze_single_detailed(&replay_path, &main_handles, &resources)
            .unwrap_or_else(|error| panic!("replay analysis should succeed: {error}"));
    let report = result.report();

    assert_eq!(report.map_name, "Part and Parcel");
    assert_eq!(report.positions.main, 2);
    assert_eq!(report.positions.ally, 1);
    assert_eq!(report.main_commander, "Abathur");
    assert_eq!(report.ally_commander, "Stukov");
    assert_eq!(icon_count(&report.main_icons, PARTS_ICON_KEY), 104);
    assert_eq!(icon_count(&report.ally_icons, PARTS_ICON_KEY), 120);
}
