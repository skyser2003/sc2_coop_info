use sco_tauri_overlay::path_manager::{get_cache_path, get_pretty_cache_path};

#[test]
fn convert_to_pretty_json() {
    let original_path = get_cache_path();
    let pretty_path = get_pretty_cache_path();

    let data = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(&original_path).unwrap(),
    )
    .unwrap();
    let pretty_data = serde_json::to_string_pretty(&data).unwrap();
    std::fs::write(&pretty_path, pretty_data).expect("Failed to write pretty JSON file");
}
