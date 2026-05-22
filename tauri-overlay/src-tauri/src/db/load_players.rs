use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, Row, params, params_from_iter};
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
            apm: row
                .get::<_, Option<i64>>(player_offset + 1)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            commander: row.get::<_, Option<String>>(player_offset + 2)?,
            commander_level: row
                .get::<_, Option<i64>>(player_offset + 3)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            commander_mastery_level: row
                .get::<_, Option<i64>>(player_offset + 4)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            handle: row.get::<_, Option<String>>(player_offset + 5)?,
            kills: row
                .get::<_, Option<i64>>(player_offset + 6)?
                .map(ReplayCacheEntryRecord::i64_to_u64),
            name: row.get::<_, Option<String>>(player_offset + 7)?,
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
    pub(super) fn load_players(
        &self,
        replay_id: i64,
    ) -> Result<Vec<CachePlayer>, ReplayCacheDbError> {
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
                    pid, apm, commander, commander_level, commander_mastery_level,
                    handle, kills, name, observer, prestige, prestige_name,
                    race, result, has_masteries, has_icons, has_units, mastery_values
                FROM replay_cache_players
                WHERE replay_id = ?1
                ORDER BY pid ASC
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

    pub(super) fn load_players_summary_by_replay_ids(
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
                    replay_id, pid, apm, commander, commander_level, commander_mastery_level,
                    handle, kills, name, observer, prestige, prestige_name,
                    race, result, has_masteries, has_icons, has_units, mastery_values
                FROM replay_cache_players
                WHERE replay_id IN ({placeholders})
                ORDER BY replay_id ASC, pid ASC
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
