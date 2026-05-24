use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheIconValue, CachePlayer, CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats,
    ReplayMessage,
};
use s2coop_analyzer::detailed_replay_analysis::CacheReplayCheck;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Default)]
struct PlayerInfoRefreshPlan {
    full_handles: BTreeSet<String>,
    kill_ratio_handles: BTreeSet<String>,
}

impl PlayerInfoRefreshPlan {
    fn add_full_handles(&mut self, handles: impl IntoIterator<Item = String>) {
        self.full_handles.extend(handles);
    }

    fn add_kill_ratio_handles(&mut self, handles: impl IntoIterator<Item = String>) {
        self.kill_ratio_handles.extend(handles);
    }

    fn extend(&mut self, other: Self) {
        self.full_handles.extend(other.full_handles);
        self.kill_ratio_handles.extend(other.kill_ratio_handles);
    }

    fn full_handles(&self) -> &BTreeSet<String> {
        &self.full_handles
    }

    fn kill_ratio_only_handles(&self) -> impl Iterator<Item = &String> {
        self.kill_ratio_handles
            .iter()
            .filter(|handle| !self.full_handles.contains(*handle))
    }
}

#[derive(Clone, Debug)]
struct PlayerInfoSourceRow {
    result: String,
    apm: Option<u32>,
    commander: String,
    date_seconds: u64,
    kills: Option<u64>,
    replay_total_kills: u64,
}

impl PlayerInfoSourceRow {
    fn from_row_with_handle(row: &Row<'_>) -> rusqlite::Result<(String, Self)> {
        Ok((
            row.get::<_, String>(0)?,
            Self {
                result: row.get(1)?,
                apm: row
                    .get::<_, Option<i64>>(2)?
                    .map(ReplayCacheEntryRecord::i64_to_u32),
                commander: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                date_seconds: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(4)?),
                kills: row
                    .get::<_, Option<i64>>(5)?
                    .map(ReplayCacheEntryRecord::i64_to_u64),
                replay_total_kills: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(6)?),
            },
        ))
    }

    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            result: row.get(0)?,
            apm: row
                .get::<_, Option<i64>>(1)?
                .map(ReplayCacheEntryRecord::i64_to_u32),
            commander: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            date_seconds: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(3)?),
            kills: row
                .get::<_, Option<i64>>(4)?
                .map(ReplayCacheEntryRecord::i64_to_u64),
            replay_total_kills: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(5)?),
        })
    }

    fn is_win(&self) -> Option<bool> {
        let normalized = self.result.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "victory" | "win" | "1" | "true" => Some(true),
            "defeat" | "loss" | "lose" | "0" | "false" => Some(false),
            _ => None,
        }
    }

    fn kill_ratio(&self) -> f64 {
        if self.replay_total_kills == 0 {
            return 0.0;
        }
        self.kills.unwrap_or_default() as f64 / self.replay_total_kills as f64
    }
}

#[derive(Clone, Debug)]
struct PlayerInfoAggregateRecord {
    wins: u64,
    losses: u64,
    average_apm: f64,
    latest_commander: String,
    commander_frequency: f64,
    kill_ratio: f64,
    latest_played_time: u64,
}

impl PlayerInfoAggregateRecord {
    fn from_rows(rows: &[PlayerInfoSourceRow]) -> Option<Self> {
        let valid_rows = rows
            .iter()
            .filter(|row| row.is_win().is_some())
            .collect::<Vec<_>>();
        if valid_rows.is_empty() {
            return None;
        }

        let wins = valid_rows
            .iter()
            .filter(|row| row.is_win() == Some(true))
            .count() as u64;
        let losses = valid_rows.len() as u64 - wins;
        let apm_values = valid_rows
            .iter()
            .filter_map(|row| row.apm)
            .map(f64::from)
            .collect::<Vec<_>>();
        let average_apm = if apm_values.is_empty() {
            0.0
        } else {
            apm_values.iter().sum::<f64>() / apm_values.len() as f64
        };
        let latest_played_time = valid_rows
            .iter()
            .map(|row| row.date_seconds)
            .max()
            .unwrap_or_default();
        let latest_commander = valid_rows
            .iter()
            .filter(|row| !row.commander.trim().is_empty())
            .max_by(|left, right| {
                left.date_seconds
                    .cmp(&right.date_seconds)
                    .then_with(|| left.commander.cmp(&right.commander))
            })
            .map(|row| row.commander.clone())
            .unwrap_or_default();
        let commander_games = if latest_commander.is_empty() {
            0usize
        } else {
            valid_rows
                .iter()
                .filter(|row| row.commander == latest_commander)
                .count()
        };
        let commander_frequency = if valid_rows.is_empty() {
            0.0
        } else {
            commander_games as f64 / valid_rows.len() as f64
        };
        let kill_ratio =
            valid_rows.iter().map(|row| row.kill_ratio()).sum::<f64>() / valid_rows.len() as f64;

        Some(Self {
            wins,
            losses,
            average_apm,
            latest_commander,
            commander_frequency,
            kill_ratio,
            latest_played_time,
        })
    }

    fn wins(&self) -> u64 {
        self.wins
    }

    fn losses(&self) -> u64 {
        self.losses
    }

    fn average_apm(&self) -> f64 {
        self.average_apm
    }

    fn latest_commander(&self) -> &str {
        &self.latest_commander
    }

    fn commander_frequency(&self) -> f64 {
        self.commander_frequency
    }

    fn kill_ratio(&self) -> f64 {
        self.kill_ratio
    }

    fn latest_played_time(&self) -> u64 {
        self.latest_played_time
    }
}

impl ReplayCacheDatabase {
    pub fn upsert_entries_preserving_detailed(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        Self::retry_sqlite_lock(|| self.upsert_entries_preserving_detailed_once(entries))
    }

    fn upsert_entries_preserving_detailed_once(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        let mut changed = 0usize;
        let mut refresh_plan = PlayerInfoRefreshPlan::default();
        for entry in entries {
            if entry.hash.is_empty() {
                continue;
            }
            let record = ReplayCacheEntryRecord::from_entry(entry)?;
            let (entry_changed, entry_refresh_plan) =
                Self::upsert_record(&tx, &record, entry, true, &db_path)?;
            changed = changed.saturating_add(entry_changed);
            refresh_plan.extend(entry_refresh_plan);
        }
        Self::refresh_player_infos(&tx, &refresh_plan, &db_path)?;
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    pub fn upsert_unsaved_replay_checks(
        &mut self,
        checks: &[CacheReplayCheck],
    ) -> Result<usize, ReplayCacheDbError> {
        Self::retry_sqlite_lock(|| self.upsert_unsaved_replay_checks_once(checks))
    }

    fn upsert_unsaved_replay_checks_once(
        &mut self,
        checks: &[CacheReplayCheck],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        let mut changed = 0usize;
        for check in checks {
            if check.hash().trim().is_empty() || check.file().trim().is_empty() {
                continue;
            }
            changed =
                changed.saturating_add(Self::upsert_unsaved_replay_check(&tx, check, &db_path)?);
        }
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    fn upsert_unsaved_replay_check(
        tx: &Transaction<'_>,
        check: &CacheReplayCheck,
        db_path: &Path,
    ) -> Result<usize, ReplayCacheDbError> {
        let file_name = ReplayCacheFileName::from_replay_file(check.file()).into_string();
        tx.execute(
            "
            INSERT INTO replay_cache_unsaved_replay_checks (
                hash, file, file_name, file_modified_seconds, updated_at_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(hash) DO UPDATE SET
                file = excluded.file,
                file_name = excluded.file_name,
                file_modified_seconds = excluded.file_modified_seconds,
                updated_at_seconds = excluded.updated_at_seconds
            ",
            params![
                check.hash(),
                check.file(),
                &file_name,
                ReplayCacheEntryRecord::u64_to_i64(check.modified_seconds()),
                ReplayCacheEntryRecord::u64_to_i64(ReplayCacheDatabase::now_seconds()),
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })
    }

    pub fn replace_entries(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        Self::retry_sqlite_lock(|| self.replace_entries_once(entries))
    }

    fn replace_entries_once(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        tx.execute(ReplayCacheEntrySql::DELETE_ALL, [])
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        tx.execute("DELETE FROM replay_cache_unsaved_replay_checks", [])
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        tx.execute("DELETE FROM replay_player_infos", [])
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
            let (entry_changed, _) = Self::upsert_record(&tx, &record, entry, false, &db_path)?;
            changed = changed.saturating_add(entry_changed);
        }
        Self::rebuild_all_player_infos(&tx, &db_path)?;
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    fn upsert_record(
        tx: &Transaction<'_>,
        record: &ReplayCacheEntryRecord,
        entry: &CacheReplayEntry,
        preserve_detailed: bool,
        db_path: &Path,
    ) -> Result<(usize, PlayerInfoRefreshPlan), ReplayCacheDbError> {
        let mut refresh_plan = PlayerInfoRefreshPlan::default();
        let file_replacement_handles =
            Self::load_handles_by_file_except_hash(tx, &record.file, &record.hash, db_path)?;
        refresh_plan.add_full_handles(file_replacement_handles);
        tx.execute(
            ReplayCacheEntrySql::DELETE_BY_FILE_EXCEPT_HASH,
            params![&record.file, &record.hash],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;
        if record.detailed_analysis {
            tx.execute(
                "DELETE FROM replay_cache_unsaved_replay_checks WHERE hash = ?1",
                params![&record.hash],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        }

        let existing = tx
            .query_row(
                "
                SELECT id, detailed_analysis
                FROM replay_cache_entries
                WHERE hash = ?1
                ",
                params![&record.hash],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?
            .map(|(id, detailed)| (id, ReplayCacheEntryRecord::i64_to_bool(detailed)));
        let existing_detailed = existing.map(|(_, detailed)| detailed);
        if preserve_detailed && existing_detailed == Some(true) && !record.detailed_analysis {
            return Ok((0, refresh_plan));
        }
        let existing_handles = if let Some((replay_id, _)) = existing {
            Self::load_handles_by_replay_id(tx, replay_id, db_path)?
        } else {
            Vec::new()
        };

        let (changed, replay_id) = Self::upsert_parent_row(tx, record, db_path)?;
        Self::delete_child_rows(tx, replay_id, db_path)?;
        Self::insert_child_rows(tx, replay_id, entry, db_path)?;
        let new_handles = Self::player_handles(entry);
        if preserve_detailed && existing_detailed == Some(false) && record.detailed_analysis {
            refresh_plan.add_kill_ratio_handles(existing_handles);
            refresh_plan.add_kill_ratio_handles(new_handles);
        } else {
            refresh_plan.add_full_handles(existing_handles);
            refresh_plan.add_full_handles(new_handles);
        }
        Ok((changed, refresh_plan))
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
        let replay_id = tx
            .query_row(
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
            RETURNING id
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
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let changed = usize::try_from(tx.changes()).unwrap_or(usize::MAX);
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

    fn player_handle(player: &CachePlayer) -> String {
        player
            .handle
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string()
    }

    fn player_name(player: &CachePlayer) -> String {
        player
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string()
    }

    fn player_handles_by_pid(players: &[CachePlayer]) -> HashMap<u8, String> {
        players
            .iter()
            .filter(|player| player.pid > 0)
            .map(|player| (player.pid, Self::player_handle(player)))
            .filter(|(_, handle)| !handle.is_empty())
            .collect()
    }

    fn player_handles(entry: &CacheReplayEntry) -> Vec<String> {
        entry
            .players
            .iter()
            .filter(|player| player.pid > 0)
            .map(Self::player_handle)
            .filter(|handle| !handle.is_empty())
            .collect()
    }

    fn load_handles_by_file_except_hash(
        tx: &Transaction<'_>,
        file: &str,
        hash: &str,
        db_path: &Path,
    ) -> Result<Vec<String>, ReplayCacheDbError> {
        let mut statement = tx
            .prepare(
                "
                SELECT DISTINCT p.player_handle
                FROM replay_cache_players p
                INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                WHERE e.file = ?1 AND e.hash <> ?2
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let rows = statement
            .query_map(params![file, hash], |row| row.get::<_, String>(0))
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        Self::collect_player_handles(rows, db_path)
    }

    fn load_handles_by_replay_id(
        tx: &Transaction<'_>,
        replay_id: i64,
        db_path: &Path,
    ) -> Result<Vec<String>, ReplayCacheDbError> {
        let mut statement = tx
            .prepare(
                "
                SELECT DISTINCT player_handle
                FROM replay_cache_players
                WHERE replay_id = ?1
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let rows = statement
            .query_map(params![replay_id], |row| row.get::<_, String>(0))
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        Self::collect_player_handles(rows, db_path)
    }

    fn collect_player_handles(
        rows: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<String>>,
        db_path: &Path,
    ) -> Result<Vec<String>, ReplayCacheDbError> {
        let mut handles = Vec::new();
        for row in rows {
            let handle = row.map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
            if !handle.trim().is_empty() {
                handles.push(handle);
            }
        }
        Ok(handles)
    }

    fn ensure_player_info(
        tx: &Transaction<'_>,
        player_handle: &str,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        if player_handle.is_empty() {
            return Ok(());
        }
        tx.execute(
            "
            INSERT INTO replay_player_infos (
                handle, wins, losses, average_apm, latest_commander,
                commander_frequency, kill_ratio, latest_played_time, updated_at_seconds
            ) VALUES (?1, 0, 0, 0.0, '', 0.0, 0.0, 0, ?2)
            ON CONFLICT(handle) DO NOTHING
            ",
            params![
                player_handle,
                ReplayCacheEntryRecord::u64_to_i64(ReplayCacheDatabase::now_seconds()),
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    fn rebuild_all_player_infos(
        tx: &Transaction<'_>,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let source_rows_by_handle = Self::load_all_player_info_source_rows(tx, db_path)?;
        for (handle, source_rows) in source_rows_by_handle {
            if let Some(aggregate) = PlayerInfoAggregateRecord::from_rows(&source_rows) {
                Self::upsert_player_info_aggregate(tx, &handle, &aggregate, db_path)?;
            } else {
                Self::ensure_player_info(tx, &handle, db_path)?;
            }
        }
        Ok(())
    }

    fn refresh_player_infos(
        tx: &Transaction<'_>,
        plan: &PlayerInfoRefreshPlan,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        for handle in plan.full_handles() {
            Self::refresh_player_info_full(tx, handle, db_path)?;
        }
        for handle in plan.kill_ratio_only_handles() {
            Self::refresh_player_info_kill_ratio(tx, handle, db_path)?;
        }
        Ok(())
    }

    fn load_player_info_source_rows(
        tx: &Transaction<'_>,
        player_handle: &str,
        db_path: &Path,
    ) -> Result<Vec<PlayerInfoSourceRow>, ReplayCacheDbError> {
        let mut statement = tx
            .prepare(
                "
                SELECT
                    e.result,
                    p.apm,
                    p.commander,
                    e.date_seconds,
                    p.kills,
                    COALESCE(kills_by_replay.total_kills, 0) AS total_kills
                FROM replay_cache_players p
                INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                LEFT JOIN (
                    SELECT replay_id, SUM(COALESCE(kills, 0)) AS total_kills
                    FROM replay_cache_players
                    GROUP BY replay_id
                ) kills_by_replay ON kills_by_replay.replay_id = p.replay_id
                WHERE p.player_handle = ?1
                ORDER BY e.date_seconds DESC, e.date_text DESC, e.file DESC, e.hash DESC
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let rows = statement
            .query_map(params![player_handle], PlayerInfoSourceRow::from_row)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let mut source_rows = Vec::new();
        for row in rows {
            source_rows.push(row.map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?);
        }
        Ok(source_rows)
    }

    fn load_all_player_info_source_rows(
        tx: &Transaction<'_>,
        db_path: &Path,
    ) -> Result<BTreeMap<String, Vec<PlayerInfoSourceRow>>, ReplayCacheDbError> {
        let mut statement = tx
            .prepare(
                "
                SELECT
                    p.player_handle,
                    e.result,
                    p.apm,
                    p.commander,
                    e.date_seconds,
                    p.kills,
                    COALESCE(kills_by_replay.total_kills, 0) AS total_kills
                FROM replay_cache_players p
                INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                LEFT JOIN (
                    SELECT replay_id, SUM(COALESCE(kills, 0)) AS total_kills
                    FROM replay_cache_players
                    GROUP BY replay_id
                ) kills_by_replay ON kills_by_replay.replay_id = p.replay_id
                WHERE TRIM(COALESCE(p.player_handle, '')) <> ''
                ORDER BY p.player_handle ASC,
                    e.date_seconds DESC,
                    e.date_text DESC,
                    e.file DESC,
                    e.hash DESC
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let rows = statement
            .query_map([], PlayerInfoSourceRow::from_row_with_handle)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        let mut source_rows_by_handle = BTreeMap::<String, Vec<PlayerInfoSourceRow>>::new();
        for row in rows {
            let (handle, source_row) = row.map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
            if !handle.trim().is_empty() {
                source_rows_by_handle
                    .entry(handle)
                    .or_default()
                    .push(source_row);
            }
        }
        Ok(source_rows_by_handle)
    }

    fn refresh_player_info_full(
        tx: &Transaction<'_>,
        player_handle: &str,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let source_rows = Self::load_player_info_source_rows(tx, player_handle, db_path)?;
        let Some(aggregate) = PlayerInfoAggregateRecord::from_rows(&source_rows) else {
            if source_rows.is_empty() {
                tx.execute(
                    "DELETE FROM replay_player_infos WHERE handle = ?1",
                    params![player_handle],
                )
                .map_err(|source| ReplayCacheDbError::Sqlite {
                    path: db_path.to_path_buf(),
                    source,
                })?;
            } else {
                Self::ensure_player_info(tx, player_handle, db_path)?;
            }
            return Ok(());
        };
        Self::upsert_player_info_aggregate(tx, player_handle, &aggregate, db_path)
    }

    fn refresh_player_info_kill_ratio(
        tx: &Transaction<'_>,
        player_handle: &str,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let source_rows = Self::load_player_info_source_rows(tx, player_handle, db_path)?;
        let Some(aggregate) = PlayerInfoAggregateRecord::from_rows(&source_rows) else {
            return Self::refresh_player_info_full(tx, player_handle, db_path);
        };
        let changed = tx
            .execute(
                "
                UPDATE replay_player_infos
                SET kill_ratio = ?2, updated_at_seconds = ?3
                WHERE handle = ?1
                ",
                params![
                    player_handle,
                    aggregate.kill_ratio(),
                    ReplayCacheEntryRecord::u64_to_i64(ReplayCacheDatabase::now_seconds()),
                ],
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        if changed == 0 {
            Self::upsert_player_info_aggregate(tx, player_handle, &aggregate, db_path)?;
        }
        Ok(())
    }

    fn upsert_player_info_aggregate(
        tx: &Transaction<'_>,
        player_handle: &str,
        aggregate: &PlayerInfoAggregateRecord,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        tx.execute(
            "
            INSERT INTO replay_player_infos (
                handle, wins, losses, average_apm, latest_commander,
                commander_frequency, kill_ratio, latest_played_time, updated_at_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(handle) DO UPDATE SET
                wins = excluded.wins,
                losses = excluded.losses,
                average_apm = excluded.average_apm,
                latest_commander = excluded.latest_commander,
                commander_frequency = excluded.commander_frequency,
                kill_ratio = excluded.kill_ratio,
                latest_played_time = excluded.latest_played_time,
                updated_at_seconds = excluded.updated_at_seconds
            ",
            params![
                player_handle,
                ReplayCacheEntryRecord::u64_to_i64(aggregate.wins()),
                ReplayCacheEntryRecord::u64_to_i64(aggregate.losses()),
                aggregate.average_apm(),
                aggregate.latest_commander(),
                aggregate.commander_frequency(),
                aggregate.kill_ratio(),
                ReplayCacheEntryRecord::u64_to_i64(aggregate.latest_played_time()),
                ReplayCacheEntryRecord::u64_to_i64(ReplayCacheDatabase::now_seconds()),
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;
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

        if let Some(units) = player.units.as_ref() {
            Self::insert_player_units(tx, replay_id, player.pid, units, db_path)?;
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
