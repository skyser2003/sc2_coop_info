use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, params};
use s2coop_analyzer::cache_overall_stats_generator::{CacheIconValue, CachePlayer, CacheUnitStats};
use std::collections::BTreeMap;

impl ReplayCacheDatabase {
    pub(super) fn load_players(
        &self,
        replay_id: i64,
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
                Ok((
                    ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(0)?) as u8,
                    row.get::<_, Option<i64>>(1)?
                        .map(ReplayCacheEntryRecord::i64_to_u32),
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?
                        .map(ReplayCacheEntryRecord::i64_to_u32),
                    row.get::<_, Option<i64>>(4)?
                        .map(ReplayCacheEntryRecord::i64_to_u32),
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?
                        .map(ReplayCacheEntryRecord::i64_to_u64),
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<i64>>(8)?
                        .map(ReplayCacheEntryRecord::i64_to_bool),
                    row.get::<_, Option<i64>>(9)?
                        .map(ReplayCacheEntryRecord::i64_to_u32),
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    ReplayCacheEntryRecord::i64_to_bool(row.get::<_, i64>(13)?),
                    ReplayCacheEntryRecord::i64_to_bool(row.get::<_, i64>(14)?),
                    ReplayCacheEntryRecord::i64_to_bool(row.get::<_, i64>(15)?),
                    row.get::<_, String>(16)?,
                ))
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut players = Vec::new();
        for row in rows {
            let (
                pid,
                apm,
                commander,
                commander_level,
                commander_mastery_level,
                handle,
                kills,
                name,
                observer,
                prestige,
                prestige_name,
                race,
                result,
                has_masteries,
                has_icons,
                has_units,
                mastery_values,
            ) = row.map_err(|source| self.sqlite_error(source))?;
            players.push(CachePlayer {
                pid,
                apm,
                commander,
                commander_level,
                commander_mastery_level,
                handle,
                icons: if has_icons {
                    Some(self.load_player_icons(replay_id, pid)?)
                } else {
                    None
                },
                kills,
                masteries: if has_masteries {
                    Some(Self::mastery_values_from_json(&mastery_values)?)
                } else {
                    None
                },
                name,
                observer,
                prestige,
                prestige_name,
                race,
                result,
                units: if has_units {
                    Some(self.load_player_units(replay_id, pid)?)
                } else {
                    None
                },
            });
        }
        Ok(players)
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
