use crate::detailed_replay_analysis::{
    analyze_replay_file, build_basic_cache_entry, build_cache_entry_from_report,
    cache_hidden_created_lost_units, normalize_json_float, normalized_path_string,
    parse_replay_input_bundle, ReplayBaseParseFilters, ReplayBaseParseOptions,
    ReplayParsedInputBundle,
};
use crate::dictionary_data::{self, CacheGenerationData};
use crate::tauri_replay_analysis_impl::{ParsedReplayMessage, ReplayReport};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::ThreadPoolBuilder;
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
    Arc,
};
use std::time::{Duration, Instant};
use thiserror::Error;
use walkdir::WalkDir;

pub use crate::detailed_replay_analysis::{ProtocolBuildValue, ReplayBuildInfo};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheNumericValue {
    Integer(u64),
    Float(f64),
}

pub type ReplayMessage = ParsedReplayMessage;

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
    build_cache_entry_from_report(report, None, hidden_created_lost)
}

#[derive(Debug, Clone)]
struct CandidateReplay {
    path: PathBuf,
    parsed: ReplayParsedInputBundle,
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
    let parsed = parse_replay_input_bundle(
        replay_path,
        cache_data,
        ReplayBaseParseOptions {
            include_events: false,
            filters: ReplayBaseParseFilters {
                only_blizzard: true,
                require_recover_disabled: true,
            },
        },
    )
    .ok()??;

    Some(CandidateReplay {
        path: replay_path.to_path_buf(),
        parsed,
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

fn partition_cached_candidates(
    candidates: Vec<CandidateReplay>,
    existing_detailed_analysis_entries: &HashMap<String, CacheReplayEntry>,
) -> (
    HashMap<String, CacheReplayEntry>,
    HashMap<String, CandidateReplay>,
) {
    let mut reused_entries = HashMap::new();
    let mut pending_candidates = HashMap::new();

    for candidate in candidates {
        let hash = candidate.parsed.parser.hash.clone().unwrap_or_default();
        if let Some(existing_entry) = existing_detailed_analysis_entries.get(&hash) {
            let reused_entry =
                reuse_cached_detailed_analysis_entry(&candidate.parsed, existing_entry);

            reused_entries.insert(reused_entry.hash.clone(), reused_entry);
        } else {
            pending_candidates.insert(hash, candidate);
        }
    }

    (reused_entries, pending_candidates)
}

fn reuse_cached_detailed_analysis_entry(
    parsed: &ReplayParsedInputBundle,
    existing_entry: &CacheReplayEntry,
) -> CacheReplayEntry {
    let mut reused_entry = existing_entry.clone();
    reused_entry.file = normalized_path_string(Path::new(&parsed.parser.file));
    reused_entry.hash = parsed.parser.hash.clone().unwrap_or_default();
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
    let CandidateReplay { path, parsed } = candidate;
    let basic = build_basic_cache_entry(&parsed);

    if let Ok(report) = analyze_replay_file(&path, main_handles) {
        if has_non_empty_player_stats(&report) {
            return build_cache_entry_from_report(&report, Some(&basic), hidden_created_lost);
        }
    }

    basic
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
) -> Option<CacheReplayEntry> {
    let parsed = parse_replay_input_bundle(
        replay_path,
        inputs,
        ReplayBaseParseOptions {
            include_events: options.parse_events,
            filters: ReplayBaseParseFilters {
                only_blizzard: options.only_blizzard,
                require_recover_disabled: options.without_recover_enabled,
            },
        },
    )
    .ok()??;

    if parsed.parser.accurate_length == 0.0 {
        return None;
    }

    if options.only_blizzard && !parsed.commander_found {
        return None;
    }

    Some(build_basic_cache_entry(&parsed))
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
