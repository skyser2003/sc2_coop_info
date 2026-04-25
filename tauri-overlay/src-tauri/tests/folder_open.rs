use sco_tauri_overlay::overlay_info;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_path(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("sco-folder-open-{name}-{millis}"))
}

#[test]
fn open_folder_rejects_empty_path() {
    let error = overlay_info::OverlayInfoOps::open_folder_in_explorer("  ")
        .expect_err("empty folder path should be rejected");
    assert_eq!(error, "Folder path is empty");
}

#[test]
fn open_folder_rejects_missing_folder() {
    let missing = unique_temp_path("missing");
    let error = overlay_info::OverlayInfoOps::open_folder_in_explorer(&missing.to_string_lossy())
        .expect_err("missing folder should be rejected");
    assert_eq!(error, "Folder does not exist");
}
