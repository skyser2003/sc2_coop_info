use crate::detailed_replay_analysis::GenerateCacheError;
use crate::tauri_replay_analysis_impl::{
    ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer, ReplayReport,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value as JsonValue};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use crate::detailed_replay_analysis::{ProtocolBuildValue, ReplayBuildInfo};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheNumericValue {
    Integer(u64),
    Float(f64),
}

pub type ReplayMessage = ParsedReplayMessage;
type CacheUnitStatsTuple = (i64, i64, i64, f64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisPlayerStatsSeries {
    pub name: String,
    pub supply: Vec<f64>,
    pub mining: Vec<f64>,
    pub army: Vec<f64>,
    pub killed: Vec<f64>,
    #[serde(skip, default)]
    pub army_force_float_indices: BTreeSet<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum CacheCountValue {
    Count(i64),
    Hidden(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheUnitStats(pub CacheCountValue, pub CacheCountValue, pub i64, pub f64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachePlayerStatsSeries {
    pub name: String,
    pub supply: Vec<f64>,
    pub mining: Vec<f64>,
    pub army: Vec<CacheStatValue>,
    pub killed: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheStatValue {
    Integer(u64),
    Float(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheIconValue {
    Count(u64),
    Order(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachePlayer {
    pub pid: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apm: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander_mastery_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<BTreeMap<String, CacheIconValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kills: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masteries: Option<[u32; 6]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prestige: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prestige_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<BTreeMap<String, CacheUnitStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheReplayEntry {
    pub accurate_length: CacheNumericValue,
    pub amon_units: Option<BTreeMap<String, CacheUnitStats>>,
    pub bonus: Option<Vec<String>>,
    pub brutal_plus: u32,
    pub build: ReplayBuildInfo,
    pub comp: Option<String>,
    pub date: String,
    pub difficulty: (String, String),
    pub enemy_race: Option<String>,
    pub ext_difficulty: String,
    pub extension: bool,
    pub file: String,
    pub form_alength: String,
    pub detailed_analysis: bool,
    pub hash: String,
    pub length: u64,
    pub map_name: String,
    pub messages: Vec<ReplayMessage>,
    pub mutators: Vec<String>,
    pub player_stats: Option<BTreeMap<u8, CachePlayerStatsSeries>>,
    pub players: Vec<CachePlayer>,
    pub region: String,
    pub result: String,
    pub weekly: bool,
}

#[derive(Debug, Error)]
pub enum PrettyCacheError {
    #[error("failed to read cache file '{0}': {1}")]
    ReadFailed(PathBuf, #[source] io::Error),
    #[error("failed to parse cache json '{0}': {1}")]
    ParseFailed(PathBuf, #[source] serde_json::Error),
    #[error("failed to serialize pretty cache json '{0}': {1}")]
    SerializeFailed(PathBuf, #[source] serde_json::Error),
    #[error("failed to write pretty cache file '{0}': {1}")]
    WriteFailed(PathBuf, #[source] io::Error),
}

pub fn pretty_output_path(path: &Path) -> PathBuf {
    let extension = path.extension();
    let file_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("cache_overall_stats");

    path.with_file_name(format!(
        "{file_name}_pretty.{}",
        extension.and_then(|s| s.to_str()).unwrap_or("json")
    ))
}

pub fn write_pretty_cache_file(
    minified_path: &Path,
    pretty_path: Option<&Path>,
) -> Result<PathBuf, PrettyCacheError> {
    let payload = fs::read(minified_path)
        .map_err(|error| PrettyCacheError::ReadFailed(minified_path.to_path_buf(), error))?;
    let parsed: JsonValue = serde_json::from_slice(&payload)
        .map_err(|error| PrettyCacheError::ParseFailed(minified_path.to_path_buf(), error))?;
    let target_path = pretty_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| pretty_output_path(minified_path));
    let pretty_text = serde_json::to_string_pretty(&parsed)
        .map_err(|error| PrettyCacheError::SerializeFailed(minified_path.to_path_buf(), error))?;
    fs::write(&target_path, format!("{pretty_text}\n"))
        .map_err(|error| PrettyCacheError::WriteFailed(target_path.clone(), error))?;
    Ok(target_path)
}

pub(crate) fn normalized_path_string(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        normalized.push(component.as_os_str());
    }
    normalized.display().to_string()
}

pub(crate) fn duration_to_u64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round_ties_even() as u64
    }
}

fn normalize_json_float(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    if value == 0.0 {
        return 0.0;
    }
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

impl CachePlayer {
    fn normalized_commander_name(commander: &str) -> String {
        match commander {
            "Han & Horner" => "Horner".to_string(),
            _ => commander.to_string(),
        }
    }

    fn empty(pid: u8) -> Self {
        Self {
            pid,
            apm: None,
            commander: None,
            commander_level: None,
            commander_mastery_level: None,
            handle: None,
            icons: None,
            kills: None,
            masteries: None,
            name: None,
            observer: None,
            prestige: None,
            prestige_name: None,
            race: None,
            result: None,
            units: None,
        }
    }

    fn from_parsed_player(player: &ParsedReplayPlayer) -> Self {
        if player.pid == 0 || player.is_placeholder() {
            return Self::empty(player.pid);
        }

        Self {
            pid: player.pid,
            apm: Some(player.apm),
            commander: Some(Self::normalized_commander_name(player.commander.as_str())),
            commander_level: Some(player.commander_level),
            commander_mastery_level: Some(player.commander_mastery_level),
            handle: Some(player.handle.clone()),
            icons: None,
            kills: None,
            masteries: Some(player.masteries),
            name: Some(player.name.clone()),
            observer: Some(player.observer),
            prestige: Some(player.prestige),
            prestige_name: Some(player.prestige_name.clone()),
            race: Some(player.race.clone()),
            result: Some(player.result.clone()),
            units: None,
        }
    }
}

impl CacheStatValue {
    fn from_army_value(value: f64, force_float: bool) -> Self {
        if !value.is_finite() || value < 0.0 {
            return Self::Integer(0);
        }
        if force_float {
            return Self::Float(value);
        }
        if value == 0.0 {
            return Self::Integer(0);
        }
        if value.fract().abs() < 1e-9 {
            Self::Integer(duration_to_u64(value))
        } else {
            Self::Float(value)
        }
    }
}

impl CacheNumericValue {
    pub(crate) fn from_duration(value: f64, force_float: bool) -> Self {
        if force_float {
            Self::Float(value)
        } else if !value.is_finite() || value <= 0.0 {
            Self::Integer(0)
        } else if value.fract().abs() < 1e-9 {
            Self::Integer(duration_to_u64(value))
        } else {
            Self::Float(value)
        }
    }
}

impl CachePlayerStatsSeries {
    fn from_analysis_player_stats(stats: &AnalysisPlayerStatsSeries) -> Self {
        Self {
            name: stats.name.clone(),
            supply: stats.supply.clone(),
            mining: stats.mining.clone(),
            army: stats
                .army
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    CacheStatValue::from_army_value(
                        *value,
                        stats.army_force_float_indices.contains(&index),
                    )
                })
                .collect(),
            killed: stats
                .killed
                .iter()
                .map(|value| duration_to_u64(*value))
                .collect(),
        }
    }

    fn from_analysis_player_stats_map(
        player_stats: &BTreeMap<u8, AnalysisPlayerStatsSeries>,
    ) -> BTreeMap<u8, Self> {
        let mut out = BTreeMap::new();
        for (player_id, stats) in player_stats {
            out.insert(*player_id, Self::from_analysis_player_stats(stats));
        }
        out
    }
}

impl CacheUnitStats {
    fn from_unit_stats(unit_stats: &CacheUnitStatsTuple, hide_counts: bool) -> Self {
        let (created, lost, kills, kill_fraction) = *unit_stats;
        Self(
            if hide_counts {
                CacheCountValue::Hidden("-".to_string())
            } else {
                CacheCountValue::Count(created)
            },
            if hide_counts {
                CacheCountValue::Hidden("-".to_string())
            } else {
                CacheCountValue::Count(lost)
            },
            kills,
            kill_fraction,
        )
    }

    fn from_unit_stats_map(
        units: &BTreeMap<String, CacheUnitStatsTuple>,
        hidden_created_lost: Option<&HashSet<String>>,
    ) -> BTreeMap<String, Self> {
        let mut out = BTreeMap::new();
        for (unit_name, unit_stats) in units {
            let hide_counts =
                hidden_created_lost.is_some_and(|values| values.contains(unit_name.as_str()));
            out.insert(
                unit_name.clone(),
                Self::from_unit_stats(unit_stats, hide_counts),
            );
        }
        out
    }
}

impl CacheIconValue {
    fn from_icon_counts(
        icons: &BTreeMap<String, u64>,
        outlaw_order: Option<Vec<String>>,
    ) -> BTreeMap<String, Self> {
        let mut out = BTreeMap::new();
        for (icon_key, count) in icons {
            out.insert(icon_key.clone(), Self::Count(*count));
        }
        if let Some(order) = outlaw_order {
            out.insert("outlaws".to_string(), Self::Order(order));
        }
        out
    }
}

impl CacheReplayEntry {
    pub fn from_report(report: &ReplayReport, hidden_created_lost: &HashSet<String>) -> Self {
        Self::from_report_with_basic(report, None, hidden_created_lost)
    }

    pub(crate) fn from_report_with_basic(
        report: &ReplayReport,
        basic: Option<&Self>,
        hidden_created_lost: &HashSet<String>,
    ) -> Self {
        let mut entry = Self::fallback_from_parser(&report.parser);
        if let Some(basic) = basic {
            entry.apply_basic_overrides(basic);
        }
        entry.amon_units = Some(CacheUnitStats::from_unit_stats_map(
            &report.amon_units,
            None,
        ));
        entry.bonus = Some(report.bonus.clone());
        entry.comp = Some(report.comp.clone());
        entry.player_stats = Some(CachePlayerStatsSeries::from_analysis_player_stats_map(
            &report.player_stats,
        ));
        entry.apply_report_player_overlay(report, hidden_created_lost);
        entry.detailed_analysis = true;
        entry
    }

    pub(crate) fn from_parser_projection(
        parser: &ParsedReplayInput,
        players: &[ParsedReplayPlayer],
        messages: &[ParsedReplayMessage],
        accurate_length_force_float: bool,
        enemy_race_present: bool,
        detailed_fallback: bool,
    ) -> Self {
        let accurate_length = normalize_json_float(parser.accurate_length);
        let accurate_length = if detailed_fallback {
            CacheNumericValue::Float(accurate_length)
        } else {
            CacheNumericValue::from_duration(accurate_length, accurate_length_force_float)
        };
        let enemy_race = if detailed_fallback {
            Some(parser.enemy_race.clone())
        } else {
            enemy_race_present.then_some(parser.enemy_race.clone())
        };

        Self {
            accurate_length,
            amon_units: None,
            bonus: None,
            brutal_plus: parser.brutal_plus,
            build: parser.build.clone(),
            comp: None,
            date: parser.date.clone(),
            difficulty: parser.difficulty.clone(),
            enemy_race,
            ext_difficulty: parser.ext_difficulty.clone(),
            extension: parser.extension,
            file: normalized_path_string(Path::new(&parser.file)),
            form_alength: parser.form_alength.clone(),
            detailed_analysis: false,
            hash: parser.hash.clone().unwrap_or_default(),
            length: parser.length,
            map_name: parser.map_name.clone(),
            messages: messages.to_vec(),
            mutators: parser.mutators.clone(),
            player_stats: None,
            players: players
                .iter()
                .map(CachePlayer::from_parsed_player)
                .collect(),
            region: parser.region.clone(),
            result: parser.result.clone(),
            weekly: parser.weekly,
        }
    }

    pub(crate) fn fallback_from_parser(parser: &ParsedReplayInput) -> Self {
        let players = parser.normalized_cache_players();
        Self::from_parser_projection(
            parser,
            &players,
            &parser.messages,
            false,
            !parser.enemy_race.is_empty(),
            true,
        )
    }

    fn apply_basic_overrides(&mut self, basic: &Self) {
        self.brutal_plus = basic.brutal_plus;
        self.build = basic.build.clone();
        self.date = basic.date.clone();
        self.difficulty = basic.difficulty.clone();
        self.enemy_race = basic.enemy_race.clone();
        self.ext_difficulty = basic.ext_difficulty.clone();
        self.extension = basic.extension;
        self.file = basic.file.clone();
        self.hash = basic.hash.clone();
        self.length = basic.length;
        self.map_name = basic.map_name.clone();
        self.result = basic.result.clone();
    }

    fn apply_report_player_overlay(
        &mut self,
        report: &ReplayReport,
        hidden_created_lost: &HashSet<String>,
    ) {
        self.players = self
            .players
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<CachePlayer>>();
        while self.players.len() < 3 {
            self.players
                .push(CachePlayer::empty(self.players.len() as u8));
        }

        let main_index = match report.positions.main {
            1 | 2 => usize::from(report.positions.main),
            _ => 1,
        };

        for player_index in [1_usize, 2_usize] {
            let use_main = player_index == main_index;
            let player = self
                .players
                .get_mut(player_index)
                .expect("players vector must contain pids 0, 1, and 2");
            player.kills = Some(if use_main {
                report.main_kills
            } else {
                report.ally_kills
            });
            let commander_name = if use_main {
                report.main_commander.as_str()
            } else {
                report.ally_commander.as_str()
            };
            player.icons = Some(CacheIconValue::from_icon_counts(
                if use_main {
                    &report.main_icons
                } else {
                    &report.ally_icons
                },
                if commander_name == "Tychus" {
                    Some(report.outlaw_order.clone().unwrap_or_default())
                } else {
                    None
                },
            ));
            player.units = Some(CacheUnitStats::from_unit_stats_map(
                if use_main {
                    &report.main_units
                } else {
                    &report.ally_units
                },
                Some(hidden_created_lost),
            ));
        }
    }

    fn sort_date_key(&self) -> Option<String> {
        let compact = self.date.replace(':', "");
        if compact.len() == 14 && compact.chars().all(|ch| ch.is_ascii_digit()) {
            Some(compact)
        } else {
            None
        }
    }

    pub(crate) fn cmp_cache_order(&self, other: &Self) -> Ordering {
        match (self.sort_date_key(), other.sort_date_key()) {
            (Some(left_date), Some(right_date)) => left_date
                .cmp(&right_date)
                .then_with(|| self.file.cmp(&other.file))
                .then_with(|| self.hash.cmp(&other.hash)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => self
                .file
                .cmp(&other.file)
                .then_with(|| self.hash.cmp(&other.hash)),
        }
    }

    pub fn serialize_entries(entries: &[Self]) -> Result<Vec<u8>, serde_json::Error> {
        let mut canonical_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let value = serde_json::to_value(entry)?;
            canonical_entries.push(Self::canonicalize_json_value(value));
        }
        serde_json::to_vec(&canonical_entries)
    }

    pub fn write_entries(entries: &[Self], path: &Path) -> Result<(), GenerateCacheError> {
        let path = path.to_path_buf();
        let payload =
            Self::serialize_entries(entries).map_err(GenerateCacheError::SerializeFailed)?;

        let temp_file = PathBuf::from(format!("{}.temp", path.display()));

        fs::write(&temp_file, payload)
            .map_err(|error| GenerateCacheError::TempWriteFailed(temp_file.clone(), error))?;

        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                GenerateCacheError::TempMoveFailed(temp_file.clone(), path.clone(), error)
            })?;
        }

        fs::rename(&temp_file, &path)
            .map_err(|error| GenerateCacheError::TempMoveFailed(temp_file, path.clone(), error))?;

        Ok(())
    }

    pub fn load_existing_detailed_cache_entries(
        cache_path: &Path,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
    ) -> HashMap<String, Self> {
        let payload = match fs::read(cache_path) {
            Ok(payload) => payload,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
            Err(error) => {
                if let Some(logger) = logger {
                    logger(format!(
                        "Ignoring existing cache '{}': failed to read: {error}",
                        cache_path.display()
                    ));
                }
                return HashMap::new();
            }
        };
        let entries = match serde_json::from_slice::<Vec<Self>>(&payload) {
            Ok(entries) => entries,
            Err(error) => {
                if let Some(logger) = logger {
                    logger(format!(
                        "Ignoring existing cache '{}': failed to parse: {error}",
                        cache_path.display()
                    ));
                }
                return HashMap::new();
            }
        };

        entries
            .into_iter()
            .filter(|entry| entry.detailed_analysis && !entry.hash.is_empty())
            .map(|entry| (entry.hash.clone(), entry))
            .collect()
    }

    pub fn persist_simple_cache_entries(
        entries: &[Self],
        cache_path: &Path,
    ) -> Result<(), GenerateCacheError> {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                GenerateCacheError::OutputDirectoryCreateFailed(parent.to_path_buf(), error)
            })?;
        }

        let all_entries = match std::fs::read(cache_path) {
            Ok(payload) => serde_json::from_slice::<Vec<Self>>(&payload).map_err(|error| {
                GenerateCacheError::ParseExistingCache(cache_path.to_path_buf(), error)
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(GenerateCacheError::ReadExistingCache(
                    cache_path.to_path_buf(),
                    error,
                ))
            }
        };

        let mut merged_entries = all_entries
            .into_iter()
            .filter(|entry| !entry.hash.is_empty())
            .map(|entry| (entry.hash.clone(), entry))
            .collect::<HashMap<_, _>>();

        for entry in entries {
            if entry.hash.is_empty() {
                continue;
            }

            merged_entries
                .retain(|hash, existing| hash == &entry.hash || existing.file != entry.file);
            match merged_entries.get(&entry.hash) {
                Some(existing) if existing.detailed_analysis && !entry.detailed_analysis => {}
                _ => {
                    merged_entries.insert(entry.hash.clone(), entry.clone());
                }
            }
        }

        let mut all_entries = merged_entries.into_values().collect::<Vec<_>>();
        all_entries.sort_by(|left, right| {
            right
                .date
                .cmp(&left.date)
                .then_with(|| right.file.cmp(&left.file))
        });

        Self::write_entries(&all_entries, cache_path)
    }

    fn canonicalize_json_value(value: JsonValue) -> JsonValue {
        match value {
            JsonValue::Null | JsonValue::Bool(_) | JsonValue::String(_) => value,
            JsonValue::Number(number) => {
                if number.is_f64() {
                    let normalized = normalize_json_float(number.as_f64().unwrap_or_default());
                    match Number::from_f64(normalized) {
                        Some(value) => JsonValue::Number(value),
                        None => JsonValue::Null,
                    }
                } else {
                    JsonValue::Number(number)
                }
            }
            JsonValue::Array(values) => JsonValue::Array(
                values
                    .into_iter()
                    .map(Self::canonicalize_json_value)
                    .collect::<Vec<JsonValue>>(),
            ),
            JsonValue::Object(map) => {
                let mut entries = map.into_iter().collect::<Vec<(String, JsonValue)>>();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                let mut sorted = Map::new();
                for (key, value) in entries {
                    sorted.insert(key, Self::canonicalize_json_value(value));
                }
                JsonValue::Object(sorted)
            }
        }
    }
}

impl AnalysisPlayerStatsSeries {
    pub(crate) fn empty_named(name: String) -> Self {
        Self {
            name,
            supply: Vec::new(),
            mining: Vec::new(),
            army: Vec::new(),
            killed: Vec::new(),
            army_force_float_indices: Default::default(),
        }
    }
}
