use sco_tauri_overlay::{ActiveWindowDetector, ActiveWindowInfo};

#[test]
fn active_window_info_identifies_sc2_by_process_or_title() {
    assert!(ActiveWindowInfo::new("SC2_x64.exe", "").is_sc2_window());
    assert!(ActiveWindowInfo::new("StarCraft II", "").is_sc2_window());
    assert!(ActiveWindowInfo::new("", "com.blizzard.starcraft2").is_sc2_window());
    assert!(ActiveWindowInfo::new("", "StarCraft II").is_sc2_window());
    assert!(!ActiveWindowInfo::new("notepad.exe", "StarCraft notes").is_sc2_window());
}

#[test]
fn active_window_identity_trims_and_normalizes_values() {
    assert!(ActiveWindowDetector::is_sc2_window_identity(
        "  sc2.exe  ",
        ""
    ));
    assert!(ActiveWindowDetector::is_sc2_window_identity(
        "",
        "  starcraft ii  "
    ));
}
