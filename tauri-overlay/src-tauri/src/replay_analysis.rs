use chrono::{Local, NaiveDate};
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheIconValue, CacheNumericValue, CacheOverallStatsFile, CachePlayer, CacheReplayEntry,
    CacheUnitStats, ReplayMessage,
};
use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, ReplayAnalysisResources, ReplayFileIdentity,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use s2coop_analyzer::tauri_replay_analysis_impl::{
    ParsedReplayMessage, ParsedReplayPlayer, ReplayReport,
};
use s2coop_analyzer::weekly_mutation_manager::{WeeklyMutationManager, WeeklyMutationStatus};
use serde::Serialize;
use serde_json::{Map, Value};
use std::borrow::{Borrow, Cow};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

use crate::backend_state::ReplayScanProgress;
use crate::path_manager::PathManagerOps;
use crate::shared_types::{
    LocalizedLabels, LocalizedText, ReplayScanProgressPayload, UiMutatorRow,
};
use crate::{
    AppSettings, CommanderUnitRollup, ReplayChatMessage, ReplayInfo, ReplayPlayerInfo,
    StatsSnapshot, StatsState, TauriOverlayOps, UnitStatsRollup, UNLIMITED_REPLAY_LIMIT,
};

const PRESTIGE_TRACKING_START_YMD: u32 = 20200726;
const MASTERY_DISTRIBUTION_RATIO_SCALE: u64 = 100_000;

type MasteryDistributionCounts = [BTreeMap<u64, u64>; 3];
type MasteryDistributionByPrestigeCounts = [MasteryDistributionCounts; 4];

struct ScanInFlightGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for ScanInFlightGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

pub struct ReplayAnalysisOps;

impl ReplayAnalysisOps {
    fn default_main_identity() -> (HashSet<String>, HashSet<String>) {
        let settings = AppSettings::from_saved_file();
        (
            settings.configured_main_names(),
            settings.configured_main_handles(),
        )
    }
}

impl ReplayAnalysisOps {
    fn decode_html_entities(value: &str) -> String {
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
    }
}

impl ReplayAnalysisOps {
    fn canonical_mutator_id_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        let canonical = if dictionary.mutator_data(mutator).is_some() {
            mutator.to_string()
        } else if let Some(mutator_id) = dictionary.mutator_id_from_name(mutator) {
            mutator_id.to_string()
        } else {
            mutator.to_string()
        };

        match canonical.as_str() {
            "HeroesfromtheStormOld" => "HeroesFromTheStorm".to_string(),
            "AfraidOfTheDark" => "UberDarkness".to_string(),
            _ => canonical,
        }
    }
}

impl ReplayAnalysisOps {
    fn mutator_display_name_en_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        let mutator_id =
            ReplayAnalysisOps::canonical_mutator_id_with_dictionary(mutator, dictionary);
        dictionary
            .mutator_data(&mutator_id)
            .map(|value| ReplayAnalysisOps::decode_html_entities(&value.name.en))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                dictionary
                    .mutator_ids
                    .get(&mutator_id)
                    .map(|value| value.to_string())
            })
            .unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn accurate_length_seconds_from_cache(value: &CacheNumericValue, fallback: u64) -> f64 {
        let seconds = match value {
            CacheNumericValue::Integer(value) => *value as f64,
            CacheNumericValue::Float(value) => *value,
        };
        if seconds.is_finite() && seconds > 0.0 {
            seconds
        } else {
            fallback as f64
        }
    }
}

impl ReplayAnalysisOps {
    fn display_length_seconds(value: f64) -> u64 {
        if !value.is_finite() || value <= 0.0 {
            0
        } else {
            value.floor() as u64
        }
    }
}

impl ReplayAnalysisOps {
    fn build_ratio_map(values: &[u64], total_games: u64) -> Map<String, Value> {
        let mut result = Map::new();
        for (idx, value) in values.iter().enumerate() {
            result.insert(
                idx.to_string(),
                Value::from(TauriOverlayOps::ratio(*value, total_games)),
            );
        }
        result
    }
}

impl ReplayAnalysisOps {
    fn build_mastery_ratio_map(raw_values: &[f64; 6]) -> Map<String, Value> {
        let mut result = Map::new();
        for pair_index in 0..3 {
            let left = raw_values[pair_index * 2];
            let right = raw_values[pair_index * 2 + 1];
            let pair_total = left + right;
            let left_ratio = ReplayAnalysisOps::ratio_f64(left, pair_total);
            let right_ratio = ReplayAnalysisOps::ratio_f64(right, pair_total);
            result.insert((pair_index * 2).to_string(), Value::from(left_ratio));
            result.insert((pair_index * 2 + 1).to_string(), Value::from(right_ratio));
        }
        result
    }
}

impl ReplayAnalysisOps {
    fn build_mastery_by_prestige_ratio_map(raw_values: &[[f64; 6]; 4]) -> Map<String, Value> {
        let mut result = Map::new();
        for (prestige, mastery_values) in raw_values.iter().enumerate() {
            let mut grouped = Map::new();
            for pair_index in 0..3 {
                let left_idx = pair_index * 2;
                let right_idx = pair_index * 2 + 1;
                let left = mastery_values[left_idx];
                let right = mastery_values[right_idx];
                let pair_total = left + right;
                grouped.insert(
                    left_idx.to_string(),
                    Value::from(ReplayAnalysisOps::ratio_f64(left, pair_total)),
                );
                grouped.insert(
                    right_idx.to_string(),
                    Value::from(ReplayAnalysisOps::ratio_f64(right, pair_total)),
                );
            }
            result.insert(prestige.to_string(), Value::Object(grouped));
        }
        result
    }
}

impl ReplayAnalysisOps {
    fn empty_mastery_distribution_counts() -> MasteryDistributionCounts {
        std::array::from_fn(|_| BTreeMap::new())
    }

    fn empty_mastery_distribution_by_prestige_counts() -> MasteryDistributionByPrestigeCounts {
        std::array::from_fn(|_| ReplayAnalysisOps::empty_mastery_distribution_counts())
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

    fn build_mastery_distribution_map(
        raw_values: &MasteryDistributionCounts,
    ) -> Map<String, Value> {
        let mut result = Map::new();
        for (pair_index, pair_counts) in raw_values.iter().enumerate() {
            let pair_total = pair_counts.values().sum::<u64>();
            let mut buckets = Map::new();
            for (bucket, count) in pair_counts.iter() {
                buckets.insert(
                    ReplayAnalysisOps::mastery_distribution_ratio_key(*bucket),
                    Value::from(TauriOverlayOps::ratio(*count, pair_total)),
                );
            }
            result.insert(pair_index.to_string(), Value::Object(buckets));
        }
        result
    }
}

impl ReplayAnalysisOps {
    fn build_mastery_distribution_by_prestige_map(
        raw_values: &MasteryDistributionByPrestigeCounts,
    ) -> Map<String, Value> {
        let mut result = Map::new();
        for (prestige, prestige_values) in raw_values.iter().enumerate() {
            result.insert(
                prestige.to_string(),
                Value::Object(ReplayAnalysisOps::build_mastery_distribution_map(
                    prestige_values,
                )),
            );
        }
        result
    }
}

impl ReplayAnalysisOps {
    fn ratio_f64(numerator: f64, denominator: f64) -> f64 {
        if denominator == 0.0 {
            0.0
        } else {
            numerator / denominator
        }
    }
}

impl ReplayAnalysisOps {
    fn normalize_mastery_vector(raw_values: &[u64]) -> [f64; 6] {
        let mut normalized = [0f64; 6];
        let total_points = ReplayAnalysisOps::mastery_points_invested(raw_values) as f64;
        if total_points <= f64::EPSILON {
            return normalized;
        }

        for (idx, raw) in raw_values.iter().take(6).enumerate() {
            normalized[idx] = *raw as f64 / total_points;
        }
        normalized
    }
}

impl ReplayAnalysisOps {
    fn mastery_points_invested(raw_values: &[u64]) -> u64 {
        raw_values.iter().take(6).copied().sum::<u64>()
    }
}

impl ReplayAnalysisOps {
    fn record_mastery_distribution(target: &mut MasteryDistributionCounts, raw_values: &[u64]) {
        for pair_index in 0..3 {
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
            let entry = target[pair_index].entry(bucket).or_insert(0);
            *entry = entry.saturating_add(1);
        }
    }
}

impl ReplayAnalysisOps {
    fn record_mastery_distribution_by_prestige(
        target: &mut MasteryDistributionByPrestigeCounts,
        prestige: u64,
        raw_values: &[u64],
    ) {
        let prestige_bucket = usize::try_from(prestige.min(3)).unwrap_or(3);
        ReplayAnalysisOps::record_mastery_distribution(&mut target[prestige_bucket], raw_values);
    }
}

impl ReplayAnalysisOps {
    fn record_command_mastery_counts(target: &mut [f64; 6], raw_values: &[f64; 6]) {
        for (idx, raw) in raw_values.iter().take(6).enumerate() {
            target[idx] += *raw;
        }
    }
}

impl ReplayAnalysisOps {
    fn record_command_mastery_by_prestige(
        target: &mut [[f64; 6]; 4],
        prestige: u64,
        raw_values: &[f64; 6],
    ) {
        let prestige_bucket = usize::try_from(prestige.min(3)).unwrap_or(3);
        for (idx, raw) in raw_values.iter().take(6).enumerate() {
            target[prestige_bucket][idx] += *raw;
        }
    }
}

impl ReplayAnalysisOps {
    fn should_count_prestige(date: u64) -> bool {
        TauriOverlayOps::ymd_from_unix_seconds(date)
            .is_some_and(|value| value > PRESTIGE_TRACKING_START_YMD)
    }
}

impl ReplayAnalysisOps {
    fn record_prestige_count(target: &mut [u64; 4], raw_prestige: u64) {
        let prestige = usize::try_from(raw_prestige.min(3)).unwrap_or(3);
        target[prestige] = target[prestige].saturating_add(1);
    }
}

impl ReplayAnalysisOps {
    fn fastest_map_prestige_name_with_dictionary(
        commander: &str,
        prestige: u64,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        let sanitized_commander = TauriOverlayOps::sanitize_replay_text(commander);
        dictionary
            .prestige_name(&sanitized_commander, prestige)
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("P{prestige}"))
    }
}

#[derive(Serialize)]
struct FastestMapPlayer {
    name: String,
    handle: String,
    commander: String,
    apm: u64,
    mastery_level: u64,
    masteries: Vec<u64>,
    prestige: u64,
    prestige_name: String,
}

impl ReplayAnalysisOps {
    fn fastest_map_player_value_with_dictionary(
        name: &str,
        handle: &str,
        commander: &str,
        apm: u64,
        mastery_level: u64,
        masteries: &[u64],
        prestige: u64,
        dictionary: &Sc2DictionaryData,
    ) -> Value {
        ReplayAnalysisOps::report_value(&FastestMapPlayer {
            name: TauriOverlayOps::sanitize_replay_text(name),
            handle: handle.to_string(),
            commander: TauriOverlayOps::sanitize_replay_text(commander),
            apm,
            mastery_level,
            masteries: TauriOverlayOps::normalize_mastery_values(masteries),
            prestige,
            prestige_name: ReplayAnalysisOps::fastest_map_prestige_name_with_dictionary(
                commander, prestige, dictionary,
            ),
        })
    }
}

impl ReplayAnalysisOps {
    fn report_value<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct PlayerRowPayload {
    pub handle: String,
    pub player: String,
    pub player_names: Vec<String>,
    #[ts(type = "number")]
    pub wins: u64,
    #[ts(type = "number")]
    pub losses: u64,
    pub winrate: f64,
    pub apm: f64,
    pub commander: String,
    pub frequency: f64,
    pub kills: f64,
    #[ts(type = "number")]
    pub last_seen: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct WeeklyRowPayload {
    pub mutation: String,
    #[serde(rename = "nameEn")]
    pub name_en: String,
    #[serde(rename = "nameKo")]
    pub name_ko: String,
    pub map: String,
    pub mutators: Vec<UiMutatorRow>,
    #[serde(rename = "mutationOrder")]
    #[ts(type = "number")]
    pub mutation_order: usize,
    #[serde(rename = "isCurrent")]
    pub is_current: bool,
    #[serde(rename = "nextDurationDays")]
    #[ts(type = "number")]
    pub next_duration_days: i64,
    #[serde(rename = "nextDuration")]
    pub next_duration: String,
    pub difficulty: String,
    #[ts(type = "number")]
    pub wins: u64,
    #[ts(type = "number")]
    pub losses: u64,
    pub winrate: f64,
}

impl ReplayAnalysisOps {
    fn hidden_unit_stats_names_with_dictionary(dictionary: &Sc2DictionaryData) -> HashSet<String> {
        dictionary
            .replay_analysis_data
            .dont_show_created_lost
            .iter()
            .cloned()
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn sanitize_hidden_unit_stats_with_hidden_units(
        mut units: Value,
        hidden_units: &HashSet<String>,
    ) -> Value {
        let Some(map) = units.as_object_mut() else {
            return units;
        };

        for (unit_name, row) in map.iter_mut() {
            if !hidden_units.contains(unit_name) {
                continue;
            }

            let Some(values) = row.as_array_mut() else {
                continue;
            };
            if values.len() < 2 {
                continue;
            }

            values[0] = Value::String("-".to_string());
            values[1] = Value::String("-".to_string());
        }

        units
    }
}

impl ReplayAnalysisOps {
    pub fn sanitize_hidden_unit_stats(units: Value) -> Value {
        let hidden_units = HashSet::new();
        ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(units, &hidden_units)
    }
}

impl ReplayAnalysisOps {
    pub fn sanitize_hidden_unit_stats_with_dictionary(
        units: Value,
        dictionary: &Sc2DictionaryData,
    ) -> Value {
        let hidden_units = ReplayAnalysisOps::hidden_unit_stats_names_with_dictionary(dictionary);
        ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(units, &hidden_units)
    }
}

impl ReplayAnalysisOps {
    pub fn collect_main_identity_lists_with_dictionary<R>(
        replays: &[R],
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> (Vec<String>, Vec<String>)
    where
        R: Borrow<ReplayInfo>,
    {
        let mut player_names = BTreeSet::new();
        let mut player_handles = BTreeSet::new();
        let has_known_identity = !main_names.is_empty() || !main_handles.is_empty();

        for replay in replays.iter().map(Borrow::borrow).filter(|replay| {
            replay.result != "Unparsed"
                && dictionary.canonicalize_coop_map_id(&replay.map).is_some()
        }) {
            let p1_is_main = ReplayAnalysis::is_main_player_identity(
                &replay.main().name,
                &replay.main().handle,
                main_names,
                main_handles,
            );
            let p2_is_main = ReplayAnalysis::is_main_player_identity(
                &replay.ally().name,
                &replay.ally().handle,
                main_names,
                main_handles,
            );
            let should_take_p1 = p1_is_main || (!has_known_identity && !p2_is_main);

            if should_take_p1 {
                let name = replay.main().name.trim();
                if !name.is_empty() {
                    player_names.insert(name.to_string());
                }

                let handle = replay.main().handle.trim();
                if !handle.is_empty() {
                    player_handles.insert(handle.to_string());
                }
            }

            if p2_is_main {
                let name = replay.ally().name.trim();
                if !name.is_empty() {
                    player_names.insert(name.to_string());
                }

                let handle = replay.ally().handle.trim();
                if !handle.is_empty() {
                    player_handles.insert(handle.to_string());
                }
            }
        }

        (
            player_names.into_iter().collect(),
            player_handles.into_iter().collect(),
        )
    }
}

impl ReplayAnalysisOps {
    fn report_player(report: &ReplayReport, pid: u8) -> Option<&ParsedReplayPlayer> {
        report
            .parser
            .players
            .iter()
            .find(|player| player.pid == pid)
    }
}

impl ReplayAnalysisOps {
    fn with_outlaw_icons(
        mut icons: Value,
        commander: &str,
        outlaw_order: Option<&Vec<String>>,
    ) -> Value {
        if commander != "Tychus" {
            return icons;
        }

        let Some(order) = outlaw_order else {
            return icons;
        };
        if order.is_empty() {
            return icons;
        }

        let Some(object) = icons.as_object_mut() else {
            return icons;
        };
        object.insert(
            "outlaws".to_string(),
            Value::Array(order.iter().cloned().map(Value::String).collect()),
        );
        icons
    }
}

impl ReplayAnalysisOps {
    fn file_modified_seconds(path: &Path) -> u64 {
        path.metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .map_or(0, TauriOverlayOps::format_date_from_system_time)
    }
}

impl ReplayAnalysisOps {
    fn days_in_month(year: i64, month: u32) -> Option<u32> {
        if !(1..=12).contains(&month) {
            return None;
        }

        let leap_year = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        Some(match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap_year => 29,
            2 => 28,
            _ => return None,
        })
    }
}

impl ReplayAnalysisOps {
    fn unix_seconds_from_ymdhms(
        year: i64,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Option<u64> {
        let max_day = ReplayAnalysisOps::days_in_month(year, month)?;
        if !(1..=max_day).contains(&day) || hour > 23 || minute > 59 || second > 59 {
            return None;
        }

        let adjusted_year = year - if month <= 2 { 1 } else { 0 };
        let era = if adjusted_year >= 0 {
            adjusted_year
        } else {
            adjusted_year - 399
        } / 400;
        let year_of_era = adjusted_year - era * 400;
        let adjusted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
        let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        let days_since_epoch = era * 146_097 + day_of_era - 719_468;
        if days_since_epoch < 0 {
            return None;
        }

        let seconds_since_epoch = days_since_epoch
            .checked_mul(86_400)?
            .checked_add(i64::from(hour) * 3_600)?
            .checked_add(i64::from(minute) * 60)?
            .checked_add(i64::from(second))?;
        u64::try_from(seconds_since_epoch).ok()
    }
}

impl ReplayAnalysisOps {
    pub fn parse_replay_timestamp_seconds(value: &str) -> Option<u64> {
        let parts = value
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 3 {
            return None;
        }

        let year = parts.first()?.parse::<i64>().ok()?;
        let month = parts.get(1)?.parse::<u32>().ok()?;
        let day = parts.get(2)?.parse::<u32>().ok()?;
        let hour = parts
            .get(3)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        let minute = parts
            .get(4)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        let second = parts
            .get(5)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);

        ReplayAnalysisOps::unix_seconds_from_ymdhms(year, month, day, hour, minute, second)
    }
}

impl ReplayAnalysisOps {
    fn query_date_boundary_seconds(path: &str, key: &str) -> Option<u64> {
        let value = TauriOverlayOps::parse_query_value(path, key)?;
        ReplayAnalysisOps::parse_replay_timestamp_seconds(&value)
    }
}

#[derive(Default)]
struct Aggregate {
    wins: u64,
    losses: u64,
}

#[derive(Default)]
struct RegionAggregate {
    wins: u64,
    losses: u64,
    max_asc: u64,
    max_com: HashSet<String>,
    prestiges: HashMap<String, u64>,
}

#[derive(Default)]
struct CommanderAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    mastery_counts: [f64; 6],
    mastery_distribution_counts: MasteryDistributionCounts,
    mastery_distribution_by_prestige_counts: MasteryDistributionByPrestigeCounts,
    mastery_by_prestige_counts: [[f64; 6]; 4],
    prestige_counts: [u64; 4],
    detailed_count: u64,
}

#[derive(Default)]
struct PlayerAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    last_seen: u64,
    handles: BTreeSet<String>,
    names: HashMap<String, u64>,
    commander: String,
    commander_counts: HashMap<String, u64>,
}

#[derive(Default)]
struct MapAggregate {
    wins: u64,
    losses: u64,
    victory_length_sum: f64,
    victory_games: u64,
    bonus_fraction_sum: f64,
    bonus_games: u64,
    fastest_length: f64,
    fastest_file: String,
    fastest_p1: String,
    fastest_p2: String,
    fastest_p1_handle: String,
    fastest_p2_handle: String,
    fastest_p1_commander: String,
    fastest_p2_commander: String,
    fastest_p1_apm: u64,
    fastest_p2_apm: u64,
    fastest_p1_mastery_level: u64,
    fastest_p2_mastery_level: u64,
    fastest_p1_masteries: Vec<u64>,
    fastest_p2_masteries: Vec<u64>,
    fastest_p1_prestige: u64,
    fastest_p2_prestige: u64,
    fastest_date: u64,
    fastest_difficulty: String,
    fastest_enemy_race: String,
    detailed_count: u64,
}

impl PlayerAggregate {
    fn record_replay(
        &mut self,
        player_name: &str,
        handle: &str,
        commander: &str,
        replay_is_victory: bool,
        apm: u64,
        kill_fraction: f64,
        replay_date: u64,
    ) {
        let sanitized_name = TauriOverlayOps::sanitize_replay_text(player_name);
        if !sanitized_name.is_empty() {
            self.names
                .entry(sanitized_name)
                .and_modify(|last_seen| *last_seen = (*last_seen).max(replay_date))
                .or_insert(replay_date);
        }
        let sanitized_handle = TauriOverlayOps::sanitize_replay_text(handle);
        if !sanitized_handle.is_empty() {
            self.handles.insert(sanitized_handle);
        }
        if !commander.is_empty() {
            self.commander = commander.to_string();
            self.commander_counts
                .entry(commander.to_string())
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }
        if replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
        self.apm_values.push(apm);
        self.kill_fractions.push(kill_fraction);
        if replay_date > self.last_seen {
            self.last_seen = replay_date;
        }
    }

    fn dominant_commander(&self) -> (String, f64) {
        let games = self.wins.saturating_add(self.losses);
        let Some((commander, count)) = self
            .commander_counts
            .iter()
            .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        else {
            return (TauriOverlayOps::sanitize_replay_text(&self.commander), 0.0);
        };

        (
            TauriOverlayOps::sanitize_replay_text(commander),
            TauriOverlayOps::ratio(*count, games),
        )
    }

    fn names_by_recency(&self) -> Vec<String> {
        let mut names = self
            .names
            .iter()
            .map(|(name, last_seen)| (name.clone(), *last_seen))
            .collect::<Vec<_>>();
        names.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        names.into_iter().map(|(name, _)| name).collect()
    }
}

impl ReplayAnalysisOps {
    fn wildcard_match(pattern: &str, value: &str) -> bool {
        let pattern_bytes = pattern.as_bytes();
        let value_bytes = value.as_bytes();
        let mut previous = vec![false; value_bytes.len() + 1];
        previous[0] = true;

        for &pattern_ch in pattern_bytes {
            let mut current = vec![false; value_bytes.len() + 1];
            if pattern_ch == b'*' {
                current[0] = previous[0];
            }

            for index in 1..=value_bytes.len() {
                current[index] = match pattern_ch {
                    b'*' => previous[index] || current[index - 1],
                    b'?' => previous[index - 1],
                    _ => previous[index - 1] && pattern_ch == value_bytes[index - 1],
                };
            }

            previous = current;
        }

        previous[value_bytes.len()]
    }
}

impl ReplayAnalysisOps {
    fn bonus_objective_total_for_canonical_map_with_dictionary(
        map_name: &str,
        dictionary: &Sc2DictionaryData,
    ) -> Option<u64> {
        dictionary.bonus_objectives.get(map_name).copied()
    }
}

impl ReplayAnalysisOps {
    pub fn bonus_objective_total_for_map_id_with_dictionary(
        map_id: &str,
        dictionary: &Sc2DictionaryData,
    ) -> Option<u64> {
        dictionary
            .coop_map_id_to_english(map_id)
            .as_deref()
            .and_then(|name| {
                ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(
                    name, dictionary,
                )
            })
    }
}

impl ReplayAnalysisOps {
    fn cache_json_value<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

impl ReplayAnalysisOps {
    fn cache_player(entry: &CacheReplayEntry, pid: u8) -> Option<&CachePlayer> {
        entry.players.iter().find(|player| player.pid == pid)
    }
}

impl ReplayAnalysisOps {
    fn cache_player_text(
        player: Option<&CachePlayer>,
        select: impl Fn(&CachePlayer) -> Option<&String>,
    ) -> String {
        player.and_then(select).cloned().unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn cache_player_u64(
        player: Option<&CachePlayer>,
        select: impl Fn(&CachePlayer) -> Option<u64>,
    ) -> u64 {
        player.and_then(select).unwrap_or(0)
    }
}

impl ReplayAnalysisOps {
    fn cache_player_masteries(player: Option<&CachePlayer>) -> Vec<u64> {
        player
            .and_then(|player| player.masteries)
            .map(|masteries| masteries.into_iter().map(u64::from).collect())
            .unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn cache_player_units(player: Option<&CachePlayer>) -> Value {
        let hidden_units = HashSet::new();
        ReplayAnalysisOps::cache_player_units_with_hidden_units(player, &hidden_units)
    }
}

impl ReplayAnalysisOps {
    fn cache_player_units_with_hidden_units(
        player: Option<&CachePlayer>,
        hidden_units: &HashSet<String>,
    ) -> Value {
        player
            .and_then(|player| player.units.as_ref())
            .map(
                |units: &std::collections::BTreeMap<String, CacheUnitStats>| {
                    ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(
                        ReplayAnalysisOps::cache_json_value(units),
                        hidden_units,
                    )
                },
            )
            .unwrap_or_else(|| Value::Object(Default::default()))
    }
}

impl ReplayAnalysisOps {
    fn cache_player_icons(player: Option<&CachePlayer>) -> Value {
        player
            .and_then(|player| player.icons.as_ref())
            .map(
                |icons: &std::collections::BTreeMap<String, CacheIconValue>| {
                    ReplayAnalysisOps::cache_json_value(icons)
                },
            )
            .unwrap_or_else(|| Value::Object(Default::default()))
    }
}

impl ReplayAnalysisOps {
    fn replay_chat_messages_from_cache(messages: &[ReplayMessage]) -> Vec<ReplayChatMessage> {
        messages
            .iter()
            .map(|message| ReplayChatMessage {
                player: message.player,
                text: message.text.clone(),
                time: message.time,
            })
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn replay_chat_messages_from_report(
        messages: &[ParsedReplayMessage],
    ) -> Vec<ReplayChatMessage> {
        messages
            .iter()
            .map(|message| ReplayChatMessage {
                player: message.player,
                text: message.text.clone(),
                time: message.time,
            })
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn temp_cache_path(cache_path: &Path) -> PathBuf {
        cache_path.with_extension("temp.jsonl")
    }
}

impl ReplayAnalysisOps {
    fn load_temp_cache_entries(temp_path: &Path) -> Vec<CacheReplayEntry> {
        let content = match std::fs::read_to_string(temp_path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to read temp cache '{}': {error}",
                    temp_path.display()
                );
                return Vec::new();
            }
        };

        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }

                match serde_json::from_str::<CacheReplayEntry>(trimmed) {
                    Ok(entry) if !entry.hash.is_empty() => Some(entry),
                    Ok(_) => None,
                    Err(error) => {
                        crate::sco_log!(
                            "[SCO/cache] failed to parse temp cache entry in '{}': {error}",
                            temp_path.display()
                        );
                        None
                    }
                }
            })
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn read_cache_entries(cache_path: &Path, log_label: &str) -> Vec<CacheReplayEntry> {
        let payload = match std::fs::read(cache_path) {
            Ok(payload) => payload,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to read {log_label} '{}': {error}",
                    cache_path.display()
                );
                return Vec::new();
            }
        };

        match serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload) {
            Ok(entries) => entries,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to parse {log_label} '{}': {error}",
                    cache_path.display()
                );
                Vec::new()
            }
        }
    }
}

impl ReplayAnalysisOps {
    fn recover_cache_entries_from_temp(
        cache_path: &Path,
        log_label: &str,
    ) -> Vec<CacheReplayEntry> {
        let mut merged = ReplayAnalysisOps::read_cache_entries(cache_path, log_label)
            .into_iter()
            .filter(|entry| !entry.hash.is_empty())
            .map(|entry| (entry.hash.clone(), entry))
            .collect::<HashMap<_, _>>();
        let temp_path = ReplayAnalysisOps::temp_cache_path(cache_path);
        let temp_entries = ReplayAnalysisOps::load_temp_cache_entries(&temp_path);
        if temp_entries.is_empty() {
            return merged.into_values().collect();
        }

        for entry in temp_entries {
            merged.retain(|hash, existing| hash == &entry.hash || existing.file != entry.file);
            match merged.get(&entry.hash) {
                Some(existing)
                    if ReplayInfo::should_keep_existing_detailed_variant(
                        existing.detailed_analysis,
                        entry.detailed_analysis,
                    ) => {}
                _ => {
                    merged.insert(entry.hash.clone(), entry);
                }
            }
        }

        let mut entries = merged.into_values().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .date
                .cmp(&left.date)
                .then_with(|| right.file.cmp(&left.file))
        });

        if let Err(error) = CacheReplayEntry::write_entries(&entries, cache_path) {
            crate::sco_log!(
                "[SCO/cache] failed to persist recovered cache '{}': {error}",
                cache_path.display()
            );
        } else {
            if let Err(error) = CacheOverallStatsFile::write_pretty_cache_file(
                cache_path,
                Some(&CacheOverallStatsFile::pretty_output_path(cache_path)),
            ) {
                crate::sco_log!(
                    "[SCO/cache] failed to update pretty cache '{}': {error}",
                    cache_path.display()
                );
            }
            if let Err(error) = std::fs::remove_file(&temp_path) {
                crate::sco_log!(
                    "[SCO/cache] failed to remove recovered temp cache '{}': {error}",
                    temp_path.display()
                );
            }
        }

        entries
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ReplayUnitCountValue {
    #[default]
    Missing,
    Number(i64),
    Hidden,
}

impl ReplayUnitCountValue {
    fn is_explicit_zero(self) -> bool {
        matches!(self, Self::Number(0))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReplayUnitRow {
    created: ReplayUnitCountValue,
    lost: ReplayUnitCountValue,
    kills: i64,
}

impl ReplayAnalysisOps {
    fn replay_unit_count_value(value: Option<&Value>) -> ReplayUnitCountValue {
        value
            .and_then(Value::as_i64)
            .map(ReplayUnitCountValue::Number)
            .or_else(|| {
                value
                    .and_then(Value::as_f64)
                    .filter(|entry| entry.is_finite())
                    .map(|entry| ReplayUnitCountValue::Number(entry.round() as i64))
            })
            .or_else(|| {
                value
                    .filter(|entry| entry.is_string())
                    .map(|_| ReplayUnitCountValue::Hidden)
            })
            .unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn numeric_unit_stat_value(value: Option<&Value>) -> i64 {
        match ReplayAnalysisOps::replay_unit_count_value(value) {
            ReplayUnitCountValue::Number(number) => number,
            ReplayUnitCountValue::Missing | ReplayUnitCountValue::Hidden => 0,
        }
    }
}

impl ReplayAnalysisOps {
    fn replay_unit_row(row: &[Value]) -> ReplayUnitRow {
        ReplayUnitRow {
            created: ReplayAnalysisOps::replay_unit_count_value(row.first()),
            lost: ReplayAnalysisOps::replay_unit_count_value(row.get(1)),
            kills: ReplayAnalysisOps::numeric_unit_stat_value(row.get(2)),
        }
    }
}

impl ReplayAnalysisOps {
    fn apply_replay_unit_count(target: &mut i64, hidden: &mut bool, value: ReplayUnitCountValue) {
        match value {
            ReplayUnitCountValue::Number(number) if !*hidden => {
                *target = target.saturating_add(number);
            }
            ReplayUnitCountValue::Hidden => {
                *hidden = true;
            }
            ReplayUnitCountValue::Missing | ReplayUnitCountValue::Number(_) => {}
        }
    }
}

impl ReplayAnalysisOps {
    pub fn append_units_to_rollup_with_dictionary(
        side_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
        dictionary: &Sc2DictionaryData,
    ) {
        let commander = TauriOverlayOps::sanitize_replay_text(commander_name);
        if commander.trim().is_empty() {
            return;
        }
        let Some(units) = units_payload.as_object() else {
            return;
        };

        let commander_entry = side_rollup.entry(commander.clone()).or_default();
        commander_entry.count = commander_entry.count.saturating_add(1);

        let mut replay_units: Vec<(String, ReplayUnitRow)> = Vec::new();
        for (unit_name, row) in units {
            let Some(values) = row.as_array() else {
                continue;
            };
            replay_units.push((
                TauriOverlayOps::sanitize_replay_text(unit_name),
                ReplayAnalysisOps::replay_unit_row(values),
            ));
        }

        let mc_unit = dictionary.commander_mind_control_unit(&commander);
        let mut mc_unit_bonus_kills = 0_i64;
        if let Some(mc_unit_name) = mc_unit {
            if replay_units.iter().any(|(unit, _)| unit == mc_unit_name) {
                for (unit, row) in &replay_units {
                    if row.created.is_explicit_zero()
                        || (commander != "Fenix" && unit == "Disruptor")
                        || (commander != "Tychus" && unit == "Auto-Turret")
                    {
                        mc_unit_bonus_kills = mc_unit_bonus_kills.saturating_add(row.kills);
                    }
                }
            }
        }

        for (unit, row) in replay_units {
            let is_mc_bonus_target = mc_unit == Some(unit.as_str());
            let entry = commander_entry.units.entry(unit.clone()).or_default();
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.created,
                &mut entry.created_hidden,
                row.created,
            );
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.lost,
                &mut entry.lost_hidden,
                row.lost,
            );
            entry.kills = entry.kills.saturating_add(row.kills);
            if !matches!(row.created, ReplayUnitCountValue::Hidden) || commander == "Tychus" {
                entry.made = entry.made.saturating_add(1);
            }

            if mc_unit_bonus_kills > 0 && is_mc_bonus_target {
                entry.kills = entry.kills.saturating_add(mc_unit_bonus_kills);
                let kills_in_game = row.kills.saturating_add(mc_unit_bonus_kills);
                if player_kills > 0 {
                    entry
                        .kill_percentages
                        .push(kills_in_game as f64 / player_kills as f64);
                } else {
                    entry.kill_percentages.push(1.0);
                }
                mc_unit_bonus_kills = 0;
            } else if player_kills > 0 {
                entry
                    .kill_percentages
                    .push(row.kills as f64 / player_kills as f64);
            }
        }
    }
}

impl ReplayAnalysisOps {
    pub fn append_units_to_rollup(
        side_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
    ) {
        let commander = TauriOverlayOps::sanitize_replay_text(commander_name);
        if commander.trim().is_empty() {
            return;
        }
        let Some(units) = units_payload.as_object() else {
            return;
        };

        let commander_entry = side_rollup.entry(commander.clone()).or_default();
        commander_entry.count = commander_entry.count.saturating_add(1);

        for (unit_name, row) in units {
            let Some(values) = row.as_array() else {
                continue;
            };
            let row = ReplayAnalysisOps::replay_unit_row(values);
            let entry = commander_entry
                .units
                .entry(TauriOverlayOps::sanitize_replay_text(unit_name))
                .or_default();
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.created,
                &mut entry.created_hidden,
                row.created,
            );
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.lost,
                &mut entry.lost_hidden,
                row.lost,
            );
            entry.kills = entry.kills.saturating_add(row.kills);
            if !matches!(row.created, ReplayUnitCountValue::Hidden) || commander == "Tychus" {
                entry.made = entry.made.saturating_add(1);
            }
            if player_kills > 0 {
                entry
                    .kill_percentages
                    .push(row.kills as f64 / player_kills as f64);
            }
        }
    }
}

impl ReplayAnalysisOps {
    pub fn append_player_units_to_rollups_with_dictionary(
        main_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
        player_handle: &str,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) {
        if ReplayAnalysis::is_main_player_by_handle(player_handle, main_handles) {
            ReplayAnalysisOps::append_units_to_rollup_with_dictionary(
                main_rollup,
                commander_name,
                units_payload,
                player_kills,
                dictionary,
            );
        } else {
            ReplayAnalysisOps::append_units_to_rollup_with_dictionary(
                ally_rollup,
                commander_name,
                units_payload,
                player_kills,
                dictionary,
            );
        }
    }
}

impl ReplayAnalysisOps {
    pub fn append_player_units_to_rollups(
        main_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
        player_handle: &str,
        main_handles: &HashSet<String>,
    ) {
        if ReplayAnalysis::is_main_player_by_handle(player_handle, main_handles) {
            ReplayAnalysisOps::append_units_to_rollup(
                main_rollup,
                commander_name,
                units_payload,
                player_kills,
            );
        } else {
            ReplayAnalysisOps::append_units_to_rollup(
                ally_rollup,
                commander_name,
                units_payload,
                player_kills,
            );
        }
    }
}

impl ReplayAnalysisOps {
    pub fn replay_info_from_cache_entry_with_dictionary(
        entry: &CacheReplayEntry,
        dictionary: &Sc2DictionaryData,
    ) -> ReplayInfo {
        let player_one = ReplayAnalysisOps::cache_player(entry, 1);
        let player_two = ReplayAnalysisOps::cache_player(entry, 2);
        let hidden_units = ReplayAnalysisOps::hidden_unit_stats_names_with_dictionary(dictionary);
        let slot1 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_one, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_one, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_one),
            units: ReplayAnalysisOps::cache_player_units_with_hidden_units(
                player_one,
                &hidden_units,
            ),
            icons: ReplayAnalysisOps::cache_player_icons(player_one),
        };
        let slot2 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_two, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_two, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_two),
            units: ReplayAnalysisOps::cache_player_units_with_hidden_units(
                player_two,
                &hidden_units,
            ),
            icons: ReplayAnalysisOps::cache_player_icons(player_two),
        };
        let normalized_mutators = entry
            .mutators
            .iter()
            .map(|mutator| {
                ReplayAnalysisOps::normalize_mutator_id_with_dictionary(mutator, dictionary)
            })
            .collect::<Vec<_>>();
        let weekly_name = if entry.weekly {
            ReplayAnalysisOps::resolve_weekly_mutation_name_with_dictionary(
                &entry.map_name,
                &normalized_mutators,
                dictionary,
            )
        } else {
            None
        };
        let bonus_total = dictionary
            .canonicalize_coop_map_id(&entry.map_name)
            .as_deref()
            .and_then(|map_id| dictionary.coop_map_id_to_english(map_id))
            .as_deref()
            .and_then(|map_name| {
                ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(
                    map_name, dictionary,
                )
            });
        let file_path = Path::new(&entry.file);
        let accurate_length = ReplayAnalysisOps::accurate_length_seconds_from_cache(
            &entry.accurate_length,
            entry.length,
        );
        let difficulty = if !entry.ext_difficulty.trim().is_empty() {
            entry.ext_difficulty.trim().to_string()
        } else if !entry.difficulty.1.trim().is_empty() {
            entry.difficulty.1.trim().to_string()
        } else if !entry.difficulty.0.trim().is_empty() {
            entry.difficulty.0.trim().to_string()
        } else {
            "Unknown".to_string()
        };

        ReplayInfo {
            file: entry.file.clone(),
            date: ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date)
                .unwrap_or_else(|| ReplayAnalysisOps::file_modified_seconds(file_path)),
            map: dictionary
                .canonicalize_coop_map_id(&entry.map_name)
                .unwrap_or_else(|| entry.map_name.clone()),
            result: entry.result.clone(),
            difficulty,
            enemy: entry
                .enemy_race
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string()),
            length: ReplayAnalysisOps::display_length_seconds(accurate_length),
            accurate_length,
            slot1,
            slot2,
            main_slot: 0,
            amon_units: entry
                .amon_units
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            player_stats: entry
                .player_stats
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            extension: entry.extension,
            brutal_plus: u64::from(entry.brutal_plus),
            weekly: entry.weekly,
            weekly_name,
            mutators: normalized_mutators,
            comp: entry
                .comp
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Unidentified AI".to_string()),
            bonus: entry
                .bonus
                .as_ref()
                .map(|bonus| vec![1; bonus.len()])
                .unwrap_or_default(),
            bonus_total,
            messages: ReplayAnalysisOps::replay_chat_messages_from_cache(&entry.messages),
            is_detailed: entry.detailed_analysis,
        }
    }
}

impl ReplayAnalysisOps {
    pub fn replay_info_from_cache_entry(entry: &CacheReplayEntry) -> ReplayInfo {
        let player_one = ReplayAnalysisOps::cache_player(entry, 1);
        let player_two = ReplayAnalysisOps::cache_player(entry, 2);
        let slot1 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_one, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_one, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_one),
            units: ReplayAnalysisOps::cache_player_units(player_one),
            icons: ReplayAnalysisOps::cache_player_icons(player_one),
        };
        let slot2 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_two, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_two, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_two),
            units: ReplayAnalysisOps::cache_player_units(player_two),
            icons: ReplayAnalysisOps::cache_player_icons(player_two),
        };
        let file_path = Path::new(&entry.file);
        let accurate_length = ReplayAnalysisOps::accurate_length_seconds_from_cache(
            &entry.accurate_length,
            entry.length,
        );
        let difficulty = if !entry.ext_difficulty.trim().is_empty() {
            entry.ext_difficulty.trim().to_string()
        } else if !entry.difficulty.1.trim().is_empty() {
            entry.difficulty.1.trim().to_string()
        } else if !entry.difficulty.0.trim().is_empty() {
            entry.difficulty.0.trim().to_string()
        } else {
            "Unknown".to_string()
        };

        ReplayInfo {
            file: entry.file.clone(),
            date: ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date)
                .unwrap_or_else(|| ReplayAnalysisOps::file_modified_seconds(file_path)),
            map: entry.map_name.clone(),
            result: entry.result.clone(),
            difficulty,
            enemy: entry
                .enemy_race
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string()),
            length: ReplayAnalysisOps::display_length_seconds(accurate_length),
            accurate_length,
            slot1,
            slot2,
            main_slot: 0,
            amon_units: entry
                .amon_units
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            player_stats: entry
                .player_stats
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            extension: entry.extension,
            brutal_plus: u64::from(entry.brutal_plus),
            weekly: entry.weekly,
            weekly_name: None,
            mutators: entry.mutators.clone(),
            comp: entry
                .comp
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Unidentified AI".to_string()),
            bonus: entry
                .bonus
                .as_ref()
                .map(|bonus| vec![1; bonus.len()])
                .unwrap_or_default(),
            bonus_total: None,
            messages: ReplayAnalysisOps::replay_chat_messages_from_cache(&entry.messages),
            is_detailed: entry.detailed_analysis,
        }
    }
}

impl ReplayAnalysisOps {
    fn replay_info_from_report_with_dictionary(
        path: &Path,
        report: &ReplayReport,
        dictionary: &Sc2DictionaryData,
    ) -> ReplayInfo {
        let hidden_units = ReplayAnalysisOps::hidden_unit_stats_names_with_dictionary(dictionary);
        let normalized_mutators = report
            .mutators
            .iter()
            .map(|mutator| {
                ReplayAnalysisOps::normalize_mutator_id_with_dictionary(mutator, dictionary)
            })
            .collect::<Vec<_>>();
        let weekly_name = if report.weekly {
            ReplayAnalysisOps::resolve_weekly_mutation_name_with_dictionary(
                &report.map_name,
                &normalized_mutators,
                dictionary,
            )
        } else {
            None
        };
        let bonus_total = dictionary
            .canonicalize_coop_map_id(&report.map_name)
            .as_deref()
            .and_then(|map_id| dictionary.coop_map_id_to_english(map_id))
            .as_deref()
            .and_then(|map_name| {
                ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(
                    map_name, dictionary,
                )
            });
        let slot1_player = ReplayAnalysisOps::report_player(report, 1);
        let slot2_player = ReplayAnalysisOps::report_player(report, 2);
        let accurate_length =
            if report.parser.accurate_length.is_finite() && report.parser.accurate_length > 0.0 {
                report.parser.accurate_length
            } else {
                report.length.max(0.0)
            };
        let main_slot = match report.positions.main {
            2 => 1,
            _ => 0,
        };
        let slot_player = |slot_index: usize,
                           player: Option<&ParsedReplayPlayer>,
                           commander: &str,
                           commander_level: u64,
                           mastery_level: u64,
                           prestige: u64,
                           masteries: Vec<u64>,
                           units: Value,
                           icons: Value,
                           kills: u64|
         -> ReplayPlayerInfo {
            let fallback_name = if slot_index == 0 {
                report.main.clone()
            } else {
                report.ally.clone()
            };
            ReplayPlayerInfo {
                name: player
                    .map(|value| value.name.clone())
                    .unwrap_or_else(|| fallback_name),
                handle: player.map(|value| value.handle.clone()).unwrap_or_default(),
                apm: player.map(|value| u64::from(value.apm)).unwrap_or(0),
                kills,
                commander: player
                    .map(|value| value.commander.clone())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| commander.to_string()),
                commander_level: player
                    .map(|value| u64::from(value.commander_level))
                    .unwrap_or(commander_level),
                mastery_level: player
                    .map(|value| u64::from(value.commander_mastery_level))
                    .unwrap_or(mastery_level),
                prestige: player
                    .map(|value| u64::from(value.prestige))
                    .unwrap_or(prestige),
                masteries: player
                    .map(|value| {
                        value
                            .masteries
                            .iter()
                            .map(|entry| u64::from(*entry))
                            .collect()
                    })
                    .unwrap_or(masteries),
                units,
                icons,
            }
        };
        let slot1_is_main = main_slot == 0;
        let slot1 = slot_player(
            0,
            slot1_player,
            if slot1_is_main {
                &report.main_commander
            } else {
                &report.ally_commander
            },
            if slot1_is_main {
                u64::from(report.main_commander_level)
            } else {
                u64::from(report.ally_commander_level)
            },
            slot1_player
                .map(|value| u64::from(value.commander_mastery_level))
                .unwrap_or(0),
            slot1_player
                .map(|value| u64::from(value.prestige))
                .unwrap_or(0),
            if slot1_is_main {
                report
                    .main_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            } else {
                report
                    .ally_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            },
            ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.main_units
                } else {
                    &report.ally_units
                }),
                &hidden_units,
            ),
            ReplayAnalysisOps::with_outlaw_icons(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.main_icons
                } else {
                    &report.ally_icons
                }),
                if slot1_is_main {
                    &report.main_commander
                } else {
                    &report.ally_commander
                },
                if (if slot1_is_main {
                    &report.main_commander
                } else {
                    &report.ally_commander
                }) == "Tychus"
                {
                    report.outlaw_order.as_ref()
                } else {
                    None
                },
            ),
            if slot1_is_main {
                report.main_kills
            } else {
                report.ally_kills
            },
        );
        let slot2 = slot_player(
            1,
            slot2_player,
            if slot1_is_main {
                &report.ally_commander
            } else {
                &report.main_commander
            },
            if slot1_is_main {
                u64::from(report.ally_commander_level)
            } else {
                u64::from(report.main_commander_level)
            },
            slot2_player
                .map(|value| u64::from(value.commander_mastery_level))
                .unwrap_or(0),
            slot2_player
                .map(|value| u64::from(value.prestige))
                .unwrap_or(0),
            if slot1_is_main {
                report
                    .ally_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            } else {
                report
                    .main_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            },
            ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.ally_units
                } else {
                    &report.main_units
                }),
                &hidden_units,
            ),
            ReplayAnalysisOps::with_outlaw_icons(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.ally_icons
                } else {
                    &report.main_icons
                }),
                if slot1_is_main {
                    &report.ally_commander
                } else {
                    &report.main_commander
                },
                if (if slot1_is_main {
                    &report.ally_commander
                } else {
                    &report.main_commander
                }) == "Tychus"
                {
                    report.outlaw_order.as_ref()
                } else {
                    None
                },
            ),
            if slot1_is_main {
                report.ally_kills
            } else {
                report.main_kills
            },
        );

        ReplayInfo {
            file: path.display().to_string(),
            date: ReplayAnalysisOps::parse_replay_timestamp_seconds(&report.parser.date)
                .unwrap_or_else(|| ReplayAnalysisOps::file_modified_seconds(path)),
            map: dictionary
                .canonicalize_coop_map_id(&report.map_name)
                .unwrap_or_else(|| report.map_name.clone()),
            result: report.result.clone(),
            difficulty: report.difficulty.clone(),
            enemy: if report.parser.enemy_race.trim().is_empty() {
                "Unknown".to_string()
            } else {
                report.parser.enemy_race.clone()
            },
            length: ReplayAnalysisOps::display_length_seconds(accurate_length),
            accurate_length,
            slot1,
            slot2,
            main_slot,
            amon_units: ReplayAnalysisOps::report_value(&report.amon_units),
            player_stats: ReplayAnalysisOps::report_value(&report.player_stats),
            extension: report.extension,
            brutal_plus: u64::from(report.brutal_plus),
            weekly: report.weekly,
            weekly_name,
            mutators: normalized_mutators,
            comp: report.comp.clone(),
            bonus: vec![1; report.bonus.len()],
            bonus_total,
            messages: ReplayAnalysisOps::replay_chat_messages_from_report(&report.parser.messages),
            is_detailed: true,
        }
    }
}

impl ReplayAnalysisOps {
    fn unparsed_replay(path: &Path) -> ReplayInfo {
        ReplayInfo {
            file: path.display().to_string(),
            date: ReplayAnalysisOps::file_modified_seconds(path),
            map: "Unknown map".to_string(),
            result: "Unparsed".to_string(),
            difficulty: "Unknown".to_string(),
            enemy: "Unknown".to_string(),
            comp: "Unidentified AI".to_string(),
            accurate_length: 0.0,
            ..ReplayInfo::default()
        }
    }
}

pub struct ReplayAnalysis;

impl ReplayAnalysis {
    pub fn normalized_player_key(name: &str) -> String {
        TauriOverlayOps::sanitize_replay_text(name)
            .trim()
            .to_ascii_lowercase()
    }

    pub fn normalized_handle_key(handle: &str) -> String {
        let normalized = TauriOverlayOps::sanitize_replay_text(handle)
            .trim()
            .to_ascii_lowercase();
        if normalized.contains("-s2-") {
            normalized
        } else {
            String::new()
        }
    }

    pub(crate) fn is_main_player_by_name(
        player_name: &str,
        main_names: &std::collections::HashSet<String>,
    ) -> bool {
        if main_names.is_empty() {
            return false;
        }
        let normalized = Self::normalized_player_key(player_name);
        !normalized.is_empty() && main_names.contains(&normalized)
    }

    pub(crate) fn is_main_player_by_handle(
        player_handle: &str,
        main_handles: &std::collections::HashSet<String>,
    ) -> bool {
        if main_handles.is_empty() {
            return false;
        }
        let normalized = Self::normalized_handle_key(player_handle);
        !normalized.is_empty() && main_handles.contains(&normalized)
    }

    pub(crate) fn is_main_player_identity(
        player_name: &str,
        player_handle: &str,
        main_names: &std::collections::HashSet<String>,
        main_handles: &std::collections::HashSet<String>,
    ) -> bool {
        Self::is_main_player_by_handle(player_handle, main_handles)
            || Self::is_main_player_by_name(player_name, main_names)
    }

    pub fn rebuild_analysis_payload<R>(replays: &[R], include_detailed: bool) -> Value
    where
        R: Borrow<ReplayInfo>,
    {
        let (main_names, main_handles) = ReplayAnalysisOps::default_main_identity();
        Self::rebuild_analysis_payload_with_identity(
            replays,
            include_detailed,
            &main_names,
            &main_handles,
        )
    }

    pub fn rebuild_analysis_payload_with_identity<R>(
        replays: &[R],
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Value
    where
        R: Borrow<ReplayInfo>,
    {
        let dictionary = Sc2DictionaryData::default();
        Self::rebuild_analysis_payload_with_dictionary(
            replays,
            include_detailed,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn rebuild_analysis_payload_with_dictionary<R>(
        replays: &[R],
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Value
    where
        R: Borrow<ReplayInfo>,
    {
        #[derive(Serialize)]
        struct FastestMapDetails {
            length: f64,
            file: String,
            date: u64,
            difficulty: String,
            players: Vec<Value>,
            enemy_race: String,
        }

        #[derive(Serialize)]
        struct MapDataRow {
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
            fastest: FastestMapDetails,
        }

        #[derive(Serialize)]
        struct CommanderDataRow {
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
        struct DifficultyDataRow {
            #[serde(rename = "Victory")]
            victory: u64,
            #[serde(rename = "Defeat")]
            defeat: u64,
            #[serde(rename = "Winrate")]
            winrate: f64,
        }

        #[derive(Serialize)]
        struct RegionDataRow {
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
        struct PlayerDataRow {
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
        struct UnitDataPayload {
            main: Value,
            ally: Value,
            amon: Value,
        }

        #[derive(Serialize)]
        struct AnalysisPayload {
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

        #[derive(Serialize)]
        struct RebuildAnalysisPayload {
            analysis: Value,
            prestige_names: std::collections::BTreeMap<String, LocalizedLabels>,
        }

        let started_at = Instant::now();
        crate::sco_log!(
            "[SCO/stats] rebuild_analysis_payload start include_detailed={} replays={}",
            include_detailed,
            replays.len()
        );

        let mut map_values: std::collections::BTreeMap<String, MapAggregate> =
            std::collections::BTreeMap::new();
        let mut main_commander: std::collections::BTreeMap<String, CommanderAggregate> =
            std::collections::BTreeMap::new();
        let mut ally_commander: std::collections::BTreeMap<String, CommanderAggregate> =
            std::collections::BTreeMap::new();
        let mut region_values: std::collections::BTreeMap<String, RegionAggregate> =
            std::collections::BTreeMap::new();
        let mut difficulty_values: std::collections::BTreeMap<String, Aggregate> =
            std::collections::BTreeMap::new();
        let mut player_values: std::collections::BTreeMap<String, PlayerAggregate> =
            std::collections::BTreeMap::new();

        let mut invalid_result = 0u64;
        let mut sum_main_wins = 0u64;
        let mut sum_main_losses = 0u64;
        let mut _sum_ally_wins = 0u64;
        let mut _sum_ally_losses = 0u64;
        let mut sum_main_apm: Vec<u64> = Vec::new();
        let mut sum_main_kill_fraction: Vec<f64> = Vec::new();
        let mut sum_ally_apm: Vec<u64> = Vec::new();
        let mut sum_ally_kill_fraction: Vec<f64> = Vec::new();
        let mut sum_main_mastery_counts = [0f64; 6];
        let mut sum_ally_mastery_counts = [0f64; 6];
        let mut sum_main_mastery_distribution_counts =
            ReplayAnalysisOps::empty_mastery_distribution_counts();
        let mut sum_ally_mastery_distribution_counts =
            ReplayAnalysisOps::empty_mastery_distribution_counts();
        let mut sum_main_mastery_distribution_by_prestige_counts =
            ReplayAnalysisOps::empty_mastery_distribution_by_prestige_counts();
        let mut sum_ally_mastery_distribution_by_prestige_counts =
            ReplayAnalysisOps::empty_mastery_distribution_by_prestige_counts();
        let mut sum_main_mastery_by_prestige_counts = [[0f64; 6]; 4];
        let mut sum_ally_mastery_by_prestige_counts = [[0f64; 6]; 4];
        let mut sum_main_prestige_counts = [0u64; 4];
        let mut sum_ally_prestige_counts = [0u64; 4];

        let total_scanned = replays.len() as u64;
        let has_known_main_handles = !main_handles.is_empty();
        let mut considered_games = 0u64;
        for replay in replays.iter().map(Borrow::borrow) {
            if replay.result == "Unparsed" {
                continue;
            }
            let Some(map_key) = dictionary.canonicalize_coop_map_id(&replay.map) else {
                continue;
            };
            let main_player_name = TauriOverlayOps::sanitize_replay_text(&replay.main().name);
            let ally_player_name = TauriOverlayOps::sanitize_replay_text(&replay.ally().name);
            let main_commander_text =
                TauriOverlayOps::sanitize_replay_text(replay.main_commander());
            let ally_commander_text =
                TauriOverlayOps::sanitize_replay_text(replay.ally_commander());
            let map_bonus_total = replay.bonus_total.or_else(|| {
                ReplayAnalysisOps::bonus_objective_total_for_map_id_with_dictionary(
                    &map_key, dictionary,
                )
            });

            let replay_is_victory = match TauriOverlayOps::result_is_victory(&replay.result) {
                Some(result) => result,
                None => {
                    invalid_result += 1;
                    if invalid_result <= 5 {
                        crate::sco_log!(
                            "[SCO/stats] unrecognized result for {:?}: {}",
                            replay.file,
                            replay.result
                        );
                    }
                    continue;
                }
            };

            let main_kill_fraction =
                TauriOverlayOps::kill_fraction(replay.main_kills(), replay.ally_kills());
            let ally_kill_fraction = 1.0 - main_kill_fraction;
            let main_commander_name =
                TauriOverlayOps::normalized_commander_name(&main_commander_text, &main_player_name);
            let ally_commander_name =
                TauriOverlayOps::normalized_commander_name(&ally_commander_text, &ally_player_name);

            if main_commander_name.is_empty() || ally_commander_name.is_empty() {
                invalid_result += 1;
                continue;
            }
            considered_games += 1;

            let map_entry = map_values.entry(map_key).or_insert_with(|| MapAggregate {
                fastest_length: f64::INFINITY,
                ..Default::default()
            });

            if replay.is_detailed {
                map_entry.detailed_count += 1;
            }

            if replay_is_victory {
                map_entry.victory_length_sum += replay.accurate_length;
                map_entry.victory_games += 1;

                if replay.is_detailed {
                    if let Some(total) = map_bonus_total {
                        if total > 0 {
                            let completed = (replay.bonus.len() as u64).min(total);
                            map_entry.bonus_fraction_sum += completed as f64 / total as f64;
                            map_entry.bonus_games += 1;
                        }
                    }
                }

                let has_no_fastest = !map_entry.fastest_length.is_finite();
                let is_faster = replay.accurate_length < map_entry.fastest_length;
                let is_same_fastest_time =
                    (replay.accurate_length - map_entry.fastest_length).abs() < f64::EPSILON;
                let is_older_tied_fastest = is_same_fastest_time
                    && replay.date > 0
                    && (map_entry.fastest_date == 0 || replay.date < map_entry.fastest_date);
                if has_no_fastest || is_faster || is_older_tied_fastest {
                    map_entry.fastest_length = replay.accurate_length;
                    map_entry.fastest_file = replay.file.clone();
                    map_entry.fastest_date = replay.date;
                    map_entry.fastest_difficulty = replay.difficulty.clone();
                    map_entry.fastest_enemy_race = replay.enemy.clone();
                    map_entry.fastest_p1 = replay.main().name.clone();
                    map_entry.fastest_p2 = replay.ally().name.clone();
                    map_entry.fastest_p1_handle = replay.main().handle.clone();
                    map_entry.fastest_p2_handle = replay.ally().handle.clone();
                    map_entry.fastest_p1_commander = main_commander_name.clone();
                    map_entry.fastest_p2_commander = ally_commander_name.clone();
                    map_entry.fastest_p1_apm = replay.main_apm();
                    map_entry.fastest_p2_apm = replay.ally_apm();
                    map_entry.fastest_p1_mastery_level = replay.main_mastery_level();
                    map_entry.fastest_p2_mastery_level = replay.ally_mastery_level();
                    map_entry.fastest_p1_masteries = replay.main_masteries().to_vec();
                    map_entry.fastest_p2_masteries = replay.ally_masteries().to_vec();
                    map_entry.fastest_p1_prestige = replay.main_prestige();
                    map_entry.fastest_p2_prestige = replay.ally_prestige();
                }
            }
            if replay_is_victory {
                map_entry.wins += 1;
            } else {
                map_entry.losses += 1;
            }

            let normalized_p1_handle = Self::normalized_handle_key(&replay.main().handle);
            let normalized_p2_handle = Self::normalized_handle_key(&replay.ally().handle);
            let mut p1_is_main = if has_known_main_handles {
                !normalized_p1_handle.is_empty() && main_handles.contains(&normalized_p1_handle)
            } else {
                true
            };
            let p2_is_main = if has_known_main_handles {
                !normalized_p2_handle.is_empty() && main_handles.contains(&normalized_p2_handle)
            } else {
                false
            };
            if has_known_main_handles && !p1_is_main && !p2_is_main {
                p1_is_main = true;
            }

            let region = if p1_is_main {
                TauriOverlayOps::infer_region_from_handle(&replay.main().handle)
            } else if p2_is_main {
                TauriOverlayOps::infer_region_from_handle(&replay.ally().handle)
            } else {
                TauriOverlayOps::infer_region_from_handle(&replay.main().handle)
                    .or_else(|| TauriOverlayOps::infer_region_from_handle(&replay.ally().handle))
            }
            .unwrap_or_else(|| "Unknown".to_string());
            let replay_difficulty = replay.difficulty.trim();
            let difficulty = if replay.brutal_plus > 0 {
                let level = u8::try_from(replay.brutal_plus).unwrap_or(0).clamp(1, 6);
                format!("B+{}", level)
            } else if replay_difficulty.eq_ignore_ascii_case("Brutal+") {
                "Brutal+".to_string()
            } else if replay_difficulty.is_empty() {
                "Unknown".to_string()
            } else {
                replay_difficulty.to_string()
            };
            let region_entry = region_values.entry(region).or_default();
            if replay_is_victory {
                region_entry.wins += 1;
            } else {
                region_entry.losses += 1;
            }
            if p1_is_main {
                if replay.main_mastery_level() > region_entry.max_asc {
                    region_entry.max_asc = replay.main_mastery_level();
                }
                if replay.main_commander_level() == 15 && !main_commander_text.is_empty() {
                    region_entry.max_com.insert(main_commander_text.clone());
                }
                if !main_commander_name.is_empty() {
                    let value = replay.main_prestige().min(3);
                    region_entry
                        .prestiges
                        .entry(main_commander_name.clone())
                        .and_modify(|current| *current = (*current).max(value))
                        .or_insert(value);
                }
            }
            if p2_is_main {
                if replay.ally_mastery_level() > region_entry.max_asc {
                    region_entry.max_asc = replay.ally_mastery_level();
                }
                if replay.ally_commander_level() == 15 && !ally_commander_text.is_empty() {
                    region_entry.max_com.insert(ally_commander_text.clone());
                }
                if !ally_commander_name.is_empty() {
                    let value = replay.ally_prestige().min(3);
                    region_entry
                        .prestiges
                        .entry(ally_commander_name.clone())
                        .and_modify(|current| *current = (*current).max(value))
                        .or_insert(value);
                }
            }

            if !difficulty.contains('/') {
                let diff_entry = difficulty_values.entry(difficulty).or_default();
                if replay_is_victory {
                    diff_entry.wins += 1;
                } else {
                    diff_entry.losses += 1;
                }
            }

            if replay_is_victory {
                sum_main_wins += 1;
                _sum_ally_wins += 1;
            } else {
                sum_main_losses += 1;
                _sum_ally_losses += 1;
            }

            let main_mastery_normalized =
                ReplayAnalysisOps::normalize_mastery_vector(replay.main_masteries());
            let ally_mastery_normalized =
                ReplayAnalysisOps::normalize_mastery_vector(replay.ally_masteries());
            let include_prestige = ReplayAnalysisOps::should_count_prestige(replay.date);

            let main = main_commander
                .entry(main_commander_name.clone())
                .or_default();

            if replay.is_detailed {
                main.detailed_count += 1;
            }

            if replay_is_victory {
                main.wins += 1;
            } else {
                main.losses += 1;
            }

            main.apm_values.push(replay.main_apm());
            sum_main_apm.push(replay.main_apm());

            if replay.is_detailed {
                main.kill_fractions.push(main_kill_fraction);
                sum_main_kill_fraction.push(main_kill_fraction);
            }

            ReplayAnalysisOps::record_command_mastery_counts(
                &mut main.mastery_counts,
                &main_mastery_normalized,
            );
            ReplayAnalysisOps::record_mastery_distribution(
                &mut main.mastery_distribution_counts,
                replay.main_masteries(),
            );
            ReplayAnalysisOps::record_mastery_distribution_by_prestige(
                &mut main.mastery_distribution_by_prestige_counts,
                replay.main_prestige(),
                replay.main_masteries(),
            );
            if include_prestige {
                ReplayAnalysisOps::record_prestige_count(
                    &mut main.prestige_counts,
                    replay.main_prestige(),
                );
            }
            ReplayAnalysisOps::record_command_mastery_counts(
                &mut sum_main_mastery_counts,
                &main_mastery_normalized,
            );
            ReplayAnalysisOps::record_mastery_distribution(
                &mut sum_main_mastery_distribution_counts,
                replay.main_masteries(),
            );
            ReplayAnalysisOps::record_mastery_distribution_by_prestige(
                &mut sum_main_mastery_distribution_by_prestige_counts,
                replay.main_prestige(),
                replay.main_masteries(),
            );
            if include_prestige {
                ReplayAnalysisOps::record_prestige_count(
                    &mut sum_main_prestige_counts,
                    replay.main_prestige(),
                );
            }
            ReplayAnalysisOps::record_command_mastery_by_prestige(
                &mut main.mastery_by_prestige_counts,
                replay.main_prestige(),
                &main_mastery_normalized,
            );
            ReplayAnalysisOps::record_command_mastery_by_prestige(
                &mut sum_main_mastery_by_prestige_counts,
                replay.main_prestige(),
                &main_mastery_normalized,
            );

            let ally = ally_commander
                .entry(ally_commander_name.clone())
                .or_default();

            if replay.is_detailed {
                ally.detailed_count += 1;
            }

            if replay_is_victory {
                ally.wins += 1;
            } else {
                ally.losses += 1;
            }

            ally.apm_values.push(replay.ally_apm());
            sum_ally_apm.push(replay.ally_apm());

            if replay.is_detailed {
                ally.kill_fractions.push(ally_kill_fraction);
                sum_ally_kill_fraction.push(ally_kill_fraction);
            }

            ReplayAnalysisOps::record_command_mastery_counts(
                &mut ally.mastery_counts,
                &ally_mastery_normalized,
            );
            ReplayAnalysisOps::record_mastery_distribution(
                &mut ally.mastery_distribution_counts,
                replay.ally_masteries(),
            );
            ReplayAnalysisOps::record_mastery_distribution_by_prestige(
                &mut ally.mastery_distribution_by_prestige_counts,
                replay.ally_prestige(),
                replay.ally_masteries(),
            );
            if include_prestige {
                ReplayAnalysisOps::record_prestige_count(
                    &mut ally.prestige_counts,
                    replay.ally_prestige(),
                );
            }
            ReplayAnalysisOps::record_command_mastery_counts(
                &mut sum_ally_mastery_counts,
                &ally_mastery_normalized,
            );
            ReplayAnalysisOps::record_mastery_distribution(
                &mut sum_ally_mastery_distribution_counts,
                replay.ally_masteries(),
            );
            ReplayAnalysisOps::record_mastery_distribution_by_prestige(
                &mut sum_ally_mastery_distribution_by_prestige_counts,
                replay.ally_prestige(),
                replay.ally_masteries(),
            );
            if include_prestige {
                ReplayAnalysisOps::record_prestige_count(
                    &mut sum_ally_prestige_counts,
                    replay.ally_prestige(),
                );
            }
            ReplayAnalysisOps::record_command_mastery_by_prestige(
                &mut ally.mastery_by_prestige_counts,
                replay.ally_prestige(),
                &ally_mastery_normalized,
            );
            ReplayAnalysisOps::record_command_mastery_by_prestige(
                &mut sum_ally_mastery_by_prestige_counts,
                replay.ally_prestige(),
                &ally_mastery_normalized,
            );

            if !main_player_name.is_empty() {
                let p1 = player_values.entry(main_player_name).or_default();
                p1.record_replay(
                    &replay.main().name,
                    &replay.main().handle,
                    &main_commander_text,
                    replay_is_victory,
                    replay.main_apm(),
                    main_kill_fraction,
                    replay.date,
                );
            }

            if !ally_player_name.is_empty() {
                let p2 = player_values.entry(ally_player_name).or_default();
                p2.record_replay(
                    &replay.ally().name,
                    &replay.ally().handle,
                    &ally_commander_text,
                    replay_is_victory,
                    replay.ally_apm(),
                    ally_kill_fraction,
                    replay.date,
                );
            }
        }

        let total_games = considered_games;
        if total_games == 0 {
            crate::sco_log!(
                "[SCO/stats] aggregate stage filtered all replays; scanned={} invalid_result={}",
                total_scanned,
                invalid_result
            );
        }

        let map_count = map_values.len();
        let main_commander_count = main_commander.len();
        let ally_commander_count = ally_commander.len();
        let region_count = region_values.len();
        let difficulty_count = difficulty_values.len();
        let player_count = player_values.len();
        crate::sco_log!(
            "[SCO/stats] aggregate stage done in {}ms (maps={} commanders={} allies={} regions={} diffs={} players={})",
            started_at.elapsed().as_millis(),
            map_count,
            main_commander_count,
            ally_commander_count,
            region_count,
            difficulty_count,
            player_count
        );

        let mut map_data = Map::new();
        let map_started_at = Instant::now();
        for (map_id, aggregate) in map_values {
            let map_name = dictionary
                .coop_map_id_to_english(&map_id)
                .unwrap_or_else(|| map_id.clone());
            let games = aggregate.wins + aggregate.losses;
            let winrate = TauriOverlayOps::ratio(aggregate.wins, games);
            let bonus_rate = if aggregate.bonus_games == 0 {
                0.0
            } else {
                aggregate.bonus_fraction_sum / aggregate.bonus_games as f64
            };
            let avg_len = if aggregate.victory_games == 0 {
                999999.0
            } else {
                aggregate.victory_length_sum / aggregate.victory_games as f64
            };
            let fastest_length = if !aggregate.fastest_length.is_finite() {
                999999.0
            } else {
                aggregate.fastest_length
            };
            let fastest_p1 = ReplayAnalysisOps::fastest_map_player_value_with_dictionary(
                &aggregate.fastest_p1,
                &aggregate.fastest_p1_handle,
                &aggregate.fastest_p1_commander,
                aggregate.fastest_p1_apm,
                aggregate.fastest_p1_mastery_level,
                &aggregate.fastest_p1_masteries,
                aggregate.fastest_p1_prestige,
                dictionary,
            );
            let fastest_p2 = ReplayAnalysisOps::fastest_map_player_value_with_dictionary(
                &aggregate.fastest_p2,
                &aggregate.fastest_p2_handle,
                &aggregate.fastest_p2_commander,
                aggregate.fastest_p2_apm,
                aggregate.fastest_p2_mastery_level,
                &aggregate.fastest_p2_masteries,
                aggregate.fastest_p2_prestige,
                dictionary,
            );
            let p1_is_main = ReplayAnalysis::is_main_player_identity(
                &aggregate.fastest_p1,
                &aggregate.fastest_p1_handle,
                main_names,
                main_handles,
            );
            let p2_is_main = ReplayAnalysis::is_main_player_identity(
                &aggregate.fastest_p2,
                &aggregate.fastest_p2_handle,
                main_names,
                main_handles,
            );
            let players = if p2_is_main && !p1_is_main {
                vec![fastest_p2, fastest_p1]
            } else {
                vec![fastest_p1, fastest_p2]
            };
            map_data.insert(
                map_name,
                ReplayAnalysisOps::report_value(&MapDataRow {
                    id: map_id,
                    average_victory_time: avg_len,
                    frequency: TauriOverlayOps::ratio(games, total_games),
                    victory: aggregate.wins,
                    defeat: aggregate.losses,
                    winrate,
                    bonus: bonus_rate,
                    detailed_count: aggregate.detailed_count,
                    fastest: FastestMapDetails {
                        length: fastest_length,
                        file: aggregate.fastest_file,
                        date: aggregate.fastest_date,
                        difficulty: TauriOverlayOps::sanitize_replay_text(
                            &aggregate.fastest_difficulty,
                        ),
                        players,
                        enemy_race: TauriOverlayOps::sanitize_replay_text(
                            &aggregate.fastest_enemy_race,
                        ),
                    },
                }),
            );
        }
        crate::sco_log!(
            "[SCO/stats] map_data stage done in {}ms (rows={})",
            map_started_at.elapsed().as_millis(),
            map_data.len()
        );

        let sum_main_games = sum_main_wins + sum_main_losses;
        let main_commander_frequency = main_commander
            .iter()
            .map(|(name, agg)| {
                let games = agg.wins + agg.losses;
                (
                    name.clone(),
                    if sum_main_games == 0 {
                        0.0
                    } else {
                        games as f64 / sum_main_games as f64
                    },
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        let mut commander_data = Map::new();
        let commander_started_at = Instant::now();
        for (name, agg) in &main_commander {
            let games = agg.wins + agg.losses;
            let prestige_games = agg.prestige_counts.iter().sum::<u64>();
            commander_data.insert(
                name.clone(),
                ReplayAnalysisOps::report_value(&CommanderDataRow {
                    frequency: TauriOverlayOps::ratio(games, total_games),
                    victory: agg.wins,
                    defeat: agg.losses,
                    winrate: TauriOverlayOps::ratio(agg.wins, games),
                    median_apm: TauriOverlayOps::median_u64(&agg.apm_values),
                    kill_fraction: TauriOverlayOps::median_f64(&agg.kill_fractions),
                    mastery: ReplayAnalysisOps::build_mastery_ratio_map(&agg.mastery_counts),
                    mastery_distribution: ReplayAnalysisOps::build_mastery_distribution_map(
                        &agg.mastery_distribution_counts,
                    ),
                    mastery_distribution_by_prestige:
                        ReplayAnalysisOps::build_mastery_distribution_by_prestige_map(
                            &agg.mastery_distribution_by_prestige_counts,
                        ),
                    prestige: ReplayAnalysisOps::build_ratio_map(
                        &agg.prestige_counts,
                        prestige_games,
                    ),
                    mastery_by_prestige: ReplayAnalysisOps::build_mastery_by_prestige_ratio_map(
                        &agg.mastery_by_prestige_counts,
                    ),
                    detailed_count: agg.detailed_count,
                }),
            );
        }

        let main_detailed_count = main_commander
            .values()
            .map(|agg| agg.detailed_count)
            .sum::<u64>();

        commander_data.insert(
            "any".to_string(),
            ReplayAnalysisOps::report_value(&CommanderDataRow {
                frequency: if sum_main_games == 0 { 0.0 } else { 1.0 },
                victory: sum_main_wins,
                defeat: sum_main_losses,
                winrate: TauriOverlayOps::ratio(sum_main_wins, sum_main_games),
                median_apm: TauriOverlayOps::median_u64(&sum_main_apm),
                kill_fraction: TauriOverlayOps::median_f64(&sum_main_kill_fraction),
                mastery: ReplayAnalysisOps::build_mastery_ratio_map(&sum_main_mastery_counts),
                mastery_distribution: ReplayAnalysisOps::build_mastery_distribution_map(
                    &sum_main_mastery_distribution_counts,
                ),
                mastery_distribution_by_prestige:
                    ReplayAnalysisOps::build_mastery_distribution_by_prestige_map(
                        &sum_main_mastery_distribution_by_prestige_counts,
                    ),
                prestige: ReplayAnalysisOps::build_ratio_map(
                    &sum_main_prestige_counts,
                    sum_main_prestige_counts.iter().sum::<u64>(),
                ),
                mastery_by_prestige: ReplayAnalysisOps::build_mastery_by_prestige_ratio_map(
                    &sum_main_mastery_by_prestige_counts,
                ),
                detailed_count: main_detailed_count,
            }),
        );
        crate::sco_log!(
            "[SCO/stats] commander_data stage done in {}ms (rows={})",
            commander_started_at.elapsed().as_millis(),
            commander_data.len()
        );

        let mut ally_commander_data = Map::new();
        let ally_started_at = Instant::now();
        let mut corrected_ally_frequency = std::collections::BTreeMap::new();
        let mut corrected_ally_frequency_total = 0.0;
        for (name, agg) in &ally_commander {
            let games = (agg.wins + agg.losses) as f64;
            let main_frequency = main_commander_frequency.get(name).copied().unwrap_or(0.0);
            let corrected_games = if games == 0.0 {
                0.0
            } else {
                let divisor = 1.0 - main_frequency;
                if divisor <= f64::EPSILON {
                    0.0
                } else {
                    games / divisor
                }
            };
            corrected_ally_frequency.insert(name.clone(), corrected_games);
            corrected_ally_frequency_total += corrected_games;
        }
        for (name, agg) in &ally_commander {
            let games = agg.wins + agg.losses;
            let prestige_games = agg.prestige_counts.iter().sum::<u64>();
            let corrected_frequency = corrected_ally_frequency.get(name).copied().unwrap_or(0.0);
            ally_commander_data.insert(
                name.clone(),
                ReplayAnalysisOps::report_value(&CommanderDataRow {
                    frequency: if corrected_ally_frequency_total <= f64::EPSILON {
                        0.0
                    } else {
                        corrected_frequency / corrected_ally_frequency_total
                    },
                    victory: agg.wins,
                    defeat: agg.losses,
                    winrate: TauriOverlayOps::ratio(agg.wins, games),
                    median_apm: TauriOverlayOps::median_u64(&agg.apm_values),
                    kill_fraction: TauriOverlayOps::median_f64(&agg.kill_fractions),
                    mastery: ReplayAnalysisOps::build_mastery_ratio_map(&agg.mastery_counts),
                    mastery_distribution: ReplayAnalysisOps::build_mastery_distribution_map(
                        &agg.mastery_distribution_counts,
                    ),
                    mastery_distribution_by_prestige:
                        ReplayAnalysisOps::build_mastery_distribution_by_prestige_map(
                            &agg.mastery_distribution_by_prestige_counts,
                        ),
                    prestige: ReplayAnalysisOps::build_ratio_map(
                        &agg.prestige_counts,
                        prestige_games,
                    ),
                    mastery_by_prestige: ReplayAnalysisOps::build_mastery_by_prestige_ratio_map(
                        &agg.mastery_by_prestige_counts,
                    ),
                    detailed_count: agg.detailed_count,
                }),
            );
        }

        let sum_ally_games = _sum_ally_wins + _sum_ally_losses;
        let ally_detailed_count = ally_commander
            .values()
            .map(|agg| agg.detailed_count)
            .sum::<u64>();

        ally_commander_data.insert(
            "any".to_string(),
            ReplayAnalysisOps::report_value(&CommanderDataRow {
                frequency: if corrected_ally_frequency_total <= f64::EPSILON {
                    0.0
                } else {
                    1.0
                },
                victory: _sum_ally_wins,
                defeat: _sum_ally_losses,
                winrate: TauriOverlayOps::ratio(_sum_ally_wins, sum_ally_games),
                median_apm: TauriOverlayOps::median_u64(&sum_ally_apm),
                kill_fraction: TauriOverlayOps::median_f64(&sum_ally_kill_fraction),
                mastery: ReplayAnalysisOps::build_mastery_ratio_map(&sum_ally_mastery_counts),
                mastery_distribution: ReplayAnalysisOps::build_mastery_distribution_map(
                    &sum_ally_mastery_distribution_counts,
                ),
                mastery_distribution_by_prestige:
                    ReplayAnalysisOps::build_mastery_distribution_by_prestige_map(
                        &sum_ally_mastery_distribution_by_prestige_counts,
                    ),
                prestige: ReplayAnalysisOps::build_ratio_map(
                    &sum_ally_prestige_counts,
                    sum_ally_prestige_counts.iter().sum::<u64>(),
                ),
                mastery_by_prestige: ReplayAnalysisOps::build_mastery_by_prestige_ratio_map(
                    &sum_ally_mastery_by_prestige_counts,
                ),
                detailed_count: ally_detailed_count,
            }),
        );
        crate::sco_log!(
            "[SCO/stats] ally_commander_data stage done in {}ms (rows={})",
            ally_started_at.elapsed().as_millis(),
            ally_commander_data.len()
        );

        let mut difficulty_data = Map::new();
        let difficulty_started_at = Instant::now();
        for (name, agg) in difficulty_values {
            let games = agg.wins + agg.losses;
            difficulty_data.insert(
                name,
                ReplayAnalysisOps::report_value(&DifficultyDataRow {
                    victory: agg.wins,
                    defeat: agg.losses,
                    winrate: TauriOverlayOps::ratio(agg.wins, games),
                }),
            );
        }
        crate::sco_log!(
            "[SCO/stats] difficulty_data stage done in {}ms (rows={})",
            difficulty_started_at.elapsed().as_millis(),
            difficulty_data.len()
        );

        let mut region_data = Map::new();
        let region_started_at = Instant::now();
        for (name, agg) in region_values {
            let games = agg.wins + agg.losses;
            let mut max_com: Vec<String> = agg
                .max_com
                .into_iter()
                .map(|value| TauriOverlayOps::sanitize_replay_text(&value))
                .filter(|value| !value.is_empty())
                .collect();
            max_com.sort();
            max_com.dedup();
            let prestiges = agg
                .prestiges
                .into_iter()
                .filter_map(|(commander, value)| {
                    let commander = TauriOverlayOps::sanitize_replay_text(&commander);
                    if commander.is_empty() {
                        None
                    } else {
                        Some((commander, Value::from(value)))
                    }
                })
                .collect::<Map<String, Value>>();
            region_data.insert(
                name,
                ReplayAnalysisOps::report_value(&RegionDataRow {
                    frequency: TauriOverlayOps::ratio(games, total_games),
                    victory: agg.wins,
                    defeat: agg.losses,
                    winrate: TauriOverlayOps::ratio(agg.wins, games),
                    max_asc: agg.max_asc,
                    prestiges,
                    max_com,
                }),
            );
        }
        crate::sco_log!(
            "[SCO/stats] region_data stage done in {}ms (rows={})",
            region_started_at.elapsed().as_millis(),
            region_data.len()
        );

        let mut player_data = Map::new();
        let player_started_at = Instant::now();
        for (name, agg) in &player_values {
            let name = TauriOverlayOps::sanitize_replay_text(name);
            let games = agg.wins + agg.losses;
            let (commander, commander_frequency) = agg.dominant_commander();
            player_data.insert(
                name,
                ReplayAnalysisOps::report_value(&PlayerDataRow {
                    wins: agg.wins,
                    losses: agg.losses,
                    winrate: TauriOverlayOps::ratio(agg.wins, games),
                    kills: TauriOverlayOps::median_f64(&agg.kill_fractions),
                    apm: if games == 0 {
                        0.0
                    } else {
                        TauriOverlayOps::median_u64(&agg.apm_values)
                    },
                    frequency: commander_frequency,
                    last_seen: agg.last_seen,
                    commander,
                }),
            );
        }
        crate::sco_log!(
            "[SCO/stats] player_data stage done in {}ms (rows={})",
            player_started_at.elapsed().as_millis(),
            player_data.len()
        );

        let prestige_names = dictionary.prestige_names_json.clone();

        let unit_data = if include_detailed {
            let mut main_rollup: std::collections::BTreeMap<String, CommanderUnitRollup> =
                std::collections::BTreeMap::new();
            let mut ally_rollup: std::collections::BTreeMap<String, CommanderUnitRollup> =
                std::collections::BTreeMap::new();
            let mut amon_rollup: std::collections::BTreeMap<String, UnitStatsRollup> =
                std::collections::BTreeMap::new();

            let mut append_amon_units = |units_payload: &Value| {
                let Some(units) = units_payload.as_object() else {
                    return;
                };
                for (unit_name, row) in units {
                    let Some(values) = row.as_array() else {
                        continue;
                    };
                    let created = ReplayAnalysisOps::numeric_unit_stat_value(values.first());
                    let lost = ReplayAnalysisOps::numeric_unit_stat_value(values.get(1));
                    let kills = ReplayAnalysisOps::numeric_unit_stat_value(values.get(2));
                    if created == 0 && lost == 0 && kills == 0 {
                        continue;
                    }
                    let entry = amon_rollup
                        .entry(TauriOverlayOps::sanitize_replay_text(unit_name))
                        .or_default();
                    entry.created = entry.created.saturating_add(created);
                    entry.lost = entry.lost.saturating_add(lost);
                    entry.kills = entry.kills.saturating_add(kills);
                }
            };

            for replay in replays.iter().map(Borrow::borrow) {
                if replay.result == "Unparsed" {
                    continue;
                }
                if dictionary.canonicalize_coop_map_id(&replay.map).is_none() {
                    continue;
                }

                ReplayAnalysisOps::append_player_units_to_rollups_with_dictionary(
                    &mut main_rollup,
                    &mut ally_rollup,
                    replay.main_commander(),
                    replay.main_units(),
                    replay.main_kills(),
                    &replay.main().handle,
                    main_handles,
                    dictionary,
                );
                ReplayAnalysisOps::append_player_units_to_rollups_with_dictionary(
                    &mut main_rollup,
                    &mut ally_rollup,
                    replay.ally_commander(),
                    replay.ally_units(),
                    replay.ally_kills(),
                    &replay.ally().handle,
                    main_handles,
                    dictionary,
                );
                append_amon_units(&replay.amon_units);
            }

            ReplayAnalysisOps::report_value(&UnitDataPayload {
                main: TauriOverlayOps::build_commander_unit_data_with_dictionary(
                    main_rollup,
                    dictionary,
                ),
                ally: TauriOverlayOps::build_commander_unit_data_with_dictionary(
                    ally_rollup,
                    dictionary,
                ),
                amon: TauriOverlayOps::build_amon_unit_data(amon_rollup),
            })
        } else {
            Value::Null
        };
        let analysis = ReplayAnalysisOps::report_value(&AnalysisPayload {
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

        crate::sco_log!(
            "[SCO/stats] rebuild_analysis_payload completed in {}ms",
            started_at.elapsed().as_millis()
        );
        ReplayAnalysisOps::report_value(&RebuildAnalysisPayload {
            analysis,
            prestige_names: prestige_names
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
                .collect(),
        })
    }

    pub fn rebuild_player_rows_fast(replays: &[ReplayInfo]) -> Vec<PlayerRowPayload> {
        let mut player_values: std::collections::BTreeMap<String, PlayerAggregate> =
            std::collections::BTreeMap::new();

        for replay in replays.iter() {
            let replay_is_victory = match TauriOverlayOps::result_is_victory(&replay.result) {
                Some(result) => result,
                None => continue,
            };
            let main_kill_fraction =
                TauriOverlayOps::kill_fraction(replay.main_kills(), replay.ally_kills());
            let ally_kill_fraction = 1.0 - main_kill_fraction;
            let p1_name = TauriOverlayOps::sanitize_replay_text(&replay.main().name);
            let p2_name = TauriOverlayOps::sanitize_replay_text(&replay.ally().name);
            let main_commander = TauriOverlayOps::sanitize_replay_text(replay.main_commander());
            let ally_commander = TauriOverlayOps::sanitize_replay_text(replay.ally_commander());
            if !p1_name.is_empty() {
                let p1_handle_key = ReplayAnalysis::normalized_handle_key(&replay.main().handle);
                let p1 = player_values.entry(p1_handle_key).or_default();
                p1.record_replay(
                    &p1_name,
                    &replay.main().handle,
                    &main_commander,
                    replay_is_victory,
                    replay.main_apm(),
                    main_kill_fraction,
                    replay.date,
                );
            }

            if !p2_name.is_empty() {
                let p2_handle_key = ReplayAnalysis::normalized_handle_key(&replay.ally().handle);
                let p2 = player_values.entry(p2_handle_key).or_default();
                p2.record_replay(
                    &p2_name,
                    &replay.ally().handle,
                    &ally_commander,
                    replay_is_victory,
                    replay.ally_apm(),
                    ally_kill_fraction,
                    replay.date,
                );
            }
        }

        let mut rows = Vec::new();
        for (handle_key, agg) in player_values {
            if handle_key.is_empty() {
                continue;
            }
            let games = agg.wins + agg.losses;
            let (commander, commander_frequency) = agg.dominant_commander();
            let apm = if games == 0 {
                0.0
            } else {
                TauriOverlayOps::median_u64(&agg.apm_values)
            };
            let handle = agg
                .handles
                .iter()
                .next()
                .cloned()
                .unwrap_or_else(|| handle_key.clone());
            let player_names = agg.names_by_recency();
            let player = player_names
                .first()
                .cloned()
                .unwrap_or_else(|| handle.clone());
            rows.push(PlayerRowPayload {
                handle,
                player,
                player_names,
                wins: agg.wins,
                losses: agg.losses,
                winrate: TauriOverlayOps::ratio(agg.wins, games),
                apm,
                commander,
                frequency: commander_frequency,
                kills: TauriOverlayOps::median_f64(&agg.kill_fractions),
                last_seen: agg.last_seen,
            });
        }
        rows
    }

    fn format_next_weekly_duration(days: i64) -> String {
        if days <= 0 {
            return "Now".to_string();
        }

        let weeks = days / 7;
        let remaining_days = days % 7;
        match (weeks, remaining_days) {
            (0, days_only) => format!("{days_only}d"),
            (weeks_only, 0) => format!("{weeks_only}w"),
            (weeks_only, days_only) => format!("{weeks_only}w {days_only}d"),
        }
    }

    pub fn rebuild_weeklies_rows(replays: &[ReplayInfo]) -> Vec<WeeklyRowPayload> {
        let dictionary = Sc2DictionaryData::default();
        Self::rebuild_weeklies_rows_with_dictionary(replays, Local::now().date_naive(), &dictionary)
    }

    pub fn rebuild_weeklies_rows_for_date(
        replays: &[ReplayInfo],
        current_date: NaiveDate,
    ) -> Vec<WeeklyRowPayload> {
        let dictionary = Sc2DictionaryData::default();
        Self::rebuild_weeklies_rows_with_dictionary(replays, current_date, &dictionary)
    }

    pub fn rebuild_weeklies_rows_with_dictionary(
        replays: &[ReplayInfo],
        current_date: NaiveDate,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<WeeklyRowPayload> {
        #[derive(Default)]
        struct WeeklyMutatorUi<'a> {
            name_en: &'a str,
            name_ko: &'a str,
            map: &'a str,
            mutators: Vec<UiMutatorRow>,
        }

        #[derive(Default)]
        struct WeeklyAggregate {
            wins: u64,
            losses: u64,
            best_difficulty_rank: i64,
            best_difficulty_label: String,
        }

        fn weekly_difficulty_rank_and_label(difficulty: &str, brutal_plus: u64) -> (i64, String) {
            if brutal_plus > 0 {
                let level = brutal_plus.min(6);
                return (100 + level as i64, format!("B+{level}"));
            }

            let trimmed = difficulty.trim();
            if trimmed.is_empty() {
                return (0, "Unknown".to_string());
            }

            let lower = trimmed.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("b+") {
                if let Ok(level) = rest.trim().parse::<u64>() {
                    let level = level.min(6);
                    return (100 + level as i64, format!("B+{level}"));
                }
            }

            let rank = if lower == "casual" {
                10
            } else if lower == "normal" {
                20
            } else if lower == "hard" {
                30
            } else if lower == "brutal" {
                40
            } else {
                5
            };

            (rank, trimmed.to_string())
        }

        let weekly_mutation_order = dictionary
            .weekly_mutations_json
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect::<HashMap<String, usize>>();

        let schedule_statuses = WeeklyMutationManager::from_dictionary_data(dictionary)
            .ok()
            .and_then(|manager| manager.statuses_for_date(current_date).ok());
        let schedule_lookup = schedule_statuses
            .as_ref()
            .map(|statuses| {
                statuses
                    .iter()
                    .cloned()
                    .map(|status| (status.name.clone(), status))
                    .collect::<HashMap<String, WeeklyMutationStatus>>()
            })
            .unwrap_or_default();

        let mut aggregates = HashMap::<String, WeeklyAggregate>::new();
        let weekly_mutation_details = dictionary
            .weekly_mutations_json
            .iter()
            .map(|(weekly_name, weekly_data)| {
                let mutators = weekly_data
                    .mutators
                    .iter()
                    .map(|mutator| {
                        let mutator_id = ReplayAnalysisOps::canonical_mutator_id_with_dictionary(
                            mutator, dictionary,
                        );
                        let (name_en, name_ko, description_en, description_ko) = dictionary
                            .mutator_data(&mutator_id)
                            .map(|value| {
                                (
                                    ReplayAnalysisOps::decode_html_entities(&value.name.en),
                                    ReplayAnalysisOps::decode_html_entities(&value.name.ko),
                                    ReplayAnalysisOps::decode_html_entities(&value.description.en),
                                    ReplayAnalysisOps::decode_html_entities(&value.description.ko),
                                )
                            })
                            .unwrap_or_default();
                        let fallback_name_en =
                            ReplayAnalysisOps::mutator_display_name_en_with_dictionary(
                                &mutator_id,
                                dictionary,
                            );
                        let icon_name = if name_en.is_empty() {
                            fallback_name_en.to_string()
                        } else {
                            name_en.to_string()
                        };
                        let display_name_en = if name_en.is_empty() {
                            fallback_name_en
                        } else {
                            name_en
                        };
                        UiMutatorRow {
                            id: mutator_id.clone(),
                            name: LocalizedText {
                                en: display_name_en,
                                ko: name_ko,
                            },
                            icon_name,
                            description: LocalizedText {
                                en: description_en,
                                ko: description_ko,
                            },
                        }
                    })
                    .collect::<Vec<_>>();
                (
                    weekly_name.clone(),
                    WeeklyMutatorUi {
                        name_en: if weekly_data.name_en.trim().is_empty() {
                            weekly_name.as_str()
                        } else {
                            weekly_data.name_en.as_str()
                        },
                        name_ko: weekly_data.name_ko.as_str(),
                        map: weekly_data.map.as_str(),
                        mutators,
                    },
                )
            })
            .collect::<HashMap<String, WeeklyMutatorUi<'_>>>();

        for replay in replays {
            if replay.result == "Unparsed" {
                continue;
            }
            if !replay.weekly {
                continue;
            }

            let Some(replay_wins_main) = TauriOverlayOps::result_is_victory(&replay.result) else {
                continue;
            };
            let mutation_name = replay
                .weekly_name
                .clone()
                .map(|value| TauriOverlayOps::sanitize_replay_text(&value))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Unknown Weekly".to_string());
            let aggregate = aggregates.entry(mutation_name).or_default();
            if replay_wins_main {
                aggregate.wins = aggregate.wins.saturating_add(1);
            } else {
                aggregate.losses = aggregate.losses.saturating_add(1);
            }

            let (difficulty_rank, difficulty_label) = weekly_difficulty_rank_and_label(
                &TauriOverlayOps::sanitize_replay_text(&replay.difficulty),
                replay.brutal_plus,
            );
            if difficulty_rank > aggregate.best_difficulty_rank {
                aggregate.best_difficulty_rank = difficulty_rank;
                aggregate.best_difficulty_label = difficulty_label;
            }
        }

        let mut rows = Vec::new();
        for mutation in dictionary.weekly_mutations_json.keys() {
            let aggregate = aggregates.remove(mutation).unwrap_or_default();
            let total = aggregate.wins + aggregate.losses;
            let weekly_details = weekly_mutation_details.get(mutation);
            let mutation_order = weekly_mutation_order
                .get(mutation)
                .copied()
                .unwrap_or(usize::MAX);
            let schedule_status = schedule_lookup.get(mutation);
            let is_current = schedule_status
                .map(|status| status.is_current)
                .unwrap_or(false);
            let next_duration_days = schedule_status
                .map(|status| status.next_duration_days)
                .unwrap_or(i64::MAX);
            rows.push(WeeklyRowPayload {
                mutation: mutation.clone(),
                name_en: weekly_details
                    .map(|value| value.name_en.to_string())
                    .unwrap_or_else(|| mutation.clone()),
                name_ko: weekly_details
                    .map(|value| value.name_ko.to_string())
                    .unwrap_or_default(),
                map: weekly_details
                    .map(|value| value.map.to_string())
                    .unwrap_or_default(),
                mutators: weekly_details
                    .map(|value| value.mutators.clone())
                    .unwrap_or_default(),
                mutation_order,
                is_current,
                next_duration_days,
                next_duration: if next_duration_days == i64::MAX {
                    "Unknown".to_string()
                } else {
                    Self::format_next_weekly_duration(next_duration_days)
                },
                difficulty: if aggregate.best_difficulty_label.is_empty() {
                    "N/A".to_string()
                } else {
                    aggregate.best_difficulty_label.clone()
                },
                wins: aggregate.wins,
                losses: aggregate.losses,
                winrate: if total == 0 {
                    0.0
                } else {
                    aggregate.wins as f64 / total as f64
                },
            });
        }

        for (mutation, aggregate) in aggregates {
            let total = aggregate.wins + aggregate.losses;
            rows.push(WeeklyRowPayload {
                mutation: mutation.clone(),
                name_en: mutation,
                name_ko: String::new(),
                map: String::new(),
                mutators: Vec::new(),
                mutation_order: usize::MAX,
                is_current: false,
                next_duration_days: i64::MAX,
                next_duration: "Unknown".to_string(),
                difficulty: if aggregate.best_difficulty_label.is_empty() {
                    "N/A".to_string()
                } else {
                    aggregate.best_difficulty_label
                },
                wins: aggregate.wins,
                losses: aggregate.losses,
                winrate: if total == 0 {
                    0.0
                } else {
                    aggregate.wins as f64 / total as f64
                },
            });
        }

        rows.sort_by(|left, right| {
            let left_is_current = left.is_current;
            let right_is_current = right.is_current;
            let left_order = left.mutation_order;
            let right_order = right.mutation_order;
            right_is_current
                .cmp(&left_is_current)
                .then_with(|| left_order.cmp(&right_order))
                .then_with(|| left.mutation.cmp(&right.mutation))
        });

        rows
    }

    pub fn build_rebuild_snapshot(replays: &[ReplayInfo], include_detailed: bool) -> StatsSnapshot {
        let (main_names, main_handles) = ReplayAnalysisOps::default_main_identity();
        Self::build_rebuild_snapshot_with_identity(
            replays,
            include_detailed,
            &main_names,
            &main_handles,
        )
    }

    pub fn build_rebuild_snapshot_with_identity(
        replays: &[ReplayInfo],
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> StatsSnapshot {
        let dictionary = Sc2DictionaryData::default();
        Self::build_rebuild_snapshot_with_dictionary(
            replays,
            include_detailed,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn build_rebuild_snapshot_with_dictionary(
        replays: &[ReplayInfo],
        include_detailed: bool,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> StatsSnapshot {
        let started_at = Instant::now();
        crate::sco_log!(
            "[SCO/stats] rebuild_state_from_replays start mode={} replays={}",
            if include_detailed {
                "detailed"
            } else {
                "simple"
            },
            replays.len()
        );
        let replay_count = replays
            .iter()
            .filter(|replay| {
                replay.result != "Unparsed"
                    && dictionary.canonicalize_coop_map_id(&replay.map).is_some()
            })
            .count();
        let payload = Self::rebuild_analysis_payload_with_dictionary(
            replays,
            include_detailed,
            main_names,
            main_handles,
            dictionary,
        );
        let analysis = payload
            .get("analysis")
            .cloned()
            .unwrap_or_else(TauriOverlayOps::empty_stats_payload);
        let (main_players, main_handles) =
            ReplayAnalysisOps::collect_main_identity_lists_with_dictionary(
                replays,
                main_names,
                main_handles,
                dictionary,
            );
        crate::sco_log!(
            "[SCO/stats] rebuild_state_from_replays extracted {} main identities",
            main_players.len().max(main_handles.len())
        );

        let message = if replay_count == 0 {
            "No replay files found.".to_string()
        } else {
            format!("Scanned {replay_count} replay file(s).")
        };
        crate::sco_log!(
            "[SCO/stats] rebuild_state_from_replays end mode={} ready={} games={} duration={}ms",
            if include_detailed {
                "detailed"
            } else {
                "simple"
            },
            true,
            replay_count,
            started_at.elapsed().as_millis()
        );

        StatsSnapshot::new(
            true,
            replay_count as u64,
            main_players,
            main_handles,
            analysis,
            payload
                .get("prestige_names")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .unwrap_or_default()
                .unwrap_or_default(),
            message,
        )
    }

    pub fn load_detailed_analysis_replays_snapshot_from_path(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = Sc2DictionaryData::default();
        Self::load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
            cache_path,
            limit,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let mut replays = ReplayAnalysisOps::recover_cache_entries_from_temp(
            cache_path,
            "detailed-analysis cache",
        )
        .into_iter()
        .filter(|entry| entry.detailed_analysis && Path::new(&entry.file).exists())
        .map(|entry| {
            ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(&entry, dictionary)
                .oriented_for_main_identity(main_names, main_handles)
        })
        .collect::<Vec<_>>();

        replays.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.file.cmp(&a.file)));
        if limit > 0 && replays.len() > limit {
            replays.truncate(limit);
        }

        crate::sco_log!(
            "[SCO/cache] loaded {} replay(s) from detailed-analysis cache '{}'",
            replays.len(),
            cache_path.display()
        );
        replays
    }

    pub fn load_detailed_analysis_replays_snapshot(
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        Self::load_detailed_analysis_replays_snapshot_from_path(
            &PathManagerOps::get_cache_path(),
            limit,
            main_names,
            main_handles,
        )
    }

    pub fn load_all_analysis_replays_snapshot(
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        Self::load_all_analysis_replays_snapshot_from_path(
            &PathManagerOps::get_cache_path(),
            limit,
            main_names,
            main_handles,
        )
    }

    pub(crate) fn load_all_analysis_replays_snapshot_from_path(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = Sc2DictionaryData::default();
        Self::load_all_analysis_replays_snapshot_from_path_with_dictionary(
            cache_path,
            limit,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub(crate) fn load_all_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let mut replays =
            ReplayAnalysisOps::recover_cache_entries_from_temp(cache_path, "unified cache")
                .into_iter()
                .filter(|entry| Path::new(&entry.file).exists())
                .map(|entry| {
                    ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                        &entry, dictionary,
                    )
                    .oriented_for_main_identity(main_names, main_handles)
                })
                .collect::<Vec<_>>();

        replays.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.file.cmp(&a.file)));
        if limit > 0 && replays.len() > limit {
            replays.truncate(limit);
        }

        crate::sco_log!(
            "[SCO/cache] loaded {} replay(s) from unified cache '{}' (includes both simple and detailed)",
            replays.len(),
            cache_path.display()
        );

        replays
    }

    pub fn modified_seconds(path: &Path) -> u64 {
        path.metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .map_or(0, TauriOverlayOps::format_date_from_system_time)
    }

    pub fn collect_replay_paths(root: &Path, limit: usize) -> Vec<PathBuf> {
        if !root.exists() || !root.is_dir() {
            return Vec::new();
        }

        let mut stack = vec![root.to_path_buf()];
        let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();

        while let Some(current) = stack.pop() {
            let entries_on_disk = match std::fs::read_dir(&current) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for dir_entry in entries_on_disk.filter_map(Result::ok) {
                let path = dir_entry.path();
                let meta = match dir_entry.metadata() {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }

                if !meta.is_file() {
                    continue;
                }

                if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sc2replay"))
                {
                    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                    entries.push((path, modified));
                }
            }
        }

        entries.sort_by(|(_, a), (_, b)| b.cmp(a));
        if limit == 0 {
            entries.into_iter().map(|(path, _)| path).collect()
        } else {
            entries
                .into_iter()
                .take(limit)
                .map(|(path, _)| path)
                .collect()
        }
    }

    pub fn summarize_replay_with_cache_entry(
        path: &Path,
    ) -> Option<(ReplayInfo, Option<CacheReplayEntry>)> {
        let _ = path;
        None
    }

    pub fn summarize_replay_with_cache_entry_with_resources(
        path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Option<(ReplayInfo, Option<CacheReplayEntry>)> {
        let parse_started_at = Instant::now();
        let file_label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>");
        let empty_handles = std::collections::HashSet::new();

        match DetailedReplayAnalyzer::analyze_single_detailed(path, &empty_handles, resources) {
            Ok(result) => {
                let replay = ReplayAnalysisOps::replay_info_from_report_with_dictionary(
                    path,
                    result.report(),
                    resources.dictionary_data(),
                )
                .sanitized();
                let cache_entry = result
                    .cache_persistable()
                    .then_some(result.into_cache_entry());
                crate::sco_log!(
                    "[SCO/replay] parsed file='{}' for cache projection in {}ms persistable={}",
                    file_label,
                    parse_started_at.elapsed().as_millis(),
                    cache_entry.is_some()
                );
                Some((replay, cache_entry))
            }
            Err(error) => {
                crate::sco_log!(
                    "[SCO/replay] cache persistence parse failed for {file_label} in {}ms: {error}",
                    parse_started_at.elapsed().as_millis()
                );
                None
            }
        }
    }

    pub fn summarize_replay(path: &Path) -> ReplayInfo {
        Self::summarize_replay_lightweight(path)
    }

    pub fn summarize_replay_lightweight_with_resources(
        path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> ReplayInfo {
        CacheReplayEntry::parse_basic_with_resources(path, resources)
            .map(|entry| {
                ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                    &entry,
                    resources.dictionary_data(),
                )
                .sanitized()
            })
            .unwrap_or_else(|| ReplayAnalysisOps::unparsed_replay(path))
    }

    pub fn summarize_replay_lightweight(path: &Path) -> ReplayInfo {
        ReplayAnalysisOps::unparsed_replay(path)
    }

    pub fn analyze_replays(limit: usize) -> Vec<ReplayInfo> {
        let settings = AppSettings::from_saved_file();
        let main_names = settings.configured_main_names();
        let main_handles = settings.configured_main_handles();
        Self::load_all_analysis_replays_snapshot(limit, &main_names, &main_handles)
    }

    pub fn analyze_replays_with_identity(
        limit: usize,
        settings: &AppSettings,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        scan_progress: &ReplayScanProgress,
        replay_scan_in_flight: &AtomicBool,
    ) -> Vec<ReplayInfo> {
        let _ = (settings, scan_progress, replay_scan_in_flight);
        Self::load_all_analysis_replays_snapshot(limit, main_names, main_handles)
    }

    pub fn analyze_replays_with_resources(
        limit: usize,
        settings: &AppSettings,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        scan_progress: &ReplayScanProgress,
        replay_scan_in_flight: &AtomicBool,
        resources: &ReplayAnalysisResources,
    ) -> Vec<ReplayInfo> {
        let _scan_guard = match replay_scan_in_flight.compare_exchange(
            false,
            true,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => ScanInFlightGuard {
                flag: replay_scan_in_flight,
            },
            Err(_) => {
                scan_progress.set_stage("busy");
                // When busy, return all cached replays from unified cache
                let replays =
                    Self::load_all_analysis_replays_snapshot(limit, main_names, main_handles);
                return replays;
            }
        };

        scan_progress.reset("starting");
        scan_progress.set_status("Loading cache");

        let scan_started_at = Instant::now();
        crate::sco_log!("[SCO/replay] analyze_replays start limit={limit}");
        scan_progress.set_stage("resolving_replay_root");

        let Some(root) = settings.resolve_replay_root() else {
            crate::sco_log!("[SCO/replay] Replay root not configured");
            scan_progress.set_status("Completed");
            scan_progress.set_stage("no_replay_root");
            return Vec::new();
        };
        crate::sco_log!("[SCO/replay] scan root: {}", root.display());

        // Load existing cache (unified for both simple and detailed)
        let existing_replays = Self::load_all_analysis_replays_snapshot(
            UNLIMITED_REPLAY_LIMIT,
            main_names,
            main_handles,
        );

        // Create set of files that already have any analysis
        let analyzed_files: HashSet<String> =
            existing_replays.iter().map(|r| r.file.clone()).collect();

        let collect_started_at = Instant::now();
        scan_progress.set_stage("collecting_paths");
        let all_paths = Self::collect_replay_paths(&root, limit);
        let all_paths_len = all_paths.len();
        scan_progress.set_total(all_paths_len as u64);

        // Filter paths to only those not in cache
        let paths_to_parse: Vec<PathBuf> = all_paths
            .into_iter()
            .filter(|path| {
                let path_str = path.to_string_lossy().to_string();
                !analyzed_files.contains(&path_str)
            })
            .collect();

        let paths_to_parse_len = paths_to_parse.len();
        scan_progress.set_to_parse(paths_to_parse_len as u64);
        scan_progress.set_cache_hits((all_paths_len - paths_to_parse_len) as u64);

        crate::sco_log!(
            "[SCO/replay] collected {} path(s) in {}ms, {} already cached, parsing {}",
            all_paths_len,
            collect_started_at.elapsed().as_millis(),
            all_paths_len - paths_to_parse_len,
            paths_to_parse_len
        );

        if paths_to_parse.is_empty() {
            scan_progress.set_status("Completed");
            scan_progress.set_stage("cache_only");
            // Return cached results (already sorted and limited by load_all_analysis_replays_snapshot)
            let mut replays = existing_replays;
            if limit > 0 && replays.len() > limit {
                replays.truncate(limit);
            }
            crate::sco_log!(
                "[SCO/replay] analyze_replays finished from cache in {}ms (total={})",
                scan_started_at.elapsed().as_millis(),
                replays.len()
            );
            return replays;
        }

        struct ParseResult {
            replay: ReplayInfo,
            cache_entry: Option<CacheReplayEntry>,
        }

        scan_progress.set_cache_hits(0);
        scan_progress.set_to_parse(paths_to_parse_len as u64);

        let parse_started_at = Instant::now();
        scan_progress.set_stage("parsing_replays");
        let worker_threads = crate::AppSettings::simple_analysis_worker_threads();
        let progress = scan_progress;
        let parsed_results: Vec<Result<ParseResult, String>> = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_threads)
            .build()
            .unwrap()
            .install(|| {
                paths_to_parse
                    .into_par_iter()
                    .enumerate()
                    .map(|(_index, path)| {
                        let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let replay =
                                Self::summarize_replay_lightweight_with_resources(&path, resources);
                            let cache_entry =
                                CacheReplayEntry::parse_basic_with_resources(&path, resources);
                            (replay, cache_entry)
                        }));
                        let (replay, cache_entry) = match parsed {
                            Ok((replay, cache_entry)) => (replay, cache_entry),
                            Err(_) => {
                                progress.increment_completed();
                                progress.increment_failed();
                                return Err(path.to_string_lossy().to_string());
                            }
                        };
                        let oriented = replay.oriented_for_main_identity(main_names, main_handles);
                        progress.increment_completed();
                        progress.increment_newly_parsed();
                        Ok(ParseResult {
                            replay: oriented,
                            cache_entry,
                        })
                    })
                    .collect()
            });

        let mut failed_to_parse = Vec::<String>::new();
        let mut successful_results = Vec::<ParseResult>::with_capacity(parsed_results.len());
        for parse_result in parsed_results {
            match parse_result {
                Ok(value) => successful_results.push(value),
                Err(path) => failed_to_parse.push(path),
            }
        }

        if !failed_to_parse.is_empty() {
            crate::sco_log!(
                "[SCO/replay] failed to parse {} replay(s): {}",
                failed_to_parse.len(),
                failed_to_parse.join(", ")
            );
        }

        let failed_to_parse = failed_to_parse.len();
        scan_progress.set_failed(failed_to_parse as u64);
        scan_progress.set_parse_skipped(0);

        crate::sco_log!(
            "[SCO/replay] parsed {} replay(s) with rayon in {}ms (threads={worker_threads})",
            successful_results.len(),
            parse_started_at.elapsed().as_millis()
        );

        scan_progress.set_stage("finalizing_results");
        scan_progress.set_status("Finalizing results");
        crate::sco_log!(
            "[SCO/replay] finalizing {} parsed replay result(s) against {} existing replay(s)",
            successful_results.len(),
            existing_replays.len()
        );

        let mut replay_map = HashMap::<String, ReplayInfo>::new();
        let mut simple_cache_entries = Vec::<CacheReplayEntry>::new();
        for replay in existing_replays {
            let replay_hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(&replay.file));
            if replay_hash.is_empty() {
                continue;
            }
            replay_map.retain(|hash, entry| hash == &replay_hash || entry.file != replay.file);
            match replay_map.get(&replay_hash) {
                Some(existing)
                    if ReplayInfo::should_keep_existing_detailed_variant(
                        existing.is_detailed,
                        replay.is_detailed,
                    ) => {}
                _ => {
                    replay_map.insert(replay_hash, replay);
                }
            }
        }

        for result in successful_results {
            if let Some(entry) = result.cache_entry.as_ref() {
                simple_cache_entries.push(entry.clone());

                if !entry.hash.is_empty() {
                    replay_map.retain(|hash, cached| {
                        hash == &entry.hash || cached.file != result.replay.file
                    });
                    match replay_map.get(&entry.hash) {
                        Some(existing)
                            if ReplayInfo::should_keep_existing_detailed_variant(
                                existing.is_detailed,
                                result.replay.is_detailed,
                            ) => {}
                        _ => {
                            replay_map.insert(entry.hash.clone(), result.replay.clone());
                        }
                    }
                    continue;
                }
            }

            let replay_hash =
                ReplayFileIdentity::calculate_hash(&PathBuf::from(&result.replay.file));
            if replay_hash.is_empty() {
                continue;
            }
            replay_map
                .retain(|hash, cached| hash == &replay_hash || cached.file != result.replay.file);
            match replay_map.get(&replay_hash) {
                Some(existing)
                    if ReplayInfo::should_keep_existing_detailed_variant(
                        existing.is_detailed,
                        result.replay.is_detailed,
                    ) => {}
                _ => {
                    replay_map.insert(replay_hash, result.replay);
                }
            }
        }

        crate::sco_log!(
            "[SCO/cache] persisting {} simple-analysis cache entr(y/ies) in one batch",
            simple_cache_entries.len()
        );
        if let Err(error) = CacheReplayEntry::persist_simple_cache_entries(
            &simple_cache_entries,
            &PathManagerOps::get_cache_path(),
        ) {
            crate::sco_log!("[SCO/cache] failed to persist simple analysis cache batch: {error}");
        }

        let mut all_replays = replay_map.into_values().collect::<Vec<_>>();
        ReplayInfo::sort_replays(&mut all_replays);
        if limit > 0 && all_replays.len() > limit {
            all_replays.truncate(limit);
        }

        scan_progress.set_stage("completed");
        scan_progress.set_status("Completed");
        let unparsed_count = all_replays
            .iter()
            .filter(|replay| replay.result == "Unparsed")
            .count();
        crate::sco_log!(
            "[SCO/replay] analyze_replays finished in {}ms (parsed={}, unparsed={}, cached={})",
            scan_started_at.elapsed().as_millis(),
            all_replays.len() - unparsed_count,
            unparsed_count,
            all_paths_len - paths_to_parse_len
        );

        all_replays
    }

    fn replay_matches_stats_filters(
        path: &str,
        replay: &ReplayInfo,
        main_handles: &HashSet<String>,
    ) -> bool {
        let dictionary = Sc2DictionaryData::default();
        Self::replay_matches_stats_filters_with_dictionary(path, replay, main_handles, &dictionary)
    }

    pub(crate) fn replay_matches_stats_filters_with_dictionary(
        path: &str,
        replay: &ReplayInfo,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> bool {
        let include_mutations = TauriOverlayOps::parse_query_bool(path, "include_mutations", true);
        let include_normal_games =
            TauriOverlayOps::parse_query_bool(path, "include_normal_games", true);
        let include_wins = TauriOverlayOps::parse_query_value(path, "include_wins")
            .map(|_| TauriOverlayOps::parse_query_bool(path, "include_wins", true))
            .unwrap_or(true);
        let include_losses = TauriOverlayOps::parse_query_value(path, "include_losses")
            .map(|_| TauriOverlayOps::parse_query_bool(path, "include_losses", true))
            .unwrap_or_else(|| !TauriOverlayOps::parse_query_bool(path, "wins_only", false));
        let include_both_main = TauriOverlayOps::parse_query_bool(path, "include_both_main", true);
        let include_sub_15 = TauriOverlayOps::parse_query_bool(path, "sub_15", true);
        let include_over_15 = TauriOverlayOps::parse_query_bool(path, "over_15", true);
        let include_ally_sub_15 = TauriOverlayOps::parse_query_bool(path, "ally_sub_15", true);
        let include_ally_over_15 = TauriOverlayOps::parse_query_bool(path, "ally_over_15", true);
        let include_main_normal_mastery =
            TauriOverlayOps::parse_query_bool(path, "main_normal_mastery", true);
        let include_main_abnormal_mastery =
            TauriOverlayOps::parse_query_bool(path, "main_abnormal_mastery", true);
        let include_ally_normal_mastery =
            TauriOverlayOps::parse_query_bool(path, "ally_normal_mastery", true);
        let include_ally_abnormal_mastery =
            TauriOverlayOps::parse_query_bool(path, "ally_abnormal_mastery", true);

        let min_length_minutes = TauriOverlayOps::parse_query_i64(path, "minlength")
            .and_then(|value| u64::try_from(value.max(0)).ok())
            .unwrap_or(0);
        let max_length_minutes = TauriOverlayOps::parse_query_i64(path, "maxlength")
            .and_then(|value| u64::try_from(value.max(0)).ok())
            .unwrap_or(0);

        let min_date_seconds = ReplayAnalysisOps::query_date_boundary_seconds(path, "mindate");
        let max_date_seconds = ReplayAnalysisOps::query_date_boundary_seconds(path, "maxdate");

        let player_filter = TauriOverlayOps::parse_query_value(path, "player")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let difficulty_filter = TauriOverlayOps::parse_query_csv(path, "difficulty_filter");
        let region_filter: HashSet<String> =
            TauriOverlayOps::parse_query_csv(path, "region_filter")
                .into_iter()
                .map(|value| value.to_ascii_uppercase())
                .collect();

        let has_main_handles = !main_handles.is_empty();

        if replay.result == "Unparsed" {
            return false;
        }
        if dictionary.canonicalize_coop_map_id(&replay.map).is_none() {
            return false;
        }

        if !include_mutations && replay.extension {
            return false;
        }
        if !include_normal_games && !replay.extension {
            return false;
        }
        let Some(is_victory) = TauriOverlayOps::result_is_victory(&replay.result) else {
            return false;
        };
        if !include_wins && is_victory {
            return false;
        }
        if !include_losses && !is_victory {
            return false;
        }

        if min_length_minutes > 0 && replay.accurate_length < (min_length_minutes * 60) as f64 {
            return false;
        }
        if max_length_minutes > 0 && replay.accurate_length > (max_length_minutes * 60) as f64 {
            return false;
        }

        let replay_date_seconds = replay.date_seconds_for_filter();
        if let Some(min_date) = min_date_seconds {
            if replay_date_seconds <= min_date {
                return false;
            }
        }
        if let Some(max_date) = max_date_seconds {
            if replay_date_seconds >= max_date {
                return false;
            }
        }

        if !include_sub_15 && replay.main_commander_level() < 15 {
            return false;
        }
        if !include_over_15 && replay.main_commander_level() >= 15 {
            return false;
        }
        if !include_ally_sub_15 && replay.ally_commander_level() < 15 {
            return false;
        }
        if !include_ally_over_15 && replay.ally_commander_level() >= 15 {
            return false;
        }
        let main_mastery_points =
            ReplayAnalysisOps::mastery_points_invested(replay.main_masteries());
        let ally_mastery_points =
            ReplayAnalysisOps::mastery_points_invested(replay.ally_masteries());
        if !include_main_normal_mastery && main_mastery_points <= 90 {
            return false;
        }
        if !include_main_abnormal_mastery && main_mastery_points > 90 {
            return false;
        }
        if !include_ally_normal_mastery && ally_mastery_points <= 90 {
            return false;
        }
        if !include_ally_abnormal_mastery && ally_mastery_points > 90 {
            return false;
        }

        if has_main_handles && !include_both_main {
            let p1_is_main =
                main_handles.contains(&Self::normalized_handle_key(&replay.main().handle));
            let p2_is_main =
                main_handles.contains(&Self::normalized_handle_key(&replay.ally().handle));
            if p1_is_main && p2_is_main {
                return false;
            }
        }

        if !player_filter.is_empty() {
            let p1 = replay.main().name.to_ascii_lowercase();
            let p2 = replay.ally().name.to_ascii_lowercase();
            if !ReplayAnalysisOps::wildcard_match(&player_filter, &p1)
                && !ReplayAnalysisOps::wildcard_match(&player_filter, &p2)
            {
                return false;
            }
        }

        if !difficulty_filter.is_empty() {
            for blocked in &difficulty_filter {
                if let Ok(bplus) = blocked.parse::<u64>() {
                    if replay.brutal_plus == bplus {
                        return false;
                    }
                    continue;
                }

                if replay.brutal_plus > 0 && blocked.eq_ignore_ascii_case("Brutal") {
                    continue;
                }

                if replay.difficulty.contains(blocked) {
                    return false;
                }
            }
        }

        if !region_filter.is_empty() {
            let region = TauriOverlayOps::infer_region_from_handle(&replay.main().handle)
                .or_else(|| TauriOverlayOps::infer_region_from_handle(&replay.ally().handle))
                .unwrap_or_else(|| "Unknown".to_string())
                .to_ascii_uppercase();
            if !matches!(region.as_str(), "NA" | "EU" | "KR" | "CN" | "PTR") {
                return false;
            }
            if region_filter.contains(&region) {
                return false;
            }
        }

        true
    }

    fn filter_replays_for_stats_refs_with_dictionary<'a>(
        path: &str,
        replays: &[&'a ReplayInfo],
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<&'a ReplayInfo> {
        replays
            .iter()
            .copied()
            .filter(|replay| {
                Self::replay_matches_stats_filters_with_dictionary(
                    path,
                    replay,
                    main_handles,
                    dictionary,
                )
            })
            .collect()
    }

    pub fn filter_replays_for_stats(path: &str, replays: &[ReplayInfo]) -> Vec<ReplayInfo> {
        let (_, main_handles) = ReplayAnalysisOps::default_main_identity();
        replays
            .iter()
            .filter(|replay| Self::replay_matches_stats_filters(path, replay, &main_handles))
            .cloned()
            .collect()
    }

    pub fn detailed_stats_counts(filtered_replays: &[&ReplayInfo]) -> (u64, u64) {
        let total_valid_files = filtered_replays.len() as u64;
        let detailed_parsed_count = filtered_replays
            .iter()
            .filter(|replay| replay.has_detailed_unit_stats())
            .count() as u64;
        (detailed_parsed_count, total_valid_files)
    }

    pub fn should_include_detailed_stats_response(
        response: &Value,
        cached_replays: &[ReplayInfo],
    ) -> bool {
        response
            .get("analysis")
            .and_then(|value| value.get("UnitData"))
            .is_some_and(|value| !value.is_null())
            || cached_replays
                .iter()
                .any(ReplayInfo::has_detailed_unit_stats)
    }

    pub fn build_stats_response(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        replays: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
    ) -> Result<Value, String> {
        let (main_names, main_handles) = ReplayAnalysisOps::default_main_identity();
        Self::build_stats_response_with_identity(
            path,
            stats,
            replays,
            stats_current_replay_files,
            ReplayScanProgress::default().as_payload(),
            &main_names,
            &main_handles,
        )
    }

    pub fn build_stats_response_with_identity(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        replays: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
        scan_progress: ReplayScanProgressPayload,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Result<Value, String> {
        let dictionary = Sc2DictionaryData::default();
        Self::build_stats_response_with_dictionary(
            path,
            stats,
            replays,
            stats_current_replay_files,
            scan_progress,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn build_stats_response_with_dictionary(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        replays: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
        scan_progress: ReplayScanProgressPayload,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<Value, String> {
        let mut response = match stats.try_lock() {
            Ok(state) => state.as_payload(scan_progress.clone()),
            Err(error) => match error {
                TryLockError::WouldBlock => {
                    let fallback = StatsState::default();
                    let mut payload = fallback.as_payload(scan_progress);
                    payload["message"] = Value::from("Statistics are updating. Try again.");
                    payload
                }
                TryLockError::Poisoned(_) => {
                    return Err("Failed to access stats state: mutex is poisoned".to_string());
                }
            },
        };

        let is_ready = response
            .get("ready")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let analysis_running = response
            .get("analysis_running")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_ready && !analysis_running {
            match replays.try_lock() {
                Ok(cached_replays) => match stats_current_replay_files.try_lock() {
                    Ok(current_replay_files) => {
                        let mut cached_replays =
                            cached_replays.values().cloned().collect::<Vec<_>>();
                        ReplayInfo::sort_replays(&mut cached_replays);
                        let include_detailed = Self::should_include_detailed_stats_response(
                            &response,
                            &cached_replays,
                        );
                        let stats_replays = Self::stats_replays_for_response_with_dictionary(
                            include_detailed,
                            &cached_replays,
                            main_names,
                            main_handles,
                            dictionary,
                        );
                        let selected_replays = Self::stats_source_replays_for_response(
                            path,
                            stats_replays.as_ref(),
                            &current_replay_files,
                        );
                        let filtered_replays = Self::filter_replays_for_stats_refs_with_dictionary(
                            path,
                            &selected_replays,
                            main_handles,
                            dictionary,
                        );
                        let filtered_payload = Self::rebuild_analysis_payload_with_dictionary(
                            &filtered_replays,
                            include_detailed,
                            main_names,
                            main_handles,
                            dictionary,
                        );
                        if let Some(analysis) = filtered_payload.get("analysis") {
                            response["analysis"] = analysis.clone();
                        }
                        if let Some(prestige_names) = filtered_payload.get("prestige_names") {
                            response["prestige_names"] = prestige_names.clone();
                        }
                        response["games"] = Value::from(filtered_replays.len() as u64);
                        let (detailed_parsed_count, total_valid_files) =
                            Self::detailed_stats_counts(&filtered_replays);
                        response["detailed_parsed_count"] = Value::from(detailed_parsed_count);
                        response["total_valid_files"] = Value::from(total_valid_files);

                        let (main_players, main_handles) =
                            ReplayAnalysisOps::collect_main_identity_lists_with_dictionary(
                                &filtered_replays,
                                main_names,
                                main_handles,
                                dictionary,
                            );
                        response["main_players"] = ReplayAnalysisOps::report_value(&main_players);
                        response["main_handles"] = ReplayAnalysisOps::report_value(&main_handles);
                    }
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Poisoned(_)) => {
                        return Err(
                            "Failed to access current replay file set: mutex is poisoned"
                                .to_string(),
                        );
                    }
                },
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Poisoned(_)) => {
                    return Err("Failed to access replay cache: mutex is poisoned".to_string());
                }
            }
        }
        if let Some(query) = path.split('?').nth(1) {
            response["query"] = Value::from(query);
        }

        Ok(response)
    }

    pub fn stats_replays_for_response<'a>(
        include_detailed: bool,
        cached_replays: &'a [ReplayInfo],
    ) -> Cow<'a, [ReplayInfo]> {
        let (main_names, main_handles) = ReplayAnalysisOps::default_main_identity();
        Self::stats_replays_for_response_with_identity(
            include_detailed,
            cached_replays,
            &main_names,
            &main_handles,
        )
    }

    pub fn stats_replays_for_response_with_identity<'a>(
        include_detailed: bool,
        cached_replays: &'a [ReplayInfo],
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Cow<'a, [ReplayInfo]> {
        if !cached_replays.is_empty() {
            return Cow::Borrowed(cached_replays);
        }

        Self::stats_replays_for_response_from_path(
            include_detailed,
            cached_replays,
            &PathManagerOps::get_cache_path(),
            main_names,
            main_handles,
        )
    }

    pub fn stats_replays_for_response_with_dictionary<'a>(
        include_detailed: bool,
        cached_replays: &'a [ReplayInfo],
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Cow<'a, [ReplayInfo]> {
        if !cached_replays.is_empty() {
            return Cow::Borrowed(cached_replays);
        }

        Self::stats_replays_for_response_from_path_with_dictionary(
            include_detailed,
            cached_replays,
            &PathManagerOps::get_cache_path(),
            main_names,
            main_handles,
            dictionary,
        )
    }

    pub fn stats_replays_for_response_from_path<'a>(
        include_detailed: bool,
        cached_replays: &'a [ReplayInfo],
        cache_path: &Path,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Cow<'a, [ReplayInfo]> {
        let dictionary = Sc2DictionaryData::default();
        Self::stats_replays_for_response_from_path_with_dictionary(
            include_detailed,
            cached_replays,
            cache_path,
            main_names,
            main_handles,
            &dictionary,
        )
    }

    pub fn stats_replays_for_response_from_path_with_dictionary<'a>(
        include_detailed: bool,
        cached_replays: &'a [ReplayInfo],
        cache_path: &Path,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Cow<'a, [ReplayInfo]> {
        if !include_detailed {
            return Cow::Borrowed(cached_replays);
        }

        let from_detailed_analysis =
            Self::load_detailed_analysis_replays_snapshot_from_path_with_dictionary(
                cache_path,
                UNLIMITED_REPLAY_LIMIT,
                main_names,
                main_handles,
                dictionary,
            );
        if from_detailed_analysis.is_empty() {
            Cow::Borrowed(cached_replays)
        } else {
            Cow::Owned(from_detailed_analysis)
        }
    }

    pub fn stats_source_replays_for_response<'a>(
        path: &str,
        replays: &'a [ReplayInfo],
        current_replay_files: &HashSet<String>,
    ) -> Vec<&'a ReplayInfo> {
        let show_all = TauriOverlayOps::parse_query_bool(path, "show_all", true);
        if show_all {
            return replays.iter().collect();
        }

        replays
            .iter()
            .filter(|replay| current_replay_files.contains(&replay.file))
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn normalize_lookup_key(value: &str) -> String {
        value
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .flat_map(|ch| ch.to_lowercase())
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn normalize_mutator_id_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        ReplayAnalysisOps::canonical_mutator_id_with_dictionary(mutator, dictionary)
    }
}

impl ReplayAnalysisOps {
    fn resolve_weekly_mutation_name_with_dictionary(
        map_name: &str,
        mutators: &[String],
        dictionary: &Sc2DictionaryData,
    ) -> Option<String> {
        if mutators.is_empty() {
            return None;
        }

        let map_key =
            ReplayAnalysisOps::normalize_lookup_key(&TauriOverlayOps::map_display_name(map_name));
        if map_key.is_empty() {
            return None;
        }

        let mutator_set: HashSet<String> = mutators
            .iter()
            .map(|mutator| {
                ReplayAnalysisOps::normalize_lookup_key(
                    &ReplayAnalysisOps::normalize_mutator_id_with_dictionary(mutator, dictionary),
                )
            })
            .filter(|key| !key.is_empty())
            .collect();
        if mutator_set.is_empty() {
            return None;
        }

        for (weekly_name, row) in dictionary.weekly_mutations_as_sets.iter() {
            if ReplayAnalysisOps::normalize_lookup_key(&row.map) != map_key {
                continue;
            }
            if row.mutators == mutator_set {
                return Some(weekly_name.to_string());
            }
        }

        None
    }
}
