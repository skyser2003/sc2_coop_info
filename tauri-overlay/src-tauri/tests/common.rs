use std::path::PathBuf;

fn test_path_root_from_env(var_name: &str, default: &str) -> PathBuf {
    std::env::var_os(var_name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

pub fn test_replay_path(file_name: &str) -> String {
    test_path_root_from_env("SCO_TEST_REPLAY_ROOT", r"")
        .join(file_name)
        .display()
        .to_string()
}

pub fn test_config_path(file_name: &str) -> PathBuf {
    test_path_root_from_env("SCO_TEST_CONFIG_ROOT", r"").join(file_name)
}
