mod child_rows;
mod player_info;

use super::core::*;
use player_info::PlayerInfoRefreshPlan;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{CacheReplayCheck, DetailedReplayAnalyzer};
use std::path::Path;

impl ReplayCacheDatabase {
    fn is_mm_replay_entry(entry: &CacheReplayEntry) -> bool {
        DetailedReplayAnalyzer::is_mm_replay_file(&entry.file)
    }

    fn is_mm_replay_check(check: &CacheReplayCheck) -> bool {
        DetailedReplayAnalyzer::is_mm_replay_file(check.file())
    }

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
            if entry.hash.is_empty() || Self::is_mm_replay_entry(entry) {
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
            if check.hash().trim().is_empty()
                || check.file().trim().is_empty()
                || Self::is_mm_replay_check(check)
            {
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
            if entry.hash.is_empty() || Self::is_mm_replay_entry(entry) {
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
}
