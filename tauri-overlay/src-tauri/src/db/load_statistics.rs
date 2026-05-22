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

use crate::replay_analysis::{ReplayAnalysis, ReplayAnalysisOps};
use crate::shared_types::LocalizedLabels;
use crate::{CommanderUnitRollup, TauriOverlayOps, UnitStatsRollup};

const PRESTIGE_TRACKING_START_YMD: u32 = 20200726;
const MASTERY_DISTRIBUTION_RATIO_SCALE: u64 = 100_000;

#[derive(Clone, Debug, Default)]
struct StatsPlayerSnapshot {
    pid: u8,
    name: String,
    handle: String,
    commander: String,
    apm: u64,
    kills: u64,
    commander_level: u64,
    mastery_level: u64,
    prestige: u64,
    masteries: Vec<u64>,
}

#[derive(Clone, Debug)]
struct StatsPlayerUnitSnapshot {
    pid: u8,
    unit_name: String,
    created_hidden: bool,
    created_count: i64,
    lost_hidden: bool,
    lost_count: i64,
    kills: u64,
}

#[derive(Clone, Debug)]
struct StatsAmonUnitSnapshot {
    unit_name: String,
    created_hidden: bool,
    created_count: i64,
    lost_hidden: bool,
    lost_count: i64,
    kills: i64,
}

#[derive(Clone, Debug)]
struct StatsReplaySnapshot {
    file: String,
    map_name: String,
    result: String,
    difficulty: String,
    enemy_race: String,
    date_seconds: u64,
    detailed_analysis: bool,
    brutal_plus: u64,
    extension: bool,
    length_realtime: f64,
    bonus_completed: u64,
    main: StatsPlayerSnapshot,
    ally: StatsPlayerSnapshot,
    player_units: Vec<StatsPlayerUnitSnapshot>,
    amon_units: Vec<StatsAmonUnitSnapshot>,
}

#[derive(Default)]
struct StatsWinLossAggregate {
    wins: u64,
    losses: u64,
}

#[derive(Default)]
struct StatsMapAggregate {
    wins: u64,
    losses: u64,
    victory_length_sum: f64,
    victory_games: u64,
    bonus_fraction_sum: f64,
    bonus_games: u64,
    detailed_count: u64,
    fastest: Option<StatsReplaySnapshot>,
}

type StatsMasteryDistributionCounts = [BTreeMap<u64, u64>; 3];
type StatsMasteryDistributionByPrestigeCounts = [StatsMasteryDistributionCounts; 4];

#[derive(Default)]
struct StatsCommanderAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    mastery_counts: [f64; 6],
    mastery_distribution_counts: StatsMasteryDistributionCounts,
    mastery_distribution_by_prestige_counts: StatsMasteryDistributionByPrestigeCounts,
    mastery_by_prestige_counts: [[f64; 6]; 4],
    prestige_counts: [u64; 4],
    detailed_count: u64,
}

#[derive(Default)]
struct StatsCommanderTotals {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    mastery_counts: [f64; 6],
    mastery_distribution_counts: StatsMasteryDistributionCounts,
    mastery_distribution_by_prestige_counts: StatsMasteryDistributionByPrestigeCounts,
    mastery_by_prestige_counts: [[f64; 6]; 4],
    prestige_counts: [u64; 4],
}

impl StatsCommanderTotals {
    fn record_result(&mut self, replay_is_victory: bool) {
        if replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
    }

    fn record_player(
        &mut self,
        player: &StatsPlayerSnapshot,
        detailed_analysis: bool,
        kill_fraction: f64,
        include_prestige: bool,
    ) {
        self.apm_values.push(player.apm);
        if detailed_analysis {
            self.kill_fractions.push(kill_fraction);
        }
        let normalized_masteries = normalize_mastery_vector(&player.masteries);
        record_mastery_counts(&mut self.mastery_counts, &normalized_masteries);
        record_mastery_distribution(&mut self.mastery_distribution_counts, &player.masteries);
        record_mastery_distribution_by_prestige(
            &mut self.mastery_distribution_by_prestige_counts,
            player.prestige,
            &player.masteries,
        );
        record_mastery_by_prestige(
            &mut self.mastery_by_prestige_counts,
            player.prestige,
            &normalized_masteries,
        );
        if include_prestige {
            record_prestige_count(&mut self.prestige_counts, player.prestige);
        }
    }

    fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }
}

struct StatsCommanderDataInput<'a> {
    aggregates: &'a BTreeMap<String, StatsCommanderAggregate>,
    total_games: u64,
    totals: &'a StatsCommanderTotals,
    main_frequency: Option<&'a HashMap<String, f64>>,
}

struct StatsUnitRollupInput<'a> {
    commander: &'a str,
    unit_name: &'a str,
    created_hidden: bool,
    created_count: i64,
    lost_hidden: bool,
    lost_count: i64,
    kills: u64,
    player_kills: u64,
}

#[derive(Default)]
struct StatsRegionAggregate {
    wins: u64,
    losses: u64,
    max_asc: u64,
    max_com: BTreeSet<String>,
    prestiges: HashMap<String, u64>,
}

#[derive(Default)]
struct StatsPlayerAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    last_seen: u64,
    commander_counts: HashMap<String, u64>,
    latest_commander: String,
}

impl StatsPlayerAggregate {
    fn record(
        &mut self,
        player: &StatsPlayerSnapshot,
        replay_is_victory: bool,
        kill_fraction: f64,
        date_seconds: u64,
    ) {
        if replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
        self.apm_values.push(player.apm);
        self.kill_fractions.push(kill_fraction);
        if !player.commander.is_empty() {
            self.latest_commander = player.commander.clone();
            self.commander_counts
                .entry(player.commander.clone())
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }
        self.last_seen = self.last_seen.max(date_seconds);
    }

    fn dominant_commander(&self) -> (String, f64) {
        let games = self.wins.saturating_add(self.losses);
        let Some((commander, count)) = self
            .commander_counts
            .iter()
            .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        else {
            return (Self::sanitize(&self.latest_commander), 0.0);
        };
        (Self::sanitize(commander), ratio(*count, games))
    }

    fn sanitize(value: &str) -> String {
        sanitize_replay_text(value)
    }
}

#[derive(Serialize)]
struct SqlStatsFastestMapDetails {
    length: f64,
    file: String,
    date: u64,
    difficulty: String,
    players: Vec<Value>,
    enemy_race: String,
}

#[derive(Serialize)]
struct SqlStatsMapDataRow {
    id: String,
    average_victory_time: f64,
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
    bonus: f64,
    #[serde(rename = "detailedCount")]
    detailed_count: u64,
    #[serde(rename = "Fastest")]
    fastest: SqlStatsFastestMapDetails,
}

#[derive(Serialize)]
struct SqlStatsCommanderDataRow {
    #[serde(rename = "Frequency")]
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
    #[serde(rename = "MedianAPM")]
    median_apm: f64,
    #[serde(rename = "KillFraction")]
    kill_fraction: f64,
    #[serde(rename = "Mastery")]
    mastery: Map<String, Value>,
    #[serde(rename = "MasteryDistribution")]
    mastery_distribution: Map<String, Value>,
    #[serde(rename = "MasteryDistributionByPrestige")]
    mastery_distribution_by_prestige: Map<String, Value>,
    #[serde(rename = "Prestige")]
    prestige: Map<String, Value>,
    #[serde(rename = "MasteryByPrestige")]
    mastery_by_prestige: Map<String, Value>,
    #[serde(rename = "detailedCount")]
    detailed_count: u64,
}

#[derive(Serialize)]
struct SqlStatsDifficultyDataRow {
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
}

#[derive(Serialize)]
struct SqlStatsRegionDataRow {
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    winrate: f64,
    max_asc: u64,
    prestiges: Map<String, Value>,
    max_com: Vec<String>,
}

#[derive(Serialize)]
struct SqlStatsPlayerDataRow {
    wins: u64,
    losses: u64,
    winrate: f64,
    kills: f64,
    apm: f64,
    frequency: f64,
    last_seen: u64,
    commander: String,
}

#[derive(Serialize)]
struct SqlStatsUnitDataPayload {
    main: Value,
    ally: Value,
    amon: Value,
}

#[derive(Serialize)]
struct SqlStatsAnalysisPayload {
    #[serde(rename = "MapData")]
    map_data: Map<String, Value>,
    #[serde(rename = "CommanderData")]
    commander_data: Map<String, Value>,
    #[serde(rename = "AllyCommanderData")]
    ally_commander_data: Map<String, Value>,
    #[serde(rename = "DifficultyData")]
    difficulty_data: Map<String, Value>,
    #[serde(rename = "RegionData")]
    region_data: Map<String, Value>,
    #[serde(rename = "PlayerData")]
    player_data: Map<String, Value>,
    #[serde(rename = "AmonData")]
    amon_data: Map<String, Value>,
    #[serde(rename = "UnitData")]
    unit_data: Value,
    #[serde(rename = "MapDataReady")]
    map_data_ready: bool,
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn ratio_f64(numerator: f64, denominator: f64) -> f64 {
    if denominator <= f64::EPSILON {
        0.0
    } else {
        numerator / denominator
    }
}

fn median_u64(values: &[u64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid] as f64
    } else {
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    }
}

fn median_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
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

fn ymd_from_unix_seconds(seconds: u64) -> Option<u32> {
    let days = i64::try_from(seconds / 86_400).ok()?;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    if year < 0 {
        return None;
    }
    let year_u32 = u32::try_from(year).ok()?;
    let month_u32 = u32::try_from(m).ok()?;
    let day_u32 = u32::try_from(d).ok()?;
    year_u32
        .checked_mul(10_000)
        .and_then(|value| {
            month_u32
                .checked_mul(100)
                .and_then(|month| value.checked_add(month))
        })
        .and_then(|value| value.checked_add(day_u32))
}

fn should_count_prestige(date_seconds: u64) -> bool {
    ymd_from_unix_seconds(date_seconds).is_some_and(|value| value > PRESTIGE_TRACKING_START_YMD)
}

fn mastery_points_invested(raw_values: &[u64]) -> u64 {
    raw_values.iter().take(6).copied().sum::<u64>()
}

fn normalize_mastery_vector(raw_values: &[u64]) -> [f64; 6] {
    let mut normalized = [0f64; 6];
    let total = mastery_points_invested(raw_values) as f64;
    if total <= f64::EPSILON {
        return normalized;
    }
    for (idx, raw) in raw_values.iter().take(6).enumerate() {
        normalized[idx] = *raw as f64 / total;
    }
    normalized
}

fn normalize_mastery_values(raw: &[u64]) -> Vec<u64> {
    let mut values = vec![0u64; 6];
    for (index, value) in raw.iter().take(6).enumerate() {
        values[index] = *value;
    }
    values
}

fn record_mastery_counts(target: &mut [f64; 6], values: &[f64; 6]) {
    for (idx, value) in values.iter().enumerate() {
        target[idx] += *value;
    }
}

fn record_prestige_count(target: &mut [u64; 4], prestige: u64) {
    let prestige = usize::try_from(prestige.min(3)).unwrap_or(3);
    target[prestige] = target[prestige].saturating_add(1);
}

fn record_mastery_by_prestige(target: &mut [[f64; 6]; 4], prestige: u64, values: &[f64; 6]) {
    let prestige = usize::try_from(prestige.min(3)).unwrap_or(3);
    for (idx, value) in values.iter().enumerate() {
        target[prestige][idx] += *value;
    }
}

fn record_mastery_distribution(target: &mut StatsMasteryDistributionCounts, raw_values: &[u64]) {
    for (pair_index, counts) in target.iter_mut().enumerate().take(3) {
        let left = raw_values.get(pair_index * 2).copied().unwrap_or(0);
        let right = raw_values.get(pair_index * 2 + 1).copied().unwrap_or(0);
        let pair_total = left.saturating_add(right);
        if pair_total == 0 {
            continue;
        }
        let bucket = left
            .saturating_mul(MASTERY_DISTRIBUTION_RATIO_SCALE)
            .saturating_add(pair_total / 2)
            .checked_div(pair_total)
            .unwrap_or(0)
            .min(MASTERY_DISTRIBUTION_RATIO_SCALE);
        counts
            .entry(bucket)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
}

fn record_mastery_distribution_by_prestige(
    target: &mut StatsMasteryDistributionByPrestigeCounts,
    prestige: u64,
    raw_values: &[u64],
) {
    let prestige = usize::try_from(prestige.min(3)).unwrap_or(3);
    record_mastery_distribution(&mut target[prestige], raw_values);
}

fn build_ratio_map(values: &[u64], total_games: u64) -> Map<String, Value> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| (index.to_string(), Value::from(ratio(*value, total_games))))
        .collect()
}

fn build_mastery_ratio_map(values: &[f64; 6]) -> Map<String, Value> {
    let mut result = Map::new();
    for pair_index in 0..3 {
        let left_idx = pair_index * 2;
        let right_idx = left_idx + 1;
        let pair_total = values[left_idx] + values[right_idx];
        result.insert(
            left_idx.to_string(),
            Value::from(ratio_f64(values[left_idx], pair_total)),
        );
        result.insert(
            right_idx.to_string(),
            Value::from(ratio_f64(values[right_idx], pair_total)),
        );
    }
    result
}

fn mastery_distribution_ratio_key(bucket: u64) -> String {
    let integer = bucket / 1_000;
    let fractional = bucket % 1_000;
    if fractional == 0 {
        return integer.to_string();
    }
    format!("{integer}.{fractional:03}")
        .trim_end_matches('0')
        .to_string()
}

fn build_mastery_distribution_map(values: &StatsMasteryDistributionCounts) -> Map<String, Value> {
    let mut result = Map::new();
    for (pair_index, pair_counts) in values.iter().enumerate() {
        let pair_total = pair_counts.values().sum::<u64>();
        let buckets = pair_counts
            .iter()
            .map(|(bucket, count)| {
                (
                    mastery_distribution_ratio_key(*bucket),
                    Value::from(ratio(*count, pair_total)),
                )
            })
            .collect::<Map<String, Value>>();
        result.insert(pair_index.to_string(), Value::Object(buckets));
    }
    result
}

fn build_mastery_distribution_by_prestige_map(
    values: &StatsMasteryDistributionByPrestigeCounts,
) -> Map<String, Value> {
    values
        .iter()
        .enumerate()
        .map(|(prestige, distribution)| {
            (
                prestige.to_string(),
                Value::Object(build_mastery_distribution_map(distribution)),
            )
        })
        .collect()
}

fn build_mastery_by_prestige_ratio_map(values: &[[f64; 6]; 4]) -> Map<String, Value> {
    values
        .iter()
        .enumerate()
        .map(|(prestige, mastery_values)| {
            (
                prestige.to_string(),
                Value::Object(build_mastery_ratio_map(mastery_values)),
            )
        })
        .collect()
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
            WITH filtered_entry_ids AS (
                SELECT e.id
                FROM replay_cache_entries e
                WHERE {where_sql}
            )
            SELECT
                0 AS row_kind,
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
                NULL AS unit_pid,
                NULL AS unit_name,
                NULL AS unit_created_kind,
                NULL AS unit_created_count,
                NULL AS unit_lost_kind,
                NULL AS unit_lost_count,
                NULL AS unit_kills,
                NULL AS amon_unit_name,
                NULL AS amon_created_kind,
                NULL AS amon_created_count,
                NULL AS amon_lost_kind,
                NULL AS amon_lost_count,
                NULL AS amon_kills
            FROM filtered_entry_ids ids
            INNER JOIN replay_cache_entries e ON e.id = ids.id
            LEFT JOIN replay_cache_players p1 ON p1.replay_id = e.id AND p1.pid = 1
            LEFT JOIN replay_cache_players p2 ON p2.replay_id = e.id AND p2.pid = 2

            UNION ALL

            SELECT
                1 AS row_kind,
                u.replay_id,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                u.pid,
                u.unit_name,
                u.created_kind,
                u.created_count,
                u.lost_kind,
                u.lost_count,
                u.kills,
                NULL, NULL, NULL, NULL, NULL, NULL
            FROM filtered_entry_ids ids
            INNER JOIN replay_cache_player_units u ON u.replay_id = ids.id

            UNION ALL

            SELECT
                2 AS row_kind,
                a.replay_id,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                a.unit_name,
                a.created_kind,
                a.created_count,
                a.lost_kind,
                a.lost_count,
                a.kills
            FROM filtered_entry_ids ids
            INNER JOIN replay_cache_amon_units a ON a.replay_id = ids.id

            ORDER BY replay_id ASC, row_kind ASC, unit_pid ASC, unit_name ASC, amon_unit_name ASC
            "
        );
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        let mut rows = statement
            .query(params_from_iter(bind_values.iter()))
            .map_err(|source| self.sqlite_error(source))?;
        let mut snapshots_by_id = BTreeMap::<i64, StatsReplaySnapshot>::new();
        while let Some(row) = rows.next().map_err(|source| self.sqlite_error(source))? {
            let row_kind = self.sqlite_row(row.get::<_, i64>(0))?;
            let replay_id = self.sqlite_row(row.get::<_, i64>(1))?;
            match row_kind {
                0 => {
                    let p1 = self.sqlite_row(Self::stats_player_from_row(row, 13))?;
                    let p2 = self.sqlite_row(Self::stats_player_from_row(row, 23))?;
                    let file = self.sqlite_row(row.get::<_, String>(2))?;
                    let (main, ally) =
                        Self::orient_stats_players(&file, p1, p2, main_names, main_handles);
                    snapshots_by_id.insert(
                        replay_id,
                        StatsReplaySnapshot {
                            file,
                            map_name: self.sqlite_row(row.get(3))?,
                            result: self.sqlite_row(row.get(4))?,
                            date_seconds: ReplayCacheEntryRecord::i64_to_u64(
                                self.sqlite_row(row.get::<_, i64>(5))?,
                            ),
                            detailed_analysis: self.sqlite_row(row.get::<_, i64>(6))? != 0,
                            brutal_plus: ReplayCacheEntryRecord::i64_to_u64(
                                self.sqlite_row(row.get::<_, i64>(7))?,
                            ),
                            extension: self.sqlite_row(row.get::<_, i64>(8))? != 0,
                            length_realtime: self.sqlite_row(row.get(9))?,
                            difficulty: self.sqlite_row(row.get(10))?,
                            enemy_race: self.sqlite_row(row.get(11))?,
                            bonus_completed: ReplayCacheEntryRecord::i64_to_u64(
                                self.sqlite_row(row.get::<_, i64>(12))?,
                            ),
                            main,
                            ally,
                            player_units: Vec::new(),
                            amon_units: Vec::new(),
                        },
                    );
                }
                1 => {
                    if let Some(snapshot) = snapshots_by_id.get_mut(&replay_id) {
                        snapshot.player_units.push(StatsPlayerUnitSnapshot {
                            pid: ReplayCacheEntryRecord::i64_to_u32(
                                self.sqlite_row(row.get::<_, i64>(33))?,
                            ) as u8,
                            unit_name: self.sqlite_row(row.get(34))?,
                            created_hidden: self.sqlite_row(row.get::<_, String>(35))? == "hidden",
                            created_count: self
                                .sqlite_row(row.get::<_, Option<i64>>(36))?
                                .unwrap_or_default(),
                            lost_hidden: self.sqlite_row(row.get::<_, String>(37))? == "hidden",
                            lost_count: self
                                .sqlite_row(row.get::<_, Option<i64>>(38))?
                                .unwrap_or_default(),
                            kills: ReplayCacheEntryRecord::i64_to_u64(
                                self.sqlite_row(row.get::<_, i64>(39))?,
                            ),
                        });
                    }
                }
                2 => {
                    if let Some(snapshot) = snapshots_by_id.get_mut(&replay_id) {
                        snapshot.amon_units.push(StatsAmonUnitSnapshot {
                            unit_name: self.sqlite_row(row.get(40))?,
                            created_hidden: self.sqlite_row(row.get::<_, String>(41))? == "hidden",
                            created_count: self
                                .sqlite_row(row.get::<_, Option<i64>>(42))?
                                .unwrap_or_default(),
                            lost_hidden: self.sqlite_row(row.get::<_, String>(43))? == "hidden",
                            lost_count: self
                                .sqlite_row(row.get::<_, Option<i64>>(44))?
                                .unwrap_or_default(),
                            kills: self.sqlite_row(row.get(45))?,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(snapshots_by_id.into_values().collect())
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

        if let Some(owner_handle) = infer_owner_handle_from_replay_path(file) {
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
        if query.restrict_to_current_replay_files()
            && !query.current_replay_files().contains(&snapshot.file)
        {
            return false;
        }
        if !query.include_mutations() && snapshot.extension {
            return false;
        }
        if !query.include_normal_games() && !snapshot.extension {
            return false;
        }
        let Some(is_victory) = result_is_victory(&snapshot.result) else {
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
        let main_mastery_points = mastery_points_invested(&snapshot.main.masteries);
        let ally_mastery_points = mastery_points_invested(&snapshot.ally.masteries);
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
            let region = infer_region_from_handle(&snapshot.main.handle)
                .or_else(|| infer_region_from_handle(&snapshot.ally.handle))
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
            let Some(replay_is_victory) = result_is_victory(&snapshot.result) else {
                continue;
            };
            let main_name = sanitize_replay_text(&snapshot.main.name);
            let ally_name = sanitize_replay_text(&snapshot.ally.name);
            let main_commander_text = sanitize_replay_text(&snapshot.main.commander);
            let ally_commander_text = sanitize_replay_text(&snapshot.ally.commander);
            let main_commander_name = normalized_commander_name(&main_commander_text);
            let ally_commander_name = normalized_commander_name(&ally_commander_text);
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

            let main_kill_fraction = kill_fraction(snapshot.main.kills, snapshot.ally.kills);
            let ally_kill_fraction = 1.0 - main_kill_fraction;
            let include_prestige = should_count_prestige(snapshot.date_seconds);

            let map_entry = map_values.entry(map_id.clone()).or_default();
            if snapshot.detailed_analysis {
                map_entry.detailed_count = map_entry.detailed_count.saturating_add(1);
            }
            if replay_is_victory {
                map_entry.victory_games = map_entry.victory_games.saturating_add(1);
                map_entry.victory_length_sum += snapshot.length_realtime;
                if snapshot.detailed_analysis {
                    let bonus_total = dictionary
                        .coop_map_id_to_english(&map_id)
                        .as_deref()
                        .and_then(|name| {
                            crate::replay_analysis::ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(name, dictionary)
                        })
                        .unwrap_or(0);
                    if bonus_total > 0 {
                        let completed = snapshot.bonus_completed.min(bonus_total);
                        map_entry.bonus_fraction_sum += completed as f64 / bonus_total as f64;
                        map_entry.bonus_games = map_entry.bonus_games.saturating_add(1);
                    }
                }
                let should_replace_fastest = map_entry.fastest.as_ref().is_none_or(|fastest| {
                    snapshot.length_realtime < fastest.length_realtime
                        || ((snapshot.length_realtime - fastest.length_realtime).abs()
                            < f64::EPSILON
                            && snapshot.date_seconds < fastest.date_seconds)
                });
                if should_replace_fastest {
                    map_entry.fastest = Some(snapshot.clone());
                }
            }
            if replay_is_victory {
                map_entry.wins = map_entry.wins.saturating_add(1);
            } else {
                map_entry.losses = map_entry.losses.saturating_add(1);
            }

            let (main_is_region_main, ally_is_region_main) =
                Self::stats_region_main_flags(&snapshot, has_known_main_handles, main_handles);
            let region = Self::stats_region_for_snapshot(
                &snapshot,
                has_known_main_handles,
                main_is_region_main,
                ally_is_region_main,
            );
            let region_entry = region_values.entry(region).or_default();
            if replay_is_victory {
                region_entry.wins = region_entry.wins.saturating_add(1);
            } else {
                region_entry.losses = region_entry.losses.saturating_add(1);
            }
            if main_is_region_main {
                Self::record_region_player(
                    region_entry,
                    &snapshot.main,
                    &main_commander_text,
                    &main_commander_name,
                );
            }
            if ally_is_region_main {
                Self::record_region_player(
                    region_entry,
                    &snapshot.ally,
                    &ally_commander_text,
                    &ally_commander_name,
                );
            }

            let difficulty = Self::stats_difficulty_label(&snapshot);
            if !difficulty.contains('/') {
                let diff_entry = difficulty_values.entry(difficulty).or_default();
                if replay_is_victory {
                    diff_entry.wins = diff_entry.wins.saturating_add(1);
                } else {
                    diff_entry.losses = diff_entry.losses.saturating_add(1);
                }
            }

            sum_main.record_result(replay_is_victory);
            sum_ally.record_result(replay_is_victory);

            Self::record_commander(
                main_commander
                    .entry(main_commander_name.clone())
                    .or_default(),
                &snapshot.main,
                replay_is_victory,
                snapshot.detailed_analysis,
                main_kill_fraction,
                include_prestige,
            );
            Self::record_commander(
                ally_commander
                    .entry(ally_commander_name.clone())
                    .or_default(),
                &snapshot.ally,
                replay_is_victory,
                snapshot.detailed_analysis,
                ally_kill_fraction,
                include_prestige,
            );
            sum_main.record_player(
                &snapshot.main,
                snapshot.detailed_analysis,
                main_kill_fraction,
                include_prestige,
            );
            sum_ally.record_player(
                &snapshot.ally,
                snapshot.detailed_analysis,
                ally_kill_fraction,
                include_prestige,
            );

            if !main_name.is_empty() {
                player_values.entry(main_name).or_default().record(
                    &snapshot.main,
                    replay_is_victory,
                    main_kill_fraction,
                    snapshot.date_seconds,
                );
            }
            if !ally_name.is_empty() {
                player_values.entry(ally_name).or_default().record(
                    &snapshot.ally,
                    replay_is_victory,
                    ally_kill_fraction,
                    snapshot.date_seconds,
                );
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
            let games = aggregate.wins.saturating_add(aggregate.losses);
            let bonus = if aggregate.bonus_games == 0 {
                0.0
            } else {
                aggregate.bonus_fraction_sum / aggregate.bonus_games as f64
            };
            let fastest = aggregate.fastest.unwrap_or_else(|| StatsReplaySnapshot {
                file: String::new(),
                map_name: String::new(),
                result: String::new(),
                difficulty: String::new(),
                enemy_race: String::new(),
                date_seconds: 0,
                detailed_analysis: false,
                brutal_plus: 0,
                extension: false,
                length_realtime: 999_999.0,
                bonus_completed: 0,
                main: StatsPlayerSnapshot::default(),
                ally: StatsPlayerSnapshot::default(),
                player_units: Vec::new(),
                amon_units: Vec::new(),
            });
            let fastest_players =
                Self::fastest_players_value(&fastest, main_names, main_handles, dictionary);
            map_data.insert(
                map_name,
                to_value(&SqlStatsMapDataRow {
                    id: map_id,
                    average_victory_time: if aggregate.victory_games == 0 {
                        999_999.0
                    } else {
                        aggregate.victory_length_sum / aggregate.victory_games as f64
                    },
                    frequency: ratio(games, total_games),
                    victory: aggregate.wins,
                    defeat: aggregate.losses,
                    winrate: ratio(aggregate.wins, games),
                    bonus,
                    detailed_count: aggregate.detailed_count,
                    fastest: SqlStatsFastestMapDetails {
                        length: fastest.length_realtime,
                        file: fastest.file,
                        date: fastest.date_seconds,
                        difficulty: sanitize_replay_text(&fastest.difficulty),
                        players: fastest_players,
                        enemy_race: sanitize_replay_text(&fastest.enemy_race),
                    },
                }),
            );
        }

        let commander_data = Self::build_commander_data(StatsCommanderDataInput {
            aggregates: &main_commander,
            total_games,
            totals: &sum_main,
            main_frequency: None,
        });
        let main_frequency = main_commander
            .iter()
            .map(|(commander, aggregate)| {
                let games = aggregate.wins.saturating_add(aggregate.losses);
                (commander.clone(), ratio(games, sum_main.games()))
            })
            .collect::<HashMap<_, _>>();
        let ally_commander_data = Self::build_commander_data(StatsCommanderDataInput {
            aggregates: &ally_commander,
            total_games,
            totals: &sum_ally,
            main_frequency: Some(&main_frequency),
        });

        let difficulty_data = difficulty_values
            .into_iter()
            .map(|(difficulty, aggregate)| {
                let games = aggregate.wins.saturating_add(aggregate.losses);
                (
                    difficulty,
                    to_value(&SqlStatsDifficultyDataRow {
                        victory: aggregate.wins,
                        defeat: aggregate.losses,
                        winrate: ratio(aggregate.wins, games),
                    }),
                )
            })
            .collect::<Map<String, Value>>();

        let region_data = region_values
            .into_iter()
            .map(|(region, aggregate)| {
                let games = aggregate.wins.saturating_add(aggregate.losses);
                let prestiges = aggregate
                    .prestiges
                    .into_iter()
                    .map(|(commander, prestige)| (commander, Value::from(prestige)))
                    .collect::<Map<String, Value>>();
                (
                    region,
                    to_value(&SqlStatsRegionDataRow {
                        frequency: ratio(games, total_games),
                        victory: aggregate.wins,
                        defeat: aggregate.losses,
                        winrate: ratio(aggregate.wins, games),
                        max_asc: aggregate.max_asc,
                        prestiges,
                        max_com: aggregate.max_com.into_iter().collect(),
                    }),
                )
            })
            .collect::<Map<String, Value>>();

        let player_data = player_values
            .into_iter()
            .map(|(name, aggregate)| {
                let games = aggregate.wins.saturating_add(aggregate.losses);
                let (commander, frequency) = aggregate.dominant_commander();
                (
                    sanitize_replay_text(&name),
                    to_value(&SqlStatsPlayerDataRow {
                        wins: aggregate.wins,
                        losses: aggregate.losses,
                        winrate: ratio(aggregate.wins, games),
                        kills: median_f64(&aggregate.kill_fractions),
                        apm: median_u64(&aggregate.apm_values),
                        frequency,
                        last_seen: aggregate.last_seen,
                        commander,
                    }),
                )
            })
            .collect::<Map<String, Value>>();

        let unit_data = if include_detailed {
            Self::load_statistics_unit_data(&valid_snapshots, main_handles, dictionary)
        } else {
            Value::Null
        };

        let analysis = to_value(&SqlStatsAnalysisPayload {
            map_data,
            commander_data,
            ally_commander_data,
            difficulty_data,
            region_data,
            player_data,
            amon_data: Map::new(),
            unit_data,
            map_data_ready: true,
        });
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
            && main_handles.contains(&normalized_handle_key(&player.handle));
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
            infer_region_from_handle(&snapshot.main.handle)
        } else if has_known_main_handles && p2_is_main {
            infer_region_from_handle(&snapshot.ally.handle)
        } else {
            infer_region_from_handle(&snapshot.main.handle)
                .or_else(|| infer_region_from_handle(&snapshot.ally.handle))
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

    fn record_region_player(
        aggregate: &mut StatsRegionAggregate,
        player: &StatsPlayerSnapshot,
        commander_text: &str,
        commander_name: &str,
    ) {
        aggregate.max_asc = aggregate.max_asc.max(player.mastery_level);
        if player.commander_level == 15 && !commander_text.is_empty() {
            aggregate.max_com.insert(commander_text.to_string());
        }
        if !commander_name.is_empty() {
            aggregate
                .prestiges
                .entry(commander_name.to_string())
                .and_modify(|current| *current = (*current).max(player.prestige.min(3)))
                .or_insert(player.prestige.min(3));
        }
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

    fn record_commander(
        aggregate: &mut StatsCommanderAggregate,
        player: &StatsPlayerSnapshot,
        replay_is_victory: bool,
        detailed_analysis: bool,
        kill_fraction: f64,
        include_prestige: bool,
    ) {
        if replay_is_victory {
            aggregate.wins = aggregate.wins.saturating_add(1);
        } else {
            aggregate.losses = aggregate.losses.saturating_add(1);
        }
        if detailed_analysis {
            aggregate.detailed_count = aggregate.detailed_count.saturating_add(1);
            aggregate.kill_fractions.push(kill_fraction);
        }
        aggregate.apm_values.push(player.apm);
        let normalized_masteries = normalize_mastery_vector(&player.masteries);
        record_mastery_counts(&mut aggregate.mastery_counts, &normalized_masteries);
        record_mastery_distribution(
            &mut aggregate.mastery_distribution_counts,
            &player.masteries,
        );
        record_mastery_distribution_by_prestige(
            &mut aggregate.mastery_distribution_by_prestige_counts,
            player.prestige,
            &player.masteries,
        );
        record_mastery_by_prestige(
            &mut aggregate.mastery_by_prestige_counts,
            player.prestige,
            &normalized_masteries,
        );
        if include_prestige {
            record_prestige_count(&mut aggregate.prestige_counts, player.prestige);
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

        let commander = sanitize_replay_text(&normalized_commander_name(&player.commander));
        let prestige_name = dictionary
            .prestige_name(&commander, player.prestige)
            .map(str::to_string)
            .unwrap_or_else(|| format!("P{}", player.prestige));
        to_value(&FastestPlayer {
            name: sanitize_replay_text(&player.name),
            handle: player.handle.clone(),
            commander,
            apm: player.apm,
            mastery_level: player.mastery_level,
            masteries: normalize_mastery_values(&player.masteries),
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

    fn build_commander_data(input: StatsCommanderDataInput<'_>) -> Map<String, Value> {
        let corrected_frequency = input
            .aggregates
            .iter()
            .map(|(name, aggregate)| {
                let games = aggregate.wins.saturating_add(aggregate.losses) as f64;
                let corrected = if let Some(main_frequency) = input.main_frequency {
                    let divisor = 1.0 - main_frequency.get(name).copied().unwrap_or(0.0);
                    if divisor <= f64::EPSILON {
                        0.0
                    } else {
                        games / divisor
                    }
                } else {
                    games
                };
                (name.clone(), corrected)
            })
            .collect::<HashMap<_, _>>();
        let corrected_total = corrected_frequency.values().sum::<f64>();

        let mut rows = Map::new();
        for (commander, aggregate) in input.aggregates {
            let games = aggregate.wins.saturating_add(aggregate.losses);
            let prestige_games = aggregate.prestige_counts.iter().sum::<u64>();
            let frequency = if input.main_frequency.is_some() {
                ratio_f64(
                    corrected_frequency.get(commander).copied().unwrap_or(0.0),
                    corrected_total,
                )
            } else {
                ratio(games, input.total_games)
            };
            rows.insert(
                commander.clone(),
                to_value(&SqlStatsCommanderDataRow {
                    frequency,
                    victory: aggregate.wins,
                    defeat: aggregate.losses,
                    winrate: ratio(aggregate.wins, games),
                    median_apm: median_u64(&aggregate.apm_values),
                    kill_fraction: median_f64(&aggregate.kill_fractions),
                    mastery: build_mastery_ratio_map(&aggregate.mastery_counts),
                    mastery_distribution: build_mastery_distribution_map(
                        &aggregate.mastery_distribution_counts,
                    ),
                    mastery_distribution_by_prestige: build_mastery_distribution_by_prestige_map(
                        &aggregate.mastery_distribution_by_prestige_counts,
                    ),
                    prestige: build_ratio_map(&aggregate.prestige_counts, prestige_games),
                    mastery_by_prestige: build_mastery_by_prestige_ratio_map(
                        &aggregate.mastery_by_prestige_counts,
                    ),
                    detailed_count: aggregate.detailed_count,
                }),
            );
        }

        let total_commander_games = input.totals.games();
        let detailed_count = input
            .aggregates
            .values()
            .map(|value| value.detailed_count)
            .sum();
        rows.insert(
            "any".to_string(),
            to_value(&SqlStatsCommanderDataRow {
                frequency: if total_commander_games == 0 { 0.0 } else { 1.0 },
                victory: input.totals.wins,
                defeat: input.totals.losses,
                winrate: ratio(input.totals.wins, total_commander_games),
                median_apm: median_u64(&input.totals.apm_values),
                kill_fraction: median_f64(&input.totals.kill_fractions),
                mastery: build_mastery_ratio_map(&input.totals.mastery_counts),
                mastery_distribution: build_mastery_distribution_map(
                    &input.totals.mastery_distribution_counts,
                ),
                mastery_distribution_by_prestige: build_mastery_distribution_by_prestige_map(
                    &input.totals.mastery_distribution_by_prestige_counts,
                ),
                prestige: build_ratio_map(
                    &input.totals.prestige_counts,
                    input.totals.prestige_counts.iter().sum::<u64>(),
                ),
                mastery_by_prestige: build_mastery_by_prestige_ratio_map(
                    &input.totals.mastery_by_prestige_counts,
                ),
                detailed_count,
            }),
        );
        rows
    }

    fn load_statistics_unit_data(
        snapshots: &[StatsReplaySnapshot],
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Value {
        let mut main_rollup = BTreeMap::<String, CommanderUnitRollup>::new();
        let mut ally_rollup = BTreeMap::<String, CommanderUnitRollup>::new();
        let mut amon_rollup = BTreeMap::<String, UnitStatsRollup>::new();

        for snapshot in snapshots {
            for unit in &snapshot.player_units {
                let Some(player) = Self::stats_player_for_pid(snapshot, unit.pid) else {
                    continue;
                };
                let commander = normalized_commander_name(&player.commander);
                let target_rollup =
                    if ReplayAnalysis::is_main_player_by_handle(&player.handle, main_handles) {
                        &mut main_rollup
                    } else {
                        &mut ally_rollup
                    };
                Self::append_unit_rollup(
                    target_rollup,
                    StatsUnitRollupInput {
                        commander: &commander,
                        unit_name: &unit.unit_name,
                        created_hidden: unit.created_hidden,
                        created_count: unit.created_count,
                        lost_hidden: unit.lost_hidden,
                        lost_count: unit.lost_count,
                        kills: unit.kills,
                        player_kills: player.kills,
                    },
                );
            }
            for unit in &snapshot.amon_units {
                let entry = amon_rollup
                    .entry(sanitize_replay_text(&unit.unit_name))
                    .or_default();
                if !unit.created_hidden {
                    entry.created = entry.created.saturating_add(unit.created_count);
                }
                if !unit.lost_hidden {
                    entry.lost = entry.lost.saturating_add(unit.lost_count);
                }
                entry.kills = entry.kills.saturating_add(unit.kills);
            }
        }

        to_value(&SqlStatsUnitDataPayload {
            main: TauriOverlayOps::build_commander_unit_data_with_dictionary(
                main_rollup,
                dictionary,
            ),
            ally: TauriOverlayOps::build_commander_unit_data_with_dictionary(
                ally_rollup,
                dictionary,
            ),
            amon: Self::build_amon_unit_data(amon_rollup),
        })
    }

    fn stats_player_for_pid(
        snapshot: &StatsReplaySnapshot,
        pid: u8,
    ) -> Option<&StatsPlayerSnapshot> {
        if pid == snapshot.main.pid {
            Some(&snapshot.main)
        } else if pid == snapshot.ally.pid {
            Some(&snapshot.ally)
        } else {
            None
        }
    }

    fn append_unit_rollup(
        rollup: &mut BTreeMap<String, CommanderUnitRollup>,
        input: StatsUnitRollupInput<'_>,
    ) {
        let commander = sanitize_replay_text(input.commander);
        if commander.is_empty() {
            return;
        }
        let unit_name = sanitize_replay_text(input.unit_name);
        if unit_name.is_empty() {
            return;
        }
        let commander_entry = rollup.entry(commander.clone()).or_default();
        commander_entry.count = commander_entry.count.saturating_add(1);
        let unit_entry = commander_entry.units.entry(unit_name).or_default();
        if input.created_hidden {
            unit_entry.created_hidden = true;
        } else {
            unit_entry.created = unit_entry.created.saturating_add(input.created_count);
        }
        if input.lost_hidden {
            unit_entry.lost_hidden = true;
        } else {
            unit_entry.lost = unit_entry.lost.saturating_add(input.lost_count);
        }
        unit_entry.kills = unit_entry
            .kills
            .saturating_add(i64::try_from(input.kills).unwrap_or(i64::MAX));
        if !input.created_hidden || commander == "Tychus" {
            unit_entry.made = unit_entry.made.saturating_add(1);
        }
        if input.player_kills > 0 {
            unit_entry
                .kill_percentages
                .push(input.kills as f64 / input.player_kills as f64);
        }
    }

    fn build_amon_unit_data(amon_rollup: BTreeMap<String, UnitStatsRollup>) -> Value {
        #[derive(Serialize)]
        struct AmonUnitRow {
            created: i64,
            lost: i64,
            kills: i64,
            #[serde(rename = "KD")]
            kd: Value,
        }

        const AMON_KD_MUTATORS: [&str; 4] = [
            "Twister",
            "Purifier Beam",
            "Moebius Corps Laser Drill",
            "Blizzard",
        ];
        const AMON_REMOVED_UNITS: [&str; 3] = [
            "AdeptPhaseShift",
            "Drakken Pulse Cannon",
            "James 'Sirius' Sykes",
        ];

        let mut rows = amon_rollup.into_iter().collect::<Vec<_>>();
        rows.sort_by(|(left_name, left), (right_name, right)| {
            right
                .created
                .cmp(&left.created)
                .then_with(|| left_name.cmp(right_name))
        });

        let mut output = Map::new();
        let mut total = UnitStatsRollup::default();
        for (unit, mut row) in rows {
            if AMON_REMOVED_UNITS
                .iter()
                .any(|removed| removed == &unit.as_str())
            {
                continue;
            }
            let kd = if AMON_KD_MUTATORS
                .iter()
                .any(|mutator| mutator == &unit.as_str())
            {
                row.lost = 0;
                Value::String("-".to_string())
            } else if row.lost <= 0 {
                Value::from(0.0)
            } else {
                Value::from(row.kills as f64 / row.lost as f64)
            };
            output.insert(
                unit,
                to_value(&AmonUnitRow {
                    created: row.created,
                    lost: row.lost,
                    kills: row.kills,
                    kd,
                }),
            );
            total.created = total.created.saturating_add(row.created);
            total.lost = total.lost.saturating_add(row.lost);
            total.kills = total.kills.saturating_add(row.kills);
        }
        output.insert(
            "sum".to_string(),
            to_value(&AmonUnitRow {
                created: total.created,
                lost: total.lost,
                kills: total.kills,
                kd: if total.lost <= 0 {
                    Value::from(0.0)
                } else {
                    Value::from(total.kills as f64 / total.lost as f64)
                },
            }),
        );
        Value::Object(output)
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

    pub(super) fn load_messages(
        &self,
        replay_id: i64,
    ) -> Result<Vec<ReplayMessage>, ReplayCacheDbError> {
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

    pub(super) fn load_amon_units(
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

    pub(super) fn load_player_stats(
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
