use s2coop_analyzer::cache_overall_stats_detailed_analysis::{repo_root, runtime_root};
use std::path::PathBuf;
use std::{collections::HashSet, path::Path};

const REQUIRED_FILES: [&str; 2] = ["mutators_exclude_ids.json", "replay_analysis_data.json"];

struct CurrentDirReset {
    original_dir: PathBuf,
}

impl Drop for CurrentDirReset {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original_dir);
    }
}

const SC2_DICTIONARY_DATA_DIRS: [&str; 2] = ["data", "s2coop-analyzer/data"];

fn add_candidate(candidates: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, candidate: PathBuf) {
    if seen.insert(candidate.clone()) {
        candidates.push(candidate);
    }
}

fn add_ancestor_candidates(
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    start: &Path,
) {
    let mut probe: Option<&Path> = Some(start);
    while let Some(base) = probe {
        for relative in SC2_DICTIONARY_DATA_DIRS {
            add_candidate(candidates, seen, base.join(relative));
        }
        probe = base.parent();
    }
}

fn has_required_files(path: &Path, required_files: &[&str]) -> bool {
    path.is_dir()
        && required_files
            .iter()
            .all(|file_name| path.join(file_name).is_file())
}

pub(crate) fn resolve_sc2_dictionary_data_dir(required_files: &[&str]) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let mut candidates = Vec::<PathBuf>::new();
    let mut seen = HashSet::<PathBuf>::new();

    add_candidate(&mut candidates, &mut seen, runtime_root().join("data"));
    add_ancestor_candidates(&mut candidates, &mut seen, cwd.as_path());

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            add_ancestor_candidates(&mut candidates, &mut seen, parent);
        }
    }

    for candidate in candidates {
        if has_required_files(&candidate, required_files) {
            return Ok(candidate);
        }
    }

    Err(format!(
        "SC2 dictionary data directory was not found from '{}'",
        cwd.display()
    ))
}

#[test]
fn resolve_sc2_dictionary_data_dir_prefers_complete_analyzer_data_from_tauri_cwd() {
    let original_dir = std::env::current_dir().expect("failed to read current dir");
    let _reset = CurrentDirReset {
        original_dir: original_dir.clone(),
    };
    let tauri_dir = repo_root().join("tauri-overlay").join("src-tauri");
    let analyzer_data_dir = runtime_root().join("data");

    assert!(tauri_dir.is_dir(), "tauri src-tauri dir should exist");
    assert!(analyzer_data_dir.is_dir(), "analyzer data dir should exist");

    std::env::set_current_dir(&tauri_dir).expect("failed to switch to tauri dir");
    let resolved = resolve_sc2_dictionary_data_dir(&REQUIRED_FILES)
        .expect("expected analyzer data dir to resolve from tauri cwd");

    assert_eq!(resolved, analyzer_data_dir);
}
