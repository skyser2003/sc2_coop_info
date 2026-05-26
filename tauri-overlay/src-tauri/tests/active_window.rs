use sco_tauri_overlay::{ActiveWindowDetector, ActiveWindowInfo, ActiveWindowRect};

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

#[test]
fn active_window_info_exposes_optional_rect() {
    let rect = ActiveWindowRect::new(-20, 40, 1600, 900).expect("valid rect");
    let info = ActiveWindowInfo::new_with_rect("SC2_x64.exe", "StarCraft II", Some(rect));

    assert_eq!(info.rect(), Some(rect));
    assert_eq!(rect.x(), -20);
    assert_eq!(rect.y(), 40);
    assert_eq!(rect.width(), 1600);
    assert_eq!(rect.height(), 900);
    assert!(ActiveWindowRect::new(0, 0, 0, 900).is_none());
}
