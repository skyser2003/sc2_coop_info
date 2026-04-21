use sco_tauri_overlay::path_manager::{get_cache_path, get_pretty_cache_path};
use std::path::{Path, PathBuf};

struct FileRestoreGuard {
    path: PathBuf,
    original: Option<Vec<u8>>,
}

impl FileRestoreGuard {
    fn capture(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            original: std::fs::read(path).ok(),
        }
    }
}

impl Drop for FileRestoreGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(contents) => {
                if let Some(parent) = self.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&self.path, contents);
            }
            None => {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

#[test]
fn convert_to_pretty_json() {
    let original_path = get_cache_path();
    let pretty_path = get_pretty_cache_path();
    let _restore_original = FileRestoreGuard::capture(&original_path);
    let _restore_pretty = FileRestoreGuard::capture(&pretty_path);

    if let Some(parent) = original_path.parent() {
        std::fs::create_dir_all(parent).expect("cache directory should be created");
    }
    if let Some(parent) = pretty_path.parent() {
        std::fs::create_dir_all(parent).expect("pretty cache directory should be created");
    }

    std::fs::write(&original_path, "{\"value\":1,\"items\":[2,3]}")
        .expect("sample cache file should be written");

    let data = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(&original_path).expect("sample cache file should exist"),
    )
    .expect("sample cache JSON should parse");
    let pretty_data = serde_json::to_string_pretty(&data).expect("sample JSON should format");
    std::fs::write(&pretty_path, pretty_data).expect("Failed to write pretty JSON file");

    assert_eq!(
        std::fs::read_to_string(&pretty_path).expect("pretty cache file should exist"),
        "{\n  \"value\": 1,\n  \"items\": [\n    2,\n    3\n  ]\n}"
    );
}
