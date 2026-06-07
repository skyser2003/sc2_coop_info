use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayerStatsSeries, CacheUnitStats, ReplayMessage,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crate::replay_analysis::{ReplayAnalysis, ReplayAnalysisOps};
use crate::shared_types::LocalizedLabels;
use crate::stats_aggregation::{
    StatsAggregateAnalysisPayload, StatsAggregateDifficultyDataRow,
    StatsAggregateFastestMapDetails, StatsAggregateMapDataRow, StatsAggregatePlayerDataRow,
    StatsAggregateRegionDataRow, StatsAggregateUnitDataPayload, StatsAggregationOps,
    StatsCommanderAggregate, StatsCommanderDataInput, StatsCommanderPlayerRecord,
    StatsCommanderTotals, StatsMapAggregate, StatsPlayerAggregate, StatsPlayerRecord,
    StatsPlayerSnapshot, StatsRegionAggregate, StatsReplaySnapshot, StatsResultSummary,
    StatsWinLossAggregate,
};
use crate::stats_units::StatsUnitDataOps;
use crate::{CommanderUnitRollup, UnitStatsRollup};

mod filters;
mod payload;
mod units;

struct ReplayCacheStatisticsLoadOps;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatisticsUnitSide {
    Main,
    Ally,
}

impl StatisticsUnitSide {
    fn from_is_main(value: i64) -> Self {
        if value != 0 { Self::Main } else { Self::Ally }
    }
}

#[derive(Clone, Debug)]
struct StatisticsPlayerUnitFactRow {
    unit_name: String,
    created_hidden: bool,
    created_count: i64,
    lost_hidden: bool,
    lost_count: i64,
    kills: i64,
}

impl StatisticsPlayerUnitFactRow {
    fn new(
        unit_name: String,
        created_hidden: bool,
        created_count: i64,
        lost_hidden: bool,
        lost_count: i64,
        kills: i64,
    ) -> Self {
        Self {
            unit_name,
            created_hidden,
            created_count,
            lost_hidden,
            lost_count,
            kills,
        }
    }

    fn unit_name(&self) -> &str {
        &self.unit_name
    }

    fn created_hidden(&self) -> bool {
        self.created_hidden
    }

    fn created_count(&self) -> i64 {
        self.created_count
    }

    fn lost_hidden(&self) -> bool {
        self.lost_hidden
    }

    fn lost_count(&self) -> i64 {
        self.lost_count
    }

    fn kills(&self) -> i64 {
        self.kills
    }
}

impl ReplayCacheStatisticsLoadOps {
    fn elapsed_ms(started_at: Instant) -> f64 {
        started_at.elapsed().as_secs_f64() * 1000.0
    }

    fn to_value<T: Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }

    fn sanitize_replay_text(value: &str) -> String {
        let mut output = String::with_capacity(value.len());
        let mut in_tag = false;
        let mut last_space = false;
        for ch in value.chars() {
            match ch {
                '<' => {
                    in_tag = true;
                    if !last_space {
                        output.push(' ');
                        last_space = true;
                    }
                }
                '>' if in_tag => {
                    in_tag = false;
                    if !last_space {
                        output.push(' ');
                        last_space = true;
                    }
                }
                _ if in_tag => {}
                _ if ch.is_control() => {}
                _ if ch.is_whitespace() => {
                    if !last_space {
                        output.push(' ');
                        last_space = true;
                    }
                }
                _ => {
                    output.push(ch);
                    last_space = false;
                }
            }
        }
        output
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .trim()
            .to_string()
    }

    fn normalized_commander_name(commander: &str) -> String {
        ReplayCacheStatsFactOps::normalized_commander_name(commander)
    }

    fn result_is_victory(result: &str) -> Option<bool> {
        match result.trim().to_ascii_lowercase().as_str() {
            "victory" | "win" | "1" | "true" => Some(true),
            "defeat" | "loss" | "lose" | "0" | "false" => Some(false),
            _ => None,
        }
    }

    fn kill_fraction(main_kills: u64, ally_kills: u64) -> f64 {
        let total = main_kills.saturating_add(ally_kills);
        if total == 0 {
            0.0
        } else {
            main_kills as f64 / total as f64
        }
    }

    fn normalized_handle_key(handle: &str) -> String {
        handle.trim().to_ascii_lowercase()
    }

    fn infer_region_from_handle(handle: &str) -> Option<String> {
        let region_code = handle.split('-').next().map(str::trim)?;
        match region_code {
            "1" => Some("NA"),
            "2" => Some("EU"),
            "3" | "8" => Some("KR"),
            "5" | "6" => Some("CN"),
            "98" => Some("PTR"),
            _ => None,
        }
        .map(str::to_string)
    }

    fn infer_owner_handle_from_replay_path(path: &str) -> Option<String> {
        let replay_path = Path::new(path);
        for component in replay_path.components() {
            let raw = component.as_os_str().to_str()?;
            let normalized = ReplayAnalysis::normalized_handle_key(raw);
            if !normalized.is_empty() {
                return Some(normalized);
            }
        }
        None
    }
}

impl ReplayCacheDatabase {
    fn sqlite_row<T>(&self, result: Result<T, rusqlite::Error>) -> Result<T, ReplayCacheDbError> {
        result.map_err(|source| self.sqlite_error(source))
    }

    pub fn load_statistics_payload(
        &self,
        query: &ReplayCacheStatsQuery,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<ReplayCacheStatisticsPayload, ReplayCacheDbError> {
        let total_started_at = Instant::now();
        let snapshots_started_at = Instant::now();
        let mut snapshots = self.load_stats_replay_snapshots(query, main_names, main_handles)?;
        let snapshot_count = snapshots.len();
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=snapshot_query scope={:?} rows={} elapsed_ms={:.3}",
            query.scope(),
            snapshot_count,
            ReplayCacheStatisticsLoadOps::elapsed_ms(snapshots_started_at)
        );

        let query_filter_started_at = Instant::now();
        snapshots.retain(|snapshot| {
            Self::stats_snapshot_matches_query(snapshot, query, main_handles, dictionary)
        });
        let matched_count = snapshots.len();
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=rust_filter rows_in={} rows_out={} elapsed_ms={:.3}",
            snapshot_count,
            matched_count,
            ReplayCacheStatisticsLoadOps::elapsed_ms(query_filter_started_at)
        );

        let detail_filter_started_at = Instant::now();
        let include_detailed = if query.scope() == ReplayCacheReadScope::DetailedOnly {
            true
        } else {
            snapshots.iter().any(|snapshot| snapshot.detailed_analysis)
        };
        if include_detailed {
            snapshots.retain(|snapshot| snapshot.detailed_analysis);
        }
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=detailed_filter include_detailed={} rows_in={} rows_out={} elapsed_ms={:.3}",
            include_detailed,
            matched_count,
            snapshots.len(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(detail_filter_started_at)
        );

        let aggregate_started_at = Instant::now();
        let payload = self.statistics_payload_from_snapshots(
            snapshots,
            include_detailed,
            main_names,
            main_handles,
            dictionary,
        )?;
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=payload_aggregate games={} elapsed_ms={:.3}",
            payload.games(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(aggregate_started_at)
        );
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=load_statistics_payload_total games={} elapsed_ms={:.3}",
            payload.games(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(total_started_at)
        );
        Ok(payload)
    }

    pub fn has_detailed_entries_for_stats(
        &self,
        query: &ReplayCacheStatsQuery,
    ) -> Result<bool, ReplayCacheDbError> {
        let detailed_query = query
            .clone()
            .with_scope(ReplayCacheReadScope::DetailedOnly)
            .with_limit(1);
        let (where_sql, mut bind_values) = Self::stats_where_clause(&detailed_query);
        let limit_sql = if detailed_query.limit() > 0 {
            bind_values.push(SqlValue::Integer(Self::usize_to_i64(
                detailed_query.limit(),
            )));
            "LIMIT ?"
        } else {
            ""
        };
        let sql = format!(
            "
            SELECT EXISTS(
                SELECT 1
                FROM replay_cache_entries e
                WHERE {where_sql}
                {limit_sql}
            )
            "
        );
        let exists = self
            .connection
            .query_row(&sql, params_from_iter(bind_values.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Ok(exists != 0)
    }

    pub fn count_entries_for_stats(
        &self,
        query: &ReplayCacheStatsQuery,
    ) -> Result<u64, ReplayCacheDbError> {
        let (where_sql, bind_values) = Self::stats_where_clause(query);
        let sql = format!(
            "
            SELECT COUNT(*)
            FROM replay_cache_entries e
            WHERE {where_sql}
            "
        );
        let count = self
            .connection
            .query_row(&sql, params_from_iter(bind_values.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Ok(ReplayCacheEntryRecord::i64_to_u64(count))
    }

    fn load_stats_replay_snapshots(
        &self,
        query: &ReplayCacheStatsQuery,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Result<Vec<StatsReplaySnapshot>, ReplayCacheDbError> {
        let (where_sql, bind_values) = Self::stats_prefilter_where_clause(query);
        let sql = format!(
            "
            SELECT
                e.id AS replay_id,
                e.file,
                e.map_name,
                e.result,
                e.date_seconds,
                e.detailed_analysis,
                e.brutal_plus,
                e.extension,
                CASE e.length_realtime_kind
                    WHEN 'float' THEN COALESCE(e.length_realtime_float, 0.0)
                    ELSE COALESCE(e.length_realtime_int, 0)
                END AS length_realtime,
                CASE
                    WHEN TRIM(e.ext_difficulty) <> '' THEN e.ext_difficulty
                    WHEN TRIM(e.difficulty_p2) <> '' THEN e.difficulty_p2
                    WHEN TRIM(e.difficulty_p1) <> '' THEN e.difficulty_p1
                    ELSE 'Unknown'
                END AS difficulty,
                COALESCE(e.enemy_race, 'Unknown') AS enemy_race,
                COALESCE(json_array_length(e.bonus_values), 0) AS bonus_completed,
                COALESCE(p1.pid, 1) AS p1_pid,
                COALESCE(p1.player_name, '') AS p1_name,
                COALESCE(p1.player_handle, '') AS p1_handle,
                COALESCE(p1.commander, '') AS p1_commander,
                COALESCE(p1.apm, 0) AS p1_apm,
                COALESCE(p1.kills, 0) AS p1_kills,
                COALESCE(p1.commander_level, 0) AS p1_commander_level,
                COALESCE(p1.commander_mastery_level, 0) AS p1_mastery_level,
                COALESCE(p1.prestige, 0) AS p1_prestige,
                COALESCE(p1.mastery_values, '[]') AS p1_masteries,
                COALESCE(p2.pid, 2) AS p2_pid,
                COALESCE(p2.player_name, '') AS p2_name,
                COALESCE(p2.player_handle, '') AS p2_handle,
                COALESCE(p2.commander, '') AS p2_commander,
                COALESCE(p2.apm, 0) AS p2_apm,
                COALESCE(p2.kills, 0) AS p2_kills,
                COALESCE(p2.commander_level, 0) AS p2_commander_level,
                COALESCE(p2.commander_mastery_level, 0) AS p2_mastery_level,
                COALESCE(p2.prestige, 0) AS p2_prestige,
                COALESCE(p2.mastery_values, '[]') AS p2_masteries
            FROM replay_cache_entries e
            LEFT JOIN replay_cache_players p1 ON p1.replay_id = e.id AND p1.pid = 1
            LEFT JOIN replay_cache_players p2 ON p2.replay_id = e.id AND p2.pid = 2
            WHERE {where_sql}
            ORDER BY e.id ASC
            "
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let mut rows = statement
            .query(params_from_iter(bind_values.iter()))
            .map_err(|source| self.sqlite_error(source))?;
        let mut snapshots = Vec::<StatsReplaySnapshot>::new();
        while let Some(row) = rows.next().map_err(|source| self.sqlite_error(source))? {
            let p1 = self.sqlite_row(Self::stats_player_from_row(row, 12))?;
            let p2 = self.sqlite_row(Self::stats_player_from_row(row, 22))?;
            let file = self.sqlite_row(row.get::<_, String>(1))?;
            let (main, ally) = Self::orient_stats_players(&file, p1, p2, main_names, main_handles);
            snapshots.push(StatsReplaySnapshot {
                replay_id: self.sqlite_row(row.get(0))?,
                file,
                map_name: self.sqlite_row(row.get(2))?,
                result: self.sqlite_row(row.get(3))?,
                date_seconds: ReplayCacheEntryRecord::i64_to_u64(
                    self.sqlite_row(row.get::<_, i64>(4))?,
                ),
                detailed_analysis: self.sqlite_row(row.get::<_, i64>(5))? != 0,
                brutal_plus: ReplayCacheEntryRecord::i64_to_u64(
                    self.sqlite_row(row.get::<_, i64>(6))?,
                ),
                extension: self.sqlite_row(row.get::<_, i64>(7))? != 0,
                length_realtime: self.sqlite_row(row.get(8))?,
                difficulty: self.sqlite_row(row.get(9))?,
                enemy_race: self.sqlite_row(row.get(10))?,
                bonus_completed: ReplayCacheEntryRecord::i64_to_u64(
                    self.sqlite_row(row.get::<_, i64>(11))?,
                ),
                main,
                ally,
            });
        }
        Ok(snapshots)
    }

    fn stats_player_from_row(
        row: &rusqlite::Row<'_>,
        offset: usize,
    ) -> Result<StatsPlayerSnapshot, rusqlite::Error> {
        let mastery_values = row.get::<_, String>(offset + 9)?;
        Ok(StatsPlayerSnapshot {
            name: row.get(offset + 1)?,
            handle: row.get(offset + 2)?,
            commander: row.get(offset + 3)?,
            apm: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(offset + 4)?),
            kills: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(offset + 5)?),
            commander_level: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(offset + 6)?),
            mastery_level: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(offset + 7)?),
            prestige: ReplayCacheEntryRecord::i64_to_u64(row.get::<_, i64>(offset + 8)?),
            masteries: ReplayCacheArrayJson::decode_u64(&mastery_values).unwrap_or_default(),
        })
    }

    fn orient_stats_players(
        file: &str,
        p1: StatsPlayerSnapshot,
        p2: StatsPlayerSnapshot,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> (StatsPlayerSnapshot, StatsPlayerSnapshot) {
        if Self::stats_players_should_swap(file, &p1, &p2, main_names, main_handles) {
            (p2, p1)
        } else {
            (p1, p2)
        }
    }

    fn stats_players_should_swap(
        file: &str,
        p1: &StatsPlayerSnapshot,
        p2: &StatsPlayerSnapshot,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> bool {
        let p1_handle = ReplayAnalysis::normalized_handle_key(&p1.handle);
        let p2_handle = ReplayAnalysis::normalized_handle_key(&p2.handle);
        if !main_handles.is_empty() && (!p1_handle.is_empty() || !p2_handle.is_empty()) {
            let p1_is_main = ReplayAnalysis::is_main_player_by_handle(&p1.handle, main_handles);
            let p2_is_main = ReplayAnalysis::is_main_player_by_handle(&p2.handle, main_handles);
            if p1_is_main != p2_is_main {
                return !p1_is_main && p2_is_main;
            }
        }

        if let Some(owner_handle) =
            ReplayCacheStatisticsLoadOps::infer_owner_handle_from_replay_path(file)
        {
            let p1_owner = !p1_handle.is_empty() && p1_handle == owner_handle;
            let p2_owner = !p2_handle.is_empty() && p2_handle == owner_handle;
            if p1_owner != p2_owner {
                return !p1_owner && p2_owner;
            }
        }

        if !main_names.is_empty() {
            let p1_is_main = ReplayAnalysis::is_main_player_by_name(&p1.name, main_names);
            let p2_is_main = ReplayAnalysis::is_main_player_by_name(&p2.name, main_names);
            if p1_is_main != p2_is_main {
                return !p1_is_main && p2_is_main;
            }
        }

        false
    }

    fn stats_snapshot_matches_query(
        snapshot: &StatsReplaySnapshot,
        query: &ReplayCacheStatsQuery,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> bool {
        if query.scope() == ReplayCacheReadScope::DetailedOnly && !snapshot.detailed_analysis {
            return false;
        }
        if !query.include_mutations() && snapshot.extension {
            return false;
        }
        if !query.include_normal_games() && !snapshot.extension {
            return false;
        }
        let Some(is_victory) = ReplayCacheStatisticsLoadOps::result_is_victory(&snapshot.result)
        else {
            return false;
        };
        if !query.include_wins() && is_victory {
            return false;
        }
        if !query.include_losses() && !is_victory {
            return false;
        }
        if query.min_length_seconds() > 0
            && snapshot.length_realtime < query.min_length_seconds() as f64
        {
            return false;
        }
        if query.max_length_seconds() > 0
            && snapshot.length_realtime > query.max_length_seconds() as f64
        {
            return false;
        }
        if let Some(min_date) = query.min_date_seconds()
            && snapshot.date_seconds <= min_date
        {
            return false;
        }
        if let Some(max_date) = query.max_date_seconds()
            && snapshot.date_seconds >= max_date
        {
            return false;
        }
        if !query.include_sub_15() && snapshot.main.commander_level < 15 {
            return false;
        }
        if !query.include_over_15() && snapshot.main.commander_level >= 15 {
            return false;
        }
        if !query.include_ally_sub_15() && snapshot.ally.commander_level < 15 {
            return false;
        }
        if !query.include_ally_over_15() && snapshot.ally.commander_level >= 15 {
            return false;
        }
        let main_mastery_points =
            StatsAggregationOps::mastery_points_invested(&snapshot.main.masteries);
        let ally_mastery_points =
            StatsAggregationOps::mastery_points_invested(&snapshot.ally.masteries);
        if !query.include_main_normal_mastery() && main_mastery_points <= 90 {
            return false;
        }
        if !query.include_main_abnormal_mastery() && main_mastery_points > 90 {
            return false;
        }
        if !query.include_ally_normal_mastery() && ally_mastery_points <= 90 {
            return false;
        }
        if !query.include_ally_abnormal_mastery() && ally_mastery_points > 90 {
            return false;
        }
        if !main_handles.is_empty() && !query.include_both_main() {
            let main_is_main =
                ReplayAnalysis::is_main_player_by_handle(&snapshot.main.handle, main_handles);
            let ally_is_main =
                ReplayAnalysis::is_main_player_by_handle(&snapshot.ally.handle, main_handles);
            if main_is_main && ally_is_main {
                return false;
            }
        }
        if !query.player_filter().is_empty() {
            let p1 = snapshot.main.name.to_ascii_lowercase();
            let p2 = snapshot.ally.name.to_ascii_lowercase();
            if !ReplayAnalysisOps::wildcard_match(query.player_filter(), &p1)
                && !ReplayAnalysisOps::wildcard_match(query.player_filter(), &p2)
            {
                return false;
            }
        }
        for exclusion in query.difficulty_exclusions() {
            if let Some(bplus) = exclusion.brutal_plus_level() {
                if snapshot.brutal_plus == u64::try_from(bplus).unwrap_or(0) {
                    return false;
                }
                continue;
            }

            if snapshot.brutal_plus > 0 && exclusion.is_brutal_label() {
                continue;
            }

            if let Some(label) = exclusion.difficulty_label()
                && snapshot.difficulty.contains(label)
            {
                return false;
            }
        }
        if !query.region_exclusions().is_empty() {
            let region =
                ReplayCacheStatisticsLoadOps::infer_region_from_handle(&snapshot.main.handle)
                    .or_else(|| {
                        ReplayCacheStatisticsLoadOps::infer_region_from_handle(
                            &snapshot.ally.handle,
                        )
                    })
                    .unwrap_or_else(|| "Unknown".to_string())
                    .to_ascii_uppercase();
            if !matches!(region.as_str(), "NA" | "EU" | "KR" | "CN" | "PTR") {
                return false;
            }
            if query
                .region_exclusions()
                .iter()
                .any(|excluded| excluded.trim().eq_ignore_ascii_case(&region))
            {
                return false;
            }
        }
        dictionary
            .canonicalize_coop_map_id(&snapshot.map_name)
            .is_some()
    }
}
