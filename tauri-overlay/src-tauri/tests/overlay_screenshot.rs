use sco_tauri_overlay::{overlay_info, AppSettings};
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ONE_BY_ONE_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 255, 255, 63, 0, 5,
    254, 2, 254, 167, 53, 129, 132, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn unique_temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    std::env::temp_dir().join(format!("sco-overlay-{name}-{suffix}"))
}

#[test]
fn overlay_screenshot_output_path_uses_configured_folder_and_timestamp() {
    let captured_at = UNIX_EPOCH + Duration::from_secs(1_234_567);
    let path = AppSettings::merge_settings_with_defaults(json!({
        "screenshot_folder": "shots",
    }))
    .overlay_screenshot_output_path(captured_at)
    .expect("screenshot path should be generated");

    assert_eq!(path, PathBuf::from("shots").join("overlay-1234567.png"));
}

#[test]
fn save_overlay_screenshot_writes_png_file() {
    let dir = unique_temp_dir("write");
    let path = dir.join("overlay.png");

    overlay_info::OverlayInfoOps::save_overlay_screenshot(&path, ONE_BY_ONE_PNG)
        .expect("valid PNG screenshot should be saved");

    let written = std::fs::read(&path).expect("written screenshot should exist");
    assert_eq!(written, ONE_BY_ONE_PNG);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn save_overlay_screenshot_rejects_non_png_bytes() {
    let dir = unique_temp_dir("invalid");
    let path = dir.join("overlay.png");

    let error = overlay_info::OverlayInfoOps::save_overlay_screenshot(&path, b"not-a-png")
        .expect_err("invalid screenshot data should be rejected");

    assert_eq!(error, "Overlay screenshot data is not a PNG image");
    assert!(!path.exists());
}
