use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::path::PathBuf;

fn test_path_root_from_env(var_name: &str) -> PathBuf {
    std::env::var_os(var_name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn test_replay_path(file_name: &str) -> String {
    test_path_root_from_env("SCO_TEST_REPLAY_ROOT")
        .join(file_name)
        .display()
        .to_string()
}

pub fn test_config_path(file_name: &str) -> PathBuf {
    test_path_root_from_env("SCO_TEST_CONFIG_ROOT").join(file_name)
}

pub fn load_dictionary() -> Sc2DictionaryData {
    Sc2DictionaryData::load(None).expect("dictionary data should load for tests")
}
