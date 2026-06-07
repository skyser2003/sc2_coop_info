use super::{ACCOUNT_ENV_KEYS, ComparisonError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub struct AccountDirectoryResolver;

impl AccountDirectoryResolver {
    pub fn resolve() -> Option<PathBuf> {
        for key in ACCOUNT_ENV_KEYS {
            let Ok(value) = std::env::var(key) else {
                continue;
            };
            let candidate = PathBuf::from(value.trim().trim_matches('"').trim_matches('\''));
            if candidate.is_dir() {
                return candidate.canonicalize().ok().or(Some(candidate));
            }
        }
        None
    }
}

pub struct RecentReplaySubset;

impl RecentReplaySubset {
    pub fn copy_recent_replays(
        source_account_dir: &Path,
        destination_account_dir: &Path,
        replay_count: usize,
    ) -> Result<usize, ComparisonError> {
        if replay_count == 0 {
            return Err(ComparisonError::InvalidRecentReplayCount);
        }

        let mut replay_files = WalkDir::new(source_account_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("SC2Replay"))
            })
            .map(|path| ReplayFileCandidate::from_path(path.as_path()))
            .collect::<Vec<ReplayFileCandidate>>();
        replay_files.sort_by(ReplayFileCandidate::compare_recent_first);
        replay_files.truncate(replay_count);

        if replay_files.is_empty() {
            return Err(ComparisonError::NoReplayFiles(
                source_account_dir.to_path_buf(),
            ));
        }

        for replay_file in &replay_files {
            let relative_path = replay_file
                .path()
                .strip_prefix(source_account_dir)
                .unwrap_or_else(|_| replay_file.path());
            let destination_path = destination_account_dir.join(relative_path);
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    ComparisonError::CreateDirectoryFailed(parent.to_path_buf(), error)
                })?;
            }
            fs::copy(replay_file.path(), &destination_path).map_err(|error| {
                ComparisonError::ReplayCopyFailed {
                    source_path: replay_file.path().to_path_buf(),
                    destination: destination_path,
                    error,
                }
            })?;
        }

        Ok(replay_files.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayFileCandidate {
    path: PathBuf,
    modified_millis: u128,
    normalized_path: String,
}

impl ReplayFileCandidate {
    fn from_path(path: &Path) -> Self {
        let modified_millis = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        Self {
            path: path.to_path_buf(),
            modified_millis,
            normalized_path: path.to_string_lossy().to_lowercase(),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn compare_recent_first(left: &Self, right: &Self) -> std::cmp::Ordering {
        right
            .modified_millis
            .cmp(&left.modified_millis)
            .then_with(|| left.normalized_path.cmp(&right.normalized_path))
    }
}

pub(super) fn create_temp_root() -> Result<PathBuf, ComparisonError> {
    let base = std::env::temp_dir();
    let process_id = std::process::id();
    for attempt in 0..100_u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = base.join(format!(
            "sc2coop-cache-compare-{process_id}-{nanos}-{attempt}"
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ComparisonError::TempDirectoryCreateFailed(path, error)),
        }
    }

    let fallback = base.join(format!("sc2coop-cache-compare-{process_id}-fallback"));
    fs::create_dir(&fallback)
        .map_err(|error| ComparisonError::TempDirectoryCreateFailed(fallback.clone(), error))?;
    Ok(fallback)
}

pub(super) fn remove_temp_root(path: &Path) -> Result<(), ComparisonError> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path)
        .map_err(|error| ComparisonError::TempDirectoryRemoveFailed(path.to_path_buf(), error))
}

pub(super) fn release_executable(target_dir: &Path, bin_name: &str) -> PathBuf {
    target_dir
        .join("release")
        .join(format!("{bin_name}{}", std::env::consts::EXE_SUFFIX))
}

fn pretty_output_file(output_file: &Path) -> PathBuf {
    let directory = output_file.parent().unwrap_or_else(|| Path::new(""));
    let stem = output_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let extension = output_file
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    directory.join(format!("{stem}_pretty{extension}"))
}

pub(super) fn remove_generated_cache_outputs(output_file: &Path) -> Result<(), ComparisonError> {
    for path in [output_file.to_path_buf(), pretty_output_file(output_file)] {
        if path.is_file() {
            fs::remove_file(&path)
                .map_err(|error| ComparisonError::WriteFileFailed(path.clone(), error))?;
        }
    }
    Ok(())
}
