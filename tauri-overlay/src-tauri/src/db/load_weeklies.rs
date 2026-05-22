use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use crate::ReplayInfo;

#[derive(Debug)]
struct ReplayCacheWeeklyRecord {
    result: String,
    map_name: String,
    difficulty: String,
    brutal_plus: u64,
    mutator_values: String,
}

impl ReplayCacheWeeklyRecord {
    fn into_replay(self) -> Result<ReplayInfo, ReplayCacheDbError> {
        let mut replay = ReplayInfo::default();
        replay.set_weekly(true);
        replay.set_result(self.result);
        replay.set_map(self.map_name);
        replay.set_difficulty(self.difficulty);
        replay.set_brutal_plus(self.brutal_plus);
        replay.set_mutators(ReplayCacheArrayJson::decode_strings(&self.mutator_values)?);
        Ok(replay)
    }
}

impl ReplayCacheDatabase {
    pub fn load_weekly_replays(&self) -> Result<Vec<ReplayInfo>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT result, map_name, difficulty, brutal_plus, mutator_values
                FROM replay_cache_weeklies
                ORDER BY replay_id ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(ReplayCacheWeeklyRecord {
                    result: row.get(0)?,
                    map_name: row.get(1)?,
                    difficulty: row.get(2)?,
                    brutal_plus: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(3)?),
                    mutator_values: row.get(4)?,
                })
            })
            .map_err(|source| self.sqlite_error(source))?;
        let mut replays = Vec::new();
        for row in rows {
            replays.push(
                row.map_err(|source| self.sqlite_error(source))?
                    .into_replay()?,
            );
        }
        Ok(replays)
    }
}
