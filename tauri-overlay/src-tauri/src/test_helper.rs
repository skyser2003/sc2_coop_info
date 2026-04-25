use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::borrow::Borrow;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use crate::{
    replay_analysis::WeeklyRowPayload, AppSettings, CommanderUnitRollup, ReplayInfo, StatsSnapshot,
    TauriOverlayOps,
};

pub struct TestHelperOps;

impl TestHelperOps {
    fn test_path_root_from_env(var_name: &str) -> PathBuf {
        std::env::var_os(var_name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_default()
    }
}

impl TestHelperOps {
    pub fn test_replay_path(file_name: &str) -> String {
        TestHelperOps::test_path_root_from_env("SCO_TEST_REPLAY_ROOT")
            .join(file_name)
            .display()
            .to_string()
    }
}

impl TestHelperOps {
    pub fn test_config_path(file_name: &str) -> PathBuf {
        TestHelperOps::test_path_root_from_env("SCO_TEST_CONFIG_ROOT").join(file_name)
    }
}

impl TestHelperOps {
    pub fn load_dictionary() -> Sc2DictionaryData {
        Sc2DictionaryData::load(None).expect("dictionary data should load for tests")
    }
}

impl TestHelperOps {
    fn default_main_identity() -> (HashSet<String>, HashSet<String>) {
        let settings = AppSettings::from_saved_file();
        (
            settings.configured_main_names(),
            settings.configured_main_handles(),
        )
    }
}

impl TestHelperOps {
    pub fn canonicalize_map_id(raw: &str) -> Option<String> {
        TestHelperOps::load_dictionary().canonicalize_coop_map_id(raw)
    }
}

impl TestHelperOps {
    pub fn localized_prestige_text(commander: &str, prestige: u64, language: &str) -> String {
        let dictionary = TestHelperOps::load_dictionary();
        crate::shared_types::OverlayReplayPayload::localized_prestige_text_with_dictionary(
            commander,
            prestige,
            language,
            &dictionary,
        )
    }
}

impl TestHelperOps {
    pub fn rebuild_analysis_payload(
        replays: &[ReplayInfo],
        include_detailed: bool,
    ) -> serde_json::Value {
        let (main_names, main_handles) = TestHelperOps::default_main_identity();
        TestHelperOps::rebuild_analysis_payload_with_identity(
            replays,
            include_detailed,
            &main_names,
            &main_handles,
        )
    }
}

impl TestHelperOps {
    pub fn rebuild_analysis_payload_with_identity(
        replays: &[ReplayInfo],
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> serde_json::Value {
        let dictionary = TestHelperOps::load_dictionary();
        crate::replay_analysis::ReplayAnalysis::rebuild_analysis_payload_with_dictionary(
            replays,
            include_detailed,
            main_names,
            main_handles,
            &dictionary,
        )
    }
}

impl TestHelperOps {
    pub fn build_rebuild_snapshot(replays: &[ReplayInfo], include_detailed: bool) -> StatsSnapshot {
        let (main_names, main_handles) = TestHelperOps::default_main_identity();
        TestHelperOps::build_rebuild_snapshot_with_identity(
            replays,
            include_detailed,
            &main_names,
            &main_handles,
        )
    }
}

impl TestHelperOps {
    pub fn build_rebuild_snapshot_with_identity(
        replays: &[ReplayInfo],
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> StatsSnapshot {
        let dictionary = TestHelperOps::load_dictionary();
        crate::replay_analysis::ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
            replays,
            include_detailed,
            main_names,
            main_handles,
            &dictionary,
        )
    }
}

impl TestHelperOps {
    pub fn rebuild_weeklies_rows(replays: &[ReplayInfo]) -> Vec<WeeklyRowPayload> {
        let dictionary = TestHelperOps::load_dictionary();
        crate::replay_analysis::ReplayAnalysis::rebuild_weeklies_rows_with_dictionary(
            replays,
            chrono::Local::now().date_naive(),
            &dictionary,
        )
    }
}

impl TestHelperOps {
    pub fn load_detailed_analysis_replays_snapshot_from_path(
        cache_path: &Path,
        limit: usize,
    ) -> Vec<ReplayInfo> {
        let (main_names, main_handles) = TestHelperOps::default_main_identity();
        TestHelperOps::load_detailed_analysis_replays_snapshot_from_path_with_identity(
            cache_path,
            limit,
            &main_names,
            &main_handles,
        )
    }
}

impl TestHelperOps {
    pub fn load_detailed_analysis_replays_snapshot_from_path_with_identity(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = TestHelperOps::load_dictionary();
        crate::replay_analysis::ReplayAnalysis::load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path,
        limit,
        main_names,
        main_handles,
        &dictionary,
    )
    }
}

impl TestHelperOps {
    pub fn stats_replays_for_response_from_path(
        include_detailed: bool,
        cached_replays: &[ReplayInfo],
        cache_path: &Path,
    ) -> Vec<ReplayInfo> {
        let (main_names, main_handles) = TestHelperOps::default_main_identity();
        TestHelperOps::stats_replays_for_response_from_path_with_identity(
            include_detailed,
            cached_replays,
            cache_path,
            &main_names,
            &main_handles,
        )
    }
}

impl TestHelperOps {
    pub fn stats_replays_for_response_from_path_with_identity(
        include_detailed: bool,
        cached_replays: &[ReplayInfo],
        cache_path: &Path,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = TestHelperOps::load_dictionary();
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
}

impl TestHelperOps {
    pub fn filter_replays_for_stats(path: &str, replays: &[ReplayInfo]) -> Vec<ReplayInfo> {
        let dictionary = TestHelperOps::load_dictionary();
        let (_, main_handles) = TestHelperOps::default_main_identity();
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
}

impl TestHelperOps {
    pub fn build_commander_unit_data(
        side_rollup: std::collections::BTreeMap<String, CommanderUnitRollup>,
    ) -> serde_json::Value {
        let dictionary = TestHelperOps::load_dictionary();
        TauriOverlayOps::build_commander_unit_data_with_dictionary(side_rollup, &dictionary)
    }
}

impl TestHelperOps {
    pub fn collect_main_identity_lists<R>(
        replays: &[R],
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> (Vec<String>, Vec<String>)
    where
        R: Borrow<ReplayInfo>,
    {
        let dictionary = TestHelperOps::load_dictionary();
        crate::replay_analysis::ReplayAnalysisOps::collect_main_identity_lists_with_dictionary(
            replays,
            main_names,
            main_handles,
            &dictionary,
        )
    }
}

impl TestHelperOps {
    pub fn bonus_objective_total_for_map_id(map_id: &str) -> Option<u64> {
        let dictionary = TestHelperOps::load_dictionary();
        crate::replay_analysis::ReplayAnalysisOps::bonus_objective_total_for_map_id_with_dictionary(
            map_id,
            &dictionary,
        )
    }
}
