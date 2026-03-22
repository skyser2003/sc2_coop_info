use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cache_overall_stats_detailed_analysis::runtime_root;

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

#[cfg(test)]
#[path = "tests/sc2_dictionary_data.rs"]
mod tests;
