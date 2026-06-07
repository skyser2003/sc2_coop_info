use super::super::array_json::ReplayCacheArrayJson;
use super::super::core::*;
use rusqlite::{Transaction, params};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheIconValue, CachePlayer, CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats,
    ReplayMessage,
};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug)]
struct StatisticsPlayerFactRecord {
    player_handle_key: String,
    commander: String,
    player_kills: i64,
}

impl StatisticsPlayerFactRecord {
    fn from_player(player: &CachePlayer) -> Option<Self> {
        if player.pid > 2 {
            return None;
        }

        let player_handle = ReplayCacheDatabase::player_handle(player);
        if player_handle.is_empty() {
            return None;
        }

        let commander = ReplayCacheStatsFactOps::normalized_commander_name(
            player.commander.as_deref().unwrap_or_default(),
        );
        if commander.is_empty() {
            return None;
        }

        Some(Self {
            player_handle_key: ReplayCacheStatsFactOps::normalized_handle_key(&player_handle),
            commander,
            player_kills: player
                .kills
                .map(ReplayCacheEntryRecord::u64_to_i64)
                .unwrap_or_default(),
        })
    }

    fn player_handle_key(&self) -> &str {
        &self.player_handle_key
    }

    fn commander(&self) -> &str {
        &self.commander
    }

    fn player_kills(&self) -> i64 {
        self.player_kills
    }
}

impl ReplayCacheDatabase {
    pub(super) fn insert_child_rows(
        tx: &Transaction<'_>,
        replay_id: i64,
        entry: &CacheReplayEntry,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for player in entry.players.iter().filter(|player| player.pid > 0) {
            Self::insert_player(tx, replay_id, player, db_path)?;
        }
        if entry.weekly {
            Self::insert_weekly(tx, replay_id, entry, db_path)?;
        }
        Self::insert_messages(tx, replay_id, &entry.messages, db_path)?;
        if let Some(units) = entry.amon_units.as_ref() {
            Self::insert_amon_units(tx, replay_id, units, db_path)?;
        }
        if let Some(player_stats) = entry.player_stats.as_ref() {
            Self::insert_player_stats(tx, replay_id, &entry.players, player_stats, db_path)?;
        }
        Ok(())
    }

    fn insert_player(
        tx: &Transaction<'_>,
        replay_id: i64,
        player: &CachePlayer,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let mastery_values = player
            .masteries
            .map(|masteries| ReplayCacheArrayJson::encode_u32(&masteries))
            .unwrap_or_else(|| ReplayCacheArrayJson::encode_u32(&[]))?;
        let player_handle = Self::player_handle(player);
        if player_handle.is_empty() {
            return Ok(());
        }
        let player_name = Self::player_name(player);
        Self::ensure_player_info(tx, &player_handle, db_path)?;
        tx.execute(
            "
            INSERT INTO replay_cache_players (
                replay_id, pid, player_name, apm, commander, commander_level,
                commander_mastery_level, player_handle, kills, observer,
                prestige, prestige_name, race, result, has_masteries, has_icons,
                has_units, mastery_values
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
            )
            ",
            params![
                replay_id,
                i64::from(player.pid),
                &player_name,
                player.apm.map(i64::from),
                player.commander.as_deref(),
                player.commander_level.map(i64::from),
                player.commander_mastery_level.map(i64::from),
                &player_handle,
                player.kills.map(ReplayCacheEntryRecord::u64_to_i64),
                player.observer.map(ReplayCacheEntryRecord::bool_to_i64),
                player.prestige.map(i64::from),
                player.prestige_name.as_deref(),
                player.race.as_deref(),
                player.result.as_deref(),
                ReplayCacheEntryRecord::bool_to_i64(player.masteries.is_some()),
                ReplayCacheEntryRecord::bool_to_i64(player.icons.is_some()),
                ReplayCacheEntryRecord::bool_to_i64(player.units.is_some()),
                &mastery_values,
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;

        let statistics_fact = StatisticsPlayerFactRecord::from_player(player);
        if let Some(fact) = statistics_fact.as_ref() {
            Self::insert_statistics_player(tx, replay_id, player.pid, fact, db_path)?;
        }
        if let Some(units) = player.units.as_ref() {
            Self::insert_player_units(tx, replay_id, player.pid, units, db_path)?;
            if let Some(fact) = statistics_fact.as_ref() {
                Self::insert_statistics_player_units(
                    tx, replay_id, player.pid, fact, units, db_path,
                )?;
            }
        }
        if let Some(icons) = player.icons.as_ref() {
            Self::insert_player_icons(tx, replay_id, player.pid, icons, db_path)?;
        }

        Ok(())
    }

    fn insert_weekly(
        tx: &Transaction<'_>,
        replay_id: i64,
        entry: &CacheReplayEntry,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let difficulty = if !entry.ext_difficulty.trim().is_empty() {
            entry.ext_difficulty.trim()
        } else if !entry.difficulty.1.trim().is_empty() {
            entry.difficulty.1.trim()
        } else if !entry.difficulty.0.trim().is_empty() {
            entry.difficulty.0.trim()
        } else {
            "Unknown"
        };
        tx.execute(
            "
            INSERT INTO replay_cache_weeklies (
                replay_id, result, map_name, difficulty, brutal_plus, mutator_values
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                replay_id,
                &entry.result,
                &entry.map_name,
                difficulty,
                i64::from(entry.brutal_plus),
                ReplayCacheArrayJson::encode_strings(&entry.mutators)?,
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    fn insert_statistics_player(
        tx: &Transaction<'_>,
        replay_id: i64,
        pid: u8,
        fact: &StatisticsPlayerFactRecord,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        tx.execute(
            "
            INSERT INTO replay_cache_stats_players (
                replay_id, pid, player_handle_key, commander
            ) VALUES (?1, ?2, ?3, ?4)
            ",
            params![
                replay_id,
                i64::from(pid),
                fact.player_handle_key(),
                fact.commander(),
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    fn insert_player_units(
        tx: &Transaction<'_>,
        replay_id: i64,
        pid: u8,
        units: &BTreeMap<String, CacheUnitStats>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for (unit_name, unit_stats) in units {
            let (created_kind, created_count) = Self::count_value_columns(&unit_stats.0);
            let (lost_kind, lost_count) = Self::count_value_columns(&unit_stats.1);
            tx.execute(
                "
                INSERT INTO replay_cache_player_units (
                    replay_id, pid, unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ",
                params![
                    replay_id,
                    i64::from(pid),
                    unit_name,
                    created_kind,
                    created_count,
                    lost_kind,
                    lost_count,
                    unit_stats.2,
                    unit_stats.3,
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    fn insert_statistics_player_units(
        tx: &Transaction<'_>,
        replay_id: i64,
        pid: u8,
        fact: &StatisticsPlayerFactRecord,
        units: &BTreeMap<String, CacheUnitStats>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for (unit_name, unit_stats) in units {
            let (created_hidden, created_count) =
                ReplayCacheStatsFactOps::unit_count_fact_columns(&unit_stats.0);
            let (lost_hidden, lost_count) =
                ReplayCacheStatsFactOps::unit_count_fact_columns(&unit_stats.1);
            tx.execute(
                "
                INSERT INTO replay_cache_stats_player_units (
                    replay_id, pid, player_handle_key, commander, player_kills,
                    unit_name, created_hidden, created_count, lost_hidden,
                    lost_count, kills
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    replay_id,
                    i64::from(pid),
                    fact.player_handle_key(),
                    fact.commander(),
                    fact.player_kills(),
                    unit_name,
                    created_hidden,
                    created_count,
                    lost_hidden,
                    lost_count,
                    unit_stats.2,
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    fn insert_player_icons(
        tx: &Transaction<'_>,
        replay_id: i64,
        pid: u8,
        icons: &BTreeMap<String, CacheIconValue>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for (icon_name, icon_value) in icons {
            let (kind, count_value) = match icon_value {
                CacheIconValue::Count(value) => {
                    ("count", Some(ReplayCacheEntryRecord::u64_to_i64(*value)))
                }
                CacheIconValue::Order(_) => ("order", None),
            };
            tx.execute(
                "
                INSERT INTO replay_cache_player_icons
                    (replay_id, pid, icon_name, icon_kind, count_value)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![replay_id, i64::from(pid), icon_name, kind, count_value],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
            if let CacheIconValue::Order(values) = icon_value {
                tx.execute(
                    "
                    INSERT INTO replay_cache_player_icon_orders
                        (replay_id, pid, icon_name, order_values)
                    VALUES (?1, ?2, ?3, ?4)
                    ",
                    params![
                        replay_id,
                        i64::from(pid),
                        icon_name,
                        ReplayCacheArrayJson::encode_strings(values)?,
                    ],
                )
                .map_err(|source| ReplayCacheDbError::Sqlite {
                    path: db_path.to_path_buf(),
                    source,
                })?;
            }
        }
        Ok(())
    }

    fn insert_messages(
        tx: &Transaction<'_>,
        replay_id: i64,
        messages: &[ReplayMessage],
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for (index, message) in messages.iter().enumerate() {
            tx.execute(
                "
                INSERT INTO replay_cache_messages
                    (replay_id, message_index, player, time, text)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    replay_id,
                    i64::try_from(index).unwrap_or(i64::MAX),
                    i64::from(message.player),
                    message.time,
                    &message.text,
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    fn insert_amon_units(
        tx: &Transaction<'_>,
        replay_id: i64,
        units: &BTreeMap<String, CacheUnitStats>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for (unit_name, unit_stats) in units {
            let (created_kind, created_count) = Self::count_value_columns(&unit_stats.0);
            let (lost_kind, lost_count) = Self::count_value_columns(&unit_stats.1);
            tx.execute(
                "
                INSERT INTO replay_cache_amon_units (
                    replay_id, unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    replay_id,
                    unit_name,
                    created_kind,
                    created_count,
                    lost_kind,
                    lost_count,
                    unit_stats.2,
                    unit_stats.3,
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    fn insert_player_stats(
        tx: &Transaction<'_>,
        replay_id: i64,
        players: &[CachePlayer],
        player_stats: &BTreeMap<u8, CachePlayerStatsSeries>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let handles_by_pid = Self::player_handles_by_pid(players);
        for (pid, stats) in player_stats.iter().filter(|(pid, _)| **pid > 0) {
            let Some(player_handle) = handles_by_pid.get(pid) else {
                continue;
            };
            Self::ensure_player_info(tx, player_handle, db_path)?;
            tx.execute(
                "
                INSERT INTO replay_cache_player_stat_series (
                    replay_id, pid, player_handle, supply_values, mining_values,
                    army_values, killed_values
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    replay_id,
                    i64::from(*pid),
                    player_handle,
                    ReplayCacheArrayJson::encode_f64(&stats.supply)?,
                    ReplayCacheArrayJson::encode_f64(&stats.mining)?,
                    ReplayCacheArrayJson::encode_stat_values(&stats.army)?,
                    ReplayCacheArrayJson::encode_u64(&stats.killed)?,
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }
}
