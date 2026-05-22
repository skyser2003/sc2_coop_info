use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, params};
use s2coop_analyzer::cache_overall_stats_generator::{CacheReplayEntry, ReplayBuildInfo};
use std::collections::{HashMap, HashSet};

impl ReplayCacheDatabase {
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
        let id_query = ReplayCacheEntryIdQuery::from_entry_query(query);
        let mut statement = self
            .connection
            .prepare(id_query.sql())
            .map_err(|source| self.sqlite_error(source))?;
        let replay_ids = if let Some(limit) = id_query.limit(query) {
            let rows = statement
                .query_map(params![limit], |row| row.get::<_, i64>(0))
                .map_err(|source| self.sqlite_error(source))?;
            Self::collect_replay_ids(rows, self)?
        } else {
            let rows = statement
                .query_map([], |row| row.get::<_, i64>(0))
                .map_err(|source| self.sqlite_error(source))?;
            Self::collect_replay_ids(rows, self)?
        };
        let mut entries = Vec::with_capacity(replay_ids.len());
        for replay_id in replay_ids {
            if let Some(entry) = self.load_entry_by_id(replay_id)? {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    fn collect_replay_ids<MappedRows>(
        rows: MappedRows,
        database: &Self,
    ) -> Result<Vec<i64>, ReplayCacheDbError>
    where
        MappedRows: IntoIterator<Item = rusqlite::Result<i64>>,
    {
        let mut replay_ids = Vec::new();
        for row in rows {
            replay_ids.push(row.map_err(|source| database.sqlite_error(source))?);
        }
        Ok(replay_ids)
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
        record
            .map(|record| self.entry_from_record(record))
            .transpose()
    }

    fn load_record_by_id(
        &self,
        replay_id: i64,
    ) -> Result<Option<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        self.connection
            .query_row(
                ReplayCacheEntrySql::SELECT_BY_ID,
                params![replay_id],
                ReplayCacheEntryRecord::from_row,
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))
    }

    fn load_entry_by_id(
        &self,
        replay_id: i64,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        self.load_record_by_id(replay_id)?
            .map(|record| self.entry_from_record(record))
            .transpose()
    }

    pub fn load_entry_by_file(
        &self,
        file: &str,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        self.load_record_by_file(file)?
            .map(|record| self.entry_from_record(record))
            .transpose()
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
        let replay_id = self
            .connection
            .query_row(
                ReplayCacheEntrySql::SELECT_ID_BY_FILE_NAME,
                params![file_name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        replay_id
            .map(|replay_id| self.load_record_by_id(replay_id))
            .transpose()
            .map(Option::flatten)
    }

    fn load_record_by_exact_file(
        &self,
        file: &str,
    ) -> Result<Option<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let replay_id = self
            .connection
            .query_row(
                ReplayCacheEntrySql::SELECT_ID_BY_EXACT_FILE,
                params![file],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        replay_id
            .map(|replay_id| self.load_record_by_id(replay_id))
            .transpose()
            .map(Option::flatten)
    }

    pub fn load_latest_entry(&self) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        let replay_id = self
            .connection
            .query_row(ReplayCacheEntrySql::SELECT_LATEST_ID, [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()
            .map_err(|source| self.sqlite_error(source))?;
        replay_id
            .map(|replay_id| self.load_entry_by_id(replay_id))
            .transpose()
            .map(Option::flatten)
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
        let replay_ids = match (replay_data_active, current_file, delta) {
            (true, Some(current_file), delta) if delta != 0 => {
                match self.load_record_by_file(current_file)? {
                    Some(record) => self.load_adjacent_entry_ids(&record, delta, offset, limit)?,
                    None => self.load_entry_ids_page(offset, limit)?,
                }
            }
            _ => self.load_entry_ids_page(offset, limit)?,
        };

        let mut entries = Vec::with_capacity(replay_ids.len());
        for replay_id in replay_ids {
            if let Some(entry) = self.load_entry_by_id(replay_id)? {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    fn load_entry_ids_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<i64>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(ReplayCacheEntrySql::SELECT_IDS_PAGE)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(
                params![Self::usize_to_i64(limit), Self::usize_to_i64(offset)],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| self.sqlite_error(source))?;
        Self::collect_replay_ids(rows, self)
    }

    fn load_adjacent_entry_ids(
        &self,
        current: &ReplayCacheEntryRecord,
        delta: i64,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<i64>, ReplayCacheDbError> {
        let steps = usize::try_from(delta.unsigned_abs())
            .unwrap_or(usize::MAX)
            .max(1);
        let adjusted_offset = offset.saturating_add(steps.saturating_sub(1));
        let sql = if delta > 0 {
            ReplayCacheEntrySql::SELECT_NEWER_IDS
        } else {
            ReplayCacheEntrySql::SELECT_OLDER_IDS
        };
        let mut statement = self
            .connection
            .prepare(sql)
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
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| self.sqlite_error(source))?;
        Self::collect_replay_ids(rows, self)
    }

    fn usize_to_i64(value: usize) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }

    fn entry_from_record(
        &self,
        record: ReplayCacheEntryRecord,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        Ok(CacheReplayEntry {
            accurate_length: record.length_realtime,
            amon_units: if record.has_amon_units {
                Some(self.load_amon_units(record.id)?)
            } else {
                None
            },
            bonus: if record.has_bonus {
                Some(ReplayCacheArrayJson::decode_strings(&record.bonus_values)?)
            } else {
                None
            },
            brutal_plus: record.brutal_plus,
            build: ReplayBuildInfo::new(record.replay_build, record.protocol_build),
            comp: record.comp,
            date: record.date_text,
            difficulty: (record.difficulty_p1, record.difficulty_p2),
            enemy_race: record.enemy_race,
            ext_difficulty: record.ext_difficulty,
            extension: record.extension,
            file: record.file,
            form_alength: record.form_length_realtime,
            detailed_analysis: record.detailed_analysis,
            hash: record.hash.clone(),
            length: record.length_ingame_seconds,
            map_name: record.map_name,
            messages: self.load_messages(record.id)?,
            mutators: ReplayCacheArrayJson::decode_strings(&record.mutator_values)?,
            player_stats: if record.has_player_stats {
                Some(self.load_player_stats(record.id)?)
            } else {
                None
            },
            players: self.load_players(record.id)?,
            region: record.region,
            result: record.result,
            weekly: record.weekly,
        })
    }

    pub fn load_entries_by_hash(
        &self,
    ) -> Result<HashMap<String, CacheReplayEntry>, ReplayCacheDbError> {
        Ok(self
            .load_entries(ReplayCacheEntryQuery::all(0))?
            .into_iter()
            .filter(|entry| !entry.hash.is_empty())
            .map(|entry| (entry.hash.clone(), entry))
            .collect())
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
