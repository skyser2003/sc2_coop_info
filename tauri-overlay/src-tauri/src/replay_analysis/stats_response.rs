use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, TryLockError};

use s2coop_analyzer::dictionary_data::Sc2DictionaryData;

use crate::path_manager::PathManagerOps;
use crate::replay_scan_progress::ReplayScanProgress;
use crate::shared_types::ReplayScanProgressPayload;
use crate::stats_query::StatsQuery;
use crate::{
    ReplayCacheDatabase, ReplayCacheReadScope, ReplayCacheStatisticsPayload, ReplayInfo,
    StatsAnalysisPayload, StatsState, StatsStatePayload, UNLIMITED_REPLAY_LIMIT,
};

use super::{ReplayAnalysis, ReplayAnalysisOps};

pub struct StatsResponseBuildInput<'a> {
    path: &'a str,
    stats: &'a Arc<Mutex<StatsState>>,
    stats_current_replay_files: &'a Arc<Mutex<HashSet<String>>>,
    scan_progress: ReplayScanProgressPayload,
    main_names: &'a HashSet<String>,
    main_handles: &'a HashSet<String>,
}

impl<'a> StatsResponseBuildInput<'a> {
    pub fn new(
        path: &'a str,
        stats: &'a Arc<Mutex<StatsState>>,
        stats_current_replay_files: &'a Arc<Mutex<HashSet<String>>>,
        scan_progress: ReplayScanProgressPayload,
        main_names: &'a HashSet<String>,
        main_handles: &'a HashSet<String>,
    ) -> Self {
        Self {
            path,
            stats,
            stats_current_replay_files,
            scan_progress,
            main_names,
            main_handles,
        }
    }
}

impl ReplayAnalysis {
    fn replay_matches_stats_filters(
        path: &str,
        replay: &ReplayInfo,
        main_handles: &HashSet<String>,
    ) -> bool {
        let dictionary = Sc2DictionaryData::default();
        Self::replay_matches_stats_filters_with_dictionary(path, replay, main_handles, &dictionary)
    }

    pub fn replay_matches_stats_filters_with_dictionary(
        path: &str,
        replay: &ReplayInfo,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> bool {
        StatsQuery::from_path(path).matches_replay(replay, main_handles, dictionary)
    }

    pub fn filter_replays_for_stats(path: &str, replays: &[ReplayInfo]) -> Vec<ReplayInfo> {
        let (_, main_handles) = ReplayAnalysisOps::default_main_identity();
        replays
            .iter()
            .filter(|replay| Self::replay_matches_stats_filters(path, replay, &main_handles))
            .cloned()
            .collect()
    }

    pub fn detailed_stats_counts(filtered_replays: &[&ReplayInfo]) -> (u64, u64) {
        let total_valid_files = filtered_replays.len() as u64;
        let detailed_parsed_count = filtered_replays
            .iter()
            .filter(|replay| replay.has_detailed_analysis_cache())
            .count() as u64;
        (detailed_parsed_count, total_valid_files)
    }

    pub fn stats_response_has_detailed_analysis(response: &StatsStatePayload) -> bool {
        response
            .analysis
            .as_ref()
            .is_some_and(|analysis| analysis.unit_data.is_some())
    }

    fn stats_cache_file_exists(cache_path: &Path) -> bool {
        ReplayCacheDatabase::db_path_for_cache_path(cache_path).exists()
            || ReplayCacheDatabase::legacy_json_path_for_cache_path(cache_path).exists()
    }

    fn should_load_cached_statistics(response: &StatsStatePayload, cache_path: &Path) -> bool {
        response.ready || Self::stats_cache_file_exists(cache_path)
    }

    pub fn apply_cached_statistics_payload(
        response: &mut StatsStatePayload,
        payload: &ReplayCacheStatisticsPayload,
    ) -> Result<(), String> {
        response.ready = true;
        response.analysis = Some(
            StatsAnalysisPayload::from_value(payload.analysis().clone())
                .map_err(|error| format!("Invalid cached stats analysis payload: {error}"))?,
        );
        response.prestige_names = payload.prestige_names().clone();
        response.games = payload.games();
        response.detailed_parsed_count = payload.detailed_parsed_count();
        response.total_valid_files = payload.total_valid_files();
        response.main_players = payload.main_players().to_vec();
        response.main_handles = payload.main_handles().to_vec();
        Ok(())
    }

    pub fn build_stats_response(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
    ) -> Result<StatsStatePayload, String> {
        let (main_names, main_handles) = ReplayAnalysisOps::default_main_identity();
        Self::build_stats_response_with_identity(
            path,
            stats,
            stats_current_replay_files,
            ReplayScanProgress::default().as_payload(),
            &main_names,
            &main_handles,
        )
    }

    pub fn build_stats_response_with_identity(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
        scan_progress: ReplayScanProgressPayload,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Result<StatsStatePayload, String> {
        let dictionary = Sc2DictionaryData::default();
        Self::build_stats_response_with_dictionary(
            StatsResponseBuildInput::new(
                path,
                stats,
                stats_current_replay_files,
                scan_progress,
                main_names,
                main_handles,
            ),
            &dictionary,
        )
    }

    pub fn build_stats_response_with_dictionary(
        input: StatsResponseBuildInput<'_>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<StatsStatePayload, String> {
        let StatsResponseBuildInput {
            path,
            stats,
            stats_current_replay_files,
            scan_progress,
            main_names,
            main_handles,
        } = input;
        let stats_query = StatsQuery::from_path(path);
        let mut response = match stats.try_lock() {
            Ok(state) => state.as_payload_typed(scan_progress.clone()),
            Err(error) => match error {
                TryLockError::WouldBlock => {
                    let fallback = StatsState::default();
                    let mut payload = fallback.as_payload_typed(scan_progress);
                    payload.message = "Statistics are updating. Try again.".to_string();
                    payload
                }
                TryLockError::Poisoned(_) => {
                    return Err("Failed to access stats state: mutex is poisoned".to_string());
                }
            },
        };

        let cache_path = PathManagerOps::get_cache_path();
        if Self::should_load_cached_statistics(&response, &cache_path) {
            match stats_current_replay_files.try_lock() {
                Ok(current_replay_files) => {
                    let summary_query = stats_query.to_cache_query(
                        ReplayCacheReadScope::All,
                        UNLIMITED_REPLAY_LIMIT,
                        main_handles,
                        &current_replay_files,
                    );
                    match ReplayCacheDatabase::open_for_cache_path(&cache_path).and_then(
                        |database| {
                            database.load_statistics_payload(
                                &summary_query,
                                main_names,
                                main_handles,
                                dictionary,
                            )
                        },
                    ) {
                        Ok(payload) => {
                            Self::apply_cached_statistics_payload(&mut response, &payload)?;
                        }
                        Err(error) => {
                            crate::sco_warn!(
                                "[SCO/cache] failed to build filtered statistics from database '{}': {error}",
                                ReplayCacheDatabase::db_path_for_cache_path(&cache_path).display()
                            );
                        }
                    }
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Poisoned(_)) => {
                    return Err(
                        "Failed to access current replay file set: mutex is poisoned".to_string(),
                    );
                }
            }
        }
        if let Some(query) = path.split('?').nth(1) {
            response.query = Some(query.to_string());
        }

        Ok(response)
    }
}
