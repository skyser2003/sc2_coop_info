use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, Transaction, params};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheIconValue, CachePlayer, CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats,
    ReplayMessage,
};
use std::collections::BTreeMap;
use std::path::Path;

impl ReplayCacheDatabase {
    pub fn upsert_entries_preserving_detailed(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        let mut changed = 0usize;
        for entry in entries {
            if entry.hash.is_empty() {
                continue;
            }
            let record = ReplayCacheEntryRecord::from_entry(entry)?;
            changed =
                changed.saturating_add(Self::upsert_record(&tx, &record, entry, true, &db_path)?);
        }
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    pub fn replace_entries(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        tx.execute(ReplayCacheEntrySql::DELETE_ALL, [])
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        let mut changed = 0usize;
        for entry in entries {
            if entry.hash.is_empty() {
                continue;
            }
            let record = ReplayCacheEntryRecord::from_entry(entry)?;
            changed =
                changed.saturating_add(Self::upsert_record(&tx, &record, entry, false, &db_path)?);
        }
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    pub(super) fn upsert_record(
        tx: &Transaction<'_>,
        record: &ReplayCacheEntryRecord,
        entry: &CacheReplayEntry,
        preserve_detailed: bool,
        db_path: &Path,
    ) -> Result<usize, ReplayCacheDbError> {
        tx.execute(
            ReplayCacheEntrySql::DELETE_BY_FILE_EXCEPT_HASH,
            params![&record.file, &record.hash],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;

        let existing_detailed = tx
            .query_row(
                ReplayCacheEntrySql::SELECT_DETAILED_BY_HASH,
                params![&record.hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?
            .map(ReplayCacheEntryRecord::i64_to_bool);
        if preserve_detailed && existing_detailed == Some(true) && !record.detailed_analysis {
            return Ok(0);
        }

        let (changed, replay_id) = Self::upsert_parent_row(tx, record, db_path)?;
        Self::delete_child_rows(tx, replay_id, db_path)?;
        Self::insert_child_rows(tx, replay_id, entry, db_path)?;
        Ok(changed)
    }

    fn delete_child_rows(
        tx: &Transaction<'_>,
        replay_id: i64,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for table in REPLAY_CACHE_CHILD_TABLES {
            tx.execute(table.delete_by_replay_id_sql(), params![replay_id])
                .map_err(|source| ReplayCacheDbError::Sqlite {
                    path: db_path.to_path_buf(),
                    source,
                })?;
        }
        Ok(())
    }

    fn upsert_parent_row(
        tx: &Transaction<'_>,
        record: &ReplayCacheEntryRecord,
        db_path: &Path,
    ) -> Result<(usize, i64), ReplayCacheDbError> {
        let (length_kind, length_int, length_float) =
            ReplayCacheEntryRecord::cache_numeric_columns(&record.length_realtime);
        let (protocol_kind, protocol_int, protocol_text) =
            ReplayCacheEntryRecord::protocol_build_columns(&record.protocol_build);
        let changed = tx
            .execute(
                "
            INSERT INTO replay_cache_entries (
                hash,
                file,
                file_name,
                date_text,
                date_seconds,
                detailed_analysis,
                result,
                map_name,
                difficulty_p1,
                difficulty_p2,
                ext_difficulty,
                brutal_plus,
                extension,
                weekly,
                region,
                length_ingame_seconds,
                length_realtime_kind,
                length_realtime_int,
                length_realtime_float,
                form_length_realtime,
                replay_build,
                protocol_build_kind,
                protocol_build_int,
                protocol_build_text,
                comp,
                enemy_race,
                has_amon_units,
                has_bonus,
                has_player_stats,
                mutator_values,
                bonus_values,
                updated_at_seconds
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
                ?31, ?32
            )
            ON CONFLICT(hash) DO UPDATE SET
                file = excluded.file,
                file_name = excluded.file_name,
                date_text = excluded.date_text,
                date_seconds = excluded.date_seconds,
                detailed_analysis = excluded.detailed_analysis,
                result = excluded.result,
                map_name = excluded.map_name,
                difficulty_p1 = excluded.difficulty_p1,
                difficulty_p2 = excluded.difficulty_p2,
                ext_difficulty = excluded.ext_difficulty,
                brutal_plus = excluded.brutal_plus,
                extension = excluded.extension,
                weekly = excluded.weekly,
                region = excluded.region,
                length_ingame_seconds = excluded.length_ingame_seconds,
                length_realtime_kind = excluded.length_realtime_kind,
                length_realtime_int = excluded.length_realtime_int,
                length_realtime_float = excluded.length_realtime_float,
                form_length_realtime = excluded.form_length_realtime,
                replay_build = excluded.replay_build,
                protocol_build_kind = excluded.protocol_build_kind,
                protocol_build_int = excluded.protocol_build_int,
                protocol_build_text = excluded.protocol_build_text,
                comp = excluded.comp,
                enemy_race = excluded.enemy_race,
                has_amon_units = excluded.has_amon_units,
                has_bonus = excluded.has_bonus,
                has_player_stats = excluded.has_player_stats,
                mutator_values = excluded.mutator_values,
                bonus_values = excluded.bonus_values,
                updated_at_seconds = excluded.updated_at_seconds
            ",
                params![
                    &record.hash,
                    &record.file,
                    &record.file_name,
                    &record.date_text,
                    ReplayCacheEntryRecord::u64_to_i64(record.date_seconds),
                    ReplayCacheEntryRecord::bool_to_i64(record.detailed_analysis),
                    &record.result,
                    &record.map_name,
                    &record.difficulty_p1,
                    &record.difficulty_p2,
                    &record.ext_difficulty,
                    i64::from(record.brutal_plus),
                    ReplayCacheEntryRecord::bool_to_i64(record.extension),
                    ReplayCacheEntryRecord::bool_to_i64(record.weekly),
                    &record.region,
                    ReplayCacheEntryRecord::u64_to_i64(record.length_ingame_seconds),
                    length_kind,
                    length_int,
                    length_float,
                    &record.form_length_realtime,
                    i64::from(record.replay_build),
                    protocol_kind,
                    protocol_int,
                    protocol_text.as_deref(),
                    record.comp.as_deref(),
                    record.enemy_race.as_deref(),
                    ReplayCacheEntryRecord::bool_to_i64(record.has_amon_units),
                    ReplayCacheEntryRecord::bool_to_i64(record.has_bonus),
                    ReplayCacheEntryRecord::bool_to_i64(record.has_player_stats),
                    &record.mutator_values,
                    &record.bonus_values,
                    ReplayCacheEntryRecord::u64_to_i64(record.updated_at_seconds),
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let replay_id = tx
            .query_row(
                ReplayCacheEntrySql::SELECT_ID_BY_HASH,
                params![&record.hash],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        Ok((changed, replay_id))
    }

    fn insert_child_rows(
        tx: &Transaction<'_>,
        replay_id: i64,
        entry: &CacheReplayEntry,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for player in entry.players.iter().filter(|player| player.pid > 0) {
            Self::insert_player(tx, replay_id, player, db_path)?;
        }
        Self::insert_messages(tx, replay_id, &entry.messages, db_path)?;
        if let Some(units) = entry.amon_units.as_ref() {
            Self::insert_amon_units(tx, replay_id, units, db_path)?;
        }
        if let Some(player_stats) = entry.player_stats.as_ref() {
            Self::insert_player_stats(tx, replay_id, player_stats, db_path)?;
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
        tx.execute(
            "
            INSERT INTO replay_cache_players (
                replay_id, pid, apm, commander, commander_level,
                commander_mastery_level, handle, kills, name, observer,
                prestige, prestige_name, race, result, has_masteries, has_icons,
                has_units, mastery_values
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
            )
            ",
            params![
                replay_id,
                i64::from(player.pid),
                player.apm.map(i64::from),
                player.commander.as_deref(),
                player.commander_level.map(i64::from),
                player.commander_mastery_level.map(i64::from),
                player.handle.as_deref(),
                player.kills.map(ReplayCacheEntryRecord::u64_to_i64),
                player.name.as_deref(),
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

        if let Some(units) = player.units.as_ref() {
            Self::insert_player_units(tx, replay_id, player.pid, units, db_path)?;
        }
        if let Some(icons) = player.icons.as_ref() {
            Self::insert_player_icons(tx, replay_id, player.pid, icons, db_path)?;
        }

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
            let (created_kind, created_count, created_hidden) =
                Self::count_value_columns_with_hidden(&unit_stats.0);
            let (lost_kind, lost_count, lost_hidden) =
                Self::count_value_columns_with_hidden(&unit_stats.1);
            tx.execute(
                "
                INSERT INTO replay_cache_amon_units (
                    replay_id, unit_name, created_kind, created_count, created_hidden,
                    lost_kind, lost_count, lost_hidden, kills, fraction
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    replay_id,
                    unit_name,
                    created_kind,
                    created_count,
                    created_hidden,
                    lost_kind,
                    lost_count,
                    lost_hidden,
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
        player_stats: &BTreeMap<u8, CachePlayerStatsSeries>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for (pid, stats) in player_stats.iter().filter(|(pid, _)| **pid > 0) {
            tx.execute(
                "
                INSERT INTO replay_cache_player_stat_series (
                    replay_id, pid, name, supply_values, mining_values,
                    army_values, killed_values
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    replay_id,
                    i64::from(*pid),
                    &stats.name,
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
