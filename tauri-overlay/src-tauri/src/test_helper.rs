use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::borrow::Borrow;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use crate::{
    build_commander_unit_data_with_dictionary, configured_main_handles_from_settings,
    configured_main_names_from_settings, replay_analysis::WeeklyRowPayload, AppSettings,
    CommanderUnitRollup, ReplayInfo, StatsSnapshot,
};

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

fn default_main_identity() -> (HashSet<String>, HashSet<String>) {
    let settings = AppSettings::from_saved_file();
    (
        configured_main_names_from_settings(&settings),
        configured_main_handles_from_settings(&settings),
    )
}

pub fn canonicalize_map_id(raw: &str) -> Option<String> {
    load_dictionary().canonicalize_coop_map_id(raw)
}

pub fn localized_prestige_text(commander: &str, prestige: u64, language: &str) -> String {
    let dictionary = load_dictionary();
    crate::shared_types::OverlayReplayPayload::localized_prestige_text_with_dictionary(
        commander,
        prestige,
        language,
        &dictionary,
    )
}

pub fn rebuild_analysis_payload(
    replays: &[ReplayInfo],
    include_detailed: bool,
) -> serde_json::Value {
    let (main_names, main_handles) = default_main_identity();
    rebuild_analysis_payload_with_identity(replays, include_detailed, &main_names, &main_handles)
}

pub fn rebuild_analysis_payload_with_identity(
    replays: &[ReplayInfo],
    include_detailed: bool,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> serde_json::Value {
    let dictionary = load_dictionary();
    crate::replay_analysis::ReplayAnalysis::rebuild_analysis_payload_with_dictionary(
        replays,
        include_detailed,
        main_names,
        main_handles,
        &dictionary,
    )
}

pub fn build_rebuild_snapshot(replays: &[ReplayInfo], include_detailed: bool) -> StatsSnapshot {
    let (main_names, main_handles) = default_main_identity();
    build_rebuild_snapshot_with_identity(replays, include_detailed, &main_names, &main_handles)
}

pub fn build_rebuild_snapshot_with_identity(
    replays: &[ReplayInfo],
    include_detailed: bool,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> StatsSnapshot {
    let dictionary = load_dictionary();
    crate::replay_analysis::ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
        replays,
        include_detailed,
        main_names,
        main_handles,
        &dictionary,
    )
}

pub fn rebuild_weeklies_rows(replays: &[ReplayInfo]) -> Vec<WeeklyRowPayload> {
    let dictionary = load_dictionary();
    crate::replay_analysis::ReplayAnalysis::rebuild_weeklies_rows_with_dictionary(
        replays,
        chrono::Local::now().date_naive(),
        &dictionary,
    )
}

pub fn load_detailed_analysis_replays_snapshot_from_path(
    cache_path: &Path,
    limit: usize,
) -> Vec<ReplayInfo> {
    let (main_names, main_handles) = default_main_identity();
    load_detailed_analysis_replays_snapshot_from_path_with_identity(
        cache_path,
        limit,
        &main_names,
        &main_handles,
    )
}

pub fn load_detailed_analysis_replays_snapshot_from_path_with_identity(
    cache_path: &Path,
    limit: usize,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> Vec<ReplayInfo> {
    let dictionary = load_dictionary();
    crate::replay_analysis::ReplayAnalysis::load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path,
        limit,
        main_names,
        main_handles,
        &dictionary,
    )
}

pub fn stats_replays_for_response_from_path(
    include_detailed: bool,
    cached_replays: &[ReplayInfo],
    cache_path: &Path,
) -> Vec<ReplayInfo> {
    let (main_names, main_handles) = default_main_identity();
    stats_replays_for_response_from_path_with_identity(
        include_detailed,
        cached_replays,
        cache_path,
        &main_names,
        &main_handles,
    )
}

pub fn stats_replays_for_response_from_path_with_identity(
    include_detailed: bool,
    cached_replays: &[ReplayInfo],
    cache_path: &Path,
    main_names: &HashSet<String>,
    main_handles: &HashSet<String>,
) -> Vec<ReplayInfo> {
    let dictionary = load_dictionary();
    match crate::replay_analysis::ReplayAnalysis::stats_replays_for_response_from_path_with_dictionary(
        include_detailed,
        cached_replays,
        cache_path,
        main_names,
        main_handles,
        &dictionary,
    ) {
        Cow::Borrowed(replays) => replays.to_vec(),
        Cow::Owned(replays) => replays,
    }
}

pub fn filter_replays_for_stats(path: &str, replays: &[ReplayInfo]) -> Vec<ReplayInfo> {
    let dictionary = load_dictionary();
    let (_, main_handles) = default_main_identity();
    replays
        .iter()
        .filter(|replay| {
            crate::replay_analysis::ReplayAnalysis::replay_matches_stats_filters_with_dictionary(
                path,
                replay,
                &main_handles,
                &dictionary,
            )
        })
        .cloned()
        .collect()
}

pub fn build_commander_unit_data(
    side_rollup: std::collections::BTreeMap<String, CommanderUnitRollup>,
) -> serde_json::Value {
    let dictionary = load_dictionary();
    build_commander_unit_data_with_dictionary(side_rollup, &dictionary)
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
