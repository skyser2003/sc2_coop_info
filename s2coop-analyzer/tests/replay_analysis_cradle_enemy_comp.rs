mod common;

use s2coop_analyzer::detailed_replay_analysis::DetailedReplayAnalyzer;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

struct CradleReplayFixture;

impl CradleReplayFixture {
    fn read_env_file_value(env_file: &Path, key: &str) -> Option<String> {
        let content = fs::read_to_string(env_file).ok()?;
        content.lines().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (current_key, raw_value) = trimmed.split_once('=')?;
            if current_key.trim() != key {
                return None;
            }
            let value = raw_value.trim().trim_matches('"').trim_matches('\'');
            (!value.is_empty()).then(|| value.to_string())
        })
    }

    fn account_dir() -> Option<PathBuf> {
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

        let env_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()?
            .join(".env");
        for key in [
            "SC2_ACCOUNT_PATH",
            "SC2_ACCOUNT_PATH_WINDOWS",
            "SC2_ACCOUNT_PATH_LINUX",
        ] {
            if let Some(path) = Self::read_env_file_value(&env_file, key).map(PathBuf::from)
                && path.is_dir()
            {
                return Some(path);
            }
        }
        None
    }

    fn find_replay(account_dir: &Path, replay_name: &str) -> Option<PathBuf> {
        let mut matches = WalkDir::new(account_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file() && entry.file_name() == replay_name)
            .map(|entry| entry.into_path())
            .collect::<Vec<PathBuf>>();
        matches.sort();
        matches.into_iter().next()
    }

    fn analyzed_comp(replay_name: &str) -> Option<String> {
        let account_dir = Self::account_dir()?;
        let replay_path = Self::find_replay(&account_dir, replay_name)?;
        let resources = common::load_replay_resources();
        let result = DetailedReplayAnalyzer::analyze_single_detailed(
            &replay_path,
            &HashSet::new(),
            &resources,
        )
        .unwrap_or_else(|error| panic!("{replay_name} should analyze: {error}"));
        Some(result.report().comp.clone())
    }
}

#[test]
fn short_cradle_replay_identifies_comp_from_startup_removed_units() {
    let Some(comp) = CradleReplayFixture::analyzed_comp("죽음의 요람 (129).SC2Replay") else {
        eprintln!("skipping short Cradle composition regression test: replay not configured");
        return;
    };

    assert_eq!(comp, "Shadow Disruption");
}

#[test]
fn full_cradle_replay_keeps_spawned_wave_comp() {
    let Some(comp) = CradleReplayFixture::analyzed_comp("죽음의 요람 (128).SC2Replay") else {
        eprintln!("skipping full Cradle composition regression test: replay not configured");
        return;
    };

    assert_eq!(comp, "Explosive Threats");
}
