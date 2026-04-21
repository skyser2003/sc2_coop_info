mod common;

use s2coop_analyzer::detailed_replay_analysis::analyze_replay_file_with_resources;
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

fn find_replays(root: &Path) -> Vec<PathBuf> {
    let mut replays = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("SC2Replay"))
        })
        .map(|entry| entry.into_path())
        .collect::<Vec<PathBuf>>();
    replays.sort();
    replays
}

#[test]
fn rust_only_path_can_build_report_from_real_replay() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping smoke test: no SC2 account directory configured");
        return;
    };
    let replay_paths = find_replays(&account_dir);
    if replay_paths.is_empty() {
        eprintln!(
            "skipping smoke test: no .SC2Replay files found under {}",
            account_dir.display()
        );
        return;
    }

    let mut attempted = 0_usize;
    let resources = common::load_replay_resources();
    for replay_path in replay_paths {
        attempted += 1;
        let Ok(report) =
            analyze_replay_file_with_resources(&replay_path, &HashSet::new(), &resources)
        else {
            continue;
        };

        let has_non_empty_stats = report
            .player_stats
            .values()
            .any(|stats| !stats.army.is_empty() || !stats.killed.is_empty());
        let has_non_empty_analysis = !report.main_units.is_empty()
            || !report.ally_units.is_empty()
            || !report.amon_units.is_empty()
            || !report.main_icons.is_empty()
            || !report.ally_icons.is_empty()
            || has_non_empty_stats;
        if !has_non_empty_analysis {
            continue;
        }

        assert!(report.replaydata);
        assert!(report.length > 0.0);
        assert!(report.parser.accurate_length > 0.0);
        assert!(report.parser.hash.is_some());
        assert_eq!(report.positions.main, 1);
        assert_eq!(report.positions.ally, 2);
        assert!(report.parser.players.len() >= 3);
        assert_eq!(report.player_stats.len(), 2);
        assert!(report.player_stats.contains_key(&1));
        assert!(report.player_stats.contains_key(&2));
        return;
    }

    panic!(
        "no replay produced non-empty rust-only detailed analysis after checking {attempted} replay(s)"
    );
}
