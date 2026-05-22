use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::params;
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayerStatsSeries, CacheUnitStats, ReplayMessage,
};
use std::collections::BTreeMap;

impl ReplayCacheDatabase {
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
                SELECT pid, name, supply_values, mining_values, army_values, killed_values
                FROM replay_cache_player_stat_series
                WHERE replay_id = ?1
                ORDER BY pid ASC
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
