use s2coop_analyzer::cache_overall_stats_detailed_analysis::CacheAnalysisPaths;
use s2coop_analyzer::detailed_replay_analysis::{
    DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, DetailedReplayAnalyzer, GenerateCacheConfig,
    GenerateCacheRuntimeOptions, GenerateCacheTimingReport, ReplayAnalysisResources,
    ReplayFileIdentity,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use sco_tauri_overlay::{
    QueuedReplayCacheEntrySink, ReplayAnalysis, ReplayAnalysisOps, ReplayCacheDatabase,
    ReplayCacheEntryQuery, ReplayCacheWriteQueue, ReplayCacheWriteResult, ReplayInfo,
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
        let writer = ReplayCacheWriteQueue::start(output_file.to_path_buf());
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
    let writer = ReplayCacheWriteQueue::start(output_file.to_path_buf());
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

fn load_existing_detailed_entries_for_warm_benchmark(
    output_file: &Path,
) -> (
    Duration,
    Duration,
    HashMap<String, s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry>,
) {
    let existing_cache_open_start = Instant::now();
    let database = ReplayCacheDatabase::open_for_cache_path(output_file)
        .expect("warm benchmark database should open");
    let existing_cache_open_wall = existing_cache_open_start.elapsed();

    let existing_cache_load_start = Instant::now();
    let entries = database
        .load_entries(ReplayCacheEntryQuery::detailed_only(0))
        .expect("warm benchmark detailed entries should load");
    let existing_cache_load_wall = existing_cache_load_start.elapsed();
    let entries_by_hash = entries
        .into_iter()
        .filter(|entry| entry.detailed_analysis && !entry.hash.is_empty())
        .map(|entry| (entry.hash.clone(), entry))
        .collect::<HashMap<_, _>>();
    (
        existing_cache_open_wall,
        existing_cache_load_wall,
        entries_by_hash,
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
    let (existing_cache_open_wall, existing_cache_load_wall, existing_entries_by_hash) =
        load_existing_detailed_entries_for_warm_benchmark(output_file);
    let existing_entries = existing_entries_by_hash.len();

    let config = GenerateCacheConfig::new(account_dir, output_file);
    let writer = ReplayCacheWriteQueue::start(output_file.to_path_buf());
    let runtime = benchmark_runtime(worker_count, batch_size)
        .with_existing_detailed_cache_entries(existing_entries_by_hash)
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

    let replay_summary_start = Instant::now();
    let replays = replay_summary_from_entries(summary.cache_entries(), resources);
    let replay_summary_wall = replay_summary_start.elapsed();
    let replay_summary_count = replays.len();

    let dedupe_start = Instant::now();
    let deduped_replays = dedupe_replays_like_frontend(replays);
    let dedupe_wall = dedupe_start.elapsed();
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
