use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats, ReplayMessage,
};
use std::collections::BTreeMap;

impl ReplayCacheDatabase {
    pub fn load_summary_entries_for_stats(
        &self,
        query: &ReplayCacheStatsQuery,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let records = self.load_stats_entry_records(query)?;
        self.summary_entries_from_records(records)
    }

    pub fn load_entries_for_stats(
        &self,
        query: &ReplayCacheStatsQuery,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let records = self.load_stats_entry_records(query)?;
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            entries.push(self.entry_from_record(record)?);
        }
        Ok(entries)
    }

    fn load_stats_entry_records(
        &self,
        query: &ReplayCacheStatsQuery,
    ) -> Result<Vec<ReplayCacheEntryRecord>, ReplayCacheDbError> {
        let (where_sql, mut bind_values) = Self::stats_where_clause(query);
        let limit_sql = if query.limit() > 0 {
            bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.limit())));
            "LIMIT ?"
        } else {
            ""
        };
        let sql = format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries e
            WHERE {where_sql}
            ORDER BY e.date_seconds DESC, e.date_text DESC, e.file DESC, e.hash DESC
            {limit_sql}
            "
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                ReplayCacheEntryRecord::from_row(row)
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|source| self.sqlite_error(source))?);
        }
        Ok(records)
    }

    fn stats_where_clause(query: &ReplayCacheStatsQuery) -> (String, Vec<SqlValue>) {
        let mut clauses = Vec::new();
        let mut bind_values = Vec::new();

        if query.scope() == ReplayCacheReadScope::DetailedOnly {
            clauses.push("e.detailed_analysis = 1".to_string());
        }

        if !query.include_normal_games() && !query.include_mutations() {
            clauses.push("0 = 1".to_string());
        } else if !query.include_normal_games() {
            clauses.push("e.extension = 1".to_string());
        } else if !query.include_mutations() {
            clauses.push("e.extension = 0".to_string());
        }

        if !query.include_wins() && !query.include_losses() {
            clauses.push("0 = 1".to_string());
        } else if !query.include_wins() {
            clauses.push(
                "LOWER(TRIM(e.result)) IN ('defeat', 'loss', 'lose', '0', 'false')".to_string(),
            );
        } else if !query.include_losses() {
            clauses.push("LOWER(TRIM(e.result)) IN ('victory', 'win', '1', 'true')".to_string());
        } else {
            clauses.push(
                "
                LOWER(TRIM(e.result)) IN (
                    'victory', 'win', '1', 'true',
                    'defeat', 'loss', 'lose', '0', 'false'
                )
                "
                .to_string(),
            );
        }

        let length_expression = Self::stats_length_expression();
        if query.min_length_seconds() > 0 {
            clauses.push(format!("{length_expression} >= ?"));
            bind_values.push(SqlValue::Real(query.min_length_seconds() as f64));
        }
        if query.max_length_seconds() > 0 {
            clauses.push(format!("{length_expression} <= ?"));
            bind_values.push(SqlValue::Real(query.max_length_seconds() as f64));
        }

        if let Some(min_date_seconds) = query.min_date_seconds() {
            clauses.push("e.date_seconds > ?".to_string());
            bind_values.push(SqlValue::Integer(ReplayCacheEntryRecord::u64_to_i64(
                min_date_seconds,
            )));
        }
        if let Some(max_date_seconds) = query.max_date_seconds() {
            clauses.push("e.date_seconds < ?".to_string());
            bind_values.push(SqlValue::Integer(ReplayCacheEntryRecord::u64_to_i64(
                max_date_seconds,
            )));
        }

        if let Some(player_pattern) = Self::stats_player_like_pattern(query.player_filter()) {
            clauses.push(
                "
                e.id IN (
                    SELECT p.replay_id
                    FROM replay_cache_players p
                    WHERE p.player_name LIKE ? ESCAPE '\\' COLLATE NOCASE
                )
                "
                .to_string(),
            );
            bind_values.push(SqlValue::Text(player_pattern));
        }

        let difficulty_expression = Self::stats_difficulty_expression();
        for exclusion in query.difficulty_exclusions() {
            if let Some(level) = exclusion.brutal_plus_level() {
                clauses.push("e.brutal_plus <> ?".to_string());
                bind_values.push(SqlValue::Integer(level));
                continue;
            }

            if let Some(label) = exclusion.difficulty_label() {
                if exclusion.is_brutal_label() {
                    clauses.push(format!(
                        "(e.brutal_plus > 0 OR instr({difficulty_expression}, ?) = 0)"
                    ));
                } else {
                    clauses.push(format!("instr({difficulty_expression}, ?) = 0"));
                }
                bind_values.push(SqlValue::Text(label.to_string()));
            }
        }

        let where_sql = if clauses.is_empty() {
            "1 = 1".to_string()
        } else {
            clauses.join(" AND ")
        };
        (where_sql, bind_values)
    }

    fn stats_length_expression() -> &'static str {
        "
        CASE e.length_realtime_kind
            WHEN 'float' THEN COALESCE(e.length_realtime_float, 0.0)
            ELSE COALESCE(e.length_realtime_int, 0)
        END
        "
    }

    fn stats_difficulty_expression() -> &'static str {
        "
        CASE
            WHEN TRIM(e.ext_difficulty) <> '' THEN e.ext_difficulty
            WHEN TRIM(e.difficulty_p2) <> '' THEN e.difficulty_p2
            WHEN TRIM(e.difficulty_p1) <> '' THEN e.difficulty_p1
            ELSE 'Unknown'
        END
        "
    }

    fn stats_player_like_pattern(player_filter: &str) -> Option<String> {
        let trimmed = player_filter.trim();
        if trimmed.is_empty() || !trimmed.is_ascii() {
            return None;
        }

        let mut pattern = String::with_capacity(trimmed.len());
        for ch in trimmed.chars() {
            match ch {
                '*' => pattern.push('%'),
                '?' => pattern.push('_'),
                '%' | '_' | '\\' => {
                    pattern.push('\\');
                    pattern.push(ch);
                }
                _ => pattern.push(ch),
            }
        }
        Some(pattern)
    }

    pub(super) fn load_messages(
        &self,
        replay_id: i64,
    ) -> Result<Vec<ReplayMessage>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT text, player, time
                FROM replay_cache_messages
                WHERE replay_id = ?1
                ORDER BY message_index ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![replay_id], |row| {
                Ok(ReplayMessage {
                    text: row.get(0)?,
                    player: ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(1)?) as u8,
                    time: row.get(2)?,
                })
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|source| self.sqlite_error(source))?);
        }
        Ok(messages)
    }

    pub(super) fn load_amon_units(
        &self,
        replay_id: i64,
    ) -> Result<BTreeMap<String, CacheUnitStats>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT unit_name, created_kind, created_count, created_hidden,
                    lost_kind, lost_count, lost_hidden, kills, fraction
                FROM replay_cache_amon_units
                WHERE replay_id = ?1
                ORDER BY unit_name ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![replay_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, f64>(8)?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut units = BTreeMap::new();
        for row in rows {
            let (
                unit_name,
                created_kind,
                created_count,
                created_hidden,
                lost_kind,
                lost_count,
                lost_hidden,
                kills,
                fraction,
            ) = row.map_err(|source| self.sqlite_error(source))?;
            units.insert(
                unit_name,
                CacheUnitStats(
                    Self::count_value_from_columns(created_kind, created_count, created_hidden),
                    Self::count_value_from_columns(lost_kind, lost_count, lost_hidden),
                    kills,
                    fraction,
                ),
            );
        }
        Ok(units)
    }

    pub(super) fn load_player_stats(
        &self,
        replay_id: i64,
    ) -> Result<BTreeMap<u8, CachePlayerStatsSeries>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT stats.pid, COALESCE(player.player_name, ''), stats.supply_values, stats.mining_values,
                    stats.army_values, stats.killed_values
                FROM replay_cache_player_stat_series stats
                LEFT JOIN replay_cache_players player
                    ON player.replay_id = stats.replay_id AND player.pid = stats.pid
                WHERE stats.replay_id = ?1
                ORDER BY stats.pid ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![replay_id], |row| {
                Ok((
                    ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(0)?) as u8,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut series = BTreeMap::new();
        for row in rows {
            let (pid, name, supply_values, mining_values, army_values, killed_values) =
                row.map_err(|source| self.sqlite_error(source))?;
            series.insert(
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
        Ok(series)
    }
}
