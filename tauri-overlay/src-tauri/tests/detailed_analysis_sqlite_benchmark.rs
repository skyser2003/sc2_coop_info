use rayon::ThreadPoolBuilder;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use s2coop_analyzer::cache_overall_stats_detailed_analysis::CacheAnalysisPaths;
use s2coop_analyzer::detailed_replay_analysis::{
    DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, DetailedReplayAnalyzer, GenerateCacheConfig,
    GenerateCacheRuntimeOptions, GenerateCacheTimingReport, ReplayAnalysisResources,
    ReplayCacheFileIdentity, ReplayFileIdentity,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use s2protocol_port::{
    ProtocolStore, ProtocolStoreBuilder, ReplayParseMode, ReplayParseOptions, ReplayParser,
};
use sco_tauri_overlay::{
    QueuedReplayCacheEntrySink, ReplayAnalysis, ReplayAnalysisOps, ReplayCacheDatabase,
    ReplayCacheReadScope, ReplayCacheStatsQuery, ReplayCacheWriteQueue, ReplayCacheWriteResult,
    ReplayInfo,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchmarkMode {
    AnalyzerOnly,
    AnalyzerWithSqlite,
}

impl BenchmarkMode {
    fn label(self) -> &'static str {
        match self {
            Self::AnalyzerOnly => "analyzer_only",
            Self::AnalyzerWithSqlite => "analyzer_with_sqlite",
        }
    }
}

#[derive(Debug)]
struct DetailedAnalysisBenchmarkRun {
    mode: BenchmarkMode,
    run_index: usize,
    total_wall: Duration,
    analyze_wall: Duration,
    writer_finish_wall: Duration,
    replay_summary_wall: Duration,
    legacy_hash_dedupe_wall: Duration,
    dedupe_hash_wall: Duration,
    stats_rebuild_wall: Duration,
    scanned_replays: usize,
    replay_summary_count: usize,
    deduped_replay_count: usize,
    timing_report: GenerateCacheTimingReport,
    write_result: Option<ReplayCacheWriteResult>,
}

impl DetailedAnalysisBenchmarkRun {
    fn print(&self) {
        let write_result = self.write_result.unwrap_or_default();
        println!(
            concat!(
                "SCO_DETAILED_SQLITE_BENCH ",
                "run={} mode={} total_ms={} analyze_ms={} writer_finish_ms={} ",
                "replay_summary_ms={} legacy_hash_dedupe_ms={} dedupe_ms={} stats_rebuild_ms={} ",
                "scanned={} workers={} analyzer_total_ms={} collect_files_ms={} ",
                "summary_replays={} deduped_replays={} ",
                "collect_candidates_ms={} replay_analysis_ms={} replay_worker_ms={} ",
                "parse_detailed_ms={} detailed_report_ms={} report_to_cache_entry_ms={} ",
                "queue_send_ms={} collect_results_ms={} merge_ms={} sort_ms={} ",
                "canonicalize_ms={} canonicalize_parallel_ms={} ",
                "sqlite_open_ms={} sqlite_write_ms={} sqlite_batches={} ",
                "sqlite_attempted_entries={} sqlite_persisted_entries={} sqlite_failed_batches={}"
            ),
            self.run_index,
            self.mode.label(),
            millis(self.total_wall),
            millis(self.analyze_wall),
            millis(self.writer_finish_wall),
            millis(self.replay_summary_wall),
            millis(self.legacy_hash_dedupe_wall),
            millis(self.dedupe_hash_wall),
            millis(self.stats_rebuild_wall),
            self.scanned_replays,
            self.timing_report.worker_count(),
            millis(self.timing_report.total()),
            millis(self.timing_report.collect_replay_files()),
            self.replay_summary_count,
            self.deduped_replay_count,
            millis(self.timing_report.collect_candidates_parallel()),
            millis(self.timing_report.replay_analysis_parallel()),
            millis(self.timing_report.replay_analysis_worker()),
            millis(self.timing_report.replay_analysis_parse_detailed()),
            millis(self.timing_report.replay_analysis_detailed_report()),
            millis(self.timing_report.replay_analysis_report_to_cache_entry()),
            millis(self.timing_report.replay_analysis_temp_entry_write()),
            millis(self.timing_report.collect_analyzed_entries()),
            millis(self.timing_report.merge_entries()),
            millis(self.timing_report.sort_entries()),
            millis(self.timing_report.canonicalize_entries()),
            millis(self.timing_report.canonicalize_entries_parallel()),
            millis(write_result.database_open()),
            millis(write_result.sqlite_write()),
            write_result.processed_batches(),
            write_result.attempted_entries(),
            write_result.persisted_entries(),
            write_result.failed_batches(),
        );
    }
}

#[derive(Debug)]
struct DetailedAnalysisBenchmarkPair {
    analyzer_only: DetailedAnalysisBenchmarkRun,
    analyzer_with_sqlite: DetailedAnalysisBenchmarkRun,
}

impl DetailedAnalysisBenchmarkPair {
    fn print_delta(&self, pair_index: usize) {
        let sqlite_write = self
            .analyzer_with_sqlite
            .write_result
            .unwrap_or_default()
            .sqlite_write();
        println!(
            concat!(
                "SCO_DETAILED_SQLITE_BENCH_DELTA ",
                "pair={} sqlite_minus_analyzer_only_ms={} sqlite_write_ms={} ",
                "sqlite_finish_wait_ms={} analyzer_only_total_ms={} sqlite_total_ms={}"
            ),
            pair_index,
            signed_millis(
                self.analyzer_with_sqlite
                    .total_wall
                    .saturating_sub(self.analyzer_only.total_wall)
            ),
            millis(sqlite_write),
            millis(self.analyzer_with_sqlite.writer_finish_wall),
            millis(self.analyzer_only.total_wall),
            millis(self.analyzer_with_sqlite.total_wall),
        );
    }
}

#[derive(Debug)]
struct WarmDetailedAnalysisBenchmarkRun {
    cold_populate_wall: Duration,
    cold_write_result: ReplayCacheWriteResult,
    existing_cache_open_wall: Duration,
    existing_cache_load_wall: Duration,
    existing_entries: usize,
    warm_total_wall: Duration,
    warm_analyze_wall: Duration,
    warm_writer_finish_wall: Duration,
    warm_write_result: ReplayCacheWriteResult,
    replay_summary_wall: Duration,
    dedupe_wall: Duration,
    stats_rebuild_wall: Duration,
    scanned_replays: usize,
    replay_summary_count: usize,
    deduped_replay_count: usize,
    timing_report: GenerateCacheTimingReport,
}

impl WarmDetailedAnalysisBenchmarkRun {
    fn print(&self) {
        println!(
            concat!(
                "SCO_DETAILED_SQLITE_WARM_BENCH ",
                "cold_populate_ms={} cold_sqlite_write_ms={} cold_batches={} ",
                "existing_open_ms={} existing_load_ms={} existing_entries={} ",
                "warm_total_ms={} warm_analyze_ms={} warm_writer_finish_ms={} ",
                "warm_sqlite_open_ms={} warm_sqlite_write_ms={} warm_batches={} ",
                "warm_attempted_entries={} warm_persisted_entries={} warm_failed_batches={} ",
                "replay_summary_ms={} dedupe_ms={} stats_rebuild_ms={} ",
                "scanned={} summary_replays={} deduped_replays={} workers={} ",
                "analyzer_total_ms={} collect_files_ms={} resolve_main_handles_ms={} ",
                "load_existing_runtime_ms={} collect_candidates_ms={} ",
                "collect_candidates_hash_lookup_ms={} partition_candidates_ms={} ",
                "sort_pending_ms={} candidate_count={} reused_candidates={} ",
                "pending_candidates={} replay_analysis_ms={} replay_worker_ms={} ",
                "parse_detailed_ms={} detailed_report_ms={} queue_send_ms={} ",
                "merge_ms={} sort_ms={} canonicalize_ms={}"
            ),
            millis(self.cold_populate_wall),
            millis(self.cold_write_result.sqlite_write()),
            self.cold_write_result.processed_batches(),
            millis(self.existing_cache_open_wall),
            millis(self.existing_cache_load_wall),
            self.existing_entries,
            millis(self.warm_total_wall),
            millis(self.warm_analyze_wall),
            millis(self.warm_writer_finish_wall),
            millis(self.warm_write_result.database_open()),
            millis(self.warm_write_result.sqlite_write()),
            self.warm_write_result.processed_batches(),
            self.warm_write_result.attempted_entries(),
            self.warm_write_result.persisted_entries(),
            self.warm_write_result.failed_batches(),
            millis(self.replay_summary_wall),
            millis(self.dedupe_wall),
            millis(self.stats_rebuild_wall),
            self.scanned_replays,
            self.replay_summary_count,
            self.deduped_replay_count,
            self.timing_report.worker_count(),
            millis(self.timing_report.total()),
            millis(self.timing_report.collect_replay_files()),
            millis(self.timing_report.resolve_main_handles()),
            millis(self.timing_report.load_existing_cache()),
            millis(self.timing_report.collect_candidates_parallel()),
            millis(self.timing_report.collect_candidates_hash_lookup()),
            millis(self.timing_report.partition_candidates()),
            millis(self.timing_report.sort_pending_candidates()),
            self.timing_report.candidate_count(),
            self.timing_report.reused_candidate_count(),
            self.timing_report.pending_candidate_count(),
            millis(self.timing_report.replay_analysis_parallel()),
            millis(self.timing_report.replay_analysis_worker()),
            millis(self.timing_report.replay_analysis_parse_detailed()),
            millis(self.timing_report.replay_analysis_detailed_report()),
            millis(self.timing_report.replay_analysis_temp_entry_write()),
            millis(self.timing_report.merge_entries()),
            millis(self.timing_report.sort_entries()),
            millis(self.timing_report.canonicalize_entries()),
        );
    }
}

fn millis(duration: Duration) -> u128 {
    duration.as_millis()
}

fn signed_millis(duration: Duration) -> i128 {
    duration.as_millis() as i128
}

fn replay_summary_from_entries(
    entries: &[s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry],
    resources: &ReplayAnalysisResources,
) -> Vec<ReplayInfo> {
    let main_names = HashSet::new();
    let main_handles = HashSet::new();
    let mut replays = entries
        .iter()
        .filter(|entry| entry.detailed_analysis && Path::new(&entry.file).exists())
        .map(|entry| {
            ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                entry,
                resources.dictionary_data(),
            )
            .oriented_for_main_identity(&main_names, &main_handles)
        })
        .collect::<Vec<_>>();
    replays.sort_by(|left, right| {
        right
            .date()
            .cmp(&left.date())
            .then_with(|| right.file().cmp(left.file()))
    });
    replays
}

fn dedupe_replays_by_hash_like_old_frontend(replays: Vec<ReplayInfo>) -> Vec<ReplayInfo> {
    let mut hashes = HashMap::new();
    replays
        .into_iter()
        .filter(|replay| {
            let hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(replay.file()));
            let is_detailed = hashes.get(&hash);

            if is_detailed.is_some() && (*is_detailed.unwrap() || !replay.is_detailed()) {
                false
            } else {
                hashes.insert(hash, replay.is_detailed());
                true
            }
        })
        .collect::<Vec<_>>()
}

fn dedupe_replays_like_frontend(replays: Vec<ReplayInfo>) -> Vec<ReplayInfo> {
    let mut files = HashMap::new();
    replays
        .into_iter()
        .filter(|replay| {
            let file_key = replay.file().to_string();
            let is_detailed = files.get(&file_key);

            if is_detailed.is_some() && (*is_detailed.unwrap() || !replay.is_detailed()) {
                false
            } else {
                files.insert(file_key, replay.is_detailed());
                true
            }
        })
        .collect::<Vec<_>>()
}

fn read_env_file_value(env_file: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(env_file).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((current_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if current_key.trim() != key {
            continue;
        }
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        return Some(value.to_string());
    }
    None
}

fn resolve_account_dir() -> Option<PathBuf> {
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    let env_path = CacheAnalysisPaths::repo_root().join(".env");
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Some(value) = read_env_file_value(&env_path, key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    None
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn unique_benchmark_root() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sco_detailed_sqlite_bench_{timestamp}"))
}

fn sqlite_sidecar_paths(path: &Path) -> [PathBuf; 3] {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_os_string();
    shm.push("-shm");
    [path.to_path_buf(), PathBuf::from(wal), PathBuf::from(shm)]
}

fn remove_sqlite_files(path: &Path) {
    for path in sqlite_sidecar_paths(path) {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("failed to remove '{}': {error}", path.display()),
        }
    }
}

fn benchmark_runtime(worker_count: usize, batch_size: usize) -> GenerateCacheRuntimeOptions {
    GenerateCacheRuntimeOptions::default()
        .with_worker_count(worker_count)
        .with_timings_enabled(true)
        .with_cache_entry_sink_batch_size(batch_size)
}

fn run_benchmark_once(
    mode: BenchmarkMode,
    run_index: usize,
    account_dir: &Path,
    output_file: &Path,
    resources: &ReplayAnalysisResources,
    worker_count: usize,
    batch_size: usize,
) -> DetailedAnalysisBenchmarkRun {
    remove_sqlite_files(output_file);

    let config = GenerateCacheConfig::new(account_dir, output_file);
    let mut runtime = benchmark_runtime(worker_count, batch_size);
    let mut cache_writer = None;

    if mode == BenchmarkMode::AnalyzerWithSqlite {
        let writer = ReplayCacheWriteQueue::start_detailed_analysis(output_file.to_path_buf());
        runtime = runtime
            .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(writer.sender())));
        cache_writer = Some(writer);
    }

    let total_start = Instant::now();
    let analyze_start = Instant::now();
    let summary = DetailedReplayAnalyzer::analyze_full_detailed(&config, resources, None, &runtime)
        .expect("full detailed analysis benchmark should succeed");
    let analyze_wall = analyze_start.elapsed();
    drop(runtime);

    let writer_finish_start = Instant::now();
    let write_result = cache_writer.map(ReplayCacheWriteQueue::finish);
    let writer_finish_wall = writer_finish_start.elapsed();

    let replay_summary_start = Instant::now();
    let replays = replay_summary_from_entries(summary.cache_entries(), resources);
    let replay_summary_wall = replay_summary_start.elapsed();
    let replay_summary_count = replays.len();

    let legacy_hash_dedupe_start = Instant::now();
    let _legacy_hash_deduped_replays = dedupe_replays_by_hash_like_old_frontend(replays.clone());
    let legacy_hash_dedupe_wall = legacy_hash_dedupe_start.elapsed();

    let dedupe_hash_start = Instant::now();
    let deduped_replays = dedupe_replays_like_frontend(replays);
    let dedupe_hash_wall = dedupe_hash_start.elapsed();
    let deduped_replay_count = deduped_replays.len();

    let stats_rebuild_start = Instant::now();
    let _snapshot = ReplayAnalysis::build_rebuild_snapshot_with_dictionary(
        &deduped_replays,
        true,
        &HashSet::new(),
        &HashSet::new(),
        resources.dictionary_data(),
    );
    let stats_rebuild_wall = stats_rebuild_start.elapsed();
    let total_wall = total_start.elapsed();

    remove_sqlite_files(output_file);

    DetailedAnalysisBenchmarkRun {
        mode,
        run_index,
        total_wall,
        analyze_wall,
        writer_finish_wall,
        replay_summary_wall,
        legacy_hash_dedupe_wall,
        dedupe_hash_wall,
        stats_rebuild_wall,
        scanned_replays: summary.scanned_replays(),
        replay_summary_count,
        deduped_replay_count,
        timing_report: summary.timing_report().clone(),
        write_result,
    }
}

fn populate_sqlite_cache_for_warm_benchmark(
    account_dir: &Path,
    output_file: &Path,
    resources: &ReplayAnalysisResources,
    worker_count: usize,
    batch_size: usize,
) -> (Duration, ReplayCacheWriteResult) {
    remove_sqlite_files(output_file);

    let config = GenerateCacheConfig::new(account_dir, output_file);
    let writer = ReplayCacheWriteQueue::start_detailed_analysis(output_file.to_path_buf());
    let runtime = benchmark_runtime(worker_count, batch_size)
        .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(writer.sender())));

    let cold_populate_start = Instant::now();
    let summary = DetailedReplayAnalyzer::analyze_full_detailed(&config, resources, None, &runtime)
        .expect("warm benchmark cold cache population should succeed");
    drop(runtime);
    let write_result = writer.finish();
    assert!(
        summary.completed(),
        "warm benchmark cold cache population should complete"
    );
    (cold_populate_start.elapsed(), write_result)
}

fn load_existing_detailed_identities_for_warm_benchmark(
    output_file: &Path,
) -> (Duration, Duration, HashMap<String, ReplayCacheFileIdentity>) {
    let existing_cache_open_start = Instant::now();
    let database = ReplayCacheDatabase::open_for_cache_path(output_file)
        .expect("warm benchmark database should open");
    let existing_cache_open_wall = existing_cache_open_start.elapsed();

    let existing_cache_load_start = Instant::now();
    let identities_by_hash = database
        .load_detailed_cache_identities_by_hash()
        .expect("warm benchmark detailed identities should load");
    let existing_cache_load_wall = existing_cache_load_start.elapsed();
    (
        existing_cache_open_wall,
        existing_cache_load_wall,
        identities_by_hash,
    )
}

fn run_warm_benchmark(
    account_dir: &Path,
    output_file: &Path,
    resources: &ReplayAnalysisResources,
    worker_count: usize,
    batch_size: usize,
) -> WarmDetailedAnalysisBenchmarkRun {
    let (cold_populate_wall, cold_write_result) = populate_sqlite_cache_for_warm_benchmark(
        account_dir,
        output_file,
        resources,
        worker_count,
        batch_size,
    );
    let (existing_cache_open_wall, existing_cache_load_wall, existing_identities_by_hash) =
        load_existing_detailed_identities_for_warm_benchmark(output_file);
    let existing_entries = existing_identities_by_hash.len();

    let config = GenerateCacheConfig::new(account_dir, output_file);
    let writer = ReplayCacheWriteQueue::start_detailed_analysis(output_file.to_path_buf());
    let runtime = benchmark_runtime(worker_count, batch_size)
        .with_existing_detailed_cache_identities_by_hash(existing_identities_by_hash)
        .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(writer.sender())));

    let warm_total_start = Instant::now();
    let warm_analyze_start = Instant::now();
    let summary = DetailedReplayAnalyzer::analyze_full_detailed(&config, resources, None, &runtime)
        .expect("warm detailed analysis benchmark should succeed");
    let warm_analyze_wall = warm_analyze_start.elapsed();
    drop(runtime);

    let warm_writer_finish_start = Instant::now();
    let warm_write_result = writer.finish();
    let warm_writer_finish_wall = warm_writer_finish_start.elapsed();

    let replay_summary_wall = Duration::ZERO;
    let replay_summary_count = summary.cache_entries().len();
    let dedupe_wall = Duration::ZERO;

    let stats_rebuild_start = Instant::now();
    let stats_payload = ReplayCacheDatabase::open_for_cache_path(output_file)
        .and_then(|database| {
            database.load_statistics_payload(
                &ReplayCacheStatsQuery::new(ReplayCacheReadScope::DetailedOnly, 0),
                &HashSet::new(),
                &HashSet::new(),
                resources.dictionary_data(),
            )
        })
        .expect("warm benchmark statistics should load from sqlite");
    let stats_rebuild_wall = stats_rebuild_start.elapsed();
    let deduped_replay_count = usize::try_from(stats_payload.games()).unwrap_or(usize::MAX);
    let warm_total_wall = warm_total_start.elapsed();

    let run = WarmDetailedAnalysisBenchmarkRun {
        cold_populate_wall,
        cold_write_result,
        existing_cache_open_wall,
        existing_cache_load_wall,
        existing_entries,
        warm_total_wall,
        warm_analyze_wall,
        warm_writer_finish_wall,
        warm_write_result,
        replay_summary_wall,
        dedupe_wall,
        stats_rebuild_wall,
        scanned_replays: summary.scanned_replays(),
        replay_summary_count,
        deduped_replay_count,
        timing_report: summary.timing_report().clone(),
    };
    remove_sqlite_files(output_file);
    run
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidatePruningFlow {
    CurrentHead,
    NoPruningBaseline,
    OfficialPartialOnly,
    Md5SqliteOnly,
    OfficialThenMd5Sqlite,
}

impl CandidatePruningFlow {
    fn label(self) -> &'static str {
        match self {
            Self::CurrentHead => "current_head",
            Self::NoPruningBaseline => "no_pruning_baseline",
            Self::OfficialPartialOnly => "official_partial_only",
            Self::Md5SqliteOnly => "md5_sqlite_only",
            Self::OfficialThenMd5Sqlite => "official_then_md5_sqlite",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "current_head" => Some(Self::CurrentHead),
            "no_pruning_baseline" | "head_baseline" => Some(Self::NoPruningBaseline),
            "official_partial_only" => Some(Self::OfficialPartialOnly),
            "md5_sqlite_only" => Some(Self::Md5SqliteOnly),
            "official_then_md5_sqlite" => Some(Self::OfficialThenMd5Sqlite),
            _ => None,
        }
    }

    fn uses_official_prefilter(self) -> bool {
        matches!(
            self,
            Self::OfficialPartialOnly | Self::OfficialThenMd5Sqlite
        )
    }

    fn uses_md5_sqlite_filter(self) -> bool {
        matches!(self, Self::Md5SqliteOnly | Self::OfficialThenMd5Sqlite)
    }

    fn uses_runtime_sqlite_filter(self) -> bool {
        self == Self::CurrentHead
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FlowCacheState {
    Cold,
    Warm,
}

impl FlowCacheState {
    fn label(self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Warm => "warm",
        }
    }
}

#[derive(Debug, Clone)]
struct ReplayFileCheckIdentity {
    path: PathBuf,
    hash: String,
    modified_seconds: u64,
}

impl ReplayFileCheckIdentity {
    fn from_path(path: &Path) -> Self {
        let hash = ReplayFileIdentity::calculate_hash(path);
        let modified_seconds = ReplayFileIdentity::modified_seconds(path).unwrap_or_default();
        Self {
            path: path.to_path_buf(),
            hash,
            modified_seconds,
        }
    }

    fn needs_processing(&self, existing: &HashMap<String, ReplayCacheFileIdentity>) -> bool {
        if self.hash.trim().is_empty() {
            return true;
        }
        existing
            .get(&self.hash)
            .is_none_or(|identity| identity.modified_seconds() != self.modified_seconds)
    }
}

#[derive(Debug)]
struct CandidateFlowBenchmarkRun {
    flow: CandidatePruningFlow,
    state: FlowCacheState,
    iteration: usize,
    total_wall: Duration,
    official_prefilter_wall: Duration,
    md5_wall: Duration,
    sqlite_lookup_wall: Duration,
    analyze_wall: Duration,
    writer_finish_wall: Duration,
    total_files: usize,
    official_files: usize,
    hash_checked_files: usize,
    sqlite_known_identities: usize,
    detailed_input_files: usize,
    scanned_replays: usize,
    timing_report: GenerateCacheTimingReport,
    write_result: ReplayCacheWriteResult,
}

impl CandidateFlowBenchmarkRun {
    fn print(&self) {
        println!(
            concat!(
                "SCO_DETAILED_FLOW_BENCH ",
                "flow={} state={} iteration={} total_ms={} official_prefilter_ms={} ",
                "md5_ms={} sqlite_lookup_ms={} analyze_ms={} writer_finish_ms={} ",
                "total_files={} official_files={} hash_checked_files={} ",
                "sqlite_known_identities={} detailed_input_files={} scanned={} ",
                "workers={} analyzer_total_ms={} collect_candidates_ms={} ",
                "hash_lookup_ms={} partition_ms={} pending_candidates={} ",
                "reused_candidates={} replay_analysis_ms={} sqlite_open_ms={} ",
                "sqlite_write_ms={} sqlite_batches={} sqlite_attempted_entries={} ",
                "sqlite_persisted_entries={} sqlite_failed_batches={}"
            ),
            self.flow.label(),
            self.state.label(),
            self.iteration,
            millis(self.total_wall),
            millis(self.official_prefilter_wall),
            millis(self.md5_wall),
            millis(self.sqlite_lookup_wall),
            millis(self.analyze_wall),
            millis(self.writer_finish_wall),
            self.total_files,
            self.official_files,
            self.hash_checked_files,
            self.sqlite_known_identities,
            self.detailed_input_files,
            self.scanned_replays,
            self.timing_report.worker_count(),
            millis(self.timing_report.total()),
            millis(self.timing_report.collect_candidates_parallel()),
            millis(self.timing_report.collect_candidates_hash_lookup()),
            millis(self.timing_report.partition_candidates()),
            self.timing_report.pending_candidate_count(),
            self.timing_report.reused_candidate_count(),
            millis(self.timing_report.replay_analysis_parallel()),
            millis(self.write_result.database_open()),
            millis(self.write_result.sqlite_write()),
            self.write_result.processed_batches(),
            self.write_result.attempted_entries(),
            self.write_result.persisted_entries(),
            self.write_result.failed_batches(),
        );
    }
}

#[derive(Debug)]
struct CandidateFlowBenchmarkEnvironment {
    flow: CandidatePruningFlow,
    cold_cache_path: PathBuf,
    warm_cache_path: PathBuf,
}

impl CandidateFlowBenchmarkEnvironment {
    fn new(root: &Path, flow: CandidatePruningFlow) -> Self {
        let environment_root = root.join(flow.label());
        fs::create_dir_all(&environment_root)
            .expect("candidate flow benchmark environment should be created");
        Self {
            flow,
            cold_cache_path: environment_root.join("cold.sqlite3"),
            warm_cache_path: environment_root.join("warm.sqlite3"),
        }
    }
}

struct CandidateFlowBenchmarkContext<'a> {
    account_dir: &'a Path,
    replay_files: &'a [PathBuf],
    protocol_store: Arc<ProtocolStore>,
    resources: &'a ReplayAnalysisResources,
    worker_count: usize,
    batch_size: usize,
}

impl<'a> CandidateFlowBenchmarkContext<'a> {
    fn new(
        account_dir: &'a Path,
        replay_files: &'a [PathBuf],
        protocol_store: Arc<ProtocolStore>,
        resources: &'a ReplayAnalysisResources,
        worker_count: usize,
        batch_size: usize,
    ) -> Self {
        Self {
            account_dir,
            replay_files,
            protocol_store,
            resources,
            worker_count,
            batch_size,
        }
    }

    fn run_once(
        &self,
        flow: CandidatePruningFlow,
        state: FlowCacheState,
        iteration: usize,
        cache_path: &Path,
    ) -> CandidateFlowBenchmarkRun {
        if state == FlowCacheState::Cold {
            remove_sqlite_files(cache_path);
        }

        let total_start = Instant::now();
        let (official_prefilter_wall, official_files) = if flow.uses_official_prefilter() {
            let official = collect_official_blizzard_replays(
                self.replay_files,
                self.worker_count,
                Arc::clone(&self.protocol_store),
            );
            (official.duration, official.files)
        } else {
            (Duration::ZERO, self.replay_files.to_vec())
        };
        let official_file_count = official_files.len();

        let (md5_wall, sqlite_lookup_wall, sqlite_known_identities, detailed_input_files) =
            if flow.uses_md5_sqlite_filter() {
                let (md5_wall, file_identities) =
                    collect_replay_file_check_identities(&official_files, self.worker_count);
                let (sqlite_lookup_wall, existing_identities) =
                    load_existing_flow_identities(cache_path);
                let detailed_input_files = file_identities
                    .iter()
                    .filter(|identity| identity.needs_processing(&existing_identities))
                    .map(|identity| identity.path.clone())
                    .collect::<Vec<_>>();
                (
                    md5_wall,
                    sqlite_lookup_wall,
                    existing_identities.len(),
                    detailed_input_files,
                )
            } else {
                (Duration::ZERO, Duration::ZERO, 0, official_files)
            };
        let (sqlite_lookup_wall, sqlite_known_identities, runtime_existing_identities_by_hash) =
            if flow.uses_runtime_sqlite_filter() {
                let (wall, existing_identities) = load_existing_flow_identities(cache_path);
                (wall, existing_identities.len(), existing_identities)
            } else {
                (sqlite_lookup_wall, sqlite_known_identities, HashMap::new())
            };
        let hash_checked_files = if flow.uses_md5_sqlite_filter() {
            official_file_count
        } else {
            0
        };
        let detailed_input_file_count = detailed_input_files.len();

        let writer = ReplayCacheWriteQueue::start_detailed_analysis(cache_path.to_path_buf());
        let mut runtime = benchmark_runtime(self.worker_count, self.batch_size)
            .with_replay_files(detailed_input_files)
            .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(writer.sender())));
        if flow.uses_runtime_sqlite_filter() {
            runtime = runtime.with_existing_detailed_cache_identities_by_hash(
                runtime_existing_identities_by_hash,
            );
        }
        let config = GenerateCacheConfig::new(self.account_dir, cache_path);

        let analyze_start = Instant::now();
        let summary =
            DetailedReplayAnalyzer::analyze_full_detailed(&config, self.resources, None, &runtime)
                .expect("candidate flow detailed analysis should succeed");
        let analyze_wall = analyze_start.elapsed();
        drop(runtime);

        let writer_finish_start = Instant::now();
        let write_result = writer.finish();
        let writer_finish_wall = writer_finish_start.elapsed();
        let total_wall = total_start.elapsed();

        CandidateFlowBenchmarkRun {
            flow,
            state,
            iteration,
            total_wall,
            official_prefilter_wall,
            md5_wall,
            sqlite_lookup_wall,
            analyze_wall,
            writer_finish_wall,
            total_files: self.replay_files.len(),
            official_files: official_file_count,
            hash_checked_files,
            sqlite_known_identities,
            detailed_input_files: detailed_input_file_count,
            scanned_replays: summary.scanned_replays(),
            timing_report: summary.timing_report().clone(),
            write_result,
        }
    }

    fn prepare_warm_database(&self, environment: &CandidateFlowBenchmarkEnvironment) {
        remove_sqlite_files(&environment.warm_cache_path);
        let _ = self.run_once(
            environment.flow,
            FlowCacheState::Cold,
            0,
            &environment.warm_cache_path,
        );
    }
}

fn collect_replay_files_recursive(root: &Path) -> Vec<PathBuf> {
    fn visit_dir(dir: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                visit_dir(&path, files);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("SC2Replay"))
            {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    visit_dir(root, &mut files);
    files.sort_by(|left, right| {
        left.to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.to_string_lossy().to_ascii_lowercase())
    });
    files
}

fn is_official_blizzard_replay(path: &Path, protocol_store: &ProtocolStore) -> bool {
    if DetailedReplayAnalyzer::is_mm_replay_path(path) {
        return false;
    }

    let parsed = ReplayParser::parse_file_with_store_timed_options(
        path,
        protocol_store,
        ReplayParseMode::Simple,
        ReplayParseOptions::new().with_decode_attributes(false),
    );
    let Ok(parsed) = parsed else {
        return false;
    };
    let mut replay = parsed.take_replay();
    replay
        .take_details()
        .is_some_and(|details| details.m_isBlizzardMap)
}

fn collect_official_blizzard_replays(
    replay_files: &[PathBuf],
    worker_count: usize,
    protocol_store: Arc<ProtocolStore>,
) -> DurationAndFiles {
    let started_at = Instant::now();
    let thread_pool = ThreadPoolBuilder::new()
        .num_threads(worker_count.max(1))
        .build()
        .expect("official prefilter thread pool should build");
    let mut files = thread_pool.install(|| {
        replay_files
            .par_iter()
            .filter_map(|path| {
                is_official_blizzard_replay(path, protocol_store.as_ref()).then(|| path.clone())
            })
            .collect::<Vec<_>>()
    });
    files.sort_by(|left, right| {
        left.to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.to_string_lossy().to_ascii_lowercase())
    });
    DurationAndFiles::new(started_at.elapsed(), files)
}

#[derive(Debug)]
struct DurationAndFiles {
    duration: Duration,
    files: Vec<PathBuf>,
}

impl DurationAndFiles {
    fn new(duration: Duration, files: Vec<PathBuf>) -> Self {
        Self { duration, files }
    }
}

fn collect_replay_file_check_identities(
    replay_files: &[PathBuf],
    worker_count: usize,
) -> (Duration, Vec<ReplayFileCheckIdentity>) {
    let started_at = Instant::now();
    let thread_pool = ThreadPoolBuilder::new()
        .num_threads(worker_count.max(1))
        .build()
        .expect("md5 thread pool should build");
    let identities = thread_pool.install(|| {
        replay_files
            .par_iter()
            .map(|path| ReplayFileCheckIdentity::from_path(path))
            .collect::<Vec<_>>()
    });
    (started_at.elapsed(), identities)
}

fn load_existing_flow_identities(
    cache_path: &Path,
) -> (Duration, HashMap<String, ReplayCacheFileIdentity>) {
    let started_at = Instant::now();
    let database = ReplayCacheDatabase::open_for_cache_path(cache_path)
        .expect("candidate flow benchmark database should open");
    let identities = database
        .load_detailed_cache_identities_by_hash()
        .expect("candidate flow benchmark identities should load");
    (started_at.elapsed(), identities)
}

#[ignore = "performance benchmark: reads the configured SC2 account folder and can take minutes"]
#[test]
fn benchmark_full_detailed_analysis_to_sqlite() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping benchmark: SC2 account directory is not configured");
        return;
    };

    let worker_count = env_usize(
        "SCO_DETAILED_SQLITE_BENCH_WORKERS",
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .saturating_div(2)
            .max(1),
    );
    let pair_count = env_usize("SCO_DETAILED_SQLITE_BENCH_PAIRS", 1);
    let batch_size = env_usize(
        "SCO_DETAILED_SQLITE_BENCH_BATCH_SIZE",
        DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE,
    );
    let replay_count = CacheAnalysisPaths::count_replays(&account_dir);
    let benchmark_root = unique_benchmark_root();
    fs::create_dir_all(&benchmark_root).expect("benchmark temp directory should be created");

    let resources = ReplayAnalysisResources::from_dictionary_data(Arc::new(
        Sc2DictionaryData::load(None).expect("dictionary data should load for benchmark"),
    ))
    .expect("replay analysis resources should load for benchmark");

    println!(
        concat!(
            "SCO_DETAILED_SQLITE_BENCH_START ",
            "account_dir='{}' replay_files={} workers={} pairs={} batch_size={}"
        ),
        account_dir.display(),
        replay_count,
        worker_count,
        pair_count,
        batch_size,
    );

    for pair_index in 0..pair_count {
        let ordered_modes = if pair_index % 2 == 0 {
            [
                BenchmarkMode::AnalyzerOnly,
                BenchmarkMode::AnalyzerWithSqlite,
            ]
        } else {
            [
                BenchmarkMode::AnalyzerWithSqlite,
                BenchmarkMode::AnalyzerOnly,
            ]
        };

        let mut analyzer_only = None;
        let mut analyzer_with_sqlite = None;
        for (mode_index, mode) in ordered_modes.into_iter().enumerate() {
            let output_file =
                benchmark_root.join(format!("pair_{pair_index}_mode_{mode_index}.sqlite3"));
            let run = run_benchmark_once(
                mode,
                pair_index.saturating_mul(2).saturating_add(mode_index),
                &account_dir,
                &output_file,
                &resources,
                worker_count,
                batch_size,
            );
            run.print();
            match mode {
                BenchmarkMode::AnalyzerOnly => analyzer_only = Some(run),
                BenchmarkMode::AnalyzerWithSqlite => analyzer_with_sqlite = Some(run),
            }
        }

        DetailedAnalysisBenchmarkPair {
            analyzer_only: analyzer_only.expect("analyzer-only run should exist"),
            analyzer_with_sqlite: analyzer_with_sqlite.expect("sqlite run should exist"),
        }
        .print_delta(pair_index);
    }

    if let Err(error) = fs::remove_dir_all(&benchmark_root) {
        eprintln!(
            "failed to remove benchmark temp dir '{}': {error}",
            benchmark_root.display()
        );
    }
}

#[ignore = "performance benchmark: reads the configured SC2 account folder and can take minutes"]
#[test]
fn benchmark_warm_detailed_analysis_from_sqlite() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping benchmark: SC2 account directory is not configured");
        return;
    };

    let worker_count = env_usize(
        "SCO_DETAILED_SQLITE_BENCH_WORKERS",
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .saturating_div(2)
            .max(1),
    );
    let batch_size = env_usize(
        "SCO_DETAILED_SQLITE_BENCH_BATCH_SIZE",
        DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE,
    );
    let replay_count = CacheAnalysisPaths::count_replays(&account_dir);
    let benchmark_root = unique_benchmark_root();
    fs::create_dir_all(&benchmark_root).expect("benchmark temp directory should be created");

    let resources = ReplayAnalysisResources::from_dictionary_data(Arc::new(
        Sc2DictionaryData::load(None).expect("dictionary data should load for benchmark"),
    ))
    .expect("replay analysis resources should load for benchmark");

    println!(
        concat!(
            "SCO_DETAILED_SQLITE_WARM_BENCH_START ",
            "account_dir='{}' replay_files={} workers={} batch_size={}"
        ),
        account_dir.display(),
        replay_count,
        worker_count,
        batch_size,
    );

    let output_file = benchmark_root.join("warm.sqlite3");
    run_warm_benchmark(
        &account_dir,
        &output_file,
        &resources,
        worker_count,
        batch_size,
    )
    .print();

    if let Err(error) = fs::remove_dir_all(&benchmark_root) {
        eprintln!(
            "failed to remove benchmark temp dir '{}': {error}",
            benchmark_root.display()
        );
    }
}

#[ignore = "performance benchmark: reads the configured SC2 account folder and can take a long time"]
#[test]
fn benchmark_candidate_pruning_flows_cold_warm() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping benchmark: SC2 account directory is not configured");
        return;
    };

    let worker_count = env_usize(
        "SCO_DETAILED_FLOW_BENCH_WORKERS",
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .saturating_div(2)
            .max(1),
    );
    let run_count = env_usize("SCO_DETAILED_FLOW_BENCH_RUNS", 10);
    let batch_size = env_usize(
        "SCO_DETAILED_FLOW_BENCH_BATCH_SIZE",
        DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE,
    );
    let benchmark_root = unique_benchmark_root();
    fs::create_dir_all(&benchmark_root).expect("benchmark temp directory should be created");

    let replay_files = collect_replay_files_recursive(&account_dir);
    let resources = ReplayAnalysisResources::from_dictionary_data(Arc::new(
        Sc2DictionaryData::load(None).expect("dictionary data should load for benchmark"),
    ))
    .expect("replay analysis resources should load for benchmark");
    let protocol_store =
        Arc::new(ProtocolStoreBuilder::build().expect("protocol store should build for benchmark"));

    println!(
        concat!(
            "SCO_DETAILED_FLOW_BENCH_START ",
            "account_dir='{}' replay_files={} workers={} runs_per_state={} batch_size={}"
        ),
        account_dir.display(),
        replay_files.len(),
        worker_count,
        run_count,
        batch_size,
    );
    let context = CandidateFlowBenchmarkContext::new(
        &account_dir,
        &replay_files,
        Arc::clone(&protocol_store),
        &resources,
        worker_count,
        batch_size,
    );

    let default_flows = [
        CandidatePruningFlow::CurrentHead,
        CandidatePruningFlow::NoPruningBaseline,
        CandidatePruningFlow::OfficialPartialOnly,
        CandidatePruningFlow::Md5SqliteOnly,
        CandidatePruningFlow::OfficialThenMd5Sqlite,
    ];
    let flows = std::env::var("SCO_DETAILED_FLOW_BENCH_FLOWS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .filter_map(|label| CandidatePruningFlow::from_label(label.trim()))
                .collect::<Vec<_>>()
        })
        .filter(|flows| !flows.is_empty())
        .unwrap_or_else(|| default_flows.to_vec());
    for flow in flows {
        let environment = CandidateFlowBenchmarkEnvironment::new(&benchmark_root, flow);
        context.prepare_warm_database(&environment);

        for iteration in 0..run_count {
            let cold_run = context.run_once(
                flow,
                FlowCacheState::Cold,
                iteration,
                &environment.cold_cache_path,
            );
            cold_run.print();

            let warm_run = context.run_once(
                flow,
                FlowCacheState::Warm,
                iteration,
                &environment.warm_cache_path,
            );
            warm_run.print();
        }
    }

    if let Err(error) = fs::remove_dir_all(&benchmark_root) {
        eprintln!(
            "failed to remove benchmark temp dir '{}': {error}",
            benchmark_root.display()
        );
    }
}
