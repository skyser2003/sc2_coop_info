use std::collections::HashSet;
use std::path::Path;

use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;

use crate::path_manager::PathManagerOps;
use crate::{ReplayCacheDatabase, ReplayCacheEntryQuery, ReplayInfo};

use super::{ReplayAnalysis, ReplayAnalysisOps};

impl ReplayAnalysis {
    pub fn load_detailed_analysis_replays_snapshot_from_path(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = Sc2DictionaryData::default();
        Self::load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
            cache_path,
            limit,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let entries = ReplayAnalysisOps::recover_cache_entries_from_temp(
            cache_path,
            "detailed-analysis cache",
            ReplayCacheEntryQuery::detailed_only(0),
        );
        let replays = Self::detailed_analysis_replays_snapshot_from_entries_with_dictionary(
            &entries,
            limit,
            main_names,
            main_handles,
            dictionary,
        );

        crate::sco_debug!(
            "[SCO/cache] loaded {} replay(s) from detailed-analysis cache '{}'",
            replays.len(),
            ReplayCacheDatabase::db_path_for_cache_path(cache_path).display()
        );
        replays
    }

    pub fn detailed_analysis_replays_snapshot_from_entries_with_dictionary(
        entries: &[CacheReplayEntry],
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let mut replays = entries
            .iter()
            .filter(|entry| entry.detailed_analysis && Path::new(&entry.file).exists())
            .map(|entry| {
                ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(entry, dictionary)
                    .oriented_for_main_identity(main_names, main_handles)
            })
            .collect::<Vec<_>>();

        replays.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.file.cmp(&a.file)));
        if limit > 0 && replays.len() > limit {
            replays.truncate(limit);
        }
        replays
    }

    pub fn load_detailed_analysis_replays_snapshot(
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        Self::load_detailed_analysis_replays_snapshot_from_path(
            &PathManagerOps::get_cache_path(),
            limit,
            main_names,
            main_handles,
        )
    }

    pub fn load_all_analysis_replays_snapshot(
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        Self::load_all_analysis_replays_snapshot_from_path(
            &PathManagerOps::get_cache_path(),
            limit,
            main_names,
            main_handles,
        )
    }

    pub fn load_all_analysis_replays_snapshot_from_path(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = Sc2DictionaryData::default();
        Self::load_all_analysis_replays_snapshot_from_path_with_dictionary(
            cache_path,
            limit,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn load_all_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let mut replays = ReplayAnalysisOps::read_cache_summary_entries(
            cache_path,
            "unified cache",
            ReplayCacheEntryQuery::all(0),
        )
        .into_iter()
        .filter(|entry| Path::new(&entry.file).exists())
        .map(|entry| {
            ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(&entry, dictionary)
                .oriented_for_main_identity(main_names, main_handles)
        })
        .collect::<Vec<_>>();

        replays.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.file.cmp(&a.file)));
        if limit > 0 && replays.len() > limit {
            replays.truncate(limit);
        }

        crate::sco_debug!(
            "[SCO/cache] loaded {} replay(s) from unified cache '{}' (includes both simple and detailed)",
            replays.len(),
            ReplayCacheDatabase::db_path_for_cache_path(cache_path).display()
        );

        replays
    }
}
