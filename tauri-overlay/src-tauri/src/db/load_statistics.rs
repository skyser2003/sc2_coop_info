use super::array_json::ReplayCacheArrayJson;
use super::core::*;
use rusqlite::{params, params_from_iter, types::Value as SqlValue};
use s2coop_analyzer::cache_overall_stats_generator::{
    CachePlayerStatsSeries, CacheUnitStats, ReplayMessage,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::replay_analysis::{ReplayAnalysis, ReplayAnalysisOps};
use crate::shared_types::LocalizedLabels;
use crate::stats_aggregation::{
    StatsAggregateAnalysisPayload, StatsAggregateDifficultyDataRow,
    StatsAggregateFastestMapDetails, StatsAggregateMapDataRow, StatsAggregatePlayerDataRow,
    StatsAggregateRegionDataRow, StatsAggregationOps, StatsAmonUnitSnapshot,
    StatsCommanderAggregate, StatsCommanderDataInput, StatsCommanderPlayerRecord,
    StatsCommanderTotals, StatsMapAggregate, StatsPlayerAggregate, StatsPlayerRecord,
    StatsPlayerSnapshot, StatsPlayerUnitSnapshot, StatsRegionAggregate, StatsReplaySnapshot,
    StatsResultSummary, StatsWinLossAggregate,
};

struct ReplayCacheStatisticsLoadOps;

impl ReplayCacheStatisticsLoadOps {
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
        let trimmed = commander.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "abathur" => "Abathur",
            "alarak" => "Alarak",
            "artanis" => "Artanis",
            "dehaka" => "Dehaka",
            "fenix" => "Fenix",
            "han & horner" | "han and horner" | "hanhorner" => "Han & Horner",
            "karax" => "Karax",
            "kerrigan" => "Kerrigan",
            "mengsk" | "arcturus mengsk" => "Mengsk",
            "nova" => "Nova",
            "raynor" => "Raynor",
            "stukov" => "Stukov",
            "swann" => "Swann",
            "tychus" => "Tychus",
            "vorazun" => "Vorazun",
            "zagara" => "Zagara",
            "zeratul" => "Zeratul",
            "stetmann" => "Stetmann",
            _ => trimmed,
        }
        .to_string()
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

#[derive(Deserialize)]
struct StatsPlayerUnitJsonRow(i64, String, String, Option<i64>, String, Option<i64>, i64);

impl StatsPlayerUnitJsonRow {
    fn into_snapshot(self) -> StatsPlayerUnitSnapshot {
        let Self(pid, unit_name, created_kind, created_count, lost_kind, lost_count, kills) = self;
        StatsPlayerUnitSnapshot {
            pid: ReplayCacheEntryRecord::i64_to_u32(pid) as u8,
            unit_name,
            created_hidden: created_kind == "hidden",
            created_count: created_count.unwrap_or_default(),
            lost_hidden: lost_kind == "hidden",
            lost_count: lost_count.unwrap_or_default(),
            kills: ReplayCacheEntryRecord::i64_to_u64(kills),
        }
    }
}

#[derive(Deserialize)]
struct StatsAmonUnitJsonRow(String, String, Option<i64>, String, Option<i64>, i64);

impl StatsAmonUnitJsonRow {
    fn into_snapshot(self) -> StatsAmonUnitSnapshot {
        let Self(unit_name, created_kind, created_count, lost_kind, lost_count, kills) = self;
        StatsAmonUnitSnapshot {
            unit_name,
            created_hidden: created_kind == "hidden",
            created_count: created_count.unwrap_or_default(),
            lost_hidden: lost_kind == "hidden",
            lost_count: lost_count.unwrap_or_default(),
            kills,
        }
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
        let mut snapshots = self.load_stats_replay_snapshots(query, main_names, main_handles)?;
        snapshots.retain(|snapshot| {
            Self::stats_snapshot_matches_query(snapshot, query, main_handles, dictionary)
        });
        let include_detailed = if query.scope() == ReplayCacheReadScope::DetailedOnly {
            true
        } else {
            snapshots.iter().any(|snapshot| snapshot.detailed_analysis)
        };
        if include_detailed {
            snapshots.retain(|snapshot| snapshot.detailed_analysis);
        }
        let payload = self.statistics_payload_from_snapshots(
            snapshots,
            include_detailed,
            main_names,
            main_handles,
            dictionary,
        )?;
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
                COALESCE(p2.mastery_values, '[]') AS p2_masteries,
                COALESCE((
                    SELECT json_group_array(
                        json_array(
                            u.pid,
                            u.unit_name,
                            u.created_kind,
                            u.created_count,
                            u.lost_kind,
                            u.lost_count,
                            u.kills
                        )
                    )
                    FROM replay_cache_player_units u
                    WHERE u.replay_id = e.id
                ), '[]') AS player_unit_rows,
                COALESCE((
                    SELECT json_group_array(
                        json_array(
                            a.unit_name,
                            a.created_kind,
                            a.created_count,
                            a.lost_kind,
                            a.lost_count,
                            a.kills
                        )
                    )
                    FROM replay_cache_amon_units a
                    WHERE a.replay_id = e.id
                ), '[]') AS amon_unit_rows
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
            let player_units = self.sqlite_row(row.get::<_, String>(32))?;
            let amon_units = self.sqlite_row(row.get::<_, String>(33))?;
            snapshots.push(StatsReplaySnapshot {
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
                player_units: Self::stats_player_units_from_json(&player_units)?,
                amon_units: Self::stats_amon_units_from_json(&amon_units)?,
            });
        }
        Ok(snapshots)
    }

    fn stats_player_units_from_json(
        text: &str,
    ) -> Result<Vec<StatsPlayerUnitSnapshot>, ReplayCacheDbError> {
        let rows = serde_json::from_str::<Vec<StatsPlayerUnitJsonRow>>(text).map_err(|source| {
            ReplayCacheDbError::JsonArray {
                context: "stats player unit rows",
                source,
            }
        })?;
        Ok(rows
            .into_iter()
            .map(StatsPlayerUnitJsonRow::into_snapshot)
            .collect())
    }

    fn stats_amon_units_from_json(
        text: &str,
    ) -> Result<Vec<StatsAmonUnitSnapshot>, ReplayCacheDbError> {
        let rows = serde_json::from_str::<Vec<StatsAmonUnitJsonRow>>(text).map_err(|source| {
            ReplayCacheDbError::JsonArray {
                context: "stats amon unit rows",
                source,
            }
        })?;
        Ok(rows
            .into_iter()
            .map(StatsAmonUnitJsonRow::into_snapshot)
            .collect())
    }

    fn stats_player_from_row(
        row: &rusqlite::Row<'_>,
        offset: usize,
    ) -> Result<StatsPlayerSnapshot, rusqlite::Error> {
        let mastery_values = row.get::<_, String>(offset + 9)?;
        Ok(StatsPlayerSnapshot {
            pid: ReplayCacheEntryRecord::i64_to_u32(row.get::<_, i64>(offset)?) as u8,
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

    fn statistics_payload_from_snapshots(
        &self,
        snapshots: Vec<StatsReplaySnapshot>,
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<ReplayCacheStatisticsPayload, ReplayCacheDbError> {
        let mut map_values = BTreeMap::<String, StatsMapAggregate>::new();
        let mut main_commander = BTreeMap::<String, StatsCommanderAggregate>::new();
        let mut ally_commander = BTreeMap::<String, StatsCommanderAggregate>::new();
        let mut region_values = BTreeMap::<String, StatsRegionAggregate>::new();
        let mut difficulty_values = BTreeMap::<String, StatsWinLossAggregate>::new();
        let mut player_values = BTreeMap::<String, StatsPlayerAggregate>::new();
        let mut valid_snapshots = Vec::new();
        let mut main_players = BTreeSet::new();
        let mut main_player_handles = BTreeSet::new();

        let mut sum_main = StatsCommanderTotals::default();
        let mut sum_ally = StatsCommanderTotals::default();

        let has_known_main_identity = !main_names.is_empty() || !main_handles.is_empty();
        let has_known_main_handles = !main_handles.is_empty();
        for snapshot in snapshots {
            let Some(map_id) = dictionary.canonicalize_coop_map_id(&snapshot.map_name) else {
                continue;
            };
            let Some(replay_is_victory) =
                ReplayCacheStatisticsLoadOps::result_is_victory(&snapshot.result)
            else {
                continue;
            };
            let main_name = ReplayCacheStatisticsLoadOps::sanitize_replay_text(&snapshot.main.name);
            let ally_name = ReplayCacheStatisticsLoadOps::sanitize_replay_text(&snapshot.ally.name);
            let main_commander_text =
                ReplayCacheStatisticsLoadOps::sanitize_replay_text(&snapshot.main.commander);
            let ally_commander_text =
                ReplayCacheStatisticsLoadOps::sanitize_replay_text(&snapshot.ally.commander);
            let main_commander_name =
                ReplayCacheStatisticsLoadOps::normalized_commander_name(&main_commander_text);
            let ally_commander_name =
                ReplayCacheStatisticsLoadOps::normalized_commander_name(&ally_commander_text);
            if main_commander_name.is_empty() || ally_commander_name.is_empty() {
                continue;
            }

            let p1_is_main_identity = Self::stats_player_is_main(
                &snapshot.main,
                main_names,
                main_handles,
                has_known_main_identity,
                true,
            );
            let p2_is_main_identity = Self::stats_player_is_main(
                &snapshot.ally,
                main_names,
                main_handles,
                has_known_main_identity,
                false,
            );
            if p1_is_main_identity {
                if !snapshot.main.name.trim().is_empty() {
                    main_players.insert(snapshot.main.name.trim().to_string());
                }
                if !snapshot.main.handle.trim().is_empty() {
                    main_player_handles.insert(snapshot.main.handle.trim().to_string());
                }
            }
            if p2_is_main_identity {
                if !snapshot.ally.name.trim().is_empty() {
                    main_players.insert(snapshot.ally.name.trim().to_string());
                }
                if !snapshot.ally.handle.trim().is_empty() {
                    main_player_handles.insert(snapshot.ally.handle.trim().to_string());
                }
            }

            let main_kill_fraction = ReplayCacheStatisticsLoadOps::kill_fraction(
                snapshot.main.kills,
                snapshot.ally.kills,
            );
            let ally_kill_fraction = 1.0 - main_kill_fraction;
            let include_prestige =
                StatsAggregationOps::should_count_prestige(snapshot.date_seconds);

            let map_bonus_total = if replay_is_victory && snapshot.detailed_analysis {
                dictionary
                    .coop_map_id_to_english(&map_id)
                    .as_deref()
                    .and_then(|name| {
                        crate::replay_analysis::ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(name, dictionary)
                    })
            } else {
                None
            };
            map_values
                .entry(map_id.clone())
                .or_default()
                .record_snapshot(&snapshot, replay_is_victory, map_bonus_total, true);

            let (main_is_region_main, ally_is_region_main) =
                Self::stats_region_main_flags(&snapshot, has_known_main_handles, main_handles);
            let region = Self::stats_region_for_snapshot(
                &snapshot,
                has_known_main_handles,
                main_is_region_main,
                ally_is_region_main,
            );
            let region_entry = region_values.entry(region).or_default();
            region_entry.record_result(replay_is_victory);
            if main_is_region_main {
                region_entry.record_player(
                    snapshot.main.mastery_level,
                    snapshot.main.commander_level,
                    &main_commander_text,
                    &main_commander_name,
                    snapshot.main.prestige,
                );
            }
            if ally_is_region_main {
                region_entry.record_player(
                    snapshot.ally.mastery_level,
                    snapshot.ally.commander_level,
                    &ally_commander_text,
                    &ally_commander_name,
                    snapshot.ally.prestige,
                );
            }

            let difficulty = Self::stats_difficulty_label(&snapshot);
            if !difficulty.contains('/') {
                difficulty_values
                    .entry(difficulty)
                    .or_default()
                    .record_result(replay_is_victory);
            }

            let main_commander_record = StatsCommanderPlayerRecord::new(
                replay_is_victory,
                snapshot.detailed_analysis,
                snapshot.main.apm,
                main_kill_fraction,
                snapshot.main.prestige,
                &snapshot.main.masteries,
                include_prestige,
            );
            let ally_commander_record = StatsCommanderPlayerRecord::new(
                replay_is_victory,
                snapshot.detailed_analysis,
                snapshot.ally.apm,
                ally_kill_fraction,
                snapshot.ally.prestige,
                &snapshot.ally.masteries,
                include_prestige,
            );
            main_commander
                .entry(main_commander_name.clone())
                .or_default()
                .record_player(main_commander_record);
            ally_commander
                .entry(ally_commander_name.clone())
                .or_default()
                .record_player(ally_commander_record);
            sum_main.record_player(main_commander_record);
            sum_ally.record_player(ally_commander_record);

            if !main_name.is_empty() {
                let main_handle =
                    ReplayCacheStatisticsLoadOps::sanitize_replay_text(&snapshot.main.handle);
                player_values
                    .entry(main_name)
                    .or_default()
                    .record_replay(StatsPlayerRecord::new(
                        &snapshot.main.name,
                        &main_handle,
                        &snapshot.main.commander,
                        replay_is_victory,
                        snapshot.main.apm,
                        main_kill_fraction,
                        snapshot.date_seconds,
                    ));
            }
            if !ally_name.is_empty() {
                let ally_handle =
                    ReplayCacheStatisticsLoadOps::sanitize_replay_text(&snapshot.ally.handle);
                player_values
                    .entry(ally_name)
                    .or_default()
                    .record_replay(StatsPlayerRecord::new(
                        &snapshot.ally.name,
                        &ally_handle,
                        &snapshot.ally.commander,
                        replay_is_victory,
                        snapshot.ally.apm,
                        ally_kill_fraction,
                        snapshot.date_seconds,
                    ));
            }

            valid_snapshots.push(snapshot);
        }

        let total_games = valid_snapshots.len() as u64;
        let detailed_parsed_count = valid_snapshots
            .iter()
            .filter(|snapshot| snapshot.detailed_analysis)
            .count() as u64;
        let mut map_data = Map::new();
        for (map_id, aggregate) in map_values {
            let map_name = dictionary
                .coop_map_id_to_english(&map_id)
                .unwrap_or_else(|| map_id.clone());
            let games = aggregate.games();
            let fastest = aggregate.fastest_or_default();
            let fastest_players =
                Self::fastest_players_value(&fastest, main_names, main_handles, dictionary);
            map_data.insert(
                map_name,
                ReplayCacheStatisticsLoadOps::to_value(&StatsAggregateMapDataRow::new(
                    map_id,
                    aggregate.average_victory_time(),
                    StatsAggregationOps::ratio(games, total_games),
                    StatsResultSummary::new(
                        aggregate.wins(),
                        aggregate.losses(),
                        StatsAggregationOps::ratio(aggregate.wins(), games),
                    ),
                    aggregate.bonus_rate(),
                    aggregate.detailed_count(),
                    StatsAggregateFastestMapDetails::new(
                        fastest.length_realtime,
                        fastest.file,
                        fastest.date_seconds,
                        ReplayCacheStatisticsLoadOps::sanitize_replay_text(&fastest.difficulty),
                        fastest_players,
                        ReplayCacheStatisticsLoadOps::sanitize_replay_text(&fastest.enemy_race),
                    ),
                )),
            );
        }

        let commander_data = StatsAggregationOps::build_commander_data(
            StatsCommanderDataInput::new(&main_commander, total_games, &sum_main, None),
        );
        let main_frequency = main_commander
            .iter()
            .map(|(commander, aggregate)| {
                let games = aggregate.games();
                (
                    commander.clone(),
                    StatsAggregationOps::ratio(games, sum_main.games()),
                )
            })
            .collect::<HashMap<_, _>>();
        let ally_commander_data =
            StatsAggregationOps::build_commander_data(StatsCommanderDataInput::new(
                &ally_commander,
                total_games,
                &sum_ally,
                Some(&main_frequency),
            ));

        let difficulty_data = difficulty_values
            .into_iter()
            .map(|(difficulty, aggregate)| {
                let games = aggregate.games();
                (
                    difficulty,
                    ReplayCacheStatisticsLoadOps::to_value(&StatsAggregateDifficultyDataRow::new(
                        StatsResultSummary::new(
                            aggregate.wins(),
                            aggregate.losses(),
                            StatsAggregationOps::ratio(aggregate.wins(), games),
                        ),
                    )),
                )
            })
            .collect::<Map<String, Value>>();

        let region_data = region_values
            .into_iter()
            .map(|(region, aggregate)| {
                let games = aggregate.games();
                let prestiges = aggregate
                    .prestiges()
                    .iter()
                    .map(|(commander, prestige)| (commander.clone(), Value::from(*prestige)))
                    .collect::<Map<String, Value>>();
                (
                    region,
                    ReplayCacheStatisticsLoadOps::to_value(&StatsAggregateRegionDataRow::new(
                        StatsAggregationOps::ratio(games, total_games),
                        StatsResultSummary::new(
                            aggregate.wins(),
                            aggregate.losses(),
                            StatsAggregationOps::ratio(aggregate.wins(), games),
                        ),
                        aggregate.max_asc(),
                        prestiges,
                        aggregate.max_com().iter().cloned().collect(),
                    )),
                )
            })
            .collect::<Map<String, Value>>();

        let player_data = player_values
            .into_iter()
            .map(|(name, aggregate)| {
                let games = aggregate.games();
                let (commander, frequency) = aggregate.dominant_commander();
                (
                    ReplayCacheStatisticsLoadOps::sanitize_replay_text(&name),
                    ReplayCacheStatisticsLoadOps::to_value(&StatsAggregatePlayerDataRow::new(
                        StatsResultSummary::new(
                            aggregate.wins(),
                            aggregate.losses(),
                            StatsAggregationOps::ratio(aggregate.wins(), games),
                        ),
                        StatsAggregationOps::median_f64(aggregate.kill_fractions()),
                        StatsAggregationOps::median_u64(aggregate.apm_values()),
                        frequency,
                        aggregate.last_seen(),
                        ReplayCacheStatisticsLoadOps::sanitize_replay_text(&commander),
                    )),
                )
            })
            .collect::<Map<String, Value>>();

        let unit_data = if include_detailed {
            Self::load_statistics_unit_data(&valid_snapshots, main_handles, dictionary)
        } else {
            Value::Null
        };

        let analysis = ReplayCacheStatisticsLoadOps::to_value(
            &StatsAggregateAnalysisPayload::new_ready_map_data(
                map_data,
                commander_data,
                ally_commander_data,
                difficulty_data,
                region_data,
                player_data,
                unit_data,
            ),
        );
        let prestige_names = dictionary
            .prestige_names_json
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    LocalizedLabels {
                        en: value.en.clone(),
                        ko: value.ko.clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        Ok(ReplayCacheStatisticsPayload::new(
            analysis,
            prestige_names,
            total_games,
            detailed_parsed_count,
            total_games,
            main_players.into_iter().collect(),
            main_player_handles.into_iter().collect(),
        ))
    }

    fn stats_player_is_main(
        player: &StatsPlayerSnapshot,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        has_known_identity: bool,
        fallback_main: bool,
    ) -> bool {
        let handle_match = !main_handles.is_empty()
            && main_handles.contains(&ReplayCacheStatisticsLoadOps::normalized_handle_key(
                &player.handle,
            ));
        let name_match =
            !main_names.is_empty() && main_names.contains(&player.name.trim().to_ascii_lowercase());
        handle_match || name_match || (!has_known_identity && fallback_main)
    }

    fn stats_region_for_snapshot(
        snapshot: &StatsReplaySnapshot,
        has_known_main_handles: bool,
        p1_is_main: bool,
        p2_is_main: bool,
    ) -> String {
        if has_known_main_handles && p1_is_main {
            ReplayCacheStatisticsLoadOps::infer_region_from_handle(&snapshot.main.handle)
        } else if has_known_main_handles && p2_is_main {
            ReplayCacheStatisticsLoadOps::infer_region_from_handle(&snapshot.ally.handle)
        } else {
            ReplayCacheStatisticsLoadOps::infer_region_from_handle(&snapshot.main.handle).or_else(
                || ReplayCacheStatisticsLoadOps::infer_region_from_handle(&snapshot.ally.handle),
            )
        }
        .unwrap_or_else(|| "Unknown".to_string())
    }

    fn stats_region_main_flags(
        snapshot: &StatsReplaySnapshot,
        has_known_main_handles: bool,
        main_handles: &HashSet<String>,
    ) -> (bool, bool) {
        if !has_known_main_handles {
            return (true, false);
        }
        let mut main_is_main =
            ReplayAnalysis::is_main_player_by_handle(&snapshot.main.handle, main_handles);
        let ally_is_main =
            ReplayAnalysis::is_main_player_by_handle(&snapshot.ally.handle, main_handles);
        if !main_is_main && !ally_is_main {
            main_is_main = true;
        }
        (main_is_main, ally_is_main)
    }

    fn stats_difficulty_label(snapshot: &StatsReplaySnapshot) -> String {
        if snapshot.brutal_plus > 0 {
            return format!("B+{}", snapshot.brutal_plus.min(6));
        }
        let difficulty = snapshot.difficulty.trim();
        if difficulty.eq_ignore_ascii_case("Brutal+") {
            "Brutal+".to_string()
        } else if difficulty.is_empty() {
            "Unknown".to_string()
        } else {
            difficulty.to_string()
        }
    }

    fn fastest_player_value(player: &StatsPlayerSnapshot, dictionary: &Sc2DictionaryData) -> Value {
        #[derive(Serialize)]
        struct FastestPlayer {
            name: String,
            handle: String,
            commander: String,
            apm: u64,
            mastery_level: u64,
            masteries: Vec<u64>,
            prestige: u64,
            prestige_name: String,
        }

        let commander = ReplayCacheStatisticsLoadOps::sanitize_replay_text(
            &ReplayCacheStatisticsLoadOps::normalized_commander_name(&player.commander),
        );
        let prestige_name = dictionary
            .prestige_name(&commander, player.prestige)
            .map(str::to_string)
            .unwrap_or_else(|| format!("P{}", player.prestige));
        ReplayCacheStatisticsLoadOps::to_value(&FastestPlayer {
            name: ReplayCacheStatisticsLoadOps::sanitize_replay_text(&player.name),
            handle: player.handle.clone(),
            commander,
            apm: player.apm,
            mastery_level: player.mastery_level,
            masteries: StatsAggregationOps::normalize_mastery_values(&player.masteries),
            prestige: player.prestige,
            prestige_name,
        })
    }

    fn fastest_players_value(
        snapshot: &StatsReplaySnapshot,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<Value> {
        let main_value = Self::fastest_player_value(&snapshot.main, dictionary);
        let ally_value = Self::fastest_player_value(&snapshot.ally, dictionary);
        let main_is_main = ReplayAnalysis::is_main_player_identity(
            &snapshot.main.name,
            &snapshot.main.handle,
            main_names,
            main_handles,
        );
        let ally_is_main = ReplayAnalysis::is_main_player_identity(
            &snapshot.ally.name,
            &snapshot.ally.handle,
            main_names,
            main_handles,
        );
        if ally_is_main && !main_is_main {
            vec![ally_value, main_value]
        } else {
            vec![main_value, ally_value]
        }
    }

    fn load_statistics_unit_data(
        snapshots: &[StatsReplaySnapshot],
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Value {
        let replays = snapshots
            .iter()
            .map(StatsReplaySnapshot::to_replay_info)
            .collect::<Vec<_>>();
        ReplayAnalysisOps::build_unit_data_from_replays_with_dictionary(
            &replays,
            main_handles,
            dictionary,
        )
    }

    fn stats_where_clause(query: &ReplayCacheStatsQuery) -> (String, Vec<SqlValue>) {
        Self::stats_where_clause_with_orientation_filters(query, true)
    }

    fn stats_prefilter_where_clause(query: &ReplayCacheStatsQuery) -> (String, Vec<SqlValue>) {
        Self::stats_where_clause_with_orientation_filters(query, false)
    }

    fn stats_where_clause_with_orientation_filters(
        query: &ReplayCacheStatsQuery,
        include_orientation_filters: bool,
    ) -> (String, Vec<SqlValue>) {
        let mut clauses = Vec::new();
        let mut bind_values = Vec::new();

        if query.scope() == ReplayCacheReadScope::DetailedOnly {
            clauses.push("e.detailed_analysis = 1".to_string());
        }

        if query.restrict_to_current_replay_files() && query.current_replay_files().is_empty() {
            clauses.push("0 = 1".to_string());
        } else if !query.current_replay_files().is_empty() {
            let placeholders = std::iter::repeat_n("?", query.current_replay_files().len())
                .collect::<Vec<_>>()
                .join(", ");
            clauses.push(format!("e.file IN ({placeholders})"));
            bind_values.extend(
                query
                    .current_replay_files()
                    .iter()
                    .map(|file| SqlValue::Text(file.to_string())),
            );
        }

        if !query.include_normal_games() && !query.include_mutations() {
            clauses.push("0 = 1".to_string());
        } else if !query.include_normal_games() {
            clauses.push("e.extension = 1".to_string());
        } else if !query.include_mutations() {
            clauses.push("e.extension = 0".to_string());
        }

        if !query.include_wins() && !query.include_losses() {
            clauses.push("0 = 1".to_string());
        } else if !query.include_wins() {
            clauses.push(
                "LOWER(TRIM(e.result)) IN ('defeat', 'loss', 'lose', '0', 'false')".to_string(),
            );
        } else if !query.include_losses() {
            clauses.push("LOWER(TRIM(e.result)) IN ('victory', 'win', '1', 'true')".to_string());
        } else {
            clauses.push(
                "
                LOWER(TRIM(e.result)) IN (
                    'victory', 'win', '1', 'true',
                    'defeat', 'loss', 'lose', '0', 'false'
                )
                "
                .to_string(),
            );
        }

        let length_expression = Self::stats_length_expression();
        if query.min_length_seconds() > 0 {
            clauses.push(format!("{length_expression} >= ?"));
            bind_values.push(SqlValue::Real(query.min_length_seconds() as f64));
        }
        if query.max_length_seconds() > 0 {
            clauses.push(format!("{length_expression} <= ?"));
            bind_values.push(SqlValue::Real(query.max_length_seconds() as f64));
        }

        if let Some(min_date_seconds) = query.min_date_seconds() {
            clauses.push("e.date_seconds > ?".to_string());
            bind_values.push(SqlValue::Integer(ReplayCacheEntryRecord::u64_to_i64(
                min_date_seconds,
            )));
        }
        if let Some(max_date_seconds) = query.max_date_seconds() {
            clauses.push("e.date_seconds < ?".to_string());
            bind_values.push(SqlValue::Integer(ReplayCacheEntryRecord::u64_to_i64(
                max_date_seconds,
            )));
        }

        if let Some(player_pattern) = Self::stats_player_like_pattern(query.player_filter()) {
            clauses.push(
                "
                e.id IN (
                    SELECT p.replay_id
                    FROM replay_cache_players p
                    WHERE p.player_name LIKE ? ESCAPE '\\' COLLATE NOCASE
                )
                "
                .to_string(),
            );
            bind_values.push(SqlValue::Text(player_pattern));
        }

        if include_orientation_filters {
            Self::push_stats_commander_level_clauses(query, &mut clauses);
            Self::push_stats_mastery_clauses(query, &mut clauses);
            Self::push_stats_multibox_clause(query, &mut clauses, &mut bind_values);
        }

        let difficulty_expression = Self::stats_difficulty_expression();
        for exclusion in query.difficulty_exclusions() {
            if let Some(level) = exclusion.brutal_plus_level() {
                clauses.push("e.brutal_plus <> ?".to_string());
                bind_values.push(SqlValue::Integer(level));
                continue;
            }

            if let Some(label) = exclusion.difficulty_label() {
                if exclusion.is_brutal_label() {
                    clauses.push(format!(
                        "(e.brutal_plus > 0 OR instr({difficulty_expression}, ?) = 0)"
                    ));
                } else {
                    clauses.push(format!("instr({difficulty_expression}, ?) = 0"));
                }
                bind_values.push(SqlValue::Text(label.to_string()));
            }
        }

        if include_orientation_filters {
            Self::push_stats_region_clause(query, &mut clauses, &mut bind_values);
        }

        let where_sql = if clauses.is_empty() {
            "1 = 1".to_string()
        } else {
            clauses.join(" AND ")
        };
        (where_sql, bind_values)
    }

    fn push_stats_region_clause(
        query: &ReplayCacheStatsQuery,
        clauses: &mut Vec<String>,
        bind_values: &mut Vec<SqlValue>,
    ) {
        if query.region_exclusions().is_empty() {
            return;
        }

        clauses.push(
            "
            UPPER(TRIM(COALESCE(e.region, ''))) IN ('NA', 'EU', 'KR', 'CN', 'PTR')
            "
            .to_string(),
        );

        let placeholders = std::iter::repeat_n("?", query.region_exclusions().len())
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(format!(
            "UPPER(TRIM(COALESCE(e.region, ''))) NOT IN ({placeholders})"
        ));
        bind_values.extend(
            query
                .region_exclusions()
                .iter()
                .map(|region| SqlValue::Text(region.to_ascii_uppercase())),
        );
    }

    fn push_stats_commander_level_clauses(
        query: &ReplayCacheStatsQuery,
        clauses: &mut Vec<String>,
    ) {
        Self::push_stats_single_commander_level_clause(
            query.include_sub_15(),
            query.include_over_15(),
            clauses,
        );
        Self::push_stats_single_commander_level_clause(
            query.include_ally_sub_15(),
            query.include_ally_over_15(),
            clauses,
        );
    }

    fn push_stats_single_commander_level_clause(
        include_sub_15: bool,
        include_over_15: bool,
        clauses: &mut Vec<String>,
    ) {
        match (include_sub_15, include_over_15) {
            (false, false) => clauses.push("0 = 1".to_string()),
            (false, true) => clauses.push(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND COALESCE(p.commander_level, 0) >= 15
                )
                "
                .to_string(),
            ),
            (true, false) => clauses.push(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND COALESCE(p.commander_level, 0) < 15
                )
                "
                .to_string(),
            ),
            (true, true) => {}
        }
    }

    fn push_stats_mastery_clauses(query: &ReplayCacheStatsQuery, clauses: &mut Vec<String>) {
        Self::push_stats_single_mastery_clause(
            query.include_main_normal_mastery(),
            query.include_main_abnormal_mastery(),
            clauses,
        );
        Self::push_stats_single_mastery_clause(
            query.include_ally_normal_mastery(),
            query.include_ally_abnormal_mastery(),
            clauses,
        );
    }

    fn push_stats_single_mastery_clause(
        include_normal_mastery: bool,
        include_abnormal_mastery: bool,
        clauses: &mut Vec<String>,
    ) {
        match (include_normal_mastery, include_abnormal_mastery) {
            (false, false) => clauses.push("0 = 1".to_string()),
            (false, true) => clauses.push(format!(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND {mastery_sum} > 90
                )
                ",
                mastery_sum = Self::stats_mastery_sum_expression("p.mastery_values")
            )),
            (true, false) => clauses.push(format!(
                "
                EXISTS (
                    SELECT 1
                    FROM replay_cache_players p
                    WHERE p.replay_id = e.id
                        AND {mastery_sum} <= 90
                )
                ",
                mastery_sum = Self::stats_mastery_sum_expression("p.mastery_values")
            )),
            (true, true) => {}
        }
    }

    fn push_stats_multibox_clause(
        query: &ReplayCacheStatsQuery,
        clauses: &mut Vec<String>,
        bind_values: &mut Vec<SqlValue>,
    ) {
        if query.include_both_main() || query.main_handle_keys().is_empty() {
            return;
        }

        let placeholders = std::iter::repeat_n("?", query.main_handle_keys().len())
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(format!(
            "
            NOT EXISTS (
                SELECT 1
                FROM replay_cache_players p1
                INNER JOIN replay_cache_players p2
                    ON p2.replay_id = p1.replay_id
                    AND p2.pid = 2
                WHERE p1.replay_id = e.id
                    AND p1.pid = 1
                    AND LOWER(TRIM(p1.player_handle)) IN ({placeholders})
                    AND LOWER(TRIM(p2.player_handle)) IN ({placeholders})
            )
            "
        ));
        bind_values.extend(
            query
                .main_handle_keys()
                .iter()
                .map(|handle| SqlValue::Text(handle.to_string())),
        );
        bind_values.extend(
            query
                .main_handle_keys()
                .iter()
                .map(|handle| SqlValue::Text(handle.to_string())),
        );
    }

    fn stats_mastery_sum_expression(column: &str) -> String {
        (0..6)
            .map(|index| format!("COALESCE(json_extract({column}, '$[{index}]'), 0)"))
            .collect::<Vec<_>>()
            .join(" + ")
    }

    fn stats_length_expression() -> &'static str {
        "
        CASE e.length_realtime_kind
            WHEN 'float' THEN COALESCE(e.length_realtime_float, 0.0)
            ELSE COALESCE(e.length_realtime_int, 0)
        END
        "
    }

    fn stats_difficulty_expression() -> &'static str {
        "
        CASE
            WHEN TRIM(e.ext_difficulty) <> '' THEN e.ext_difficulty
            WHEN TRIM(e.difficulty_p2) <> '' THEN e.difficulty_p2
            WHEN TRIM(e.difficulty_p1) <> '' THEN e.difficulty_p1
            ELSE 'Unknown'
        END
        "
    }

    fn stats_player_like_pattern(player_filter: &str) -> Option<String> {
        let trimmed = player_filter.trim();
        if trimmed.is_empty() || !trimmed.is_ascii() {
            return None;
        }

        let mut pattern = String::with_capacity(trimmed.len());
        for ch in trimmed.chars() {
            match ch {
                '*' => pattern.push('%'),
                '?' => pattern.push('_'),
                '%' | '_' | '\\' => {
                    pattern.push('\\');
                    pattern.push(ch);
                }
                _ => pattern.push(ch),
            }
        }
        Some(pattern)
    }

    pub fn load_messages(&self, replay_id: i64) -> Result<Vec<ReplayMessage>, ReplayCacheDbError> {
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

    pub fn load_amon_units(
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

    pub fn load_player_stats(
        &self,
        replay_id: i64,
    ) -> Result<BTreeMap<u8, CachePlayerStatsSeries>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT stats.pid, COALESCE(player.player_name, ''), stats.supply_values, stats.mining_values,
                    stats.army_values, stats.killed_values
                FROM replay_cache_player_stat_series stats
                LEFT JOIN replay_cache_players player
                    ON player.replay_id = stats.replay_id AND player.pid = stats.pid
                WHERE stats.replay_id = ?1
                ORDER BY stats.pid ASC
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
