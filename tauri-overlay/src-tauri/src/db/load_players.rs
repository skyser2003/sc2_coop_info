use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use crate::PlayerRowPayload;
use rusqlite::{OptionalExtension, Row, params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{CacheIconValue, CachePlayer, CacheUnitStats};
use std::collections::{BTreeMap, HashMap};

const SUMMARY_PLAYER_BATCH_SIZE: usize = 900;

struct CachePlayerRecord {
    replay_id: i64,
    pid: u8,
    apm: Option<u32>,
    commander: Option<String>,
    commander_level: Option<u32>,
    commander_mastery_level: Option<u32>,
    handle: Option<String>,
    kills: Option<u64>,
    name: Option<String>,
    observer: Option<bool>,
    prestige: Option<u32>,
    prestige_name: Option<String>,
    race: Option<String>,
    result: Option<String>,
    has_masteries: bool,
    has_icons: bool,
    has_units: bool,
    mastery_values: String,
}

impl CachePlayerRecord {
    fn from_single_replay_row(replay_id: i64, row: &Row<'_>) -> rusqlite::Result<Self> {
        Self::from_row_columns(replay_id, row, 0)
    }

    fn from_multi_replay_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Self::from_row_columns(row.get("replay_id")?, row, 1)
    }

    fn from_row_columns(
        replay_id: i64,
        row: &Row<'_>,
        player_offset: usize,
    ) -> rusqlite::Result<Self> {
        Ok(Self {
            replay_id,
            pid: ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(player_offset)?) as u8,
            name: row.get::<_, Option<String>>(player_offset + 1)?,
            apm: row
                .get::<_, Option<i64>>(player_offset + 2)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            commander: row.get::<_, Option<String>>(player_offset + 3)?,
            commander_level: row
                .get::<_, Option<i64>>(player_offset + 4)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            commander_mastery_level: row
                .get::<_, Option<i64>>(player_offset + 5)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            handle: row.get::<_, Option<String>>(player_offset + 6)?,
            kills: row
                .get::<_, Option<i64>>(player_offset + 7)?
                .map(ReplayCacheEntryRecord::i64_to_u64),
            observer: row
                .get::<_, Option<i64>>(player_offset + 8)?
                .map(ReplayCacheEntryRecord::i64_to_bool),
            prestige: row
                .get::<_, Option<i64>>(player_offset + 9)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            prestige_name: row.get::<_, Option<String>>(player_offset + 10)?,
            race: row.get::<_, Option<String>>(player_offset + 11)?,
            result: row.get::<_, Option<String>>(player_offset + 12)?,
            has_masteries: ReplayCacheEntryRecord::i64_to_bool(row.get(player_offset + 13)?),
            has_icons: ReplayCacheEntryRecord::i64_to_bool(row.get(player_offset + 14)?),
            has_units: ReplayCacheEntryRecord::i64_to_bool(row.get(player_offset + 15)?),
            mastery_values: row.get(player_offset + 16)?,
        })
    }
}

impl ReplayCacheDatabase {
    pub fn load_players(&self, replay_id: i64) -> Result<Vec<CachePlayer>, ReplayCacheDbError> {
        self.load_players_with_child_data(replay_id, true)
    }

    fn load_players_with_child_data(
        &self,
        replay_id: i64,
        include_child_data: bool,
    ) -> Result<Vec<CachePlayer>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT
                    p.pid, p.player_name, p.apm, p.commander, p.commander_level,
                    p.commander_mastery_level, p.player_handle, p.kills,
                    p.observer, p.prestige,
                    p.prestige_name, p.race, p.result, p.has_masteries, p.has_icons,
                    p.has_units, p.mastery_values
                FROM replay_cache_players p
                WHERE p.replay_id = ?1
                ORDER BY p.pid ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![replay_id], |row| {
                CachePlayerRecord::from_single_replay_row(replay_id, row)
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut players = Vec::new();
        for row in rows {
            let record = row.map_err(|source| self.sqlite_error(source))?;
            players.push(self.player_from_record(record, include_child_data)?);
        }
        Ok(players)
    }

    pub fn load_players_summary_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<CachePlayer>>, ReplayCacheDbError> {
        let mut players_by_replay_id: HashMap<i64, Vec<CachePlayer>> = HashMap::new();
        for replay_id_batch in replay_ids.chunks(SUMMARY_PLAYER_BATCH_SIZE) {
            if replay_id_batch.is_empty() {
                continue;
            }
            let placeholders = std::iter::repeat_n("?", replay_id_batch.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "
                SELECT
                    p.replay_id, p.pid, p.player_name, p.apm, p.commander, p.commander_level,
                    p.commander_mastery_level, p.player_handle, p.kills,
                    p.observer, p.prestige, p.prestige_name, p.race, p.result,
                    p.has_masteries, p.has_icons, p.has_units, p.mastery_values
                FROM replay_cache_players p
                WHERE p.replay_id IN ({placeholders})
                ORDER BY p.replay_id ASC, p.pid ASC
                "
            );
            let mut statement = self
                .connection
                .prepare(&sql)
                .map_err(|source| self.sqlite_error(source))?;
            let rows = statement
                .query_map(
                    params_from_iter(replay_id_batch.iter().copied()),
                    CachePlayerRecord::from_multi_replay_row,
                )
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let record = row.map_err(|source| self.sqlite_error(source))?;
                let replay_id = record.replay_id;
                let player = self.player_from_record(record, false)?;
                players_by_replay_id
                    .entry(replay_id)
                    .or_default()
                    .push(player);
            }
        }
        Ok(players_by_replay_id)
    }

    pub fn load_player_rows_page(
        &self,
        query: &ReplayCachePlayersPageQuery,
    ) -> Result<ReplayCachePageResult<PlayerRowPayload>, ReplayCacheDbError> {
        let (cte_sql, where_sql, mut bind_values) = Self::player_rows_page_sql_parts(query);
        let total_rows = self.count_player_rows_page(&cte_sql, &where_sql, &bind_values)?;
        let rows =
            self.load_player_rows_page_rows(&cte_sql, &where_sql, &mut bind_values, query)?;
        Ok(ReplayCachePageResult::new(rows, total_rows))
    }

    fn count_player_rows_page(
        &self,
        cte_sql: &str,
        where_sql: &str,
        bind_values: &[SqlValue],
    ) -> Result<usize, ReplayCacheDbError> {
        let sql = format!(
            "
            WITH {cte_sql}
            SELECT COUNT(*)
            FROM final_rows
            WHERE {where_sql}
            "
        );
        let count = self
            .connection
            .query_row(&sql, params_from_iter(bind_values.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    fn load_player_rows_page_rows(
        &self,
        cte_sql: &str,
        where_sql: &str,
        bind_values: &mut Vec<SqlValue>,
        query: &ReplayCachePlayersPageQuery,
    ) -> Result<Vec<PlayerRowPayload>, ReplayCacheDbError> {
        let order_sql = Self::player_rows_order_clause(query);
        let sql = format!(
            "
            WITH {cte_sql}
            SELECT
                handle,
                player,
                player_names,
                wins,
                losses,
                winrate,
                apm,
                commander,
                frequency,
                kills,
                last_seen
            FROM final_rows
            WHERE {where_sql}
            {order_sql}
            LIMIT ? OFFSET ?
            "
        );
        bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().limit())));
        bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().offset())));
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                Self::player_row_payload_from_row(row)
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut payloads = Vec::new();
        for row in rows {
            payloads.push(row.map_err(|source| self.sqlite_error(source))?);
        }
        Ok(payloads)
    }

    fn player_rows_page_sql_parts(
        query: &ReplayCachePlayersPageQuery,
    ) -> (String, String, Vec<SqlValue>) {
        let mut bind_values = Vec::new();
        let note_values_sql = Self::player_note_values_sql(query, &mut bind_values);
        let cte_sql = format!(
            "
            note_values(handle_key, note) AS ({note_values_sql}),
            final_rows AS (
                SELECT
                    LOWER(TRIM(info.handle)) AS handle_key,
                    info.handle,
                    COALESCE((
                        SELECT p.player_name
                        FROM replay_cache_players p
                        INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                        WHERE p.player_handle = info.handle
                            AND TRIM(COALESCE(p.player_name, '')) <> ''
                        ORDER BY e.date_seconds DESC, e.date_text DESC, e.file DESC, e.hash DESC
                        LIMIT 1
                    ), info.handle) AS player,
                    COALESCE((
                        SELECT json_group_array(name)
                        FROM (
                            SELECT
                                p.player_name AS name,
                                MAX(e.date_seconds) AS last_seen
                            FROM replay_cache_players p
                            INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                            WHERE p.player_handle = info.handle
                                AND TRIM(COALESCE(p.player_name, '')) <> ''
                            GROUP BY p.player_name
                            ORDER BY last_seen DESC, name ASC
                        )
                    ), '[]') AS player_names,
                    info.wins,
                    info.losses,
                    CASE
                        WHEN info.wins + info.losses <= 0 THEN 0.0
                        ELSE CAST(info.wins AS REAL) / (info.wins + info.losses)
                    END AS winrate,
                    info.average_apm AS apm,
                    info.latest_commander AS commander,
                    info.commander_frequency AS frequency,
                    info.kill_ratio AS kills,
                    info.latest_played_time AS last_seen,
                    COALESCE(note_values.note, '') AS note
                FROM replay_player_infos info
                LEFT JOIN note_values ON note_values.handle_key = LOWER(TRIM(info.handle))
                WHERE info.wins + info.losses > 0
                    AND LOWER(TRIM(info.handle)) LIKE '%-s2-%'
            )
            "
        );
        let where_sql = Self::player_rows_where_clause(query, &mut bind_values);
        (cte_sql, where_sql, bind_values)
    }

    fn player_note_values_sql(
        query: &ReplayCachePlayersPageQuery,
        bind_values: &mut Vec<SqlValue>,
    ) -> String {
        if query.notes().is_empty() {
            return "SELECT '' AS handle_key, '' AS note WHERE 0".to_string();
        }

        let mut placeholders = Vec::new();
        for note in query.notes() {
            let handle_key = Self::normalized_handle_key(note.handle());
            if handle_key.is_empty() {
                continue;
            }
            placeholders.push("(?, ?)".to_string());
            bind_values.push(SqlValue::Text(handle_key));
            bind_values.push(SqlValue::Text(note.note().to_string()));
        }

        if placeholders.is_empty() {
            "SELECT '' AS handle_key, '' AS note WHERE 0".to_string()
        } else {
            format!("VALUES {}", placeholders.join(", "))
        }
    }

    fn player_rows_where_clause(
        query: &ReplayCachePlayersPageQuery,
        bind_values: &mut Vec<SqlValue>,
    ) -> String {
        let search = query.search().trim();
        if search.is_empty() {
            return "1 = 1".to_string();
        }

        let pattern = Self::sqlite_contains_pattern(search);
        for _ in 0..5 {
            bind_values.push(SqlValue::Text(pattern.clone()));
        }
        "
        (
            LOWER(COALESCE(handle, '')) LIKE ? ESCAPE '\\' OR
            LOWER(COALESCE(player, '')) LIKE ? ESCAPE '\\' OR
            LOWER(COALESCE(player_names, '')) LIKE ? ESCAPE '\\' OR
            LOWER(COALESCE(commander, '')) LIKE ? ESCAPE '\\' OR
            LOWER(COALESCE(note, '')) LIKE ? ESCAPE '\\'
        )
        "
        .to_string()
    }

    fn player_rows_order_clause(query: &ReplayCachePlayersPageQuery) -> String {
        let direction = query.sort_direction().sql_keyword();
        let expression = match query.sort_key() {
            ReplayCachePlayerSortKey::Handle => "LOWER(COALESCE(handle, ''))",
            ReplayCachePlayerSortKey::Player => "LOWER(COALESCE(player, ''))",
            ReplayCachePlayerSortKey::Wins => "wins",
            ReplayCachePlayerSortKey::Losses => "losses",
            ReplayCachePlayerSortKey::Winrate => "winrate",
            ReplayCachePlayerSortKey::Apm => "apm",
            ReplayCachePlayerSortKey::Commander => "LOWER(COALESCE(commander, ''))",
            ReplayCachePlayerSortKey::Frequency => "frequency",
            ReplayCachePlayerSortKey::Kills => "kills",
            ReplayCachePlayerSortKey::LastSeen => "last_seen",
            ReplayCachePlayerSortKey::Note => "LOWER(COALESCE(note, ''))",
        };
        let last_seen_direction = if query.sort_key() == ReplayCachePlayerSortKey::LastSeen {
            direction
        } else {
            "DESC"
        };
        format!(
            "
            ORDER BY {expression} {direction},
                last_seen {last_seen_direction}, LOWER(COALESCE(handle, '')) ASC
            "
        )
    }

    fn player_row_payload_from_row(row: &Row<'_>) -> rusqlite::Result<PlayerRowPayload> {
        let player_names_text = row.get::<_, String>(2)?;
        Ok(PlayerRowPayload {
            handle: row.get(0)?,
            player: row.get(1)?,
            player_names: ReplayCacheArrayJson::decode_strings(&player_names_text)
                .unwrap_or_default(),
            wins: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(3)?),
            losses: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(4)?),
            winrate: row.get(5)?,
            apm: row.get(6)?,
            commander: row.get(7)?,
            frequency: row.get(8)?,
            kills: row.get(9)?,
            last_seen: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(10)?),
        })
    }

    fn normalized_handle_key(value: &str) -> String {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.contains("-s2-") {
            normalized
        } else {
            String::new()
        }
    }

    fn player_from_record(
        &self,
        record: CachePlayerRecord,
        include_child_data: bool,
    ) -> Result<CachePlayer, ReplayCacheDbError> {
        Ok(CachePlayer {
            pid: record.pid,
            apm: record.apm,
            commander: record.commander,
            commander_level: record.commander_level,
            commander_mastery_level: record.commander_mastery_level,
            handle: record.handle,
            icons: if include_child_data && record.has_icons {
                Some(self.load_player_icons(record.replay_id, record.pid)?)
            } else {
                None
            },
            kills: record.kills,
            masteries: if record.has_masteries {
                Some(Self::mastery_values_from_json(&record.mastery_values)?)
            } else {
                None
            },
            name: record.name,
            observer: record.observer,
            prestige: record.prestige,
            prestige_name: record.prestige_name,
            race: record.race,
            result: record.result,
            units: if include_child_data && record.has_units {
                Some(self.load_player_units(record.replay_id, record.pid)?)
            } else {
                None
            },
        })
    }

    fn mastery_values_from_json(text: &str) -> Result<[u32; 6], ReplayCacheDbError> {
        let mut masteries = [0u32; 6];
        for (index, value) in ReplayCacheArrayJson::decode_u32(text)?
            .into_iter()
            .enumerate()
            .take(masteries.len())
        {
            masteries[index] = value;
        }
        Ok(masteries)
    }

    fn load_player_units(
        &self,
        replay_id: i64,
        pid: u8,
    ) -> Result<BTreeMap<String, CacheUnitStats>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
                FROM replay_cache_player_units
                WHERE replay_id = ?1 AND pid = ?2
                ORDER BY unit_name ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![replay_id, i64::from(pid)], |row| {
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

    fn load_player_icons(
        &self,
        replay_id: i64,
        pid: u8,
    ) -> Result<BTreeMap<String, CacheIconValue>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT icon_name, icon_kind, count_value
                FROM replay_cache_player_icons
                WHERE replay_id = ?1 AND pid = ?2
                ORDER BY icon_name ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![replay_id, i64::from(pid)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut icons = BTreeMap::new();
        for row in rows {
            let (icon_name, icon_kind, count_value) =
                row.map_err(|source| self.sqlite_error(source))?;
            let value = if icon_kind == "order" {
                CacheIconValue::Order(self.load_player_icon_order(replay_id, pid, &icon_name)?)
            } else {
                CacheIconValue::Count(
                    count_value
                        .map(ReplayCacheEntryRecord::i64_to_u64)
                        .unwrap_or_default(),
                )
            };
            icons.insert(icon_name, value);
        }
        Ok(icons)
    }

    fn load_player_icon_order(
        &self,
        replay_id: i64,
        pid: u8,
        icon_name: &str,
    ) -> Result<Vec<String>, ReplayCacheDbError> {
        let order_values = self
            .connection
            .query_row(
                "
                SELECT order_values
                FROM replay_cache_player_icon_orders
                WHERE replay_id = ?1 AND pid = ?2 AND icon_name = ?3
                ",
                params![replay_id, i64::from(pid), icon_name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))?
            .unwrap_or_else(|| "[]".to_string());
        ReplayCacheArrayJson::decode_strings(&order_values)
    }
}
