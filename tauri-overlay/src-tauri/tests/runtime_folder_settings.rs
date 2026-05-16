use sco_tauri_overlay::{AppSettings, BackendState, TestHelperOps};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    std::env::temp_dir().join(format!("sco-runtime-folder-{label}-{suffix}"))
}

fn settings_with_folders(account_folder: &Path, screenshot_folder: &Path) -> AppSettings {
    AppSettings::merge_settings_with_defaults(json!({
        "account_folder": account_folder.display().to_string(),
        "screenshot_folder": screenshot_folder.display().to_string(),
    }))
}

#[test]
fn runtime_account_folder_update_changes_active_replay_watch_root() {
    let first_account = unique_temp_dir("account-first");
    let second_account = unique_temp_dir("account-second");
    let screenshot_folder = unique_temp_dir("screenshots");
    std::fs::create_dir_all(&first_account).expect("first account folder should be created");
    std::fs::create_dir_all(&second_account).expect("second account folder should be created");
    std::fs::create_dir_all(&screenshot_folder).expect("screenshot folder should be created");

    let state =
        BackendState::new_with_settings(settings_with_folders(&first_account, &screenshot_folder));
    assert_eq!(
        TestHelperOps::replay_watch_root(&state.read_settings_memory()).as_deref(),
        Some(first_account.as_path())
    );

    state.replace_active_settings(&settings_with_folders(&second_account, &screenshot_folder));

    assert_eq!(
        TestHelperOps::replay_watch_root(&state.read_settings_memory()).as_deref(),
        Some(second_account.as_path())
    );
    assert!(!TestHelperOps::replay_watch_roots_match(
        Some(first_account.as_path()),
        Some(second_account.as_path())
    ));

    let _ = std::fs::remove_dir_all(first_account);
    let _ = std::fs::remove_dir_all(second_account);
    let _ = std::fs::remove_dir_all(screenshot_folder);
}

#[test]
fn account_folder_runtime_refresh_sends_watcher_signal() {
    let account_folder = unique_temp_dir("account-signal");
    let screenshot_folder = unique_temp_dir("screenshots-signal");
    std::fs::create_dir_all(&account_folder).expect("account folder should be created");
    std::fs::create_dir_all(&screenshot_folder).expect("screenshot folder should be created");
    let state =
        BackendState::new_with_settings(settings_with_folders(&account_folder, &screenshot_folder));

    assert!(TestHelperOps::replay_watcher_refresh_signal_is_sent(&state));

    let _ = std::fs::remove_dir_all(account_folder);
    let _ = std::fs::remove_dir_all(screenshot_folder);
}

#[test]
fn runtime_screenshot_folder_update_changes_active_screenshot_output_path() {
    let account_folder = unique_temp_dir("account");
    let first_screenshot_folder = unique_temp_dir("screenshots-first");
    let second_screenshot_folder = unique_temp_dir("screenshots-second");
    std::fs::create_dir_all(&account_folder).expect("account folder should be created");
    std::fs::create_dir_all(&first_screenshot_folder)
        .expect("first screenshot folder should be created");
    std::fs::create_dir_all(&second_screenshot_folder)
        .expect("second screenshot folder should be created");

    let state = BackendState::new_with_settings(settings_with_folders(
        &account_folder,
        &first_screenshot_folder,
    ));
    state.replace_active_settings(&settings_with_folders(
        &account_folder,
        &second_screenshot_folder,
    ));

    let output_path = state
        .read_settings_memory()
        .overlay_screenshot_output_path(UNIX_EPOCH + Duration::from_secs(42))
        .expect("screenshot output path should use active settings");

    assert_eq!(output_path, second_screenshot_folder.join("overlay-42.png"));

    let _ = std::fs::remove_dir_all(account_folder);
    let _ = std::fs::remove_dir_all(first_screenshot_folder);
    let _ = std::fs::remove_dir_all(second_screenshot_folder);
}
