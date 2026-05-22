use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayer, CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats, ReplayBuildInfo,
    ReplayMessage,
};
use std::collections::{BTreeMap, HashMap, HashSet};

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
        let records = self.load_entry_records(query)?;
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            entries.push(self.entry_from_record(record)?);
        }
        Ok(entries)
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
        let (where_sql, mut bind_values) = Self::games_page_where_clause(query);
        let total_rows = self.count_game_page_rows(&where_sql, &bind_values)?;
        let replay_ids = self.load_game_page_ids(&where_sql, &mut bind_values, query)?;
        let records = self.load_entry_records_by_ids(&replay_ids)?;
        let entries = self.summary_entries_from_records(records)?;
        Ok(ReplayCachePageResult::new(entries, total_rows))
    }

    fn summary_entries_from_records(
        &self,
        records: Vec<ReplayCacheEntryRecord>,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let replay_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let mut players_by_replay_id = self.load_players_summary_by_replay_ids(&replay_ids)?;
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            let players = players_by_replay_id.remove(&record.id).unwrap_or_default();
            entries.push(self.summary_entry_from_record(record, players)?);
        }
        Ok(entries)
    }

    fn count_game_page_rows(
        &self,
        where_sql: &str,
        bind_values: &[SqlValue],
    ) -> Result<usize, ReplayCacheDbError> {
        let sql = format!(
            "
            WITH game_rows AS ({})
            SELECT COUNT(*)
            FROM game_rows
            WHERE {where_sql}
            ",
            Self::games_page_base_sql()
        );
        let count = self
            .connection
            .query_row(&sql, params_from_iter(bind_values.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    fn load_game_page_ids(
        &self,
        where_sql: &str,
        bind_values: &mut Vec<SqlValue>,
        query: &ReplayCacheGamesPageQuery,
    ) -> Result<Vec<i64>, ReplayCacheDbError> {
        let order_sql = Self::games_page_order_clause(query);
        let sql = format!(
            "
            WITH game_rows AS ({})
            SELECT id
            FROM game_rows
            WHERE {where_sql}
            {order_sql}
            LIMIT ? OFFSET ?
            ",
            Self::games_page_base_sql()
        );
        bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().limit())));
        bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().offset())));
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Self::collect_replay_ids(rows, self)
    }

    fn load_entry_records_by_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<Vec<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let mut records = Vec::with_capacity(replay_ids.len());
        for replay_id in replay_ids {
            if let Some(record) = self.load_record_by_id(*replay_id)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn games_page_base_sql() -> &'static str {
        "
        SELECT
            e.id,
            e.file,
            e.date_seconds,
            e.date_text,
            e.hash,
            e.result,
            e.map_name,
            CASE
                WHEN TRIM(e.ext_difficulty) <> '' THEN e.ext_difficulty
                WHEN TRIM(e.difficulty_p2) <> '' THEN e.difficulty_p2
                WHEN TRIM(e.difficulty_p1) <> '' THEN e.difficulty_p1
                ELSE 'Unknown'
            END AS difficulty,
            e.enemy_race,
            CASE e.length_realtime_kind
                WHEN 'float' THEN COALESCE(e.length_realtime_float, 0.0)
                ELSE COALESCE(e.length_realtime_int, 0)
            END AS length_realtime,
            e.brutal_plus,
            e.extension,
            e.weekly,
            e.mutator_values,
            CASE
                WHEN e.weekly = 1 OR json_array_length(e.mutator_values) > 0 THEN 1
                ELSE 0
            END AS is_mutation,
            p1.player_name AS p1_name,
            p2.player_name AS p2_name,
            p1.commander AS p1_commander,
            p2.commander AS p2_commander
        FROM replay_cache_entries e
        LEFT JOIN replay_cache_players p1 ON p1.replay_id = e.id AND p1.pid = 1
        LEFT JOIN replay_cache_players p2 ON p2.replay_id = e.id AND p2.pid = 2
        "
    }

    fn games_page_where_clause(query: &ReplayCacheGamesPageQuery) -> (String, Vec<SqlValue>) {
        let mut clauses = Vec::new();
        let mut bind_values = Vec::new();

        if !query.include_normal_games() && !query.include_mutation_games() {
            clauses.push("0 = 1".to_string());
        } else if !query.include_normal_games() {
            clauses.push("is_mutation = 1".to_string());
        } else if !query.include_mutation_games() {
            clauses.push("is_mutation = 0".to_string());
        }

        if let Some(difficulty_clause) = Self::games_difficulty_where_clause(query) {
            clauses.push(difficulty_clause);
        }

        let search = query.search().trim();
        if !search.is_empty() {
            let pattern = Self::sqlite_contains_pattern(search);
            clauses.push(
                "
                (
                    LOWER(COALESCE(file, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(result, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(map_name, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(difficulty, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(enemy_race, 'Unknown')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(p1_name, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(p2_name, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(p1_commander, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(p2_commander, '')) LIKE ? ESCAPE '\\' OR
                    LOWER(COALESCE(mutator_values, '')) LIKE ? ESCAPE '\\'
                )
                "
                .to_string(),
            );
            for _ in 0..10 {
                bind_values.push(SqlValue::Text(pattern.clone()));
            }
        }

        let where_sql = if clauses.is_empty() {
            "1 = 1".to_string()
        } else {
            clauses.join(" AND ")
        };
        (where_sql, bind_values)
    }

    fn games_difficulty_where_clause(query: &ReplayCacheGamesPageQuery) -> Option<String> {
        let filters = query.difficulty_filters();
        if filters.is_empty() {
            return Some("0 = 1".to_string());
        }

        let all_filters = ReplayCacheDifficultyFilter::all();
        if all_filters
            .iter()
            .all(|filter| filters.iter().any(|value| value == filter))
        {
            return None;
        }

        let mut clauses = Vec::new();
        for filter in filters {
            if let Some(level) = filter.brutal_plus_level() {
                clauses.push(format!("brutal_plus = {level}"));
            } else if let Some(label) = filter.regular_label() {
                if *filter == ReplayCacheDifficultyFilter::Brutal {
                    clauses.push(
                        "
                        (
                            brutal_plus <= 0 AND
                            LOWER(TRIM(difficulty)) NOT IN ('casual', 'normal', 'hard')
                        )
                        "
                        .to_string(),
                    );
                } else {
                    clauses.push(format!(
                        "(brutal_plus <= 0 AND LOWER(TRIM(difficulty)) = '{label}')"
                    ));
                }
            }
        }

        if clauses.is_empty() {
            Some("0 = 1".to_string())
        } else {
            Some(format!("({})", clauses.join(" OR ")))
        }
    }

    fn games_page_order_clause(query: &ReplayCacheGamesPageQuery) -> String {
        let direction = query.sort_direction().sql_keyword();
        let expression = match query.sort_key() {
            ReplayCacheGameSortKey::Map => "LOWER(COALESCE(map_name, ''))",
            ReplayCacheGameSortKey::Result => "LOWER(COALESCE(result, ''))",
            ReplayCacheGameSortKey::PlayerOne => {
                "LOWER(COALESCE(p1_name, '') || ' ' || COALESCE(p1_commander, ''))"
            }
            ReplayCacheGameSortKey::PlayerTwo => {
                "LOWER(COALESCE(p2_name, '') || ' ' || COALESCE(p2_commander, ''))"
            }
            ReplayCacheGameSortKey::Enemy => "LOWER(COALESCE(enemy_race, 'Unknown'))",
            ReplayCacheGameSortKey::Length => "length_realtime",
            ReplayCacheGameSortKey::Difficulty => "LOWER(COALESCE(difficulty, ''))",
            ReplayCacheGameSortKey::Mutators => "LOWER(COALESCE(mutator_values, ''))",
            ReplayCacheGameSortKey::Time => "date_seconds",
            ReplayCacheGameSortKey::Actions => "LOWER(COALESCE(file, ''))",
        };
        let time_direction = if query.sort_key() == ReplayCacheGameSortKey::Time {
            direction
        } else {
            "DESC"
        };
        format!(
            "
            ORDER BY {expression} {direction},
                date_seconds {time_direction}, date_text {time_direction}, file {time_direction}, hash {time_direction}
            "
        )
    }

    fn load_entry_records(
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

    fn entry_from_record(
        &self,
        record: ReplayCacheEntryRecord,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        let amon_units = if record.has_amon_units {
            Some(self.load_amon_units(record.id)?)
        } else {
            None
        };
        let messages = self.load_messages(record.id)?;
        let player_stats = if record.has_player_stats {
            Some(self.load_player_stats(record.id)?)
        } else {
            None
        };
        let players = self.load_players(record.id)?;
        self.entry_from_record_with_payloads(record, amon_units, messages, player_stats, players)
    }

    fn summary_entry_from_record(
        &self,
        record: ReplayCacheEntryRecord,
        players: Vec<CachePlayer>,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        self.entry_from_record_with_payloads(record, None, Vec::new(), None, players)
    }

    fn entry_from_record_with_payloads(
        &self,
        record: ReplayCacheEntryRecord,
        amon_units: Option<BTreeMap<String, CacheUnitStats>>,
        messages: Vec<ReplayMessage>,
        player_stats: Option<BTreeMap<u8, CachePlayerStatsSeries>>,
        players: Vec<CachePlayer>,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        Ok(CacheReplayEntry {
            accurate_length: record.length_realtime,
            amon_units,
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
            messages,
            mutators: ReplayCacheArrayJson::decode_strings(&record.mutator_values)?,
            player_stats,
            players,
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
