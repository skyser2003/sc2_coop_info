use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestCacheOverallStatsDetailedAnalysisArgs {
    pub account_dir: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub original_output: Option<PathBuf>,
    pub help_requested: bool,
}

#[derive(Debug, Error)]
pub enum TestCacheOverallStatsDetailedAnalysisError {
    #[error(
        "legacy cache_overall_stats JSON comparison is disabled; SQLite cache is authoritative"
    )]
    LegacyJsonComparisonDisabled,
}

pub struct CacheOverallStatsDetailedAnalysis;

impl CacheOverallStatsDetailedAnalysis {
    pub fn run(
        _args: &TestCacheOverallStatsDetailedAnalysisArgs,
        _logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
    ) -> Result<String, TestCacheOverallStatsDetailedAnalysisError> {
        Ok(
            "skipping exact parity test: legacy cache_overall_stats JSON output is disabled; SQLite cache is authoritative"
                .to_string(),
        )
    }
}

pub struct CacheAnalysisPaths;

impl CacheAnalysisPaths {
    pub fn runtime_root() -> PathBuf {
        let manifest_dir_str = std::env::var("CARGO_MANIFEST_DIR");

        match manifest_dir_str {
            Ok(manifest_dir_str) => PathBuf::from(manifest_dir_str),
            Err(_) => {
                if let Ok(abs) = std::env::current_exe()
                    && let Some(parent) = abs.parent()
                {
                    return parent.to_path_buf();
                }

                PathBuf::from("./")
            }
        }
    }

    pub fn repo_root() -> PathBuf {
        Self::runtime_root()
            .parent()
            .expect("crate manifest directory should have repo root parent")
            .to_path_buf()
    }

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

    pub fn resolve_account_dir() -> Option<PathBuf> {
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

        let env_path = Self::repo_root().join(".env");
        for key in [
            "SC2_ACCOUNT_PATH",
            "SC2_ACCOUNT_PATH_WINDOWS",
            "SC2_ACCOUNT_PATH_LINUX",
        ] {
            if let Some(value) = Self::read_env_file_value(&env_path, key) {
                let path = PathBuf::from(value);
                if path.is_dir() {
                    return Some(path);
                }
            }
        }

        None
    }

    pub fn count_replays(root: &Path) -> usize {
        WalkDir::new(root)
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
            .count()
    }
}
