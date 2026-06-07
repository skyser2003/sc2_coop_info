use super::BackendState;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;

use crate::replay_state::ReplayState;
use crate::{PathManagerOps, ReplayCacheDatabase, ReplayInfo, TauriOverlayOps};

impl BackendState {
    pub fn get_replay_state(&self) -> Arc<Mutex<ReplayState>> {
        self.replay_state.clone()
    }

    fn replay_info_from_cache_entry(&self, entry: &CacheReplayEntry) -> ReplayInfo {
        let main_names = self.configured_main_names();
        let main_handles = self.configured_main_handles();
        let dictionary = self.dictionary_data().ok();
        TauriOverlayOps::replay_info_from_cache_entry_for_identity(
            entry,
            &main_names,
            &main_handles,
            dictionary.as_deref(),
        )
    }

    pub(super) fn cached_replay_by_file_or_latest(&self, file: Option<&str>) -> Option<ReplayInfo> {
        let cache_path = PathManagerOps::get_cache_path();
        let database = ReplayCacheDatabase::open_for_cache_path(&cache_path).map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] failed to open replay cache: {error}");
            error
        });
        let Ok(database) = database else {
            return None;
        };
        let entry = match file {
            Some(file) => database
                .load_entry_by_file(file)
                .and_then(|entry| match entry {
                    Some(entry) => Ok(Some(entry)),
                    None => database.load_latest_entry(),
                }),
            None => database.load_latest_entry(),
        }
        .map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] failed to load selected replay: {error}");
            error
        })
        .ok()
        .flatten()?;
        Some(self.replay_info_from_cache_entry(&entry))
    }

    pub fn get_current_replay_file(&self) -> Option<String> {
        self.replay_state
            .lock()
            .ok()
            .and_then(|state| state.get_current_replay_file())
    }

    pub fn set_current_replay_file(&self, filename: Option<&str>) {
        if let Ok(replay_state) = self.replay_state.lock() {
            replay_state.set_current_replay_file(filename);
        }
    }

    pub fn cached_replay_by_hash(&self, replay_hash: &str) -> Option<ReplayInfo> {
        let cache_path = PathManagerOps::get_cache_path();
        ReplayCacheDatabase::open_for_cache_path(&cache_path)
            .and_then(|database| database.load_entry_by_hash(replay_hash))
            .map_err(|error| {
                crate::sco_warn!("[SCO/cache-db] failed to load replay by hash: {error}");
                error
            })
            .ok()
            .flatten()
            .map(|entry| self.replay_info_from_cache_entry(&entry))
    }

    pub fn clear_current_replay_file(&self) {
        if let Ok(replay_state) = self.replay_state.lock() {
            replay_state.clear_selected_replay_file();
        }
    }

    pub fn record_replay_cache_update(&self, replay: &ReplayInfo) {
        if let Ok(mut current_replay_files) = self.stats_current_replay_files.lock() {
            current_replay_files.insert(replay.file.clone());
        }

        if let Err(error) = self.update_latest_replay_file_modified_time(Path::new(&replay.file)) {
            crate::sco_warn!(
                "[SCO/today-win-bonus] failed to record replay file modified time file='{}' error='{}'",
                replay.file,
                error
            );
        }
        self.set_current_replay_file(Some(&replay.file));
    }

    pub fn record_replay_cache_update_if_persistable(
        &self,
        replay: &ReplayInfo,
        cache_persistable: bool,
    ) -> bool {
        if !cache_persistable {
            return false;
        }

        self.record_replay_cache_update(replay);
        true
    }

    pub fn replay_count_for_launch_detector(&self) -> usize {
        ReplayCacheDatabase::open_for_cache_path(&PathManagerOps::get_cache_path())
            .and_then(|database| database.count_entries())
            .unwrap_or_default()
    }
}
