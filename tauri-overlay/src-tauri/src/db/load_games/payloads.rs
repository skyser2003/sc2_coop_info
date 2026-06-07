use super::super::array_json::ReplayCacheArrayJson;
use super::super::core::*;
use rusqlite::params_from_iter;
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayer, CachePlayerStatsSeries, CacheReplayEntry, CacheUnitStats, ReplayBuildInfo,
    ReplayMessage,
};
use std::collections::{BTreeMap, HashMap};

impl ReplayCacheDatabase {
    pub fn summary_entries_from_records(
        &self,
        records: Vec<ReplayCacheEntryRecord>,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let replay_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let mut players_by_replay_id = self.load_players_summary_by_replay_ids(&replay_ids)?;
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            let players = players_by_replay_id.remove(&record.id).unwrap_or_default();
            entries.push(self.summary_entry_from_record(record, players)?);
        }
        Ok(entries)
    }

    pub(super) fn entries_from_records(
        &self,
        records: Vec<ReplayCacheEntryRecord>,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let replay_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let mut amon_units_by_replay_id = self.load_amon_units_by_replay_ids(&replay_ids)?;
        let mut messages_by_replay_id = self.load_messages_by_replay_ids(&replay_ids)?;
        let mut player_stats_by_replay_id = self.load_player_stats_by_replay_ids(&replay_ids)?;
        let mut players_by_replay_id = self.load_players_by_replay_ids(&replay_ids, true)?;
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            let replay_id = record.id;
            let amon_units = if record.has_amon_units {
                Some(
                    amon_units_by_replay_id
                        .remove(&replay_id)
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            let messages = messages_by_replay_id.remove(&replay_id).unwrap_or_default();
            let player_stats = if record.has_player_stats {
                Some(
                    player_stats_by_replay_id
                        .remove(&replay_id)
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            let players = players_by_replay_id.remove(&replay_id).unwrap_or_default();
            entries.push(self.entry_from_record_with_payloads(
                record,
                amon_units,
                messages,
                player_stats,
                players,
            )?);
        }
        Ok(entries)
    }

    fn load_messages_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<ReplayMessage>>, ReplayCacheDbError> {
        let mut messages_by_replay_id: HashMap<i64, Vec<ReplayMessage>> = HashMap::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT replay_id, text, player, time
                FROM replay_cache_messages
                WHERE replay_id IN ({placeholders})
                ORDER BY replay_id ASC, message_index ASC
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
                        ReplayMessage {
                            text: row.get(1)?,
                            player: ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(2)?) as u8,
                            time: row.get(3)?,
                        },
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (replay_id, message) = row.map_err(|source| self.sqlite_error(source))?;
                messages_by_replay_id
                    .entry(replay_id)
                    .or_default()
                    .push(message);
            }
        }
        Ok(messages_by_replay_id)
    }

    fn load_amon_units_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, BTreeMap<String, CacheUnitStats>>, ReplayCacheDbError> {
        let mut units_by_replay_id: HashMap<i64, BTreeMap<String, CacheUnitStats>> = HashMap::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT replay_id, unit_name, created_kind, created_count,
                    lost_kind, lost_count, kills, fraction
                FROM replay_cache_amon_units
                WHERE replay_id IN ({placeholders})
                ORDER BY replay_id ASC, unit_name ASC
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
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, f64>(7)?,
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (
                    replay_id,
                    unit_name,
                    created_kind,
                    created_count,
                    lost_kind,
                    lost_count,
                    kills,
                    fraction,
                ) = row.map_err(|source| self.sqlite_error(source))?;
                units_by_replay_id.entry(replay_id).or_default().insert(
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
        Ok(units_by_replay_id)
    }

    fn load_player_stats_by_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<HashMap<i64, BTreeMap<u8, CachePlayerStatsSeries>>, ReplayCacheDbError> {
        let mut stats_by_replay_id: HashMap<i64, BTreeMap<u8, CachePlayerStatsSeries>> =
            HashMap::new();
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::in_placeholders(replay_id_batch.len());
            let sql = format!(
                "
                SELECT stats.replay_id, stats.pid, COALESCE(player.player_name, ''),
                    stats.supply_values, stats.mining_values,
                    stats.army_values, stats.killed_values
                FROM replay_cache_player_stat_series stats
                LEFT JOIN replay_cache_players player
                    ON player.replay_id = stats.replay_id AND player.pid = stats.pid
                WHERE stats.replay_id IN ({placeholders})
                ORDER BY stats.replay_id ASC, stats.pid ASC
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
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })
                .map_err(|source| self.sqlite_error(source))?;
            for row in rows {
                let (
                    replay_id,
                    pid,
                    name,
                    supply_values,
                    mining_values,
                    army_values,
                    killed_values,
                ) = row.map_err(|source| self.sqlite_error(source))?;
                stats_by_replay_id.entry(replay_id).or_default().insert(
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
        }
        Ok(stats_by_replay_id)
    }

    pub fn entry_from_record(
        &self,
        record: ReplayCacheEntryRecord,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        let amon_units = if record.has_amon_units {
            Some(self.load_amon_units(record.id)?)
        } else {
            None
        };
        let messages = self.load_messages(record.id)?;
        let player_stats = if record.has_player_stats {
            Some(self.load_player_stats(record.id)?)
        } else {
            None
        };
        let players = self.load_players(record.id)?;
        self.entry_from_record_with_payloads(record, amon_units, messages, player_stats, players)
    }

    fn summary_entry_from_record(
        &self,
        record: ReplayCacheEntryRecord,
        players: Vec<CachePlayer>,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        self.entry_from_record_with_payloads(record, None, Vec::new(), None, players)
    }

    fn entry_from_record_with_payloads(
        &self,
        record: ReplayCacheEntryRecord,
        amon_units: Option<BTreeMap<String, CacheUnitStats>>,
        messages: Vec<ReplayMessage>,
        player_stats: Option<BTreeMap<u8, CachePlayerStatsSeries>>,
        players: Vec<CachePlayer>,
    ) -> Result<CacheReplayEntry, ReplayCacheDbError> {
        Ok(CacheReplayEntry {
            accurate_length: record.length_realtime,
            amon_units,
            bonus: if record.has_bonus {
                Some(ReplayCacheArrayJson::decode_strings(&record.bonus_values)?)
            } else {
                None
            },
            brutal_plus: record.brutal_plus,
            build: ReplayBuildInfo::new(record.replay_build, record.protocol_build),
            comp: record.comp,
            date: record.date_text,
            difficulty: (record.difficulty_p1, record.difficulty_p2),
            enemy_race: record.enemy_race,
            ext_difficulty: record.ext_difficulty,
            extension: record.extension,
            file: record.file,
            form_alength: record.form_length_realtime,
            detailed_analysis: record.detailed_analysis,
            hash: record.hash.clone(),
            length: record.length_ingame_seconds,
            map_name: record.map_name,
            messages,
            mutators: ReplayCacheArrayJson::decode_strings(&record.mutator_values)?,
            player_stats,
            players,
            region: record.region,
            result: record.result,
            weekly: record.weekly,
        })
    }
}
