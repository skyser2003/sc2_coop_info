use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::ReplayAnalysisResources;
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use crate::{
    BackendState, PathManagerOps, ReplayAnalysis, ReplayAnalysisOps, ReplayCacheDatabase,
    ReplayChatPayload, ReplayInfo, ReplayVisualContext, ReplayVisualOps, ReplayVisualPayload,
    TauriOverlayOps,
};

impl TauriOverlayOps {
    pub fn replay_index_by_file(replays: &[ReplayInfo], file: &Option<String>) -> Option<usize> {
        let needle = file.as_deref()?;
        replays.iter().position(|entry| entry.file() == needle)
    }

    fn replay_visual_context_from_replay(replay: &ReplayInfo) -> ReplayVisualContext {
        let main_player_id = if replay.main_index() == 0 { 1 } else { 2 };
        let duration_seconds =
            if replay.accurate_length().is_finite() && replay.accurate_length() > 0.0 {
                replay.accurate_length().round() as u64
            } else {
                replay.length()
            };
        ReplayVisualContext::new(
            replay.file(),
            replay.map(),
            replay.result(),
            duration_seconds,
            main_player_id,
        )
    }

    pub fn replay_info_from_cache_entry_for_identity(
        entry: &CacheReplayEntry,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: Option<&Sc2DictionaryData>,
    ) -> ReplayInfo {
        dictionary
            .map(|dictionary| {
                ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(entry, dictionary)
            })
            .unwrap_or_else(|| ReplayAnalysisOps::replay_info_from_cache_entry(entry))
            .oriented_for_main_identity(main_names, main_handles)
    }

    pub fn replay_info_from_cache_entry_for_state(
        state: &BackendState,
        entry: &CacheReplayEntry,
    ) -> ReplayInfo {
        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let dictionary = state.dictionary_data().ok();
        Self::replay_info_from_cache_entry_for_identity(
            entry,
            &main_names,
            &main_handles,
            dictionary.as_deref(),
        )
    }

    pub fn replay_chat_payload_from_slots(
        main_names: HashSet<String>,
        main_handles: HashSet<String>,
        file: &str,
        dictionary: Option<Arc<Sc2DictionaryData>>,
        resources: Option<Arc<ReplayAnalysisResources>>,
    ) -> Result<ReplayChatPayload, String> {
        let requested_file = file.trim();
        if requested_file.is_empty() {
            return Err("No replay file specified.".to_string());
        }

        let dictionary_ref = dictionary.as_deref().or_else(|| {
            resources
                .as_deref()
                .map(ReplayAnalysisResources::dictionary_data)
        });
        let cached_replay =
            ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
                .and_then(|database| database.load_entry_by_file(requested_file))
                .map_err(|error| {
                    crate::sco_warn!(
                        "[SCO/cache-db] replay chat cache lookup failed for '{}': {error}",
                        requested_file
                    );
                    error
                })
                .ok()
                .flatten()
                .map(|entry| {
                    Self::replay_info_from_cache_entry_for_identity(
                        &entry,
                        &main_names,
                        &main_handles,
                        dictionary_ref,
                    )
                });

        if let Some(replay) = cached_replay {
            return Ok(dictionary
                .as_deref()
                .map(|dictionary| replay.chat_payload_with_dictionary(dictionary))
                .unwrap_or_else(|| replay.chat_payload()));
        }

        let replay_path = Path::new(requested_file);
        if !replay_path.exists() {
            return Err(format!("Replay file not found: {requested_file}"));
        }

        let resources = resources
            .as_deref()
            .ok_or_else(|| "Replay analysis resources are unavailable.".to_string())?;
        let (replay, _) = ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(
            replay_path,
            resources,
        )
        .ok_or_else(|| format!("Failed to parse replay file: {requested_file}"))?;
        let replay = replay.oriented_for_main_identity(&main_names, &main_handles);
        Ok(dictionary
            .as_deref()
            .map(|dictionary| replay.chat_payload_with_dictionary(dictionary))
            .unwrap_or_else(|| replay.chat_payload_with_dictionary(resources.dictionary_data())))
    }

    pub fn replay_visual_payload_from_slots(
        main_names: HashSet<String>,
        main_handles: HashSet<String>,
        file: &str,
        dictionary: Arc<Sc2DictionaryData>,
        resources: Arc<ReplayAnalysisResources>,
    ) -> Result<ReplayVisualPayload, String> {
        let requested_file = file.trim();
        if requested_file.is_empty() {
            return Err("No replay file specified.".to_string());
        }

        let replay_path = Path::new(requested_file);
        if !replay_path.exists() {
            return Err(format!("Replay file not found: {requested_file}"));
        }

        let cached_replay =
            ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
                .and_then(|database| database.load_entry_by_file(requested_file))
                .map_err(|error| {
                    crate::sco_warn!(
                        "[SCO/cache-db] replay visual cache lookup failed for '{}': {error}",
                        requested_file
                    );
                    error
                })
                .ok()
                .flatten()
                .map(|entry| {
                    Self::replay_info_from_cache_entry_for_identity(
                        &entry,
                        &main_names,
                        &main_handles,
                        Some(dictionary.as_ref()),
                    )
                });

        if let Some(replay) = cached_replay.as_ref() {
            let context = Self::replay_visual_context_from_replay(replay);
            return ReplayVisualOps::payload_from_file(
                replay_path,
                resources.as_ref(),
                dictionary.as_ref(),
                &context,
            );
        }

        let (replay, _) = ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(
            replay_path,
            resources.as_ref(),
        )
        .ok_or_else(|| format!("Failed to parse replay file: {requested_file}"))?;
        let replay = replay.oriented_for_main_identity(&main_names, &main_handles);
        let context = Self::replay_visual_context_from_replay(&replay);
        ReplayVisualOps::payload_from_file(
            replay_path,
            resources.as_ref(),
            dictionary.as_ref(),
            &context,
        )
    }
}
