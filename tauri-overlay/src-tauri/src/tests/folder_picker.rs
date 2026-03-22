use super::*;

fn unique_temp_path(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("sco-folder-picker-{name}-{millis}"))
}

#[test]
fn folder_picker_uses_existing_directory_as_start_directory() {
    let temp_dir = unique_temp_path("existing");
    std::fs::create_dir_all(&temp_dir).expect("temp directory should be created");

    let actual = folder_dialog_start_directory(Some(temp_dir.to_string_lossy().to_string()));

    assert_eq!(actual, Some(temp_dir.clone()));

    std::fs::remove_dir_all(temp_dir).expect("temp directory should be removed");
}

#[test]
fn folder_picker_falls_back_to_existing_parent_directory() {
    let temp_dir = unique_temp_path("parent");
    let nested_path = temp_dir.join("missing-child");
    std::fs::create_dir_all(&temp_dir).expect("temp directory should be created");

    let actual = folder_dialog_start_directory(Some(nested_path.to_string_lossy().to_string()));

    assert_eq!(actual, Some(temp_dir.clone()));

    std::fs::remove_dir_all(temp_dir).expect("temp directory should be removed");
}
