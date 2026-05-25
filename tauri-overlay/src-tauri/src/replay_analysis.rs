use chrono::{Local, NaiveDate};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheIconValue, CacheNumericValue, CachePlayer, CacheReplayEntry, CacheUnitStats, ReplayMessage,
};
use s2coop_analyzer::detailed_replay_analysis::{
    DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, DetailedReplayAnalyzer, ReplayAnalysisResources,
    ReplayCacheParallelParseOptions, ReplayFileIdentity,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use s2coop_analyzer::tauri_replay_analysis_impl::{
    ParsedReplayMessage, ParsedReplayPlayer, ReplayReport,
};
use s2coop_analyzer::weekly_mutation_manager::{WeeklyMutationManager, WeeklyMutationStatus};
use serde::Serialize;
use serde_json::{Map, Value};
use std::borrow::Borrow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

use crate::path_manager::PathManagerOps;
use crate::replay_scan_progress::ReplayScanProgress;
use crate::shared_types::{
    LocalizedLabels, LocalizedText, ReplayScanProgressPayload, UiMutatorRow,
};
use crate::stats_aggregation::{
    StatsAggregateAnalysisPayload, StatsAggregateDifficultyDataRow,
    StatsAggregateFastestMapDetails, StatsAggregateMapDataRow, StatsAggregatePlayerDataRow,
    StatsAggregateRegionDataRow, StatsAggregateUnitDataPayload, StatsAggregationOps,
    StatsCommanderAggregate, StatsCommanderDataInput, StatsCommanderPlayerRecord,
    StatsCommanderTotals, StatsMapAggregate, StatsPlayerAggregate, StatsPlayerRecord,
    StatsPlayerSnapshot, StatsRegionAggregate, StatsReplaySnapshot, StatsResultSummary,
    StatsWinLossAggregate,
};
use crate::stats_query::StatsQuery;
use crate::stats_units::StatsUnitDataOps;
use crate::{
    AppSettings, CommanderUnitRollup, QueuedReplayCacheEntrySink, ReplayCacheDatabase,
    ReplayCacheEntryQuery, ReplayCacheReadScope, ReplayCacheWriteQueue, ReplayChatMessage,
    ReplayInfo, ReplayPlayerInfo, StatsSnapshot, StatsState, TauriOverlayOps,
    UNLIMITED_REPLAY_LIMIT, UnitStatsRollup,
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

pub struct StatsResponseBuildInput<'a> {
    path: &'a str,
    stats: &'a Arc<Mutex<StatsState>>,
    stats_current_replay_files: &'a Arc<Mutex<HashSet<String>>>,
    scan_progress: ReplayScanProgressPayload,
    main_names: &'a HashSet<String>,
    main_handles: &'a HashSet<String>,
}

impl<'a> StatsResponseBuildInput<'a> {
    pub fn new(
        path: &'a str,
        stats: &'a Arc<Mutex<StatsState>>,
        stats_current_replay_files: &'a Arc<Mutex<HashSet<String>>>,
        scan_progress: ReplayScanProgressPayload,
        main_names: &'a HashSet<String>,
        main_handles: &'a HashSet<String>,
    ) -> Self {
        Self {
            path,
            stats,
            stats_current_replay_files,
            scan_progress,
            main_names,
            main_handles,
        }
    }
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
    fn read_cache_summary_entries(
        cache_path: &Path,
        log_label: &str,
        query: ReplayCacheEntryQuery,
    ) -> Vec<CacheReplayEntry> {
        let db_path = ReplayCacheDatabase::db_path_for_cache_path(cache_path);
        let database = match ReplayCacheDatabase::open_for_cache_path(cache_path) {
            Ok(database) => database,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to open {log_label} database for '{}': {error}",
                    db_path.display()
                );
                return Vec::new();
            }
        };

        match database.load_summary_entries(query) {
            Ok(entries) => entries,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to read {log_label} database for '{}': {error}",
                    db_path.display()
                );
                Vec::new()
            }
        }
    }

    fn read_cache_entries(
        cache_path: &Path,
        log_label: &str,
        query: ReplayCacheEntryQuery,
    ) -> Vec<CacheReplayEntry> {
        let db_path = ReplayCacheDatabase::db_path_for_cache_path(cache_path);
        let database = match ReplayCacheDatabase::open_for_cache_path(cache_path) {
            Ok(database) => database,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to open {log_label} database for '{}': {error}",
                    db_path.display()
                );
                return Vec::new();
            }
        };

        match database.load_entries(query) {
            Ok(entries) => entries,
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to read {log_label} database for '{}': {error}",
                    db_path.display()
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
        query: ReplayCacheEntryQuery,
    ) -> Vec<CacheReplayEntry> {
        ReplayAnalysisOps::read_cache_entries(cache_path, log_label, query)
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
        if let Some(mc_unit_name) = mc_unit
            && replay_units.iter().any(|(unit, _)| unit == mc_unit_name)
        {
            for (unit, row) in &replay_units {
                if row.created.is_explicit_zero()
                    || (commander != "Fenix" && unit == "Disruptor")
                    || (commander != "Tychus" && unit == "Auto-Turret")
                {
                    mc_unit_bonus_kills = mc_unit_bonus_kills.saturating_add(row.kills);
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
    fn append_player_units_to_rollups_with_dictionary(
        main_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        input: PlayerUnitRollupInput<'_>,
    ) {
        if ReplayAnalysis::is_main_player_by_handle(input.player_handle, input.main_handles) {
            ReplayAnalysisOps::append_units_to_rollup_with_dictionary(
                main_rollup,
                input.commander_name,
                input.units_payload,
                input.player_kills,
                input.dictionary,
            );
        } else {
            ReplayAnalysisOps::append_units_to_rollup_with_dictionary(
                ally_rollup,
                input.commander_name,
                input.units_payload,
                input.player_kills,
                input.dictionary,
            );
        }
    }
}

impl ReplayAnalysisOps {
    pub fn build_unit_data_from_replays_with_dictionary<R>(
        replays: &[R],
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Value
    where
        R: Borrow<ReplayInfo>,
    {
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
                PlayerUnitRollupInput {
                    commander_name: replay.main_commander(),
                    units_payload: replay.main_units(),
                    player_kills: replay.main_kills(),
                    player_handle: &replay.main().handle,
                    main_handles,
                    dictionary,
                },
            );
            ReplayAnalysisOps::append_player_units_to_rollups_with_dictionary(
                &mut main_rollup,
                &mut ally_rollup,
                PlayerUnitRollupInput {
                    commander_name: replay.ally_commander(),
                    units_payload: replay.ally_units(),
                    player_kills: replay.ally_kills(),
                    player_handle: &replay.ally().handle,
                    main_handles,
                    dictionary,
                },
            );
            append_amon_units(&replay.amon_units);
        }

        ReplayAnalysisOps::report_value(&StatsAggregateUnitDataPayload::new(
            StatsUnitDataOps::build_commander_unit_data_with_dictionary(main_rollup, dictionary),
            StatsUnitDataOps::build_commander_unit_data_with_dictionary(ally_rollup, dictionary),
            StatsUnitDataOps::build_amon_unit_data(amon_rollup),
        ))
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

    pub fn is_main_player_by_name(
        player_name: &str,
        main_names: &std::collections::HashSet<String>,
    ) -> bool {
        if main_names.is_empty() {
            return false;
        }
        let normalized = Self::normalized_player_key(player_name);
        !normalized.is_empty() && main_names.contains(&normalized)
    }

    pub fn is_main_player_by_handle(
        player_handle: &str,
        main_handles: &std::collections::HashSet<String>,
    ) -> bool {
        if main_handles.is_empty() {
            return false;
        }
        let normalized = Self::normalized_handle_key(player_handle);
        !normalized.is_empty() && main_handles.contains(&normalized)
    }

    pub fn is_main_player_identity(
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

        let mut map_values: std::collections::BTreeMap<String, StatsMapAggregate> =
            std::collections::BTreeMap::new();
        let mut main_commander: std::collections::BTreeMap<String, StatsCommanderAggregate> =
            std::collections::BTreeMap::new();
        let mut ally_commander: std::collections::BTreeMap<String, StatsCommanderAggregate> =
            std::collections::BTreeMap::new();
        let mut region_values: std::collections::BTreeMap<String, StatsRegionAggregate> =
            std::collections::BTreeMap::new();
        let mut difficulty_values: std::collections::BTreeMap<String, StatsWinLossAggregate> =
            std::collections::BTreeMap::new();
        let mut player_values: std::collections::BTreeMap<String, StatsPlayerAggregate> =
            std::collections::BTreeMap::new();

        let mut invalid_result = 0u64;
        let mut sum_main = StatsCommanderTotals::default();
        let mut sum_ally = StatsCommanderTotals::default();

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

            let map_snapshot = StatsReplaySnapshot {
                replay_id: 0,
                file: replay.file.clone(),
                map_name: replay.map.clone(),
                result: replay.result.clone(),
                difficulty: replay.difficulty.clone(),
                enemy_race: replay.enemy.clone(),
                date_seconds: replay.date,
                detailed_analysis: replay.is_detailed,
                brutal_plus: replay.brutal_plus,
                extension: replay.extension,
                length_realtime: replay.accurate_length,
                bonus_completed: replay.bonus.len() as u64,
                main: StatsPlayerSnapshot {
                    name: replay.main().name.clone(),
                    handle: replay.main().handle.clone(),
                    commander: main_commander_name.clone(),
                    apm: replay.main_apm(),
                    kills: replay.main_kills(),
                    commander_level: replay.main_commander_level(),
                    mastery_level: replay.main_mastery_level(),
                    prestige: replay.main_prestige(),
                    masteries: replay.main_masteries().to_vec(),
                },
                ally: StatsPlayerSnapshot {
                    name: replay.ally().name.clone(),
                    handle: replay.ally().handle.clone(),
                    commander: ally_commander_name.clone(),
                    apm: replay.ally_apm(),
                    kills: replay.ally_kills(),
                    commander_level: replay.ally_commander_level(),
                    mastery_level: replay.ally_mastery_level(),
                    prestige: replay.ally_prestige(),
                    masteries: replay.ally_masteries().to_vec(),
                },
            };
            map_values.entry(map_key).or_default().record_snapshot(
                &map_snapshot,
                replay_is_victory,
                map_bonus_total,
                false,
            );

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
            region_entry.record_result(replay_is_victory);
            if p1_is_main {
                region_entry.record_player(
                    replay.main_mastery_level(),
                    replay.main_commander_level(),
                    &main_commander_text,
                    &main_commander_name,
                    replay.main_prestige(),
                );
            }
            if p2_is_main {
                region_entry.record_player(
                    replay.ally_mastery_level(),
                    replay.ally_commander_level(),
                    &ally_commander_text,
                    &ally_commander_name,
                    replay.ally_prestige(),
                );
            }

            if !difficulty.contains('/') {
                difficulty_values
                    .entry(difficulty)
                    .or_default()
                    .record_result(replay_is_victory);
            }

            let include_prestige = ReplayAnalysisOps::should_count_prestige(replay.date);
            let main_commander_record = StatsCommanderPlayerRecord::new(
                replay_is_victory,
                replay.is_detailed,
                replay.main_apm(),
                main_kill_fraction,
                replay.main_prestige(),
                replay.main_masteries(),
                include_prestige,
            );
            let ally_commander_record = StatsCommanderPlayerRecord::new(
                replay_is_victory,
                replay.is_detailed,
                replay.ally_apm(),
                ally_kill_fraction,
                replay.ally_prestige(),
                replay.ally_masteries(),
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

            if !main_player_name.is_empty() {
                let p1 = player_values.entry(main_player_name).or_default();
                let main_player_handle =
                    TauriOverlayOps::sanitize_replay_text(&replay.main().handle);
                p1.record_replay(StatsPlayerRecord::new(
                    &replay.main().name,
                    &main_player_handle,
                    &main_commander_text,
                    replay_is_victory,
                    replay.main_apm(),
                    main_kill_fraction,
                    replay.date,
                ));
            }

            if !ally_player_name.is_empty() {
                let p2 = player_values.entry(ally_player_name).or_default();
                let ally_player_handle =
                    TauriOverlayOps::sanitize_replay_text(&replay.ally().handle);
                p2.record_replay(StatsPlayerRecord::new(
                    &replay.ally().name,
                    &ally_player_handle,
                    &ally_commander_text,
                    replay_is_victory,
                    replay.ally_apm(),
                    ally_kill_fraction,
                    replay.date,
                ));
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
            let games = aggregate.games();
            let winrate = TauriOverlayOps::ratio(aggregate.wins(), games);
            let fastest = aggregate.fastest_or_default();
            let fastest_length = if fastest.length_realtime.is_finite() {
                fastest.length_realtime
            } else {
                999_999.0
            };
            let fastest_p1 = ReplayAnalysisOps::fastest_map_player_value_with_dictionary(
                FastestMapPlayerInput {
                    name: &fastest.main.name,
                    handle: &fastest.main.handle,
                    commander: &fastest.main.commander,
                    apm: fastest.main.apm,
                    mastery_level: fastest.main.mastery_level,
                    masteries: &fastest.main.masteries,
                    prestige: fastest.main.prestige,
                },
                dictionary,
            );
            let fastest_p2 = ReplayAnalysisOps::fastest_map_player_value_with_dictionary(
                FastestMapPlayerInput {
                    name: &fastest.ally.name,
                    handle: &fastest.ally.handle,
                    commander: &fastest.ally.commander,
                    apm: fastest.ally.apm,
                    mastery_level: fastest.ally.mastery_level,
                    masteries: &fastest.ally.masteries,
                    prestige: fastest.ally.prestige,
                },
                dictionary,
            );
            let p1_is_main = ReplayAnalysis::is_main_player_identity(
                &fastest.main.name,
                &fastest.main.handle,
                main_names,
                main_handles,
            );
            let p2_is_main = ReplayAnalysis::is_main_player_identity(
                &fastest.ally.name,
                &fastest.ally.handle,
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
                ReplayAnalysisOps::report_value(&StatsAggregateMapDataRow::new(
                    map_id,
                    aggregate.average_victory_time(),
                    TauriOverlayOps::ratio(games, total_games),
                    StatsResultSummary::new(aggregate.wins(), aggregate.losses(), winrate),
                    aggregate.bonus_rate(),
                    aggregate.detailed_count(),
                    StatsAggregateFastestMapDetails::new(
                        fastest_length,
                        fastest.file,
                        fastest.date_seconds,
                        TauriOverlayOps::sanitize_replay_text(&fastest.difficulty),
                        players,
                        TauriOverlayOps::sanitize_replay_text(&fastest.enemy_race),
                    ),
                )),
            );
        }
        crate::sco_log!(
            "[SCO/stats] map_data stage done in {}ms (rows={})",
            map_started_at.elapsed().as_millis(),
            map_data.len()
        );

        let commander_started_at = Instant::now();
        let commander_data = StatsAggregationOps::build_commander_data(
            StatsCommanderDataInput::new(&main_commander, total_games, &sum_main, None),
        );
        crate::sco_log!(
            "[SCO/stats] commander_data stage done in {}ms (rows={})",
            commander_started_at.elapsed().as_millis(),
            commander_data.len()
        );

        let main_commander_frequency = main_commander
            .iter()
            .map(|(name, aggregate)| {
                let games = aggregate.games();
                (
                    name.clone(),
                    if sum_main.games() == 0 {
                        0.0
                    } else {
                        games as f64 / sum_main.games() as f64
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let ally_started_at = Instant::now();
        let ally_commander_data =
            StatsAggregationOps::build_commander_data(StatsCommanderDataInput::new(
                &ally_commander,
                total_games,
                &sum_ally,
                Some(&main_commander_frequency),
            ));
        crate::sco_log!(
            "[SCO/stats] ally_commander_data stage done in {}ms (rows={})",
            ally_started_at.elapsed().as_millis(),
            ally_commander_data.len()
        );

        let mut difficulty_data = Map::new();
        let difficulty_started_at = Instant::now();
        for (name, agg) in difficulty_values {
            let games = agg.games();
            difficulty_data.insert(
                name,
                ReplayAnalysisOps::report_value(&StatsAggregateDifficultyDataRow::new(
                    StatsResultSummary::new(
                        agg.wins(),
                        agg.losses(),
                        TauriOverlayOps::ratio(agg.wins(), games),
                    ),
                )),
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
            let games = agg.games();
            let mut max_com: Vec<String> = agg
                .max_com()
                .iter()
                .map(|value| TauriOverlayOps::sanitize_replay_text(value))
                .filter(|value| !value.is_empty())
                .collect();
            max_com.sort();
            max_com.dedup();
            let prestiges = agg
                .prestiges()
                .iter()
                .filter_map(|(commander, value)| {
                    let commander = TauriOverlayOps::sanitize_replay_text(commander);
                    if commander.is_empty() {
                        None
                    } else {
                        Some((commander, Value::from(*value)))
                    }
                })
                .collect::<Map<String, Value>>();
            region_data.insert(
                name,
                ReplayAnalysisOps::report_value(&StatsAggregateRegionDataRow::new(
                    TauriOverlayOps::ratio(games, total_games),
                    StatsResultSummary::new(
                        agg.wins(),
                        agg.losses(),
                        TauriOverlayOps::ratio(agg.wins(), games),
                    ),
                    agg.max_asc(),
                    prestiges,
                    max_com,
                )),
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
            let games = agg.games();
            let (commander, commander_frequency) = agg.dominant_commander();
            player_data.insert(
                name,
                ReplayAnalysisOps::report_value(&StatsAggregatePlayerDataRow::new(
                    StatsResultSummary::new(
                        agg.wins(),
                        agg.losses(),
                        TauriOverlayOps::ratio(agg.wins(), games),
                    ),
                    TauriOverlayOps::median_f64(agg.kill_fractions()),
                    if games == 0 {
                        0.0
                    } else {
                        TauriOverlayOps::median_u64(agg.apm_values())
                    },
                    commander_frequency,
                    agg.last_seen(),
                    TauriOverlayOps::sanitize_replay_text(&commander),
                )),
            );
        }
        crate::sco_log!(
            "[SCO/stats] player_data stage done in {}ms (rows={})",
            player_started_at.elapsed().as_millis(),
            player_data.len()
        );

        let prestige_names = dictionary.prestige_names_json.clone();

        let unit_data = if include_detailed {
            ReplayAnalysisOps::build_unit_data_from_replays_with_dictionary(
                replays,
                main_handles,
                dictionary,
            )
        } else {
            Value::Null
        };
        let analysis =
            ReplayAnalysisOps::report_value(&StatsAggregateAnalysisPayload::new_ready_map_data(
                map_data,
                commander_data,
                ally_commander_data,
                difficulty_data,
                region_data,
                player_data,
                unit_data,
            ));

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
        let mut player_values: std::collections::BTreeMap<String, StatsPlayerAggregate> =
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
                let p1_handle = TauriOverlayOps::sanitize_replay_text(&replay.main().handle);
                p1.record_replay(StatsPlayerRecord::new(
                    &p1_name,
                    &p1_handle,
                    &main_commander,
                    replay_is_victory,
                    replay.main_apm(),
                    main_kill_fraction,
                    replay.date,
                ));
            }

            if !p2_name.is_empty() {
                let p2_handle_key = ReplayAnalysis::normalized_handle_key(&replay.ally().handle);
                let p2 = player_values.entry(p2_handle_key).or_default();
                let p2_handle = TauriOverlayOps::sanitize_replay_text(&replay.ally().handle);
                p2.record_replay(StatsPlayerRecord::new(
                    &p2_name,
                    &p2_handle,
                    &ally_commander,
                    replay_is_victory,
                    replay.ally_apm(),
                    ally_kill_fraction,
                    replay.date,
                ));
            }
        }

        let mut rows = Vec::new();
        for (handle_key, agg) in player_values {
            if handle_key.is_empty() {
                continue;
            }
            let games = agg.games();
            let (commander, commander_frequency) = agg.dominant_commander();
            let apm = if games == 0 {
                0.0
            } else {
                TauriOverlayOps::median_u64(agg.apm_values())
            };
            let handle = agg
                .handles()
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
                wins: agg.wins(),
                losses: agg.losses(),
                winrate: TauriOverlayOps::ratio(agg.wins(), games),
                apm,
                commander: TauriOverlayOps::sanitize_replay_text(&commander),
                frequency: commander_frequency,
                kills: TauriOverlayOps::median_f64(agg.kill_fractions()),
                last_seen: agg.last_seen(),
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
            if let Some(rest) = lower.strip_prefix("b+")
                && let Ok(level) = rest.trim().parse::<u64>()
            {
                let level = level.min(6);
                return (100 + level as i64, format!("B+{level}"));
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
                .or_else(|| {
                    ReplayAnalysisOps::resolve_weekly_mutation_name_with_dictionary(
                        &replay.map,
                        &replay.mutators,
                        dictionary,
                    )
                    .map(|value| TauriOverlayOps::sanitize_replay_text(&value))
                    .filter(|value| !value.is_empty())
                })
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
        let entries = ReplayAnalysisOps::recover_cache_entries_from_temp(
            cache_path,
            "detailed-analysis cache",
            ReplayCacheEntryQuery::detailed_only(0),
        );
        let replays = Self::detailed_analysis_replays_snapshot_from_entries_with_dictionary(
            &entries,
            limit,
            main_names,
            main_handles,
            dictionary,
        );

        crate::sco_log!(
            "[SCO/cache] loaded {} replay(s) from detailed-analysis cache '{}'",
            replays.len(),
            ReplayCacheDatabase::db_path_for_cache_path(cache_path).display()
        );
        replays
    }

    pub fn detailed_analysis_replays_snapshot_from_entries_with_dictionary(
        entries: &[CacheReplayEntry],
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let mut replays = entries
            .iter()
            .filter(|entry| entry.detailed_analysis && Path::new(&entry.file).exists())
            .map(|entry| {
                ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(entry, dictionary)
                    .oriented_for_main_identity(main_names, main_handles)
            })
            .collect::<Vec<_>>();

        replays.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.file.cmp(&a.file)));
        if limit > 0 && replays.len() > limit {
            replays.truncate(limit);
        }
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

    pub fn load_all_analysis_replays_snapshot_from_path(
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

    pub fn load_all_analysis_replays_snapshot_from_path_with_dictionary(
        cache_path: &Path,
        limit: usize,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Vec<ReplayInfo> {
        let mut replays = ReplayAnalysisOps::read_cache_summary_entries(
            cache_path,
            "unified cache",
            ReplayCacheEntryQuery::all(0),
        )
        .into_iter()
        .filter(|entry| Path::new(&entry.file).exists())
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
            "[SCO/cache] loaded {} replay(s) from unified cache '{}' (includes both simple and detailed)",
            replays.len(),
            ReplayCacheDatabase::db_path_for_cache_path(cache_path).display()
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

        let cache_path = PathManagerOps::get_cache_path();
        let analyzed_files = ReplayCacheDatabase::open_for_cache_path(&cache_path)
            .and_then(|database| database.load_cached_files())
            .map_err(|error| {
                crate::sco_log!("[SCO/cache] failed to load cached replay file list: {error}");
                error
            })
            .unwrap_or_default();

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
            let mut replays =
                Self::load_all_analysis_replays_snapshot(limit, main_names, main_handles);
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

        scan_progress.set_cache_hits(0);
        scan_progress.set_to_parse(paths_to_parse_len as u64);

        let parse_started_at = Instant::now();
        scan_progress.set_stage("parsing_replays");
        let worker_threads = crate::AppSettings::simple_analysis_worker_threads();
        let progress = scan_progress;
        let cache_writer = ReplayCacheWriteQueue::start(cache_path.clone());
        let parse_options = ReplayCacheParallelParseOptions::simple_saved_cache(worker_threads)
            .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(
                cache_writer.sender(),
            )))
            .with_cache_entry_sink_batch_size(DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE);
        let parsed_results = match DetailedReplayAnalyzer::parse_saved_cache_entries_parallel_map(
            paths_to_parse,
            resources,
            &parse_options,
            |parsed_entry| {
                let (path, cache_entry, panicked) = parsed_entry.into_parts();
                progress.increment_completed();
                if panicked {
                    progress.increment_failed();
                    return ParsedReplayPathResult::new(
                        ReplayAnalysisOps::unparsed_replay(&path),
                        Some(path.to_string_lossy().to_string()),
                    );
                }

                progress.increment_newly_parsed();
                let replay = cache_entry
                    .as_ref()
                    .map(|entry| {
                        ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                            entry,
                            resources.dictionary_data(),
                        )
                        .sanitized()
                    })
                    .unwrap_or_else(|| ReplayAnalysisOps::unparsed_replay(&path))
                    .oriented_for_main_identity(main_names, main_handles);
                ParsedReplayPathResult::new(replay, None)
            },
        ) {
            Ok(result) => result.into_values(),
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache] failed to parse simple analysis worker batch: {error}"
                );
                Vec::new()
            }
        };
        drop(parse_options);
        let cache_writer_result = cache_writer.finish();

        let parsed_batch =
            parsed_results
                .into_iter()
                .fold(ParsedReplayBatch::new(), |mut batch, parsed| {
                    let (replay, failed_path) = parsed.into_parts();
                    if let Some(failed_path) = failed_path {
                        batch.push_failure(failed_path);
                    }
                    batch.push_success(replay);
                    batch
                });

        let failed_to_parse = parsed_batch.failed_paths;
        let parsed_replays = parsed_batch.replays;
        let persisted_cache_entries = cache_writer_result.persisted_entries();

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
            parsed_replays.len(),
            parse_started_at.elapsed().as_millis()
        );

        scan_progress.set_stage("finalizing_results");
        scan_progress.set_status("Finalizing results");
        crate::sco_log!(
            "[SCO/replay] finalizing {} parsed replay result(s) against {} cached replay file(s)",
            parsed_replays.len(),
            analyzed_files.len()
        );

        let mut replay_map = HashMap::<String, ReplayInfo>::new();
        for replay in Self::load_all_analysis_replays_snapshot(
            UNLIMITED_REPLAY_LIMIT,
            main_names,
            main_handles,
        ) {
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

        for replay in parsed_replays {
            let replay_hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(&replay.file));
            if replay_hash.is_empty() {
                continue;
            }
            replay_map.retain(|hash, cached| hash == &replay_hash || cached.file != replay.file);
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

        crate::sco_log!(
            "[SCO/cache] persisted {} simple-analysis cache entr(y/ies) with writer batches of {}",
            persisted_cache_entries,
            DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE
        );
        if cache_writer_result.failed_batches() > 0 {
            crate::sco_log!(
                "[SCO/cache] failed to persist {} simple-analysis cache writer batch(es)",
                cache_writer_result.failed_batches()
            );
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

    pub fn replay_matches_stats_filters_with_dictionary(
        path: &str,
        replay: &ReplayInfo,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> bool {
        StatsQuery::from_path(path).matches_replay(replay, main_handles, dictionary)
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
            .filter(|replay| replay.has_detailed_analysis_cache())
            .count() as u64;
        (detailed_parsed_count, total_valid_files)
    }

    pub fn stats_response_has_detailed_analysis(response: &Value) -> bool {
        response
            .get("analysis")
            .and_then(|value| value.get("UnitData"))
            .is_some_and(|value| !value.is_null())
    }

    pub fn build_stats_response(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        _replays: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
    ) -> Result<Value, String> {
        let (main_names, main_handles) = ReplayAnalysisOps::default_main_identity();
        Self::build_stats_response_with_identity(
            path,
            stats,
            _replays,
            stats_current_replay_files,
            ReplayScanProgress::default().as_payload(),
            &main_names,
            &main_handles,
        )
    }

    pub fn build_stats_response_with_identity(
        path: &str,
        stats: &Arc<Mutex<StatsState>>,
        _replays: &Arc<Mutex<HashMap<String, ReplayInfo>>>,
        stats_current_replay_files: &Arc<Mutex<HashSet<String>>>,
        scan_progress: ReplayScanProgressPayload,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Result<Value, String> {
        let dictionary = Sc2DictionaryData::default();
        Self::build_stats_response_with_dictionary(
            StatsResponseBuildInput::new(
                path,
                stats,
                stats_current_replay_files,
                scan_progress,
                main_names,
                main_handles,
            ),
            &dictionary,
        )
    }

    pub fn build_stats_response_with_dictionary(
        input: StatsResponseBuildInput<'_>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<Value, String> {
        let StatsResponseBuildInput {
            path,
            stats,
            stats_current_replay_files,
            scan_progress,
            main_names,
            main_handles,
        } = input;
        let stats_query = StatsQuery::from_path(path);
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
            let cache_path = PathManagerOps::get_cache_path();
            match stats_current_replay_files.try_lock() {
                Ok(current_replay_files) => {
                    let summary_query = stats_query.to_cache_query(
                        ReplayCacheReadScope::All,
                        UNLIMITED_REPLAY_LIMIT,
                        main_handles,
                        &current_replay_files,
                    );
                    match ReplayCacheDatabase::open_for_cache_path(&cache_path).and_then(
                        |database| {
                            database.load_statistics_payload(
                                &summary_query,
                                main_names,
                                main_handles,
                                dictionary,
                            )
                        },
                    ) {
                        Ok(payload) => {
                            response["analysis"] = payload.analysis().clone();
                            response["prestige_names"] =
                                ReplayAnalysisOps::report_value(payload.prestige_names());
                            response["games"] = Value::from(payload.games());
                            response["detailed_parsed_count"] =
                                Value::from(payload.detailed_parsed_count());
                            response["total_valid_files"] =
                                Value::from(payload.total_valid_files());
                            response["main_players"] =
                                ReplayAnalysisOps::report_value(&payload.main_players());
                            response["main_handles"] =
                                ReplayAnalysisOps::report_value(&payload.main_handles());
                        }
                        Err(error) => {
                            crate::sco_log!(
                                "[SCO/cache] failed to build filtered statistics from database '{}': {error}",
                                ReplayCacheDatabase::db_path_for_cache_path(&cache_path).display()
                            );
                        }
                    }
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Poisoned(_)) => {
                    return Err(
                        "Failed to access current replay file set: mutex is poisoned".to_string(),
                    );
                }
            }
        }
        if let Some(query) = path.split('?').nth(1) {
            response["query"] = Value::from(query);
        }

        Ok(response)
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
