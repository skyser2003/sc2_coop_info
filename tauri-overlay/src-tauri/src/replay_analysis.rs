use s2coop_analyzer::cache_overall_stats_generator::{
    CacheIconValue, CacheNumericValue, CachePlayer, CacheReplayEntry, CacheUnitStats, ReplayMessage,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use s2coop_analyzer::tauri_replay_analysis_impl::{
    ParsedReplayMessage, ParsedReplayPlayer, ReplayReport,
};
use serde::Serialize;
use serde_json::Value;
use std::borrow::Borrow;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::stats_aggregation::{StatsAggregateUnitDataPayload, StatsAggregationOps};
use crate::stats_units::StatsUnitDataOps;
use crate::{
    AppSettings, CommanderUnitRollup, ReplayCacheDatabase, ReplayCacheEntryQuery,
    ReplayChatMessage, ReplayInfo, ReplayPlayerInfo, TauriOverlayOps, UnitStatsRollup,
};

struct FastestMapPlayerInput<'a> {
    name: &'a str,
    handle: &'a str,
    commander: &'a str,
    apm: u64,
    mastery_level: u64,
    masteries: &'a [u64],
    prestige: u64,
}

struct PlayerUnitRollupInput<'a> {
    commander_name: &'a str,
    units_payload: &'a Value,
    player_kills: u64,
    player_handle: &'a str,
    main_handles: &'a HashSet<String>,
    dictionary: &'a Sc2DictionaryData,
}

struct ScanInFlightGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for ScanInFlightGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

struct ParsedReplayBatch {
    replays: Vec<ReplayInfo>,
    failed_paths: Vec<String>,
}

impl ParsedReplayBatch {
    fn new() -> Self {
        Self {
            replays: Vec::new(),
            failed_paths: Vec::new(),
        }
    }

    fn push_failure(&mut self, path: String) {
        self.failed_paths.push(path);
    }

    fn push_success(&mut self, replay: ReplayInfo) {
        self.replays.push(replay);
    }
}

struct ParsedReplayPathResult {
    replay: ReplayInfo,
    failed_path: Option<String>,
}

impl ParsedReplayPathResult {
    fn new(replay: ReplayInfo, failed_path: Option<String>) -> Self {
        Self {
            replay,
            failed_path,
        }
    }

    fn into_parts(self) -> (ReplayInfo, Option<String>) {
        (self.replay, self.failed_path)
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
    pub fn mastery_points_invested(raw_values: &[u64]) -> u64 {
        StatsAggregationOps::mastery_points_invested(raw_values)
    }
}

impl ReplayAnalysisOps {
    fn should_count_prestige(date: u64) -> bool {
        StatsAggregationOps::should_count_prestige(date)
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
        input: FastestMapPlayerInput<'_>,
        dictionary: &Sc2DictionaryData,
    ) -> Value {
        ReplayAnalysisOps::report_value(&FastestMapPlayer {
            name: TauriOverlayOps::sanitize_replay_text(input.name),
            handle: input.handle.to_string(),
            commander: TauriOverlayOps::sanitize_replay_text(input.commander),
            apm: input.apm,
            mastery_level: input.mastery_level,
            masteries: TauriOverlayOps::normalize_mastery_values(input.masteries),
            prestige: input.prestige,
            prestige_name: ReplayAnalysisOps::fastest_map_prestige_name_with_dictionary(
                input.commander,
                input.prestige,
                dictionary,
            ),
        })
    }
}

impl ReplayAnalysisOps {
    fn report_value<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
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
    pub fn wildcard_match(pattern: &str, value: &str) -> bool {
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
    pub fn bonus_objective_total_for_canonical_map_with_dictionary(
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

pub struct ReplayAnalysis;

mod analysis_payload;
mod cache_loading;
mod cache_reading;
mod identity;
mod replay_info_conversion;
mod replay_scanning;
mod row_payloads;
mod stats_response;
mod unit_rollups;

pub use row_payloads::{PlayerRowPayload, WeeklyRowPayload};
pub use stats_response::StatsResponseBuildInput;

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
    fn normalized_coop_map_match_key_with_dictionary(
        map_name: &str,
        dictionary: &Sc2DictionaryData,
    ) -> Option<String> {
        let display_name = TauriOverlayOps::map_display_name(map_name);
        if display_name.trim().is_empty() {
            return None;
        }

        let comparable_name = dictionary
            .canonicalize_coop_map_id(&display_name)
            .unwrap_or(display_name);
        let key = ReplayAnalysisOps::normalize_lookup_key(&comparable_name);
        if key.is_empty() { None } else { Some(key) }
    }

    fn resolve_weekly_mutation_name_with_dictionary(
        map_name: &str,
        mutators: &[String],
        dictionary: &Sc2DictionaryData,
    ) -> Option<String> {
        if mutators.is_empty() {
            return None;
        }

        let map_key =
            ReplayAnalysisOps::normalized_coop_map_match_key_with_dictionary(map_name, dictionary)?;

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
            let Some(weekly_map_key) =
                ReplayAnalysisOps::normalized_coop_map_match_key_with_dictionary(
                    &row.map, dictionary,
                )
            else {
                continue;
            };
            if weekly_map_key != map_key {
                continue;
            }
            if row.mutators == mutator_set {
                return Some(weekly_name.to_string());
            }
        }

        None
    }
}
