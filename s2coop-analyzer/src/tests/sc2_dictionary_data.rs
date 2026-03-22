use super::resolve_sc2_dictionary_data_dir;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::cache_overall_stats_detailed_analysis::{repo_root, runtime_root};

const REQUIRED_FILES: [&str; 2] = ["mutators_exclude_ids.json", "replay_analysis_data.json"];

fn current_dir_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct CurrentDirReset {
    original_dir: PathBuf,
}

impl Drop for CurrentDirReset {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original_dir);
    }
}

#[test]
fn resolve_sc2_dictionary_data_dir_prefers_complete_analyzer_data_from_tauri_cwd() {
    let _guard = current_dir_lock()
        .lock()
        .expect("failed to lock current-dir guard");
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
