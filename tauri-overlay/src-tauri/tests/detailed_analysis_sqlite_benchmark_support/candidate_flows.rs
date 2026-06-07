use super::common::{
    benchmark_runtime, default_worker_count, env_usize, millis, remove_sqlite_files,
    resolve_account_dir, unique_benchmark_root,
};
use rayon::ThreadPoolBuilder;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use s2coop_analyzer::detailed_replay_analysis::{
    DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, DetailedReplayAnalyzer, GenerateCacheConfig,
    GenerateCacheTimingReport, ReplayAnalysisResources, ReplayCacheFileIdentity,
    ReplayFileIdentity,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use s2protocol_port::{
    ProtocolStore, ProtocolStoreBuilder, ReplayParseMode, ReplayParseOptions, ReplayParser,
};
use sco_tauri_overlay::{
    QueuedReplayCacheEntrySink, ReplayCacheDatabase, ReplayCacheWriteQueue, ReplayCacheWriteResult,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

#[ignore = "performance benchmark: reads the configured SC2 account folder and can take a long time"]
#[test]
fn benchmark_candidate_pruning_flows_cold_warm() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping benchmark: SC2 account directory is not configured");
        return;
    };

    let worker_count = env_usize("SCO_DETAILED_FLOW_BENCH_WORKERS", default_worker_count());
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
