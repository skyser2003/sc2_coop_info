use super::super::core::*;
use rusqlite::{Row, Transaction, params, params_from_iter};
use s2coop_analyzer::cache_overall_stats_generator::{CachePlayer, CacheReplayEntry};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayerInfoRefreshMode {
    Full,
    KillRatio,
}

#[derive(Debug, Default)]
pub(super) struct PlayerInfoRefreshPlan {
    full_handles: BTreeSet<String>,
    kill_ratio_handles: BTreeSet<String>,
}

impl PlayerInfoRefreshPlan {
    pub(super) fn add_full_handles(&mut self, handles: impl IntoIterator<Item = String>) {
        self.full_handles.extend(handles);
    }

    pub(super) fn add_kill_ratio_handles(&mut self, handles: impl IntoIterator<Item = String>) {
        self.kill_ratio_handles.extend(handles);
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.full_handles.extend(other.full_handles);
        self.kill_ratio_handles.extend(other.kill_ratio_handles);
    }

    fn modes_by_handle(&self) -> BTreeMap<String, PlayerInfoRefreshMode> {
        let mut modes_by_handle = BTreeMap::new();
        for handle in &self.kill_ratio_handles {
            modes_by_handle.insert(handle.clone(), PlayerInfoRefreshMode::KillRatio);
        }
        for handle in &self.full_handles {
            modes_by_handle.insert(handle.clone(), PlayerInfoRefreshMode::Full);
        }
        modes_by_handle
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
    pub(super) fn player_handle(player: &CachePlayer) -> String {
        player
            .handle
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string()
    }

    pub(super) fn player_name(player: &CachePlayer) -> String {
        player
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string()
    }

    pub(super) fn player_handles_by_pid(players: &[CachePlayer]) -> HashMap<u8, String> {
        players
            .iter()
            .filter(|player| player.pid > 0)
            .map(|player| (player.pid, Self::player_handle(player)))
            .filter(|(_, handle)| !handle.is_empty())
            .collect()
    }

    pub(super) fn player_handles(entry: &CacheReplayEntry) -> Vec<String> {
        entry
            .players
            .iter()
            .filter(|player| player.pid > 0)
            .map(Self::player_handle)
            .filter(|handle| !handle.is_empty())
            .collect()
    }

    pub(super) fn load_handles_by_file_except_hash(
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

    pub(super) fn load_handles_by_replay_id(
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

    pub(super) fn ensure_player_info(
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

    pub(super) fn rebuild_all_player_infos(
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

    pub(super) fn refresh_player_infos(
        tx: &Transaction<'_>,
        plan: &PlayerInfoRefreshPlan,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let modes_by_handle = plan.modes_by_handle();
        let handles = modes_by_handle.keys().cloned().collect::<BTreeSet<_>>();
        let source_rows_by_handle =
            Self::load_player_info_source_rows_by_handles(tx, &handles, db_path)?;
        for (handle, mode) in modes_by_handle {
            let source_rows = source_rows_by_handle
                .get(&handle)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            match mode {
                PlayerInfoRefreshMode::Full => {
                    Self::refresh_player_info_full_from_rows(tx, &handle, source_rows, db_path)?;
                }
                PlayerInfoRefreshMode::KillRatio => {
                    Self::refresh_player_info_kill_ratio_from_rows(
                        tx,
                        &handle,
                        source_rows,
                        db_path,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn load_player_info_source_rows_by_handles(
        tx: &Transaction<'_>,
        handles: &BTreeSet<String>,
        db_path: &Path,
    ) -> Result<BTreeMap<String, Vec<PlayerInfoSourceRow>>, ReplayCacheDbError> {
        let mut source_rows_by_handle = BTreeMap::<String, Vec<PlayerInfoSourceRow>>::new();
        if handles.is_empty() {
            return Ok(source_rows_by_handle);
        }

        let handle_values = handles.iter().map(String::as_str).collect::<Vec<_>>();
        for handle_batch in ReplayCacheSqlBatch::chunks(&handle_values) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(handle_batch.len());
            let sql = format!(
                "
                WITH selected_players AS (
                    SELECT
                        p.player_handle,
                        p.replay_id,
                        e.result,
                        p.apm,
                        p.commander,
                        e.date_seconds,
                        e.date_text,
                        e.file,
                        e.hash,
                        p.kills
                    FROM replay_cache_players p
                    INNER JOIN replay_cache_entries e ON e.id = p.replay_id
                    WHERE p.player_handle IN ({placeholders})
                ),
                selected_replays AS (
                    SELECT DISTINCT replay_id
                    FROM selected_players
                ),
                kills_by_replay AS (
                    SELECT p.replay_id, SUM(COALESCE(p.kills, 0)) AS total_kills
                    FROM replay_cache_players p
                    INNER JOIN selected_replays selected
                        ON selected.replay_id = p.replay_id
                    GROUP BY p.replay_id
                )
                SELECT
                    selected_players.player_handle,
                    selected_players.result,
                    selected_players.apm,
                    selected_players.commander,
                    selected_players.date_seconds,
                    selected_players.kills,
                    COALESCE(kills_by_replay.total_kills, 0) AS total_kills
                FROM selected_players
                LEFT JOIN kills_by_replay
                    ON kills_by_replay.replay_id = selected_players.replay_id
                ORDER BY selected_players.player_handle ASC,
                    selected_players.date_seconds DESC,
                    selected_players.date_text DESC,
                    selected_players.file DESC,
                    selected_players.hash DESC
                "
            );
            let mut statement = tx
                .prepare(&sql)
                .map_err(|source| ReplayCacheDbError::Sqlite {
                    path: db_path.to_path_buf(),
                    source,
                })?;
            let rows = statement
                .query_map(
                    params_from_iter(handle_batch.iter().copied()),
                    PlayerInfoSourceRow::from_row_with_handle,
                )
                .map_err(|source| ReplayCacheDbError::Sqlite {
                    path: db_path.to_path_buf(),
                    source,
                })?;
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
        }
        Ok(source_rows_by_handle)
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

    fn refresh_player_info_full_from_rows(
        tx: &Transaction<'_>,
        player_handle: &str,
        source_rows: &[PlayerInfoSourceRow],
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let Some(aggregate) = PlayerInfoAggregateRecord::from_rows(source_rows) else {
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

    fn refresh_player_info_kill_ratio_from_rows(
        tx: &Transaction<'_>,
        player_handle: &str,
        source_rows: &[PlayerInfoSourceRow],
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let Some(aggregate) = PlayerInfoAggregateRecord::from_rows(source_rows) else {
            return Self::refresh_player_info_full_from_rows(
                tx,
                player_handle,
                source_rows,
                db_path,
            );
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
}
