use crate::detailed_replay_analysis::{
    analyze_replay_file, cache_hidden_created_lost_units, calculate_replay_hash,
};
use crate::dictionary_data::{self, CacheGenerationData, CachedMutatorsJson, MutatorIdsJson};
use crate::tauri_replay_analysis_impl::{ParsedReplayPlayer, ReplayReport};
use chrono::{DateTime, Local};
use indexmap::IndexMap;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::ThreadPoolBuilder;
use s2protocol_port::{
    build_protocol_store, parse_file_with_store, MessageEvent, ProtocolStore, ReplayEvent,
    ReplayInitData, ReplayParseMode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value as JsonValue};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};
use thiserror::Error;
use walkdir::WalkDir;

type NumericUnitStats = (i64, i64, i64, f64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProtocolBuildValue {
    Int(u32),
    Str(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayBuildInfo {
    pub replay_build: u32,
    pub protocol_build: ProtocolBuildValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheNumericValue {
    Integer(u64),
    Float(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayMessage {
    pub text: String,
    pub player: u8,
    pub time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerStatsSeries {
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

impl CacheCountValue {
    fn hidden() -> Self {
        Self::Hidden("-".to_string())
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheConfig {
    pub account_dir: PathBuf,
    pub output_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheSummary {
    pub scanned_replays: usize,
    pub output_file: PathBuf,
    pub completed: bool,
}

#[derive(Debug, Default)]
pub struct GenerateCacheStopController {
    stop_requested: AtomicBool,
}

impl GenerateCacheStopController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_stop(&self) {
        self.stop_requested.store(true, AtomicOrdering::Release);
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested.load(AtomicOrdering::Acquire)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GenerateCacheRuntimeOptions {
    pub worker_count: Option<usize>,
    pub stop_controller: Option<Arc<GenerateCacheStopController>>,
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

#[derive(Debug, Error)]
pub enum GenerateCacheError {
    #[error("account directory does not exist or is not a directory: {0}")]
    InvalidAccountDirectory(PathBuf),
    #[error("failed to create output directory '{0}': {1}")]
    OutputDirectoryCreateFailed(PathBuf, #[source] io::Error),
    #[error("failed to load detailed-analysis cache formatting rules: {0}")]
    DetailedAnalysisConfig(String),
    #[error("failed to build rayon thread pool: {0}")]
    ThreadPoolBuildFailed(String),
    #[error("failed to serialize cache payload: {0}")]
    SerializeFailed(#[source] serde_json::Error),
    #[error("failed to write cache temp file '{0}': {1}")]
    TempWriteFailed(PathBuf, #[source] io::Error),
    #[error("failed to replace cache file '{1}' from temp '{0}': {2}")]
    TempMoveFailed(PathBuf, PathBuf, #[source] io::Error),
    #[error(transparent)]
    PrettyCache(#[from] PrettyCacheError),
    #[error("failed to read existing cache file '{0}': {1}")]
    ReadExistingCache(PathBuf, #[source] io::Error),
    #[error("failed to parse existing cache file '{0}': {1}")]
    ParseExistingCache(PathBuf, #[source] serde_json::Error),
}

struct GenerateCacheProgressReporter<'a> {
    logger: Option<&'a (dyn Fn(String) + Send + Sync + 'a)>,
    total_files: usize,
    report_interval: usize,
    temp_save_interval: usize,
    start_time: Instant,
    processed_files: AtomicUsize,
    next_report_target: AtomicUsize,
    next_temp_save_target: AtomicUsize,
    temp_file_path: PathBuf,
    temp_entries: std::sync::Mutex<Vec<CacheReplayEntry>>,
}

impl<'a> GenerateCacheProgressReporter<'a> {
    fn new(
        total_files: usize,
        initial_processed_files: usize,
        logger: Option<&'a (dyn Fn(String) + Send + Sync + 'a)>,
        temp_file_path: PathBuf,
    ) -> Self {
        let report_interval = if total_files <= 10 { 1 } else { 10 };
        let temp_save_interval = 100;
        let initial_processed_files = initial_processed_files.min(total_files);
        Self {
            logger,
            total_files,
            report_interval,
            temp_save_interval,
            start_time: Instant::now(),
            processed_files: AtomicUsize::new(initial_processed_files),
            next_report_target: AtomicUsize::new(next_progress_target(
                total_files,
                report_interval,
                initial_processed_files,
            )),
            next_temp_save_target: AtomicUsize::new(next_progress_target(
                total_files,
                temp_save_interval,
                initial_processed_files,
            )),
            temp_file_path,
            temp_entries: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn log_start(&self) {
        if self.total_files == 0 {
            self.log_completion();
            return;
        }

        self.emit("Starting detailed analysis!".to_string());
        self.emit(self.progress_message(self.processed_files.load(AtomicOrdering::Relaxed)));
    }

    fn record_processed_file(&self) {
        if self.logger.is_none() || self.total_files == 0 {
            return;
        }

        let processed = self.processed_files.fetch_add(1, AtomicOrdering::Relaxed) + 1;

        if processed == self.total_files {
            self.emit(self.progress_message(processed));
            // Save any remaining temp entries
            let _ = self.save_temp_entries();
            return;
        }

        // Check for progress reporting
        let mut target = self.next_report_target.load(AtomicOrdering::Relaxed);
        while processed >= target {
            let next_target = target.saturating_add(self.report_interval);
            match self.next_report_target.compare_exchange(
                target,
                next_target,
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
            ) {
                Ok(_) => {
                    self.emit(self.progress_message(processed));
                    break;
                }
                Err(current) => {
                    target = current;
                }
            }
        }

        // Check for temp saving
        let mut temp_target = self.next_temp_save_target.load(AtomicOrdering::Relaxed);
        while processed >= temp_target {
            let next_temp_target = temp_target.saturating_add(self.temp_save_interval);
            match self.next_temp_save_target.compare_exchange(
                temp_target,
                next_temp_target,
                AtomicOrdering::SeqCst,
                AtomicOrdering::SeqCst,
            ) {
                Ok(_) => {
                    if let Err(error) = self.save_temp_entries() {
                        self.emit(format!("Warning: failed to save temp entries: {error}"));
                    }
                    break;
                }
                Err(current) => {
                    temp_target = current;
                }
            }
        }
    }

    fn log_completion(&self) {
        self.emit(format!(
            "Detailed analysis completed! {}/{} | 100%",
            self.total_files, self.total_files
        ));
        self.emit(format!(
            "Detailed analysis completed in {:.0} seconds!",
            self.start_time.elapsed().as_secs_f64()
        ));
    }

    fn add_temp_entry(&self, entry: CacheReplayEntry) {
        if let Ok(mut temp_entries) = self.temp_entries.lock() {
            temp_entries.push(entry);
        }
    }

    fn save_temp_entries(&self) -> Result<(), std::io::Error> {
        let entries = match self.temp_entries.lock() {
            Ok(mut temp_entries) => {
                let entries = temp_entries.drain(..).collect::<Vec<_>>();
                entries
            }
            Err(_) => return Ok(()), // Skip saving if lock is poisoned
        };

        if entries.is_empty() {
            return Ok(());
        }

        let mut content = String::new();
        for entry in entries {
            if let Ok(json) = serde_json::to_string(&entry) {
                content.push_str(&json);
                content.push('\n');
            }
        }

        if !content.is_empty() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.temp_file_path)?
                .write_all(content.as_bytes())?;
        }

        Ok(())
    }

    fn emit(&self, message: String) {
        if let Some(logger) = self.logger {
            logger(message);
        }
    }

    fn progress_message(&self, processed: usize) -> String {
        let percent = cache_progress_percent(processed, self.total_files);
        if processed >= self.report_interval && processed < self.total_files {
            format!(
                "Estimated remaining time: {}\nRunning... {processed}/{} ({percent}%)",
                format_eta_duration(self.estimate_remaining(processed)),
                self.total_files,
            )
        } else {
            format!("Running... {processed}/{} ({percent}%)", self.total_files)
        }
    }

    fn estimate_remaining(&self, processed: usize) -> Duration {
        if processed == 0 || processed >= self.total_files {
            return Duration::ZERO;
        }

        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed <= 0.0 {
            return Duration::ZERO;
        }

        let average_seconds_per_replay = elapsed / processed as f64;
        Duration::from_secs_f64(
            average_seconds_per_replay * self.total_files.saturating_sub(processed) as f64,
        )
    }
}

fn next_progress_target(
    total_files: usize,
    report_interval: usize,
    processed_files: usize,
) -> usize {
    if total_files == 0 || processed_files >= total_files {
        return total_files;
    }
    if processed_files == 0 {
        return report_interval.min(total_files);
    }

    let remainder = processed_files % report_interval;
    if remainder == 0 {
        processed_files
            .saturating_add(report_interval)
            .min(total_files)
    } else {
        processed_files
            .saturating_add(report_interval - remainder)
            .min(total_files)
    }
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

pub fn write_cache_file(
    replays: &Vec<CacheReplayEntry>,
    path: &Path,
) -> Result<(), GenerateCacheError> {
    let path = path.to_path_buf();
    let payload = serialize_cache_entries(&replays).map_err(GenerateCacheError::SerializeFailed)?;

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

pub fn serialize_cache_entries(entries: &[CacheReplayEntry]) -> Result<Vec<u8>, serde_json::Error> {
    let mut canonical_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let value = serde_json::to_value(entry)?;
        canonical_entries.push(canonicalize_json_value(value));
    }
    serde_json::to_vec(&canonical_entries)
}

pub fn cache_entry_from_report(
    report: &ReplayReport,
    hidden_created_lost: &HashSet<String>,
) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Float(report.parser.accurate_length),
        amon_units: Some(convert_numeric_units(&report.amon_units, None)),
        bonus: Some(report.bonus.clone()),
        brutal_plus: report.parser.brutal_plus,
        build: report.parser.build.clone(),
        comp: Some(report.comp.clone()),
        date: report.parser.date.clone(),
        difficulty: report.parser.difficulty.clone(),
        enemy_race: Some(report.parser.enemy_race.clone()),
        ext_difficulty: report.parser.ext_difficulty.clone(),
        extension: report.parser.extension,
        file: normalized_path_string(Path::new(&report.parser.file)),
        form_alength: report.parser.form_alength.clone(),
        detailed_analysis: true,
        hash: report.parser.hash.clone().unwrap_or_default(),
        length: report.parser.length,
        map_name: report.parser.map_name.clone(),
        messages: report
            .parser
            .messages
            .iter()
            .map(|message| ReplayMessage {
                text: message.text.clone(),
                player: message.player,
                time: message.time,
            })
            .collect(),
        mutators: report.parser.mutators.clone(),
        player_stats: Some(convert_player_stats(&report.player_stats)),
        players: convert_players(report, hidden_created_lost),
        region: report.parser.region.clone(),
        result: report.parser.result.clone(),
        weekly: report.parser.weekly,
    }
}

#[derive(Debug, Clone)]
pub struct ParsedCacheReplay {
    pub accurate_length: f64,
    pub accurate_length_force_float: bool,
    pub brutal_plus: u32,
    pub build: ReplayBuildInfo,
    pub date: String,
    pub difficulty: (String, String),
    pub enemy_race: Option<String>,
    pub ext_difficulty: String,
    pub extension: bool,
    pub file: String,
    pub form_alength: String,
    pub length: u64,
    pub map_name: String,
    pub messages: Vec<ReplayMessage>,
    pub mutators: Vec<String>,
    pub players: Vec<CachePlayer>,
    pub region: String,
    pub result: String,
    pub weekly: bool,
    pub hash: String,
}

impl ParsedCacheReplay {
    fn into_basic_entry(self) -> CacheReplayEntry {
        CacheReplayEntry {
            accurate_length: cache_numeric_value(
                self.accurate_length,
                self.accurate_length_force_float,
            ),
            amon_units: None,
            bonus: None,
            brutal_plus: self.brutal_plus,
            build: self.build,
            comp: None,
            date: self.date,
            difficulty: self.difficulty,
            enemy_race: self.enemy_race,
            ext_difficulty: self.ext_difficulty,
            extension: self.extension,
            file: self.file,
            form_alength: self.form_alength,
            detailed_analysis: false,
            hash: self.hash,
            length: self.length,
            map_name: self.map_name,
            messages: self.messages,
            mutators: self.mutators,
            player_stats: None,
            players: self.players,
            region: self.region,
            result: self.result,
            weekly: self.weekly,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CandidateReplay {
    pub path: PathBuf,
    pub basic: ParsedCacheReplay,
}

pub fn generate_cache_overall_stats(
    config: &GenerateCacheConfig,
) -> Result<GenerateCacheSummary, GenerateCacheError> {
    generate_cache_overall_stats_impl(config, None, &GenerateCacheRuntimeOptions::default())
}

pub fn generate_cache_overall_stats_with_logger(
    config: &GenerateCacheConfig,
    logger: &(dyn Fn(String) + Send + Sync),
) -> Result<GenerateCacheSummary, GenerateCacheError> {
    generate_cache_overall_stats_impl(
        config,
        Some(logger),
        &GenerateCacheRuntimeOptions::default(),
    )
}

pub fn generate_cache_overall_stats_with_runtime_and_logger(
    config: &GenerateCacheConfig,
    logger: &(dyn Fn(String) + Send + Sync),
    runtime: &GenerateCacheRuntimeOptions,
) -> Result<GenerateCacheSummary, GenerateCacheError> {
    generate_cache_overall_stats_impl(config, Some(logger), runtime)
}

fn generate_cache_overall_stats_impl(
    config: &GenerateCacheConfig,
    logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
    runtime: &GenerateCacheRuntimeOptions,
) -> Result<GenerateCacheSummary, GenerateCacheError> {
    if !config.account_dir.is_dir() {
        return Err(GenerateCacheError::InvalidAccountDirectory(
            config.account_dir.clone(),
        ));
    }

    if let Some(parent) = config.output_file.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            GenerateCacheError::OutputDirectoryCreateFailed(parent.to_path_buf(), error)
        })?;
    }

    let replay_files = collect_replay_files(&config.account_dir);
    let main_handles = resolve_main_handles(&config.account_dir);
    let hidden_created_lost = cache_hidden_created_lost_units()
        .map_err(|error| GenerateCacheError::DetailedAnalysisConfig(error.to_string()))?;
    let cache_data = dictionary_data::cache_generation_data()
        .map_err(|error| GenerateCacheError::DetailedAnalysisConfig(error.to_string()))?;

    let existing_detailed_analysis_entries =
        load_existing_detailed_analysis_cache(&config.output_file, logger);
    let temp_file_path = config.output_file.with_extension("temp.jsonl");

    let stop_controller = runtime.stop_controller.clone();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let entries = if replay_files.is_empty() {
        let progress = GenerateCacheProgressReporter::new(0, 0, logger, temp_file_path.clone());
        progress.log_completion();
        HashMap::new()
    } else {
        let worker_count = runtime
            .worker_count
            .map(|value| std::cmp::max(1, std::cmp::min(value, replay_files.len())))
            .unwrap_or_else(|| resolve_worker_count(replay_files.len()));
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
            .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))?;
        let stop_requested_for_candidates = stop_requested.clone();
        let stop_controller_for_candidates = stop_controller.clone();
        let candidate_replays = thread_pool.install(|| {
            replay_files
                .par_iter()
                .filter_map(|path| {
                    if stop_controller_for_candidates
                        .as_ref()
                        .is_some_and(|controller| controller.stop_requested())
                    {
                        stop_requested_for_candidates.store(true, AtomicOrdering::Release);
                        return None;
                    }
                    collect_candidate_replay(path, &cache_data)
                })
                .collect::<Vec<CandidateReplay>>()
        });
        let total_candidates = candidate_replays.len();
        let (mut reused_entries, pending_candidates) =
            partition_cached_candidates(candidate_replays, &existing_detailed_analysis_entries);
        let progress = Arc::new(GenerateCacheProgressReporter::new(
            total_candidates,
            reused_entries.len(),
            logger,
            temp_file_path.clone(),
        ));

        if total_candidates == 0 {
            progress.log_completion();
            HashMap::new()
        } else {
            progress.log_start();
            let analyzed_entries = if pending_candidates.is_empty() {
                HashMap::new()
            } else {
                let progress_for_workers = Arc::clone(&progress);
                let stop_requested_for_workers = stop_requested.clone();
                let stop_controller_for_workers = stop_controller.clone();

                thread_pool.install(|| {
                    pending_candidates
                        .into_par_iter()
                        .filter_map(|(_, candidate)| {
                            if stop_controller_for_workers
                                .as_ref()
                                .is_some_and(|controller| controller.stop_requested())
                            {
                                stop_requested_for_workers.store(true, AtomicOrdering::Release);
                                return None;
                            }
                            let entry = analyze_candidate_entry(
                                candidate,
                                &main_handles,
                                &hidden_created_lost,
                            );
                            // Add to temp entries for periodic saving
                            if entry.detailed_analysis {
                                progress_for_workers.add_temp_entry(entry.clone());
                            }
                            progress_for_workers.record_processed_file();
                            Some((entry.hash.clone(), entry))
                        })
                        .collect::<HashMap<_, _>>()
                })
            };

            reused_entries.extend(analyzed_entries);
            if stop_requested.load(AtomicOrdering::Acquire) {
                emit_optional_logger(
                    logger,
                    "Detailed analysis stopped after the current work finished.".to_string(),
                );
            } else {
                progress.log_completion();
            }
            reused_entries
        }
    };

    // Collect all entries: existing from cache + newly analyzed
    let mut all_entries = HashMap::new();
    all_entries.extend(existing_detailed_analysis_entries);
    all_entries.extend(entries);

    let mut all_entries = all_entries.into_values().collect::<Vec<_>>();

    all_entries.sort_by(cache_entry_compare);

    write_cache_file(&all_entries, &config.output_file)?;
    write_pretty_cache_file(&config.output_file, None)?;

    // Remove temp file after successful completion
    if temp_file_path.exists() {
        let _ = fs::remove_file(&temp_file_path);
    }

    Ok(GenerateCacheSummary {
        scanned_replays: all_entries.len(),
        output_file: config.output_file.clone(),
        completed: !stop_requested.load(AtomicOrdering::Acquire),
    })
}

fn collect_candidate_replay(
    replay_path: &Path,
    cache_data: &CacheGenerationData<'_>,
) -> Option<CandidateReplay> {
    parse_cache_replay(
        replay_path,
        cache_data,
        ParseCacheReplayOptions {
            parse_events: false,
            only_blizzard: true,
            without_recover_enabled: true,
        },
    )
    .map(|basic| CandidateReplay {
        path: replay_path.to_path_buf(),
        basic,
    })
}

pub fn parse_basic_cache_entry(replay_path: &Path) -> Option<CacheReplayEntry> {
    let cache_data = dictionary_data::cache_generation_data().ok()?;
    parse_cache_replay(
        replay_path,
        &cache_data,
        ParseCacheReplayOptions {
            parse_events: false,
            only_blizzard: true,
            without_recover_enabled: true,
        },
    )
    .map(ParsedCacheReplay::into_basic_entry)
}

pub fn load_existing_detailed_analysis_cache(
    cache_path: &Path,
    logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
) -> HashMap<String, CacheReplayEntry> {
    let payload = match fs::read(cache_path) {
        Ok(payload) => payload,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return HashMap::new(),
        Err(error) => {
            emit_optional_logger(
                logger,
                format!(
                    "Ignoring existing cache '{}': failed to read: {error}",
                    cache_path.display()
                ),
            );
            return HashMap::new();
        }
    };
    let entries = match serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload) {
        Ok(entries) => entries,
        Err(error) => {
            emit_optional_logger(
                logger,
                format!(
                    "Ignoring existing cache '{}': failed to parse: {error}",
                    cache_path.display()
                ),
            );
            return HashMap::new();
        }
    };

    entries
        .into_iter()
        .filter(|entry| entry.detailed_analysis && !entry.hash.is_empty())
        .map(|entry| (entry.hash.clone(), entry))
        .collect()
}

pub fn partition_cached_candidates(
    candidates: Vec<CandidateReplay>,
    existing_detailed_analysis_entries: &HashMap<String, CacheReplayEntry>,
) -> (
    HashMap<String, CacheReplayEntry>,
    HashMap<String, CandidateReplay>,
) {
    let mut reused_entries = HashMap::new();
    let mut pending_candidates = HashMap::new();

    for candidate in candidates {
        if let Some(existing_entry) = existing_detailed_analysis_entries.get(&candidate.basic.hash)
        {
            let reused_entry =
                reuse_cached_detailed_analysis_entry(&candidate.basic, existing_entry);

            reused_entries.insert(reused_entry.hash.clone(), reused_entry);
        } else {
            pending_candidates.insert(candidate.basic.hash.clone(), candidate);
        }
    }

    (reused_entries, pending_candidates)
}

fn reuse_cached_detailed_analysis_entry(
    basic: &ParsedCacheReplay,
    existing_entry: &CacheReplayEntry,
) -> CacheReplayEntry {
    let mut reused_entry = existing_entry.clone();
    reused_entry.file = basic.file.clone();
    reused_entry.hash = basic.hash.clone();
    reused_entry
}

fn emit_optional_logger(logger: Option<&(dyn Fn(String) + Send + Sync + '_)>, message: String) {
    if let Some(logger) = logger {
        logger(message);
    }
}

fn analyze_candidate_entry(
    candidate: CandidateReplay,
    main_handles: &HashSet<String>,
    hidden_created_lost: &HashSet<String>,
) -> CacheReplayEntry {
    let CandidateReplay { path, basic } = candidate;

    if let Ok(report) = analyze_replay_file(&path, main_handles) {
        if has_non_empty_player_stats(&report) {
            return cache_entry_from_report_with_basic(&basic, &report, hidden_created_lost);
        }
    }

    basic.into_basic_entry()
}

fn has_non_empty_player_stats(report: &ReplayReport) -> bool {
    [1_u8, 2_u8].into_iter().any(|player_id| {
        report.player_stats.get(&player_id).is_some_and(|stats| {
            !stats.supply.is_empty()
                || !stats.mining.is_empty()
                || !stats.army.is_empty()
                || !stats.killed.is_empty()
        })
    })
}

fn cache_entry_from_report_with_basic(
    basic: &ParsedCacheReplay,
    report: &ReplayReport,
    hidden_created_lost: &HashSet<String>,
) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Float(normalize_json_float(report.length * 1.4)),
        amon_units: Some(convert_numeric_units(&report.amon_units, None)),
        bonus: Some(report.bonus.clone()),
        brutal_plus: basic.brutal_plus,
        build: basic.build.clone(),
        comp: Some(report.comp.clone()),
        date: basic.date.clone(),
        difficulty: basic.difficulty.clone(),
        enemy_race: basic.enemy_race.clone(),
        ext_difficulty: basic.ext_difficulty.clone(),
        extension: basic.extension,
        file: basic.file.clone(),
        form_alength: report.parser.form_alength.clone(),
        detailed_analysis: true,
        hash: basic.hash.clone(),
        length: basic.length,
        map_name: basic.map_name.clone(),
        messages: report
            .parser
            .messages
            .iter()
            .map(|message| ReplayMessage {
                text: message.text.clone(),
                player: message.player,
                time: message.time,
            })
            .collect(),
        mutators: report.parser.mutators.clone(),
        player_stats: Some(convert_player_stats(&report.player_stats)),
        players: convert_players(report, hidden_created_lost),
        region: report.parser.region.clone(),
        result: basic.result.clone(),
        weekly: report.parser.weekly,
    }
}

fn convert_players(
    report: &ReplayReport,
    hidden_created_lost: &HashSet<String>,
) -> Vec<CachePlayer> {
    let mut players = report
        .parser
        .players
        .iter()
        .take(3)
        .map(base_cache_player)
        .collect::<Vec<CachePlayer>>();
    while players.len() < 3 {
        players.push(CachePlayer {
            pid: players.len() as u8,
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
        });
    }

    let main_index = match report.positions.main {
        1 | 2 => usize::from(report.positions.main),
        _ => 1,
    };

    for player_index in [1_usize, 2_usize] {
        let use_main = player_index == main_index;
        let player = players
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
        player.icons = Some(convert_icon_map(
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
        player.units = Some(convert_numeric_units(
            if use_main {
                &report.main_units
            } else {
                &report.ally_units
            },
            Some(hidden_created_lost),
        ));
    }

    players
}

fn base_cache_player(player: &ParsedReplayPlayer) -> CachePlayer {
    if player.pid == 0 || is_placeholder_player(player) {
        return CachePlayer {
            pid: player.pid,
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
        };
    }

    CachePlayer {
        pid: player.pid,
        apm: Some(player.apm),
        commander: Some(cache_commander_name(player.commander.as_str())),
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

fn cache_commander_name(commander: &str) -> String {
    match commander {
        "Han & Horner" => "Horner".to_string(),
        _ => commander.to_string(),
    }
}

fn is_placeholder_player(player: &ParsedReplayPlayer) -> bool {
    player.pid != 0
        && player.name.is_empty()
        && player.handle.is_empty()
        && player.race.is_empty()
        && !player.observer
        && player.result.is_empty()
        && player.commander.is_empty()
        && player.commander_level == 0
        && player.commander_mastery_level == 0
        && player.prestige == 0
        && player.prestige_name.is_empty()
        && player.apm == 0
        && player.masteries.iter().all(|value| *value == 0)
}

fn convert_player_stats(
    player_stats: &BTreeMap<u8, PlayerStatsSeries>,
) -> BTreeMap<u8, CachePlayerStatsSeries> {
    let mut out = BTreeMap::new();
    for (player_id, stats) in player_stats {
        out.insert(
            *player_id,
            CachePlayerStatsSeries {
                name: stats.name.clone(),
                supply: stats.supply.clone(),
                mining: stats.mining.clone(),
                army: stats
                    .army
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        cache_army_value(*value, stats.army_force_float_indices.contains(&index))
                    })
                    .collect(),
                killed: stats
                    .killed
                    .iter()
                    .map(|value| nonnegative_float_to_u64(*value))
                    .collect(),
            },
        );
    }
    out
}

fn nonnegative_float_to_u64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round_ties_even() as u64
    }
}

fn cache_army_value(value: f64, force_float: bool) -> CacheStatValue {
    if !value.is_finite() || value < 0.0 {
        return CacheStatValue::Integer(0);
    }
    if force_float {
        return CacheStatValue::Float(value);
    }
    if value == 0.0 {
        return CacheStatValue::Integer(0);
    }
    if value.fract().abs() < 1e-9 {
        CacheStatValue::Integer(value.round_ties_even() as u64)
    } else {
        CacheStatValue::Float(value)
    }
}

fn cache_numeric_value(value: f64, force_float: bool) -> CacheNumericValue {
    if force_float {
        CacheNumericValue::Float(value)
    } else if !value.is_finite() || value <= 0.0 {
        CacheNumericValue::Integer(0)
    } else if value.fract().abs() < 1e-9 {
        CacheNumericValue::Integer(value.round_ties_even() as u64)
    } else {
        CacheNumericValue::Float(value)
    }
}

fn convert_numeric_units(
    units: &BTreeMap<String, NumericUnitStats>,
    hidden_created_lost: Option<&HashSet<String>>,
) -> BTreeMap<String, CacheUnitStats> {
    let mut out = BTreeMap::new();
    for (unit_name, (created, lost, kills, kill_fraction)) in units {
        let hide_counts =
            hidden_created_lost.is_some_and(|values| values.contains(unit_name.as_str()));
        out.insert(
            unit_name.clone(),
            CacheUnitStats(
                if hide_counts {
                    CacheCountValue::hidden()
                } else {
                    CacheCountValue::Count(*created)
                },
                if hide_counts {
                    CacheCountValue::hidden()
                } else {
                    CacheCountValue::Count(*lost)
                },
                *kills,
                *kill_fraction,
            ),
        );
    }
    out
}

fn convert_icon_map(
    icons: &BTreeMap<String, u64>,
    outlaw_order: Option<Vec<String>>,
) -> BTreeMap<String, CacheIconValue> {
    let mut out = BTreeMap::new();
    for (icon_key, count) in icons {
        out.insert(icon_key.clone(), CacheIconValue::Count(*count));
    }
    if let Some(order) = outlaw_order {
        out.insert("outlaws".to_string(), CacheIconValue::Order(order));
    }
    out
}

fn protocol_store() -> Option<&'static ProtocolStore> {
    static STORE: OnceLock<Option<ProtocolStore>> = OnceLock::new();
    STORE.get_or_init(|| build_protocol_store().ok()).as_ref()
}

#[derive(Clone, Copy)]
struct ParseCacheReplayOptions {
    parse_events: bool,
    only_blizzard: bool,
    without_recover_enabled: bool,
}

fn parse_cache_replay(
    replay_path: &Path,
    inputs: &CacheGenerationData<'_>,
    options: ParseCacheReplayOptions,
) -> Option<ParsedCacheReplay> {
    if options.only_blizzard && replay_path.to_string_lossy().contains("[MM]") {
        return None;
    }

    let store = protocol_store()?;
    let parsed = parse_file_with_store(
        replay_path,
        store,
        if options.parse_events {
            ReplayParseMode::Detailed
        } else {
            ReplayParseMode::Simple
        },
    )
    .ok()?;

    let details = parsed.details.clone()?;
    let init_data = parsed.init_data.clone()?;
    let metadata = parsed.metadata.clone()?;

    let replay_build = i64::from(parsed.base_build);
    let latest_build = i64::from(store.latest().ok()?.build);
    let selected_build = if store.build(parsed.base_build).is_ok() {
        replay_build
    } else {
        store
            .closest_build(parsed.base_build)
            .map(i64::from)
            .unwrap_or(latest_build)
    };
    let protocol_build = resolve_protocol_build(replay_build, latest_build, selected_build);

    let is_blizzard = details.m_isBlizzardMap;
    if options.only_blizzard && !is_blizzard {
        return None;
    }

    if details.m_disableRecoverGame.is_none() {
        return None;
    }
    let disable_recover = details.m_disableRecoverGame.unwrap_or(false);
    if options.without_recover_enabled && !disable_recover {
        return None;
    }

    let mut events = Vec::new();
    if options.parse_events {
        events.extend(parsed.game_events.iter().cloned().map(ReplayEvent::Game));
        events.extend(
            parsed
                .tracker_events
                .iter()
                .cloned()
                .map(ReplayEvent::Tracker),
        );
        events.sort_by_key(event_gameloop);
    }

    let map_title = if metadata.Title.is_empty() {
        "Unknown map".to_string()
    } else {
        metadata.Title.clone()
    };
    let map_name = inputs
        .map_names
        .get(&map_title)
        .and_then(|row| row.get("EN"))
        .cloned()
        .unwrap_or(map_title);

    let extension = init_data
        .m_syncLobbyState
        .m_gameDescription
        .m_hasExtensionMod;
    let brutal_plus = init_data
        .m_syncLobbyState
        .m_lobbyState
        .m_slots
        .first()
        .map(|value| value.m_brutalPlusDifficulty as u32)
        .unwrap_or_default();

    let length_numeric = ReplayNumericValue::Float(metadata.Duration);
    let length = length_numeric.as_f64();
    let start_time = get_start_time(&events);
    let last_deselect_event =
        get_last_deselect_event(&events).unwrap_or(ReplayNumericValue::Float(length));
    let metadata_players = &metadata.Players;
    if metadata_players.is_empty() {
        return None;
    }

    let player0_result = metadata_players
        .first()
        .map(|value| value.Result.clone())
        .unwrap_or_default();
    let player1_result = metadata_players
        .get(1)
        .map(|value| value.Result.clone())
        .unwrap_or_default();
    let result = if player0_result == "Win" || player1_result == "Win" {
        "Victory".to_string()
    } else {
        "Defeat".to_string()
    };

    let accurate_length_numeric = if result == "Victory" && options.parse_events {
        last_deselect_event.subtract(&start_time)
    } else {
        length_numeric.subtract(&start_time)
    };
    let accurate_length = accurate_length_numeric.as_f64();
    if accurate_length == 0.0 {
        return None;
    }

    let (mutators, weekly) = identify_mutators_for_cache(
        &events,
        &inputs.mutators_all,
        &inputs.mutators_ui,
        &inputs.mutator_ids,
        &inputs.cached_mutators,
        extension,
        replay_path.to_string_lossy().contains("[MM]"),
        Some(&init_data),
    );

    let mut players = Vec::new();
    for player in metadata_players {
        let pid = (players.len() + 1) as u8;
        let apm = player.APM;
        let player_result = player.Result.clone();

        players.push(CachePlayer {
            pid,
            apm: Some((apm * length / accurate_length).round_ties_even() as u32),
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
            result: Some(player_result),
            units: None,
        });
    }

    let player_list = &details.m_playerList;
    if player_list.is_empty() {
        return None;
    }
    let mut region = String::new();
    for (idx, player) in player_list.iter().enumerate() {
        let Some(replay_player) = players.get_mut(idx) else {
            continue;
        };
        replay_player.name = Some(player.m_name.clone());
        replay_player.race = Some(player.m_race.clone());
        replay_player.observer = Some(player.m_observe != 0);

        if idx == 0 {
            region = player
                .m_toon
                .as_ref()
                .map(|value| value.m_region)
                .map(region_name)
                .unwrap_or("")
                .to_string();
        }
    }

    let slots = &init_data.m_syncLobbyState.m_lobbyState.m_slots;
    let mut commander_found = false;
    for (idx, slot) in slots.iter().enumerate() {
        let Some(replay_player) = players.get_mut(idx) else {
            continue;
        };
        let commander = slot.m_commander.clone();

        replay_player.masteries = Some(parse_masteries(&slot.m_commanderMasteryTalents));
        replay_player.commander = Some(cache_commander_name(&commander));
        replay_player.commander_level = Some(slot.m_commanderLevel as u32);
        replay_player.commander_mastery_level = Some(slot.m_commanderMasteryLevel as u32);
        let prestige = slot.m_selectedCommanderPrestige;
        replay_player.prestige = Some(prestige as u32);
        replay_player.prestige_name = Some(
            inputs
                .prestige_names
                .get(&commander)
                .and_then(|values| values.get(&prestige))
                .cloned()
                .unwrap_or_default(),
        );
        replay_player.handle = Some(slot.m_toonHandle.clone());

        if !commander.is_empty() {
            commander_found = true;
        }
    }

    if options.only_blizzard && !commander_found {
        return None;
    }

    for (idx, user) in init_data
        .m_syncLobbyState
        .m_userInitialData
        .iter()
        .enumerate()
    {
        let Some(replay_player) = players.get_mut(idx) else {
            continue;
        };
        let user_name = user.m_name.clone();
        if !user_name.is_empty() {
            replay_player.name = Some(user_name);
        }
    }

    players.insert(
        0,
        CachePlayer {
            pid: 0,
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
        },
    );

    let insert_pid2 = match players.get(2).map(|player| player.pid) {
        Some(2) => false,
        Some(_) => true,
        None => true,
    };
    if insert_pid2 {
        players.insert(
            2,
            CachePlayer {
                pid: 2,
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
            },
        );
    }

    let difficulty_from_player = |index: usize| -> Option<i64> {
        let slot = slots.get(index.saturating_sub(1))?;
        Some(slot.m_difficulty)
    };

    let enemy_race = players.get(3).and_then(|player| player.race.clone());
    let mut diff_1_code = difficulty_from_player(3);
    let mut diff_2_code = difficulty_from_player(4);
    if diff_1_code.is_none() {
        diff_1_code = difficulty_from_player(1).or_else(|| difficulty_from_player(0));
    }
    if diff_2_code.is_none() {
        diff_2_code = difficulty_from_player(2);
    }
    let diff_1_name = difficulty_name(diff_1_code.unwrap_or(1)).to_string();
    let diff_2_name = difficulty_name(diff_2_code.unwrap_or(1)).to_string();
    let ext_difficulty = if brutal_plus > 0 {
        format!("B+{brutal_plus}")
    } else if diff_1_name == diff_2_name {
        diff_1_name.clone()
    } else {
        format!("{diff_1_name}/{diff_2_name}")
    };

    let raw_messages = parsed
        .message_events
        .iter()
        .filter_map(parse_message_event)
        .collect::<Vec<ReplayMessage>>();
    let user_leave_times = collect_user_leave_times(&events);
    let messages = sorted_messages_with_leave_events(&raw_messages, &user_leave_times);

    Some(ParsedCacheReplay {
        accurate_length: normalize_json_float(accurate_length),
        accurate_length_force_float: matches!(
            accurate_length_numeric,
            ReplayNumericValue::Float(_)
        ),
        brutal_plus,
        build: ReplayBuildInfo {
            replay_build: parsed.base_build,
            protocol_build,
        },
        date: file_date_string(replay_path).ok()?,
        difficulty: (diff_1_name, diff_2_name),
        enemy_race,
        ext_difficulty,
        extension,
        file: normalized_path_string(replay_path),
        form_alength: format_duration(accurate_length),
        length: duration_to_u64(length),
        map_name,
        messages,
        mutators,
        players,
        region,
        result,
        weekly,
        hash: calculate_replay_hash(replay_path),
    })
}

fn resolve_protocol_build(
    replay_build: i64,
    latest_build: i64,
    selected_build: i64,
) -> ProtocolBuildValue {
    if let Some(mapped) = valid_protocol_mapping(replay_build) {
        if supported_legacy_protocol(mapped) {
            ProtocolBuildValue::Int(mapped as u32)
        } else {
            ProtocolBuildValue::Str(latest_build.to_string())
        }
    } else if replay_build == selected_build {
        ProtocolBuildValue::Int(replay_build as u32)
    } else {
        ProtocolBuildValue::Str(latest_build.to_string())
    }
}

fn parse_message_event(message: &MessageEvent) -> Option<ReplayMessage> {
    let text = if let Some(value) = message.m_string.as_ref().filter(|value| !value.is_empty()) {
        value.clone()
    } else if message.event == "NNet.Game.SPingMessage" {
        "*pings*".to_string()
    } else {
        return None;
    };
    let player = message.user_id.map(|value| value + 1).unwrap_or_default() as u8;
    let time = message.game_loop as f64 / 16.0;
    Some(ReplayMessage { text, player, time })
}

fn collect_user_leave_times(events: &[ReplayEvent]) -> IndexMap<i64, f64> {
    let mut user_leave_times = IndexMap::new();
    for event in events {
        if event_name(event) != "NNet.Game.SGameUserLeaveEvent" {
            continue;
        }
        let user = event_user_id(event)
            .map(|value| value + 1)
            .unwrap_or_default();
        let leave_time = event_gameloop(event) as f64 / 16.0;
        user_leave_times.insert(user, leave_time);
    }
    user_leave_times
}

fn sorted_messages_with_leave_events(
    messages: &[ReplayMessage],
    user_leave_times: &IndexMap<i64, f64>,
) -> Vec<ReplayMessage> {
    let mut rows = messages
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, message)| (message.time, index, message))
        .collect::<Vec<(f64, usize, ReplayMessage)>>();

    let base_index = rows.len();
    for (offset, (player, leave_time)) in user_leave_times.iter().enumerate() {
        if *player != 1 && *player != 2 {
            continue;
        }
        rows.push((
            *leave_time,
            base_index + offset,
            ReplayMessage {
                player: *player as u8,
                text: "*has left the game*".to_string(),
                time: *leave_time,
            },
        ));
    }

    rows.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    rows.into_iter().map(|(_, _, message)| message).collect()
}

fn file_date_string(file: &Path) -> Result<String, io::Error> {
    let modified = fs::metadata(file)?.modified()?;
    let datetime: DateTime<Local> = DateTime::from(modified);
    Ok(datetime.format("%Y:%m:%d:%H:%M:%S").to_string())
}

fn normalized_path_string(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        normalized.push(component.as_os_str());
    }
    normalized.display().to_string()
}

fn parse_masteries(values: &[u32]) -> [u32; 6] {
    let mut out = [0_u32; 6];
    for (index, value) in values.iter().take(6).enumerate() {
        out[index] = *value;
    }
    out
}

#[derive(Clone, Copy)]
enum ReplayNumericValue {
    Int(i64),
    Float(f64),
}

impl ReplayNumericValue {
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    fn subtract(self, rhs: &Self) -> Self {
        match (self, *rhs) {
            (Self::Int(left), Self::Int(right)) => Self::Int(left - right),
            _ => Self::Float(self.as_f64() - rhs.as_f64()),
        }
    }
}

fn event_name(event: &ReplayEvent) -> &str {
    event._event()
}

fn event_gameloop(event: &ReplayEvent) -> i64 {
    event._gameloop()
}

fn event_control_id(event: &ReplayEvent) -> Option<i64> {
    match event {
        ReplayEvent::Game(event) => event.m_control_id,
        ReplayEvent::Tracker(_) => None,
    }
}

fn event_event_type(event: &ReplayEvent) -> Option<i64> {
    match event {
        ReplayEvent::Game(event) => event.m_event_type,
        ReplayEvent::Tracker(_) => None,
    }
}

fn event_user_id(event: &ReplayEvent) -> Option<i64> {
    match event {
        ReplayEvent::Game(event) => event.user_id,
        ReplayEvent::Tracker(_) => None,
    }
}

fn difficulty_name(code: i64) -> &'static str {
    match code {
        1 => "Casual",
        2 => "Normal",
        3 => "Hard",
        4 => "Brutal",
        5 => "Custom",
        6 => "Cheater",
        _ => "Unknown",
    }
}

fn region_name(code: i64) -> &'static str {
    match code {
        1 => "NA",
        2 => "EU",
        3 => "KR",
        5 => "CN",
        98 => "PTR",
        _ => "",
    }
}

fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "00:00".to_string();
    }

    let total = seconds.floor() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn duration_to_u64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round_ties_even() as u64
    }
}

fn valid_protocol_mapping(build: i64) -> Option<i64> {
    match build {
        81102 => Some(81433),
        80871 => Some(81433),
        76811 => Some(76114),
        80188 => Some(78285),
        79998 => Some(78285),
        81433 => Some(83830),
        84643 => Some(83830),
        _ => None,
    }
}

fn supported_legacy_protocol(build: i64) -> bool {
    matches!(build, 76114 | 78285 | 83830)
}

fn cache_handle_id(handle: &str) -> String {
    let tail = handle.rsplit('/').next().unwrap_or("");
    tail.split('.').next().unwrap_or("").to_string()
}

fn mutator_from_button(button: i64, panel: i64, mutators: &[String]) -> Option<String> {
    let idx = (button - 41) / 3 + (panel - 1) * 15;
    if idx < 0 {
        return None;
    }
    let Ok(index) = usize::try_from(idx) else {
        return None;
    };
    mutators.get(index).cloned()
}

fn get_last_deselect_event(events: &[ReplayEvent]) -> Option<ReplayNumericValue> {
    let mut last_event = None;
    for event in events {
        if event_name(event) == "NNet.Game.SSelectionDeltaEvent" {
            last_event = Some(ReplayNumericValue::Float(
                event_gameloop(event) as f64 / 16.0 - 2.0,
            ));
        }
    }
    last_event
}

fn get_start_time(events: &[ReplayEvent]) -> ReplayNumericValue {
    for event in events {
        if let ReplayEvent::Tracker(event) = event {
            if event.event == "NNet.Replay.Tracker.SPlayerStatsEvent"
                && event.m_player_id == Some(1)
            {
                let minerals = event
                    .m_stats
                    .as_ref()
                    .and_then(|stats| stats.m_score_value_minerals_collection_rate)
                    .unwrap_or_default();
                if minerals > 0.0 {
                    return ReplayNumericValue::Float(event.game_loop as f64 / 16.0);
                }
            }

            if event.event == "NNet.Replay.Tracker.SUpgradeEvent"
                && matches!(event.m_player_id, Some(1 | 2))
            {
                let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                if upgrade_name.contains("Spray") {
                    return ReplayNumericValue::Float(event.game_loop as f64 / 16.0);
                }
            }
        }
    }

    ReplayNumericValue::Int(0)
}

fn identify_mutators_for_cache(
    events: &[ReplayEvent],
    mutators_all: &[String],
    mutators_ui: &[String],
    mutator_ids: &MutatorIdsJson,
    cached_mutators: &CachedMutatorsJson,
    extension: bool,
    mm: bool,
    detailed_info: Option<&ReplayInitData>,
) -> (Vec<String>, bool) {
    let mut mutators = Vec::new();
    let mut weekly = false;

    if mm {
        for event in events {
            let ReplayEvent::Tracker(event) = event else {
                continue;
            };
            if event.event != "NNet.Replay.Tracker.SUpgradeEvent" || event.m_player_id != Some(0) {
                continue;
            }
            let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
            if !upgrade_name.contains("mutatorinfo") {
                continue;
            }
            let mutator_key = upgrade_name.get(12..).unwrap_or_default();
            if mutator_ids.contains_key(mutator_key) {
                mutators.push(mutator_key.to_string());
            }
        }
    }

    if extension {
        if let Some(handles) =
            detailed_info.map(|value| &value.m_syncLobbyState.m_gameDescription.m_cacheHandles)
        {
            for handle in handles {
                let cached = cache_handle_id(handle);
                if cached.is_empty() {
                    continue;
                }
                if let Some(mutator_id) = cached_mutators.get(&cached) {
                    mutators.push(mutator_id.clone());
                    weekly = true;
                }
            }
        }
    }

    if !extension {
        if let Some(slot0) =
            detailed_info.and_then(|value| value.m_syncLobbyState.m_lobbyState.m_slots.first())
        {
            let brutal_plus = slot0.m_brutalPlusDifficulty;
            if brutal_plus > 0 {
                for key in &slot0.m_retryMutationIndexes {
                    if *key <= 0 {
                        continue;
                    }
                    if let Ok(index) = usize::try_from(*key - 1) {
                        if let Some(mutator) = mutators_all.get(index) {
                            mutators.push(mutator.clone());
                        }
                    }
                }
            }
        }
    }

    if extension {
        let mut actions = Vec::new();
        let mut offset = 0_i64;
        let mut last_gameloop = None;

        for event in events {
            let gameloop = event_gameloop(event);
            let name = event_name(event);

            if gameloop == 0
                && name == "NNet.Game.STriggerDialogControlEvent"
                && event_event_type(event) == Some(3)
            {
                let contains_selection_changed = matches!(
                    event,
                    ReplayEvent::Game(event)
                        if event
                            .m_event_data
                            .as_ref()
                            .is_some_and(|data| data.contains_selection_changed)
                );
                if contains_selection_changed {
                    if let Some(control_id) = event_control_id(event) {
                        offset = 129 - control_id;
                    }
                    continue;
                }
            }

            if gameloop > 0
                && Some(gameloop) != last_gameloop
                && name == "NNet.Game.STriggerDialogControlEvent"
                && event_user_id(event) == Some(0)
            {
                let contains_none = matches!(
                    event,
                    ReplayEvent::Game(event)
                        if event.m_event_data.as_ref().is_some_and(|data| data.contains_none)
                );
                if !contains_none {
                    if let Some(control_id) = event_control_id(event) {
                        actions.push(control_id + offset);
                        last_gameloop = Some(gameloop);
                    }
                    continue;
                }
            }

            if let ReplayEvent::Tracker(event) = event {
                if event.event == "NNet.Replay.Tracker.SUpgradeEvent"
                    && matches!(event.m_player_id, Some(1 | 2))
                {
                    let upgrade_name = event.m_upgrade_type_name.as_deref().unwrap_or_default();
                    if upgrade_name.contains("Spray") {
                        break;
                    }
                }
            }
        }

        let mut panel = 1_i64;
        for action in actions {
            if (41..=83).contains(&action) {
                if let Some(new_mutator) = mutator_from_button(action, panel, mutators_ui) {
                    if !mutators.contains(&new_mutator) || new_mutator == "Random" {
                        mutators.push(new_mutator);
                    } else if new_mutator != "Random" {
                        if let Some(position) =
                            mutators.iter().position(|value| value == &new_mutator)
                        {
                            mutators.remove(position);
                        }
                    }
                }
            }

            if action == 123 && panel > 1 {
                panel -= 1;
            }
            if action == 124 && panel < 4 {
                panel += 1;
            }

            if (88..=106).contains(&action) {
                if let Ok(index) = usize::try_from((action - 88) / 2) {
                    if index < mutators.len() {
                        mutators.remove(index);
                    }
                }
            }
        }
    }

    (
        mutators
            .into_iter()
            .map(|mutator| {
                mutator
                    .replace("Heroes from the Storm (old)", "Heroes from the Storm")
                    .replace("Extreme Caution", "Afraid of the Dark")
            })
            .collect(),
        weekly,
    )
}

fn resolve_half_cpu_worker_cap() -> usize {
    let cpu_count = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    std::cmp::max(1, cpu_count / 2)
}

fn resolve_worker_count(total_files: usize) -> usize {
    std::cmp::max(1, std::cmp::min(resolve_half_cpu_worker_cap(), total_files))
}

fn collect_replay_files(root: &Path) -> Vec<PathBuf> {
    let mut replay_files = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension == "SC2Replay")
        })
        .collect::<Vec<PathBuf>>();

    replay_files.sort_by(|left, right| {
        let left_norm = left.to_string_lossy().to_ascii_lowercase();
        let right_norm = right.to_string_lossy().to_ascii_lowercase();
        left_norm.cmp(&right_norm)
    });
    replay_files
}

fn resolve_main_handles(account_dir: &Path) -> HashSet<String> {
    let scan_root = main_handle_scan_root(account_dir);
    let mut handles = HashSet::new();

    for entry in WalkDir::new(&scan_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
    {
        if path_contains_component(entry.path(), "Banks") {
            continue;
        }

        let directory_name = entry.file_name().to_string_lossy();
        if directory_name.matches('-').count() < 3 {
            continue;
        }
        if directory_name.contains("Crash")
            || directory_name.contains("Desync")
            || directory_name.contains("Error")
        {
            continue;
        }

        handles.insert(directory_name.to_string());
    }

    handles
}

fn main_handle_scan_root(account_dir: &Path) -> PathBuf {
    let mut folder = account_dir.to_path_buf();
    loop {
        let Some(parent) = folder.parent() else {
            break;
        };
        if parent.to_string_lossy().contains("StarCraft") {
            folder = parent.to_path_buf();
        } else {
            break;
        }
    }
    folder
}

fn path_contains_component(path: &Path, target: &str) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|value| value == target)
    })
}

fn cache_sort_date(date: &str) -> Option<String> {
    let compact = date.replace(':', "");
    if compact.len() == 14 && compact.chars().all(|ch| ch.is_ascii_digit()) {
        Some(compact)
    } else {
        None
    }
}

fn cache_entry_compare(left: &CacheReplayEntry, right: &CacheReplayEntry) -> Ordering {
    match (cache_sort_date(&left.date), cache_sort_date(&right.date)) {
        (Some(left_date), Some(right_date)) => left_date
            .cmp(&right_date)
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.hash.cmp(&right.hash)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left
            .file
            .cmp(&right.file)
            .then_with(|| left.hash.cmp(&right.hash)),
    }
}

pub fn persist_simple_analysis_cache(
    entries: &[CacheReplayEntry],
    cache_path: &Path,
) -> Result<(), GenerateCacheError> {
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            GenerateCacheError::OutputDirectoryCreateFailed(parent.to_path_buf(), error)
        })?;
    }

    let all_entries = match std::fs::read(cache_path) {
        Ok(payload) => {
            serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).map_err(|error| {
                GenerateCacheError::ParseExistingCache(cache_path.to_path_buf(), error)
            })?
        }
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

        merged_entries.retain(|hash, existing| hash == &entry.hash || existing.file != entry.file);
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

    write_cache_file(&all_entries, cache_path)
}

fn cache_progress_percent(processed: usize, total: usize) -> usize {
    if total == 0 {
        return 100;
    }

    (((processed as f64 / total as f64) * 100.0).round() as usize).min(100)
}

fn format_eta_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn normalize_json_float(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }

    let rounded = format!("{value:.12}").parse::<f64>().unwrap_or(value);
    if rounded == -0.0 {
        0.0
    } else {
        rounded
    }
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
                .map(canonicalize_json_value)
                .collect::<Vec<JsonValue>>(),
        ),
        JsonValue::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<(String, JsonValue)>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, canonicalize_json_value(value));
            }
            JsonValue::Object(sorted)
        }
    }
}
