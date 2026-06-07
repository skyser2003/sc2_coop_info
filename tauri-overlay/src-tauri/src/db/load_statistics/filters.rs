use super::*;

impl ReplayCacheDatabase {
    pub(super) fn stats_where_clause(query: &ReplayCacheStatsQuery) -> (String, Vec<SqlValue>) {
        Self::stats_where_clause_with_orientation_filters(query, true)
    }

    pub(super) fn stats_prefilter_where_clause(
        query: &ReplayCacheStatsQuery,
    ) -> (String, Vec<SqlValue>) {
        Self::stats_where_clause_with_orientation_filters(query, false)
    }

    fn stats_where_clause_with_orientation_filters(
        query: &ReplayCacheStatsQuery,
        include_orientation_filters: bool,
    ) -> (String, Vec<SqlValue>) {
        let mut clauses = Vec::new();
        let mut bind_values = Vec::new();

        if query.scope() == ReplayCacheReadScope::DetailedOnly {
            clauses.push("e.detailed_analysis = 1".to_string());
        }

        if query.restrict_to_current_replay_files() && query.current_replay_files().is_empty() {
            clauses.push("0 = 1".to_string());
        } else if !query.current_replay_files().is_empty() {
            let placeholders =
                ReplayCacheSqlBatch::in_placeholders(query.current_replay_files().len());
            clauses.push(format!("e.file IN ({placeholders})"));
            bind_values.extend(
                query
                    .current_replay_files()
                    .iter()
                    .map(|file| SqlValue::Text(file.to_string())),
            );
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

        if include_orientation_filters {
            Self::push_stats_commander_level_clauses(query, &mut clauses);
            Self::push_stats_mastery_clauses(query, &mut clauses);
            Self::push_stats_multibox_clause(query, &mut clauses, &mut bind_values);
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

        if include_orientation_filters {
            Self::push_stats_region_clause(query, &mut clauses, &mut bind_values);
        }

        let where_sql = if clauses.is_empty() {
            "1 = 1".to_string()
        } else {
            clauses.join(" AND ")
        };
        (where_sql, bind_values)
    }

    fn push_stats_region_clause(
        query: &ReplayCacheStatsQuery,
        clauses: &mut Vec<String>,
        bind_values: &mut Vec<SqlValue>,
    ) {
        if query.region_exclusions().is_empty() {
            return;
        }

        clauses.push(
            "
            UPPER(TRIM(COALESCE(e.region, ''))) IN ('NA', 'EU', 'KR', 'CN', 'PTR')
            "
            .to_string(),
        );

        let placeholders = ReplayCacheSqlBatch::in_placeholders(query.region_exclusions().len());
        clauses.push(format!(
            "UPPER(TRIM(COALESCE(e.region, ''))) NOT IN ({placeholders})"
        ));
        bind_values.extend(
            query
                .region_exclusions()
                .iter()
                .map(|region| SqlValue::Text(region.to_ascii_uppercase())),
        );
    }

    fn push_stats_commander_level_clauses(
        query: &ReplayCacheStatsQuery,
        clauses: &mut Vec<String>,
    ) {
        Self::push_stats_single_commander_level_clause(
            query.include_sub_15(),
            query.include_over_15(),
            clauses,
        );
        Self::push_stats_single_commander_level_clause(
            query.include_ally_sub_15(),
            query.include_ally_over_15(),
            clauses,
        );
    }

    fn push_stats_single_commander_level_clause(
        include_sub_15: bool,
        include_over_15: bool,
        clauses: &mut Vec<String>,
    ) {
        match (include_sub_15, include_over_15) {
            (false, false) => clauses.push("0 = 1".to_string()),
            (false, true) => clauses.push(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND COALESCE(p.commander_level, 0) >= 15
                )
                "
                .to_string(),
            ),
            (true, false) => clauses.push(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND COALESCE(p.commander_level, 0) < 15
                )
                "
                .to_string(),
            ),
            (true, true) => {}
        }
    }

    fn push_stats_mastery_clauses(query: &ReplayCacheStatsQuery, clauses: &mut Vec<String>) {
        Self::push_stats_single_mastery_clause(
            query.include_main_normal_mastery(),
            query.include_main_abnormal_mastery(),
            clauses,
        );
        Self::push_stats_single_mastery_clause(
            query.include_ally_normal_mastery(),
            query.include_ally_abnormal_mastery(),
            clauses,
        );
    }

    fn push_stats_single_mastery_clause(
        include_normal_mastery: bool,
        include_abnormal_mastery: bool,
        clauses: &mut Vec<String>,
    ) {
        match (include_normal_mastery, include_abnormal_mastery) {
            (false, false) => clauses.push("0 = 1".to_string()),
            (false, true) => clauses.push(format!(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND {mastery_sum} > 90
                )
                ",
                mastery_sum = Self::stats_mastery_sum_expression("p.mastery_values")
            )),
            (true, false) => clauses.push(format!(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND {mastery_sum} <= 90
                )
                ",
                mastery_sum = Self::stats_mastery_sum_expression("p.mastery_values")
            )),
            (true, true) => {}
        }
    }

    fn push_stats_multibox_clause(
        query: &ReplayCacheStatsQuery,
        clauses: &mut Vec<String>,
        bind_values: &mut Vec<SqlValue>,
    ) {
        if query.include_both_main() || query.main_handle_keys().is_empty() {
            return;
        }

        let placeholders = ReplayCacheSqlBatch::in_placeholders(query.main_handle_keys().len());
        clauses.push(format!(
            "
            NOT EXISTS (
                SELECT 1
                FROM replay_cache_players p1
                INNER JOIN replay_cache_players p2
                    ON p2.replay_id = p1.replay_id
                    AND p2.pid = 2
                WHERE p1.replay_id = e.id
                    AND p1.pid = 1
                    AND LOWER(TRIM(p1.player_handle)) IN ({placeholders})
                    AND LOWER(TRIM(p2.player_handle)) IN ({placeholders})
            )
            "
        ));
        bind_values.extend(
            query
                .main_handle_keys()
                .iter()
                .map(|handle| SqlValue::Text(handle.to_string())),
        );
        bind_values.extend(
            query
                .main_handle_keys()
                .iter()
                .map(|handle| SqlValue::Text(handle.to_string())),
        );
    }

    fn stats_mastery_sum_expression(column: &str) -> String {
        (0..6)
            .map(|index| format!("COALESCE(json_extract({column}, '$[{index}]'), 0)"))
            .collect::<Vec<_>>()
            .join(" + ")
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

    pub fn load_messages(&self, replay_id: i64) -> Result<Vec<ReplayMessage>, ReplayCacheDbError> {
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

    pub fn load_amon_units(
        &self,
        replay_id: i64,
    ) -> Result<BTreeMap<String, CacheUnitStats>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
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
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, f64>(6)?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut units = BTreeMap::new();
        for row in rows {
            let (unit_name, created_kind, created_count, lost_kind, lost_count, kills, fraction) =
                row.map_err(|source| self.sqlite_error(source))?;
            units.insert(
                unit_name,
                CacheUnitStats(
                    Self::count_value_from_kind_and_count(created_kind, created_count),
                    Self::count_value_from_kind_and_count(lost_kind, lost_count),
                    kills,
                    fraction,
                ),
            );
        }
        Ok(units)
    }

    pub fn load_player_stats(
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
