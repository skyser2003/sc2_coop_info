use super::*;

impl ReplayAnalysisOps {
    pub(super) fn read_cache_summary_entries(
        cache_path: &Path,
        log_label: &str,
        query: ReplayCacheEntryQuery,
    ) -> Vec<CacheReplayEntry> {
        let db_path = ReplayCacheDatabase::db_path_for_cache_path(cache_path);
        let database = match ReplayCacheDatabase::open_for_cache_path(cache_path) {
            Ok(database) => database,
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache] failed to open {log_label} database for '{}': {error}",
                    db_path.display()
                );
                return Vec::new();
            }
        };

        match database.load_summary_entries(query) {
            Ok(entries) => entries,
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache] failed to read {log_label} database for '{}': {error}",
                    db_path.display()
                );
                Vec::new()
            }
        }
    }

    pub(super) fn read_cache_entries(
        cache_path: &Path,
        log_label: &str,
        query: ReplayCacheEntryQuery,
    ) -> Vec<CacheReplayEntry> {
        let db_path = ReplayCacheDatabase::db_path_for_cache_path(cache_path);
        let database = match ReplayCacheDatabase::open_for_cache_path(cache_path) {
            Ok(database) => database,
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache] failed to open {log_label} database for '{}': {error}",
                    db_path.display()
                );
                return Vec::new();
            }
        };

        match database.load_entries(query) {
            Ok(entries) => entries,
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache] failed to read {log_label} database for '{}': {error}",
                    db_path.display()
                );
                Vec::new()
            }
        }
    }
}

impl ReplayAnalysisOps {
    pub(super) fn recover_cache_entries_from_temp(
        cache_path: &Path,
        log_label: &str,
        query: ReplayCacheEntryQuery,
    ) -> Vec<CacheReplayEntry> {
        ReplayAnalysisOps::read_cache_entries(cache_path, log_label, query)
    }
}
