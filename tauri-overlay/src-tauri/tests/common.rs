use s2coop_analyzer::detailed_replay_analysis::ReplayAnalysisResources;
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::path::PathBuf;
use std::sync::Arc;

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

pub fn load_dictionary() -> Sc2DictionaryData {
    Sc2DictionaryData::load(None).expect("dictionary data should load for tests")
}

pub fn load_replay_resources() -> ReplayAnalysisResources {
    ReplayAnalysisResources::from_dictionary_data(Arc::new(load_dictionary()))
        .expect("replay resources should load for tests")
}
