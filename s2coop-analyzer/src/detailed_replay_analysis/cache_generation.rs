use super::cache_parallel::{
    ParallelMapResult, ReplayCacheParallelMapResult, ReplayCacheParallelParseOptions,
    ReplayCacheParsedEntry,
};
use super::cache_progress::GenerateCacheProgressReporter;
use super::cache_runtime::{
    FullAnalysisMode, GenerateCacheConfig, GenerateCacheError, GenerateCacheRuntimeOptions,
    GenerateCacheStopController, GenerateCacheSummary,
};
use super::cache_sink::{CacheReplayCheck, ReplayCacheEntrySinkBuffer};
use super::timing::{CandidateReplayAnalysisTiming, CandidateReplayCollectionTiming};
use super::{
    DetailedReplayAnalyzer, GenerateCacheTimingReport, ReplayAnalysisResources,
    ReplayBaseParseFilters, ReplayBaseParseOptions, ReplayCacheFileIdentity,
};
use crate::cache_overall_stats_generator::{CacheOverallStatsFile, CacheReplayEntry};
use chrono::NaiveDateTime;
use rayon::ThreadPoolBuilder;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayAnalysisFilePriority {
    size_bytes: u64,
    normalized_path: String,
}

impl ReplayAnalysisFilePriority {
    fn from_path(path: &Path) -> Self {
        let size_bytes = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        Self::from_size_and_path(size_bytes, path)
    }

    fn from_size_and_path(size_bytes: u64, path: &Path) -> Self {
        let normalized_path = path.to_string_lossy().to_ascii_lowercase();

        Self {
            size_bytes,
            normalized_path,
        }
    }

    fn compare_largest_first(&self, other: &Self) -> Ordering {
        other
            .size_bytes
            .cmp(&self.size_bytes)
            .then_with(|| self.normalized_path.cmp(&other.normalized_path))
    }
}

struct GeneratedCacheOutput {
    entries: Vec<CacheReplayEntry>,
    completed: bool,
    timing_report: GenerateCacheTimingReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayFileCandidate {
    path: PathBuf,
    modified: SystemTime,
}

impl ReplayFileCandidate {
    fn from_path(path: &Path) -> Self {
        let modified = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        Self {
            path: path.to_path_buf(),
            modified,
        }
    }

    fn compare_recent_first(left: &Self, right: &Self) -> std::cmp::Ordering {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.normalized_path().cmp(&right.normalized_path()))
    }

    fn normalized_path(&self) -> String {
        self.path.to_string_lossy().to_ascii_lowercase()
    }
}

#[derive(Debug, Clone)]
struct CandidateReplay {
    path: PathBuf,
    hash: String,
    modified_seconds: u64,
    analysis_priority: ReplayAnalysisFilePriority,
}

#[derive(Debug, Clone)]
struct CandidateReplayCollectionResult {
    candidate: CandidateReplay,
    timing: CandidateReplayCollectionTiming,
}

impl CandidateReplayCollectionResult {
    fn new(candidate: CandidateReplay, timing: CandidateReplayCollectionTiming) -> Self {
        Self { candidate, timing }
    }

    fn timing(&self) -> &CandidateReplayCollectionTiming {
        &self.timing
    }

    fn into_candidate(self) -> CandidateReplay {
        self.candidate
    }
}

#[derive(Debug, Clone)]
struct CandidateReplayAnalysisResult {
    entry: Option<CacheReplayEntry>,
    timing: CandidateReplayAnalysisTiming,
}

fn cache_entry_modified_seconds(entry: &CacheReplayEntry) -> u64 {
    ["%Y:%m:%d:%H:%M:%S", "%Y-%m-%d %H:%M:%S"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(&entry.date, format).ok())
        .and_then(|datetime| u64::try_from(datetime.and_utc().timestamp()).ok())
        .unwrap_or_default()
}

impl CandidateReplayAnalysisResult {
    fn new(entry: Option<CacheReplayEntry>, timing: CandidateReplayAnalysisTiming) -> Self {
        Self { entry, timing }
    }

    fn entry(&self) -> Option<&CacheReplayEntry> {
        self.entry.as_ref()
    }

    fn timing(&self) -> &CandidateReplayAnalysisTiming {
        &self.timing
    }

    fn timing_mut(&mut self) -> &mut CandidateReplayAnalysisTiming {
        &mut self.timing
    }

    fn into_parts(self) -> (Option<CacheReplayEntry>, CandidateReplayAnalysisTiming) {
        (self.entry, self.timing)
    }
}

impl CandidateReplay {
    fn collect_for_cache_lookup_timed(replay_path: &Path) -> CandidateReplayCollectionResult {
        let total_start = Instant::now();
        let hash_lookup_start = Instant::now();
        let metadata = fs::metadata(replay_path).ok();
        let modified_seconds = metadata
            .as_ref()
            .and_then(|value| value.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let digest = DetailedReplayAnalyzer::calculate_replay_file_digest(replay_path);
        let hash_lookup = hash_lookup_start.elapsed();
        let priority_start = Instant::now();
        let analysis_priority =
            ReplayAnalysisFilePriority::from_size_and_path(digest.size_bytes, replay_path);
        let priority = priority_start.elapsed();
        let candidate = Self {
            path: replay_path.to_path_buf(),
            hash: digest.hash,
            modified_seconds,
            analysis_priority,
        };
        CandidateReplayCollectionResult::new(
            candidate,
            CandidateReplayCollectionTiming::new(total_start.elapsed(), hash_lookup, priority),
        )
    }

    fn collect_without_cache_lookup_timed(replay_path: &Path) -> CandidateReplayCollectionResult {
        let total_start = Instant::now();
        let priority_start = Instant::now();
        let analysis_priority = ReplayAnalysisFilePriority::from_path(replay_path);
        let priority = priority_start.elapsed();
        let candidate = Self {
            path: replay_path.to_path_buf(),
            hash: String::new(),
            modified_seconds: DetailedReplayAnalyzer::file_modified_seconds(replay_path)
                .unwrap_or_default(),
            analysis_priority,
        };
        CandidateReplayCollectionResult::new(
            candidate,
            CandidateReplayCollectionTiming::new(total_start.elapsed(), Duration::ZERO, priority),
        )
    }

    fn analyze_timed(
        &self,
        mode: FullAnalysisMode,
        main_handles: &HashSet<String>,
        resources: &ReplayAnalysisResources,
        collect_detailed_report_timings: bool,
    ) -> CandidateReplayAnalysisResult {
        let total_start = Instant::now();
        let mut timing = CandidateReplayAnalysisTiming::default();
        let path = self.path.as_path();

        if !mode.is_detailed() {
            let parsed_simple = CacheReplayEntry::parse_with_options_timed(
                path,
                resources,
                ReplayBaseParseOptions {
                    include_events: false,
                    filters: ReplayBaseParseFilters::saved_cache(),
                },
            );
            timing.parse_simple = parsed_simple.timing().total;
            timing.parse_simple_breakdown.add(parsed_simple.timing());
            let entry = parsed_simple.into_parts().0.map(|(entry, _)| entry);
            return CandidateReplayAnalysisResult::new(entry, timing.finish(total_start.elapsed()));
        }

        let parsed_detailed = CacheReplayEntry::parse_with_options_timed(
            path,
            resources,
            ReplayBaseParseOptions {
                include_events: true,
                filters: ReplayBaseParseFilters::saved_cache(),
            },
        );
        timing.parse_detailed = parsed_detailed.timing().total;
        timing
            .parse_detailed_breakdown
            .add(parsed_detailed.timing());
        let (parsed_detailed, _parse_timing) = parsed_detailed.into_parts();

        let Some((basic, parsed)) = parsed_detailed else {
            let parse_basic_fallback_start = Instant::now();
            let parsed_basic = CacheReplayEntry::parse_with_options_timed(
                path,
                resources,
                ReplayBaseParseOptions {
                    include_events: false,
                    filters: ReplayBaseParseFilters::saved_cache(),
                },
            );
            timing.parse_basic_fallback = parse_basic_fallback_start.elapsed();
            timing
                .parse_basic_fallback_breakdown
                .add(parsed_basic.timing());
            let entry = parsed_basic.into_parts().0.map(|(entry, _)| entry);
            return CandidateReplayAnalysisResult::new(entry, timing.finish(total_start.elapsed()));
        };

        let detailed_report_start = Instant::now();
        let detailed = DetailedReplayAnalyzer::analyze_parsed_replay_with_cache_entry(
            parsed,
            main_handles,
            resources.hidden_created_lost(),
            Some(&basic),
            resources,
            collect_detailed_report_timings,
        );
        timing.detailed_report = detailed_report_start.elapsed();

        if let Ok(result) = detailed {
            timing
                .detailed_report_breakdown
                .add(result.detailed_report_timing());
            timing.add_report_to_cache_entry(result.report_to_cache_entry());
            if result.report().has_non_empty_player_stats() {
                return CandidateReplayAnalysisResult::new(
                    Some(result.into_cache_entry()),
                    timing.finish(total_start.elapsed()),
                );
            }
        }

        CandidateReplayAnalysisResult::new(Some(basic), timing.finish(total_start.elapsed()))
    }

    fn partition_cached(
        candidates: Vec<Self>,
        existing_entries: &HashMap<String, CacheReplayEntry>,
        existing_identities_by_hash: &HashMap<String, ReplayCacheFileIdentity>,
    ) -> (
        HashMap<String, CacheReplayEntry>,
        usize,
        Vec<(String, Self)>,
    ) {
        let mut reused_entries = HashMap::new();
        let mut reused_identity_count = 0usize;
        let mut pending_candidates = Vec::new();

        for candidate in candidates {
            let hash = candidate.hash.clone();
            if hash.is_empty() {
                pending_candidates.push((hash, candidate));
                continue;
            }

            if let Some(existing_entry) = existing_entries.get(&hash) {
                let cached_time_matches = existing_identities_by_hash
                    .get(&hash)
                    .map(|identity| identity.modified_seconds() == candidate.modified_seconds)
                    .unwrap_or(true);
                if cached_time_matches {
                    reused_entries.insert(
                        hash.clone(),
                        existing_entry
                            .refreshed_for_candidate(candidate.path.as_path(), hash.as_str()),
                    );
                } else {
                    pending_candidates.push((hash, candidate));
                }
            } else if existing_identities_by_hash
                .get(&hash)
                .is_some_and(|identity| identity.modified_seconds() == candidate.modified_seconds)
            {
                reused_identity_count = reused_identity_count.saturating_add(1);
            } else {
                pending_candidates.push((hash, candidate));
            }
        }

        (reused_entries, reused_identity_count, pending_candidates)
    }

    fn cache_check(&self) -> Option<CacheReplayCheck> {
        if DetailedReplayAnalyzer::is_mm_replay_path(self.path.as_path()) {
            return None;
        }
        let hash = if self.hash.is_empty() {
            DetailedReplayAnalyzer::calculate_replay_hash(self.path.as_path())
        } else {
            self.hash.clone()
        };
        if hash.is_empty() {
            return None;
        };
        Some(CacheReplayCheck::new(
            hash,
            CacheOverallStatsFile::normalized_path_string(self.path.as_path()),
            if self.modified_seconds == 0 {
                DetailedReplayAnalyzer::file_modified_seconds(self.path.as_path())
                    .unwrap_or_default()
            } else {
                self.modified_seconds
            },
        ))
    }

    fn sort_pending_by_analysis_priority(pending_candidates: &mut [(String, Self)]) {
        pending_candidates.sort_by(|left, right| {
            left.1
                .analysis_priority
                .compare_largest_first(&right.1.analysis_priority)
        });
    }
}

impl DetailedReplayAnalyzer {
    pub fn simple_analysis_worker_count() -> usize {
        GenerateCacheRuntimeOptions::half_cpu_worker_cap()
    }

    fn run_parallel_map<T, R, F>(
        items: Vec<T>,
        worker_count: usize,
        stop_controller: Option<Arc<GenerateCacheStopController>>,
        map_item: F,
    ) -> Result<ParallelMapResult<R>, GenerateCacheError>
    where
        T: Send,
        R: Send,
        F: Fn(T) -> Option<R> + Send + Sync,
    {
        if items.is_empty() {
            return Ok(ParallelMapResult::new(
                Vec::new(),
                true,
                worker_count.max(1),
            ));
        }

        let worker_count = worker_count.max(1).min(items.len().max(1));
        let thread_pool = Self::build_thread_pool(worker_count)?;
        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop_requested_for_workers = Arc::clone(&stop_requested);
        let values = thread_pool.install(|| {
            items
                .into_par_iter()
                .filter_map(|item| {
                    if stop_requested_for_workers.load(AtomicOrdering::Acquire) {
                        return None;
                    }
                    if stop_controller
                        .as_ref()
                        .is_some_and(|controller| controller.stop_requested())
                    {
                        stop_requested_for_workers.store(true, AtomicOrdering::Release);
                        return None;
                    }
                    map_item(item)
                })
                .collect::<Vec<R>>()
        });

        Ok(ParallelMapResult::new(
            values,
            !stop_requested.load(AtomicOrdering::Acquire),
            worker_count,
        ))
    }

    pub fn parse_saved_cache_entries_parallel_map<T, F>(
        replay_files: Vec<PathBuf>,
        resources: &ReplayAnalysisResources,
        options: &ReplayCacheParallelParseOptions,
        map_entry: F,
    ) -> Result<ReplayCacheParallelMapResult<T>, GenerateCacheError>
    where
        T: Send,
        F: Fn(ReplayCacheParsedEntry) -> T + Send + Sync,
    {
        if replay_files.is_empty() {
            return Ok(ReplayCacheParallelMapResult::new(
                Vec::new(),
                true,
                options.worker_count(),
                0,
            ));
        }

        let worker_count = options.resolved_worker_count(replay_files.len());
        let sink_buffer = Arc::new(ReplayCacheEntrySinkBuffer::new(
            options.cache_entry_sink(),
            options.cache_entry_sink_batch_size(),
        ));
        let sink_buffer_for_workers = Arc::clone(&sink_buffer);
        let sink_errors = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let sink_errors_for_workers = Arc::clone(&sink_errors);
        let parse_options = ReplayBaseParseOptions {
            include_events: options.mode().include_events(),
            filters: ReplayBaseParseFilters::saved_cache(),
        };

        let parallel_result = Self::run_parallel_map(
            replay_files,
            worker_count,
            options.stop_controller(),
            |path| {
                let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    CacheReplayEntry::parse_with_options_timed(&path, resources, parse_options)
                }));
                let parsed_entry = match parsed {
                    Ok(parsed) => {
                        let entry = parsed.into_parts().0.map(|(entry, _)| entry);
                        ReplayCacheParsedEntry::new(path, entry, false)
                    }
                    Err(_) => ReplayCacheParsedEntry::new(path, None, true),
                };

                if let Some(entry) = parsed_entry.entry()
                    && let Err(error) = sink_buffer_for_workers.add_entry(entry)
                {
                    match sink_errors_for_workers.lock() {
                        Ok(mut errors) => errors.push(error.to_string()),
                        Err(poisoned) => poisoned.into_inner().push(error.to_string()),
                    }
                }

                Some(map_entry(parsed_entry))
            },
        )?;

        sink_buffer
            .flush()
            .map_err(|error| GenerateCacheError::CacheEntrySink(error.to_string()))?;
        let sink_errors = match sink_errors.lock() {
            Ok(errors) => errors.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        if let Some(error) = sink_errors.first() {
            return Err(GenerateCacheError::CacheEntrySink(error.clone()));
        }

        let completed = parallel_result.completed();
        let worker_count = parallel_result.worker_count();
        Ok(ReplayCacheParallelMapResult::new(
            parallel_result.into_values(),
            completed,
            worker_count,
            sink_buffer.persisted_entries(),
        ))
    }

    fn build_thread_pool(worker_count: usize) -> Result<rayon::ThreadPool, GenerateCacheError> {
        ThreadPoolBuilder::new()
            .num_threads(worker_count.max(1))
            .build()
            .map_err(|error| GenerateCacheError::ThreadPoolBuildFailed(error.to_string()))
    }

    pub fn analyze_full_simple(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        DetailedReplayAnalyzer::run_full_analysis(
            config,
            resources,
            logger,
            runtime,
            FullAnalysisMode::Simple,
        )
    }

    pub fn analyze_full_detailed(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        DetailedReplayAnalyzer::run_full_analysis(
            config,
            resources,
            logger,
            runtime,
            FullAnalysisMode::Detailed,
        )
    }

    #[doc(hidden)]
    pub fn sort_replay_paths_by_detailed_analysis_priority(replay_paths: &mut [PathBuf]) {
        let mut prioritized_paths = replay_paths
            .iter()
            .cloned()
            .map(|path| (ReplayAnalysisFilePriority::from_path(&path), path))
            .collect::<Vec<_>>();

        prioritized_paths.sort_by(|left, right| left.0.compare_largest_first(&right.0));

        for (target, (_, path)) in replay_paths.iter_mut().zip(prioritized_paths) {
            *target = path;
        }
    }

    fn run_full_analysis(
        config: &GenerateCacheConfig,
        resources: &ReplayAnalysisResources,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
        mode: FullAnalysisMode,
    ) -> Result<GenerateCacheSummary, GenerateCacheError> {
        let total_start = Instant::now();
        if !config.account_dir().is_dir() {
            return Err(GenerateCacheError::InvalidAccountDirectory(
                config.account_dir().to_path_buf(),
            ));
        }

        let mut cache_output = DetailedReplayAnalyzer::analyze_replays_for_cache_output(
            config, logger, runtime, resources, mode,
        )?;
        let scanned_replays = cache_output.entries.len();

        let canonical_worker_count = if cache_output.timing_report.worker_count == 0 {
            runtime.resolved_worker_count(std::cmp::max(1, cache_output.entries.len()))
        } else {
            cache_output.timing_report.worker_count
        };
        let build_canonicalize_thread_pool_start = Instant::now();
        let canonicalize_thread_pool = Self::build_thread_pool(canonical_worker_count)?;
        cache_output.timing_report.build_canonicalize_thread_pool =
            build_canonicalize_thread_pool_start.elapsed();

        let canonicalize_entries_start = Instant::now();
        let canonical_payload = canonicalize_thread_pool.install(|| {
            CacheReplayEntry::canonicalized_entries_with_payload(&cache_output.entries)
        });
        let canonical_payload =
            canonical_payload.map_err(GenerateCacheError::CanonicalizeFailed)?;
        cache_output.timing_report.canonicalize_entries = canonicalize_entries_start.elapsed();
        cache_output
            .timing_report
            .apply_canonical_payload_timing(canonical_payload.timing());
        let (cache_entries, _cache_payload) = canonical_payload.into_parts();
        let timing_report = cache_output.timing_report.finish(total_start.elapsed());

        Ok(GenerateCacheSummary::new(
            scanned_replays,
            config.output_file().to_path_buf(),
            cache_entries,
            cache_output.completed,
            timing_report,
        ))
    }

    pub(super) fn collect_cache_replay_files(
        account_dir: &Path,
        recent_replay_count: Option<usize>,
    ) -> Vec<PathBuf> {
        let mut replay_files = WalkDir::new(account_dir)
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

        if let Some(recent_replay_count) = recent_replay_count {
            let mut candidates = replay_files
                .into_iter()
                .map(|path| ReplayFileCandidate::from_path(path.as_path()))
                .collect::<Vec<ReplayFileCandidate>>();
            candidates.sort_unstable_by(ReplayFileCandidate::compare_recent_first);
            candidates.truncate(recent_replay_count);
            return candidates
                .into_iter()
                .map(|candidate| candidate.path)
                .collect::<Vec<PathBuf>>();
        }

        replay_files.sort_by(|left, right| {
            let left_norm = left.to_string_lossy().to_ascii_lowercase();
            let right_norm = right.to_string_lossy().to_ascii_lowercase();
            left_norm.cmp(&right_norm)
        });
        replay_files
    }

    fn resolve_main_handles(account_dir: &Path) -> HashSet<String> {
        let scan_root = DetailedReplayAnalyzer::main_handle_scan_root(account_dir);
        let mut handles = HashSet::new();

        for entry in WalkDir::new(&scan_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir())
        {
            if DetailedReplayAnalyzer::path_contains_component(entry.path(), "Banks") {
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

    fn analyze_replays_for_cache_output(
        config: &GenerateCacheConfig,
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
        runtime: &GenerateCacheRuntimeOptions,
        resources: &ReplayAnalysisResources,
        mode: FullAnalysisMode,
    ) -> Result<GeneratedCacheOutput, GenerateCacheError> {
        let mut timing_report = GenerateCacheTimingReport::new(runtime.timings_enabled());
        let collect_replay_files_start = Instant::now();
        let replay_files = runtime.replay_files().unwrap_or_else(|| {
            DetailedReplayAnalyzer::collect_cache_replay_files(
                config.account_dir(),
                config.recent_replay_count(),
            )
        });
        timing_report.collect_replay_files = collect_replay_files_start.elapsed();
        timing_report.total_replay_files = replay_files.len();

        let main_handles = if mode.is_detailed() {
            let resolve_main_handles_start = Instant::now();
            let main_handles = DetailedReplayAnalyzer::resolve_main_handles(config.account_dir());
            timing_report.resolve_main_handles = resolve_main_handles_start.elapsed();
            main_handles
        } else {
            HashSet::new()
        };

        let load_existing_cache_start = Instant::now();
        let existing_detailed_cache_entries = if mode.is_detailed() {
            runtime
                .existing_detailed_cache_entries()
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        let mut existing_detailed_cache_identities_by_hash = if mode.is_detailed() {
            runtime
                .existing_detailed_cache_identities_by_hash()
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        for (hash, entry) in &existing_detailed_cache_entries {
            existing_detailed_cache_identities_by_hash
                .entry(hash.clone())
                .or_insert_with(|| {
                    ReplayCacheFileIdentity::new(hash.clone(), cache_entry_modified_seconds(entry))
                });
        }
        timing_report.load_existing_cache = load_existing_cache_start.elapsed();
        let cache_entry_sink = if mode.is_detailed() {
            runtime.cache_entry_sink()
        } else {
            None
        };
        let cache_entry_sink_batch_size = runtime.cache_entry_sink_batch_size();

        let stop_controller = runtime.stop_controller();
        let collect_detailed_report_timings = runtime.detailed_report_timings_enabled();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let entries = if replay_files.is_empty() {
            let progress = GenerateCacheProgressReporter::new(
                mode,
                0,
                0,
                logger,
                cache_entry_sink.clone(),
                cache_entry_sink_batch_size,
            );
            progress.log_completion();
            HashMap::new()
        } else {
            let worker_count = runtime.resolved_worker_count(replay_files.len());
            timing_report.worker_count = worker_count;
            let should_collect_cache_lookup_hashes =
                mode.is_detailed() && !existing_detailed_cache_identities_by_hash.is_empty();
            let collect_candidates_start = Instant::now();
            let candidate_replay_results = Self::run_parallel_map(
                replay_files,
                worker_count,
                stop_controller.clone(),
                |path| {
                    Some(if should_collect_cache_lookup_hashes {
                        CandidateReplay::collect_for_cache_lookup_timed(path.as_path())
                    } else {
                        CandidateReplay::collect_without_cache_lookup_timed(path.as_path())
                    })
                },
            )?;
            if !candidate_replay_results.completed() {
                stop_requested.store(true, AtomicOrdering::Release);
            }
            timing_report.collect_candidates_parallel = collect_candidates_start.elapsed();
            for candidate_result in candidate_replay_results.values() {
                timing_report.add_candidate_collection_timing(candidate_result.timing());
            }
            let candidate_replays = candidate_replay_results
                .into_values()
                .into_iter()
                .map(CandidateReplayCollectionResult::into_candidate)
                .filter(|candidate| {
                    !DetailedReplayAnalyzer::is_mm_replay_path(candidate.path.as_path())
                })
                .collect::<Vec<CandidateReplay>>();
            let total_candidates = candidate_replays.len();

            let partition_candidates_start = Instant::now();
            let (mut reused_entries, reused_identity_count, mut pending_candidates) =
                CandidateReplay::partition_cached(
                    candidate_replays,
                    &existing_detailed_cache_entries,
                    &existing_detailed_cache_identities_by_hash,
                );
            timing_report.partition_candidates = partition_candidates_start.elapsed();
            timing_report.candidate_count = total_candidates;
            let reused_candidate_count = reused_entries.len().saturating_add(reused_identity_count);
            timing_report.reused_candidate_count = reused_candidate_count;
            timing_report.pending_candidate_count = pending_candidates.len();

            let sort_pending_candidates_start = Instant::now();
            CandidateReplay::sort_pending_by_analysis_priority(&mut pending_candidates);
            timing_report.sort_pending_candidates = sort_pending_candidates_start.elapsed();
            let progress = Arc::new(GenerateCacheProgressReporter::new(
                mode,
                total_candidates,
                reused_candidate_count,
                logger,
                cache_entry_sink,
                cache_entry_sink_batch_size,
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
                    let pending_candidate_replays = pending_candidates
                        .into_iter()
                        .map(|(_, candidate)| candidate)
                        .collect::<Vec<CandidateReplay>>();

                    let replay_analysis_start = Instant::now();
                    let analyzed_result = Self::run_parallel_map(
                        pending_candidate_replays,
                        worker_count,
                        stop_controller.clone(),
                        |candidate| {
                            let mut result = candidate.analyze_timed(
                                mode,
                                &main_handles,
                                resources,
                                collect_detailed_report_timings,
                            );
                            if mode.is_detailed()
                                && let Some(entry) = result.entry()
                            {
                                let cache_entry_write_start = Instant::now();
                                if let Err(error) = progress_for_workers.add_cache_entry(entry) {
                                    progress_for_workers.emit(format!(
                                        "Warning: failed to write cache entries: {error}"
                                    ));
                                }
                                result
                                    .timing_mut()
                                    .add_temp_entry_write(cache_entry_write_start.elapsed());
                            } else if mode.is_detailed()
                                && let Some(check) = candidate.cache_check()
                            {
                                let cache_entry_write_start = Instant::now();
                                if let Err(error) = progress_for_workers.add_cache_check(check) {
                                    progress_for_workers.emit(format!(
                                        "Warning: failed to write cache checks: {error}"
                                    ));
                                }
                                result
                                    .timing_mut()
                                    .add_temp_entry_write(cache_entry_write_start.elapsed());
                            }
                            let progress_record_start = Instant::now();
                            progress_for_workers.record_processed_file();
                            result
                                .timing_mut()
                                .add_progress_record(progress_record_start.elapsed());
                            Some(result)
                        },
                    )?;
                    if !analyzed_result.completed() {
                        stop_requested.store(true, AtomicOrdering::Release);
                    }
                    let analyzed_results = analyzed_result.into_values();
                    match mode {
                        FullAnalysisMode::Simple => {
                            timing_report.simple_analysis_parallel =
                                replay_analysis_start.elapsed();
                            for result in &analyzed_results {
                                timing_report.add_simple_analysis_timing(result.timing());
                            }
                        }
                        FullAnalysisMode::Detailed => {
                            timing_report.replay_analysis_parallel =
                                replay_analysis_start.elapsed();
                            for result in &analyzed_results {
                                timing_report.add_replay_analysis_timing(result.timing());
                            }
                        }
                    }

                    let collect_analyzed_entries_start = Instant::now();
                    let analyzed_entries = analyzed_results
                        .into_iter()
                        .filter_map(|result| {
                            let (entry, _timing) = result.into_parts();
                            entry.map(|entry| (entry.hash.clone(), entry))
                        })
                        .collect::<HashMap<_, _>>();
                    timing_report.collect_analyzed_entries =
                        collect_analyzed_entries_start.elapsed();
                    timing_report.analyzed_entry_count = analyzed_entries.len();
                    analyzed_entries
                };

                let merge_entries_start = Instant::now();
                reused_entries.extend(analyzed_entries);
                timing_report.merge_entries += merge_entries_start.elapsed();
                if stop_requested.load(AtomicOrdering::Acquire) {
                    if let Some(logger) = logger {
                        logger(mode.stopped_message().to_string());
                    }
                } else {
                    progress.log_completion();
                }
                if let Err(error) = progress.flush_cache_entries() {
                    progress.emit(format!("Warning: failed to write cache entries: {error}"));
                }
                timing_report.set_temp_persist_stats(progress.cache_persisted_entries(), 0);
                reused_entries
            }
        };

        let merge_entries_start = Instant::now();
        let mut all_entries = if config.recent_replay_count().is_some() {
            HashMap::new()
        } else {
            existing_detailed_cache_entries
        };
        all_entries.extend(entries);
        timing_report.merge_entries += merge_entries_start.elapsed();

        let mut all_entries = all_entries.into_values().collect::<Vec<_>>();
        let sort_entries_start = Instant::now();
        all_entries.sort_by(|left, right| left.cmp_cache_order(right));
        timing_report.sort_entries = sort_entries_start.elapsed();

        Ok(GeneratedCacheOutput {
            entries: all_entries,
            completed: !stop_requested.load(AtomicOrdering::Acquire),
            timing_report,
        })
    }
}
