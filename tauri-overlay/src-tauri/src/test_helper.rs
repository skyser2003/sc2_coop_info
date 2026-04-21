use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::ReplayInfo;

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

pub fn collect_main_identity_lists<R>(
    replays: &[R],
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> (Vec<String>, Vec<String>)
where
    R: Borrow<ReplayInfo>,
{
    let dictionary = load_dictionary();
    crate::replay_analysis::collect_main_identity_lists_with_dictionary(
        replays,
        main_names,
        main_handles,
        &dictionary,
    )
}

pub fn bonus_objective_total_for_map_id(map_id: &str) -> Option<u64> {
    let dictionary = load_dictionary();
    crate::replay_analysis::bonus_objective_total_for_map_id_with_dictionary(map_id, &dictionary)
}
