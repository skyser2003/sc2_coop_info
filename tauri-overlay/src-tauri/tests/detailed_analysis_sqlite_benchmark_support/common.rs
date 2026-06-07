use s2coop_analyzer::cache_overall_stats_detailed_analysis::CacheAnalysisPaths;
use s2coop_analyzer::detailed_replay_analysis::GenerateCacheRuntimeOptions;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .saturating_div(2)
        .max(1)
}

pub fn millis(duration: Duration) -> u128 {
    duration.as_millis()
}

pub fn signed_millis(duration: Duration) -> i128 {
    duration.as_millis() as i128
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

pub fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub fn unique_benchmark_root() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sco_detailed_sqlite_bench_{timestamp}"))
}

fn sqlite_sidecar_paths(path: &Path) -> [PathBuf; 3] {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_os_string();
    shm.push("-shm");
    [path.to_path_buf(), PathBuf::from(wal), PathBuf::from(shm)]
}

pub fn remove_sqlite_files(path: &Path) {
    for path in sqlite_sidecar_paths(path) {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("failed to remove '{}': {error}", path.display()),
        }
    }
}

pub fn benchmark_runtime(worker_count: usize, batch_size: usize) -> GenerateCacheRuntimeOptions {
    GenerateCacheRuntimeOptions::default()
        .with_worker_count(worker_count)
        .with_timings_enabled(true)
        .with_cache_entry_sink_batch_size(batch_size)
}
