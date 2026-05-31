use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayer, CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats, ReplayBuildInfo,
    ReplayMessage,
};
use s2coop_analyzer::detailed_replay_analysis::ReplayCacheFileIdentity;
use std::collections::{BTreeMap, HashMap, HashSet};

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

    pub fn summary_entries_from_records(
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

    fn load_game_page_records(
        &self,
        where_sql: &str,
        bind_values: &[SqlValue],
        query: &ReplayCacheGamesPageQuery,
    ) -> Result<ReplayCachePageResult<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let order_sql = Self::games_page_order_clause(query);
        let sql = format!(
            "
            WITH
                count_game_rows AS ({}),
                page_game_rows AS ({})
            SELECT
                page_game_rows.*,
                total_rows.total_rows
            FROM page_game_rows
            CROSS JOIN (
                SELECT COUNT(*) AS total_rows
                FROM count_game_rows
                WHERE {where_sql}
            ) total_rows
            WHERE {where_sql}
            {order_sql}
            LIMIT ? OFFSET ?
            ",
            Self::games_page_base_sql(),
            Self::games_page_base_sql()
        );
        let mut query_bind_values = Vec::with_capacity(bind_values.len() * 2 + 2);
        query_bind_values.extend(bind_values.iter().cloned());
        query_bind_values.extend(bind_values.iter().cloned());
        query_bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().limit())));
        query_bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().offset())));
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params_from_iter(query_bind_values.iter()), |row| {
                Ok((
                    ReplayCacheEntryRecord::from_row(row)?,
                    row.get::<_, i64>("total_rows")?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut records = Vec::new();
        let mut total_rows = 0usize;
        for row in rows {
            let (record, row_total) = row.map_err(|source| self.sqlite_error(source))?;
            total_rows = usize::try_from(row_total).unwrap_or(usize::MAX);
            records.push(record);
        }
        if records.is_empty() && query.page().offset() > 0 {
            total_rows = self.count_game_page_rows(where_sql, bind_values)?;
        }
        Ok(ReplayCachePageResult::new(records, total_rows))
    }

    fn games_page_base_sql() -> &'static str {
        "
        SELECT
            e.id AS id,
            e.hash AS hash,
            e.file AS file,
            e.file_name AS file_name,
            e.date_text AS date_text,
            e.date_seconds AS date_seconds,
            e.detailed_analysis AS detailed_analysis,
            e.result AS result,
            e.map_name AS map_name,
            e.difficulty_p1 AS difficulty_p1,
            e.difficulty_p2 AS difficulty_p2,
            e.ext_difficulty AS ext_difficulty,
            e.brutal_plus AS brutal_plus,
            e.extension AS extension,
            e.weekly AS weekly,
            e.region AS region,
            e.length_ingame_seconds AS length_ingame_seconds,
            e.length_realtime_kind AS length_realtime_kind,
            e.length_realtime_int AS length_realtime_int,
            e.length_realtime_float AS length_realtime_float,
            e.form_length_realtime AS form_length_realtime,
            e.replay_build AS replay_build,
            e.protocol_build_kind AS protocol_build_kind,
            e.protocol_build_int AS protocol_build_int,
            e.protocol_build_text AS protocol_build_text,
            e.comp AS comp,
            e.enemy_race AS enemy_race,
            e.has_amon_units AS has_amon_units,
            e.has_bonus AS has_bonus,
            e.has_player_stats AS has_player_stats,
            e.mutator_values AS mutator_values,
            e.bonus_values AS bonus_values,
            e.updated_at_seconds AS updated_at_seconds,
            CASE
                WHEN TRIM(e.ext_difficulty) <> '' THEN e.ext_difficulty
                WHEN TRIM(e.difficulty_p2) <> '' THEN e.difficulty_p2
                WHEN TRIM(e.difficulty_p1) <> '' THEN e.difficulty_p1
                ELSE 'Unknown'
            END AS difficulty,
            CASE e.length_realtime_kind
                WHEN 'float' THEN COALESCE(e.length_realtime_float, 0.0)
                ELSE COALESCE(e.length_realtime_int, 0)
            END AS length_realtime,
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

    fn entries_from_records(
        &self,
        records: Vec<ReplayCacheEntryRecord>,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let replay_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let mut amon_units_by_replay_id = self.load_amon_units_by_replay_ids(&replay_ids)?;
        let mut messages_by_replay_id = self.load_messages_by_replay_ids(&replay_ids)?;
        let mut player_stats_by_replay_id = self.load_player_stats_by_replay_ids(&replay_ids)?;
        let mut players_by_replay_id = self.load_players_by_replay_ids(&replay_ids, true)?;
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            let replay_id = record.id;
            let amon_units = if record.has_amon_units {
                Some(
                    amon_units_by_replay_id
                        .remove(&replay_id)
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            let messages = messages_by_replay_id.remove(&replay_id).unwrap_or_default();
            let player_stats = if record.has_player_stats {
                Some(
                    player_stats_by_replay_id
                        .remove(&replay_id)
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            let players = players_by_replay_id.remove(&replay_id).unwrap_or_default();
            entries.push(self.entry_from_record_with_payloads(
                record,
                amon_units,
                messages,
                player_stats,
                players,
            )?);
        }
        Ok(entries)
    }

    fn load_messages_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<ReplayMessage>>, ReplayCacheDbError> {
        let mut messages_by_replay_id: HashMap<i64, Vec<ReplayMessage>> = HashMap::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT replay_id, text, player, time
                FROM replay_cache_messages
                WHERE replay_id IN ({placeholders})
                ORDER BY replay_id ASC, message_index ASC
                "
            );
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|source| self.sqlite_error(source))?;
            let rows = statement
                .query_map(params_from_iter(replay_id_batch.iter().copied()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        ReplayMessage {
                            text: row.get(1)?,
                            player: ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(2)?) as u8,
                            time: row.get(3)?,
                        },
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (replay_id, message) = row.map_err(|source| self.sqlite_error(source))?;
                messages_by_replay_id
                    .entry(replay_id)
                    .or_default()
                    .push(message);
            }
        }
        Ok(messages_by_replay_id)
    }

    fn load_amon_units_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, BTreeMap<String, CacheUnitStats>>, ReplayCacheDbError> {
        let mut units_by_replay_id: HashMap<i64, BTreeMap<String, CacheUnitStats>> = HashMap::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT replay_id, unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
                FROM replay_cache_amon_units
                WHERE replay_id IN ({placeholders})
                ORDER BY replay_id ASC, unit_name ASC
                "
            );
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|source| self.sqlite_error(source))?;
            let rows = statement
                .query_map(params_from_iter(replay_id_batch.iter().copied()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, f64>(7)?,
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (
                    replay_id,
                    unit_name,
                    created_kind,
                    created_count,
                    lost_kind,
                    lost_count,
                    kills,
                    fraction,
                ) = row.map_err(|source| self.sqlite_error(source))?;
                units_by_replay_id.entry(replay_id).or_default().insert(
                    unit_name,
                    CacheUnitStats(
                        Self::count_value_from_kind_and_count(created_kind, created_count),
                        Self::count_value_from_kind_and_count(lost_kind, lost_count),
                        kills,
                        fraction,
                    ),
                );
            }
        }
        Ok(units_by_replay_id)
    }

    fn load_player_stats_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, BTreeMap<u8, CachePlayerStatsSeries>>, ReplayCacheDbError> {
        let mut stats_by_replay_id: HashMap<i64, BTreeMap<u8, CachePlayerStatsSeries>> =
            HashMap::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT stats.replay_id, stats.pid, COALESCE(player.player_name, ''),
                    stats.supply_values, stats.mining_values,
                    stats.army_values, stats.killed_values
                FROM replay_cache_player_stat_series stats
                LEFT JOIN replay_cache_players player
                    ON player.replay_id = stats.replay_id AND player.pid = stats.pid
                WHERE stats.replay_id IN ({placeholders})
                ORDER BY stats.replay_id ASC, stats.pid ASC
                "
            );
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|source| self.sqlite_error(source))?;
            let rows = statement
                .query_map(params_from_iter(replay_id_batch.iter().copied()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(1)?) as u8,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (
                    replay_id,
                    pid,
                    name,
                    supply_values,
                    mining_values,
                    army_values,
                    killed_values,
                ) = row.map_err(|source| self.sqlite_error(source))?;
                stats_by_replay_id.entry(replay_id).or_default().insert(
                    pid,
                    CachePlayerStatsSeries {
                        name,
                        supply: ReplayCacheArrayJson::decode_f64(&supply_values)?,
                        mining: ReplayCacheArrayJson::decode_f64(&mining_values)?,
                        army: ReplayCacheArrayJson::decode_stat_values(&army_values)?,
                        killed: ReplayCacheArrayJson::decode_u64(&killed_values)?,
                    },
                );
            }
        }
        Ok(stats_by_replay_id)
    }

    pub fn entry_from_record(
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
