use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use crate::PlayerRowPayload;
use rusqlite::{Error as SqlError, Row, params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{CacheIconValue, CachePlayer, CacheUnitStats};
use std::collections::{BTreeMap, HashMap};

type ReplayPlayerKey = (i64, u8);
type PlayerUnitsByKey = HashMap<ReplayPlayerKey, BTreeMap<String, CacheUnitStats>>;
type PlayerIconsByKey = HashMap<ReplayPlayerKey, BTreeMap<String, CacheIconValue>>;

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

#[derive(Default)]
struct PlayerChildDataSets {
    units_by_player: PlayerUnitsByKey,
    icons_by_player: PlayerIconsByKey,
}

impl CachePlayerRecord {
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
        let mut players_by_replay_id = self.load_players_by_replay_ids(&[replay_id], true)?;
        Ok(players_by_replay_id.remove(&replay_id).unwrap_or_default())
    }

    pub fn load_players_by_replay_ids(
        &self,
        replay_ids: &[i64],
        include_child_data: bool,
    ) -> Result<HashMap<i64, Vec<CachePlayer>>, ReplayCacheDbError> {
        let mut players_by_replay_id: HashMap<i64, Vec<CachePlayer>> = HashMap::new();
        let mut child_data = if include_child_data {
            Some(PlayerChildDataSets {
                units_by_player: self.load_player_units_by_replay_ids(replay_ids)?,
                icons_by_player: self.load_player_icons_by_replay_ids(replay_ids)?,
            })
        } else {
            None
        };
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
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
                let player = if let Some(child_data) = child_data.as_mut() {
                    self.player_from_record_with_child_data(record, child_data)?
                } else {
                    self.player_from_record(record, false)?
                };
                players_by_replay_id
                    .entry(replay_id)
                    .or_default()
                    .push(player);
            }
        }
        Ok(players_by_replay_id)
    }

    pub fn load_players_summary_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<CachePlayer>>, ReplayCacheDbError> {
        self.load_players_by_replay_ids(replay_ids, false)
    }

    pub fn load_player_rows_page(
        &self,
        query: &ReplayCachePlayersPageQuery,
    ) -> Result<ReplayCachePageResult<PlayerRowPayload>, ReplayCacheDbError> {
        let (cte_sql, where_sql, note_bind_values, where_bind_values) =
            Self::player_rows_page_sql_parts(query);
        self.load_player_rows_page_rows(
            &cte_sql,
            &where_sql,
            &note_bind_values,
            &where_bind_values,
            query,
        )
    }

    pub fn has_player_info_rows(&self) -> Result<bool, ReplayCacheDbError> {
        let exists = self
            .connection
            .query_row(
                "
                SELECT EXISTS(
                    SELECT 1
                    FROM replay_player_infos
                    WHERE wins + losses > 0
                    LIMIT 1
                )
                ",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| self.sqlite_error(source))?;
        Ok(exists != 0)
    }

    pub fn load_overlay_player_stats_row(
        &self,
        player_handle: &str,
        player_name: &str,
    ) -> Result<Option<PlayerRowPayload>, ReplayCacheDbError> {
        let handle_key = Self::normalized_handle_key(player_handle);
        if !handle_key.is_empty()
            && let Some(row) = self.load_overlay_player_stats_row_by_handle_key(&handle_key)?
        {
            return Ok(Some(row));
        }

        let player_name = player_name.trim();
        if player_name.is_empty() {
            return Ok(None);
        }

        self.load_overlay_player_stats_row_by_latest_name(player_name)
    }

    fn overlay_player_stats_row_sql(where_sql: &str) -> String {
        format!(
            "
            SELECT
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
                info.latest_played_time AS last_seen
            FROM replay_player_infos info
            WHERE info.wins + info.losses > 0
                AND {where_sql}
            LIMIT 1
            "
        )
    }

    fn query_optional_player_row(
        &self,
        sql: &str,
        bind_value: &str,
    ) -> Result<Option<PlayerRowPayload>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|source| self.sqlite_error(source))?;
        match statement.query_row(params![bind_value], Self::player_row_payload_from_row) {
            Ok(row) => Ok(Some(row)),
            Err(SqlError::QueryReturnedNoRows) => Ok(None),
            Err(source) => Err(self.sqlite_error(source)),
        }
    }

    fn load_overlay_player_stats_row_by_handle_key(
        &self,
        handle_key: &str,
    ) -> Result<Option<PlayerRowPayload>, ReplayCacheDbError> {
        let sql = Self::overlay_player_stats_row_sql("LOWER(TRIM(info.handle)) = LOWER(TRIM(?1))");
        self.query_optional_player_row(&sql, handle_key)
    }

    fn load_overlay_player_stats_row_by_latest_name(
        &self,
        player_name: &str,
    ) -> Result<Option<PlayerRowPayload>, ReplayCacheDbError> {
        let sql = Self::overlay_player_stats_row_sql(
            "
            LOWER(TRIM(info.handle)) = (
                SELECT LOWER(TRIM(p.player_handle))
                FROM replay_cache_players p
                INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                WHERE LOWER(TRIM(p.player_name)) = LOWER(TRIM(?1))
                    AND TRIM(COALESCE(p.player_handle, '')) <> ''
                ORDER BY e.date_seconds DESC, e.date_text DESC, e.file DESC, e.hash DESC
                LIMIT 1
            )
            ",
        );
        self.query_optional_player_row(&sql, player_name)
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
        note_bind_values: &[SqlValue],
        where_bind_values: &[SqlValue],
        query: &ReplayCachePlayersPageQuery,
    ) -> Result<ReplayCachePageResult<PlayerRowPayload>, ReplayCacheDbError> {
        let order_sql = Self::player_rows_order_clause(query);
        let sql = format!(
            "
            WITH
            {cte_sql},
            total_rows AS (
                SELECT COUNT(*) AS total_rows
                FROM final_rows
                WHERE {where_sql}
            ),
            page_rows AS (
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
            )
            SELECT
                page_rows.*,
                total_rows.total_rows
            FROM page_rows
            CROSS JOIN total_rows
            "
        );
        let mut query_bind_values =
            Vec::with_capacity(note_bind_values.len() + where_bind_values.len() * 2 + 2);
        query_bind_values.extend(note_bind_values.iter().cloned());
        query_bind_values.extend(where_bind_values.iter().cloned());
        query_bind_values.extend(where_bind_values.iter().cloned());
        query_bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().limit())));
        query_bind_values.push(SqlValue::Integer(Self::usize_to_i64(query.page().offset())));
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params_from_iter(query_bind_values.iter()), |row| {
                Ok((
                    Self::player_row_payload_from_row(row)?,
                    row.get::<_, i64>("total_rows")?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut payloads = Vec::new();
        let mut total_rows = 0usize;
        for row in rows {
            let (payload, row_total) = row.map_err(|source| self.sqlite_error(source))?;
            total_rows = usize::try_from(row_total).unwrap_or(usize::MAX);
            payloads.push(payload);
        }
        if payloads.is_empty() && query.page().offset() > 0 {
            let mut count_bind_values =
                Vec::with_capacity(note_bind_values.len() + where_bind_values.len());
            count_bind_values.extend(note_bind_values.iter().cloned());
            count_bind_values.extend(where_bind_values.iter().cloned());
            total_rows = self.count_player_rows_page(cte_sql, where_sql, &count_bind_values)?;
        }
        Ok(ReplayCachePageResult::new(payloads, total_rows))
    }

    fn player_rows_page_sql_parts(
        query: &ReplayCachePlayersPageQuery,
    ) -> (String, String, Vec<SqlValue>, Vec<SqlValue>) {
        let mut note_bind_values = Vec::new();
        let note_values_sql = Self::player_note_values_sql(query, &mut note_bind_values);
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
        let mut where_bind_values = Vec::new();
        let where_sql = Self::player_rows_where_clause(query, &mut where_bind_values);
        (cte_sql, where_sql, note_bind_values, where_bind_values)
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

    fn player_from_record_with_child_data(
        &self,
        record: CachePlayerRecord,
        child_data: &mut PlayerChildDataSets,
    ) -> Result<CachePlayer, ReplayCacheDbError> {
        let player_key = (record.replay_id, record.pid);
        Ok(CachePlayer {
            pid: record.pid,
            apm: record.apm,
            commander: record.commander,
            commander_level: record.commander_level,
            commander_mastery_level: record.commander_mastery_level,
            handle: record.handle,
            icons: if record.has_icons {
                Some(
                    child_data
                        .icons_by_player
                        .remove(&player_key)
                        .unwrap_or_default(),
                )
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
            units: if record.has_units {
                Some(
                    child_data
                        .units_by_player
                        .remove(&player_key)
                        .unwrap_or_default(),
                )
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

    fn load_player_units_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<PlayerUnitsByKey, ReplayCacheDbError> {
        let mut units_by_player = PlayerUnitsByKey::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT replay_id, pid, unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
                FROM replay_cache_player_units
                WHERE replay_id IN ({placeholders})
                ORDER BY replay_id ASC, pid ASC, unit_name ASC
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
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, f64>(8)?,
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (
                    replay_id,
                    pid,
                    unit_name,
                    created_kind,
                    created_count,
                    lost_kind,
                    lost_count,
                    kills,
                    fraction,
                ) = row.map_err(|source| self.sqlite_error(source))?;
                units_by_player.entry((replay_id, pid)).or_default().insert(
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
        Ok(units_by_player)
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
                SELECT icons.icon_name, icons.icon_kind, icons.count_value,
                    COALESCE(orders.order_values, '[]')
                FROM replay_cache_player_icons icons
                LEFT JOIN replay_cache_player_icon_orders orders
                    ON orders.replay_id = icons.replay_id
                    AND orders.pid = icons.pid
                    AND orders.icon_name = icons.icon_name
                WHERE icons.replay_id = ?1 AND icons.pid = ?2
                ORDER BY icons.icon_name ASC
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
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut icons = BTreeMap::new();
        for row in rows {
            let (icon_name, icon_kind, count_value, order_values) =
                row.map_err(|source| self.sqlite_error(source))?;
            let value = if icon_kind == "order" {
                CacheIconValue::Order(ReplayCacheArrayJson::decode_strings(&order_values)?)
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

    fn load_player_icons_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<PlayerIconsByKey, ReplayCacheDbError> {
        let mut icons_by_player = PlayerIconsByKey::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT icons.replay_id, icons.pid, icons.icon_name, icons.icon_kind,
                    icons.count_value, COALESCE(orders.order_values, '[]')
                FROM replay_cache_player_icons icons
                LEFT JOIN replay_cache_player_icon_orders orders
                    ON orders.replay_id = icons.replay_id
                    AND orders.pid = icons.pid
                    AND orders.icon_name = icons.icon_name
                WHERE icons.replay_id IN ({placeholders})
                ORDER BY icons.replay_id ASC, icons.pid ASC, icons.icon_name ASC
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
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (replay_id, pid, icon_name, icon_kind, count_value, order_values) =
                    row.map_err(|source| self.sqlite_error(source))?;
                let value = if icon_kind == "order" {
                    CacheIconValue::Order(ReplayCacheArrayJson::decode_strings(&order_values)?)
                } else {
                    CacheIconValue::Count(
                        count_value
                            .map(ReplayCacheEntryRecord::i64_to_u64)
                            .unwrap_or_default(),
                    )
                };
                icons_by_player
                    .entry((replay_id, pid))
                    .or_default()
                    .insert(icon_name, value);
            }
        }
        Ok(icons_by_player)
    }
}
