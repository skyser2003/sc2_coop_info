use super::super::core::*;
use rusqlite::{OptionalExtension, params};
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use std::collections::HashSet;

impl ReplayCacheDatabase {
    fn select_record_by_exact_file_sql() -> String {
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            WHERE file = ?1
            "
        )
    }

    fn select_record_by_file_name_sql() -> String {
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            WHERE file_name = ?1
            ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
            LIMIT 1
            "
        )
    }

    fn select_latest_record_sql() -> String {
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
            LIMIT 1
            "
        )
    }

    fn select_entry_records_page_sql() -> String {
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
            LIMIT ?1 OFFSET ?2
            "
        )
    }

    fn select_newer_entry_records_sql() -> String {
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            WHERE
                date_seconds > ?1 OR
                (date_seconds = ?1 AND date_text > ?2) OR
                (date_seconds = ?1 AND date_text = ?2 AND file > ?3) OR
                (date_seconds = ?1 AND date_text = ?2 AND file = ?3 AND hash > ?4)
            ORDER BY date_seconds ASC, date_text ASC, file ASC, hash ASC
            LIMIT ?5 OFFSET ?6
            "
        )
    }

    fn select_older_entry_records_sql() -> String {
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            WHERE
                date_seconds < ?1 OR
                (date_seconds = ?1 AND date_text < ?2) OR
                (date_seconds = ?1 AND date_text = ?2 AND file < ?3) OR
                (date_seconds = ?1 AND date_text = ?2 AND file = ?3 AND hash < ?4)
            ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
            LIMIT ?5 OFFSET ?6
            "
        )
    }

    pub(super) fn load_entry_records(
        &self,
        query: ReplayCacheEntryQuery,
    ) -> Result<Vec<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let record_query = ReplayCacheEntryRecordQuery::from_entry_query(query);
        let sql = record_query.sql();
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let records = if let Some(limit) = record_query.limit(query) {
            let rows = statement
                .query_map(params![limit], ReplayCacheEntryRecord::from_row)
                .map_err(|source| self.sqlite_error(source))?;
            Self::collect_entry_records(rows, self)?
        } else {
            let rows = statement
                .query_map([], ReplayCacheEntryRecord::from_row)
                .map_err(|source| self.sqlite_error(source))?;
            Self::collect_entry_records(rows, self)?
        };
        Ok(records)
    }

    fn collect_entry_records<MappedRows>(
        rows: MappedRows,
        database: &Self,
    ) -> Result<Vec<ReplayCacheEntryRecord>, ReplayCacheDbError>
    where
        MappedRows: IntoIterator<Item = rusqlite::Result<ReplayCacheEntryRecord>>,
    {
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|source| database.sqlite_error(source))?);
        }
        Ok(records)
    }

    pub fn load_entry_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        let record = self
            .connection
            .query_row(
                ReplayCacheEntrySql::SELECT_BY_HASH,
                params![hash],
                ReplayCacheEntryRecord::from_row,
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        self.entry_from_optional_record(record)
    }

    pub fn load_entry_by_file(
        &self,
        file: &str,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        let record = self.load_record_by_file(file)?;
        self.entry_from_optional_record(record)
    }

    fn load_record_by_file(
        &self,
        file: &str,
    ) -> Result<Option<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        if let Some(record) = self.load_record_by_exact_file(file)? {
            return Ok(Some(record));
        }
        let file_name = ReplayCacheFileName::from_replay_file(file).into_string();
        if file_name.trim().is_empty() {
            return Ok(None);
        }
        let record = self
            .connection
            .query_row(
                &Self::select_record_by_file_name_sql(),
                params![file_name],
                ReplayCacheEntryRecord::from_row,
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        Ok(record)
    }

    fn load_record_by_exact_file(
        &self,
        file: &str,
    ) -> Result<Option<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        self.connection
            .query_row(
                &Self::select_record_by_exact_file_sql(),
                params![file],
                ReplayCacheEntryRecord::from_row,
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))
    }

    pub fn load_latest_entry(&self) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        let record = self
            .connection
            .query_row(
                &Self::select_latest_record_sql(),
                [],
                ReplayCacheEntryRecord::from_row,
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        self.entry_from_optional_record(record)
    }

    pub fn load_latest_entry_date_seconds(&self) -> Result<Option<u64>, ReplayCacheDbError> {
        let date_seconds = self
            .connection
            .query_row(
                "
                SELECT date_seconds
                FROM replay_cache_entries
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                LIMIT 1
                ",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        Ok(date_seconds
            .map(ReplayCacheEntryRecord::i64_to_u64)
            .filter(|seconds| *seconds > 0))
    }

    pub fn load_navigation_candidates(
        &self,
        current_file: Option<&str>,
        delta: i64,
        replay_data_active: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let current_file = current_file
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let records = match (replay_data_active, current_file, delta) {
            (true, Some(current_file), delta) if delta != 0 => {
                match self.load_record_by_file(current_file)? {
                    Some(record) => {
                        self.load_adjacent_entry_records(&record, delta, offset, limit)?
                    }
                    None => self.load_entry_records_page(offset, limit)?,
                }
            }
            _ => self.load_entry_records_page(offset, limit)?,
        };

        self.entries_from_records(records)
    }

    fn load_entry_records_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(&Self::select_entry_records_page_sql())
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(
                params![Self::usize_to_i64(limit), Self::usize_to_i64(offset)],
                ReplayCacheEntryRecord::from_row,
            )
            .map_err(|source| self.sqlite_error(source))?;
        Self::collect_entry_records(rows, self)
    }

    fn load_adjacent_entry_records(
        &self,
        current: &ReplayCacheEntryRecord,
        delta: i64,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let steps = usize::try_from(delta.unsigned_abs())
            .unwrap_or(usize::MAX)
            .max(1);
        let adjusted_offset = offset.saturating_add(steps.saturating_sub(1));
        let sql = if delta > 0 {
            Self::select_newer_entry_records_sql()
        } else {
            Self::select_older_entry_records_sql()
        };
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(
                params![
                    ReplayCacheEntryRecord::u64_to_i64(current.date_seconds),
                    &current.date_text,
                    &current.file,
                    &current.hash,
                    Self::usize_to_i64(limit),
                    Self::usize_to_i64(adjusted_offset),
                ],
                ReplayCacheEntryRecord::from_row,
            )
            .map_err(|source| self.sqlite_error(source))?;
        Self::collect_entry_records(rows, self)
    }

    fn entry_from_optional_record(
        &self,
        record: Option<ReplayCacheEntryRecord>,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        let mut entries = self.entries_from_records(record.into_iter().collect())?;
        Ok(entries.pop())
    }

    pub fn load_cached_files(&self) -> Result<HashSet<String>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(ReplayCacheEntrySql::SELECT_FILES)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| self.sqlite_error(source))?;
        let mut files = HashSet::new();
        for row in rows {
            let file = row.map_err(|source| self.sqlite_error(source))?;
            if !file.trim().is_empty() {
                files.insert(file);
            }
        }
        Ok(files)
    }

    pub fn count_entries(&self) -> Result<usize, ReplayCacheDbError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM replay_cache_entries", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }
}
