mod page_query;
mod payloads;
mod record_queries;

use super::core::*;
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::ReplayCacheFileIdentity;
use std::collections::HashMap;

impl ReplayCacheDatabase {
    pub fn load_detailed_cache_files_by_hash(
        &self,
    ) -> Result<HashMap<String, String>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT hash, file
                FROM replay_cache_entries
                WHERE detailed_analysis = 1

                UNION

                SELECT hash, file
                FROM replay_cache_unsaved_replay_checks

                ORDER BY hash ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut files_by_hash = HashMap::new();
        for row in rows {
            let (hash, file) = row.map_err(|source| self.sqlite_error(source))?;
            if !hash.trim().is_empty() && !file.trim().is_empty() {
                files_by_hash.insert(hash, file);
            }
        }
        Ok(files_by_hash)
    }

    pub fn load_detailed_cache_identities_by_hash(
        &self,
    ) -> Result<HashMap<String, ReplayCacheFileIdentity>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT hash, date_seconds
                FROM replay_cache_entries
                WHERE detailed_analysis = 1

                UNION

                SELECT hash, file_modified_seconds
                FROM replay_cache_unsaved_replay_checks

                ORDER BY hash ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut identities_by_hash = HashMap::new();
        for row in rows {
            let (hash, modified_seconds) = row.map_err(|source| self.sqlite_error(source))?;
            if !hash.trim().is_empty() {
                identities_by_hash.insert(
                    hash.clone(),
                    ReplayCacheFileIdentity::new(
                        hash,
                        ReplayCacheEntryRecord::i64_to_u64(modified_seconds),
                    ),
                );
            }
        }
        Ok(identities_by_hash)
    }

    pub fn load_entries(
        &self,
        query: ReplayCacheEntryQuery,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        self.load_entries_with_query(query)
    }

    fn load_entries_with_query(
        &self,
        query: ReplayCacheEntryQuery,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let records = self.load_entry_records(query)?;
        self.entries_from_records(records)
    }

    pub fn load_summary_entries(
        &self,
        query: ReplayCacheEntryQuery,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let records = self.load_entry_records(query)?;
        self.summary_entries_from_records(records)
    }

    pub fn load_summary_entries_page(
        &self,
        query: &ReplayCacheGamesPageQuery,
    ) -> Result<ReplayCachePageResult<CacheReplayEntry>, ReplayCacheDbError> {
        let (where_sql, bind_values) = Self::games_page_where_clause(query);
        let page = self.load_game_page_records(&where_sql, &bind_values, query)?;
        let total_rows = page.total_rows();
        let entries = self.summary_entries_from_records(page.into_rows_and_total().0)?;
        Ok(ReplayCachePageResult::new(entries, total_rows))
    }
}
