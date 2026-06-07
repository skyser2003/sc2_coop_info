use super::super::core::*;
use rusqlite::{params_from_iter, types::Value as SqlValue};

impl ReplayCacheDatabase {
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

    pub(super) fn load_game_page_records(
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

    pub(super) fn games_page_where_clause(
        query: &ReplayCacheGamesPageQuery,
    ) -> (String, Vec<SqlValue>) {
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
}
