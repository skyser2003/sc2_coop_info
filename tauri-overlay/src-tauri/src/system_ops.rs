use std::path::PathBuf;

use crate::TauriOverlayOps;

impl TauriOverlayOps {
    pub fn folder_dialog_start_directory(directory: Option<String>) -> Option<PathBuf> {
        let trimmed = directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let candidate = PathBuf::from(trimmed);

        if candidate.is_dir() {
            return Some(candidate);
        }

        candidate.parent().and_then(|parent| {
            if parent.is_dir() {
                Some(parent.to_path_buf())
            } else {
                None
            }
        })
    }

    pub fn session_counter_delta(result: &str) -> (u64, u64) {
        match result.trim().to_ascii_lowercase().as_str() {
            "victory" => (1, 0),
            "defeat" => (0, 1),
            _ => (0, 0),
        }
    }
}
