use super::{
    AnalyzerTimingConfig, CacheEntrySink, DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE,
    DetailedReplayAnalyzer, GenerateCacheTimingReport, ReplayCacheFileIdentity,
};
use crate::cache_overall_stats_generator::CacheReplayEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheConfig {
    account_dir: PathBuf,
    output_file: PathBuf,
    recent_replay_count: Option<usize>,
}

impl GenerateCacheConfig {
    pub fn new(account_dir: impl Into<PathBuf>, output_file: impl Into<PathBuf>) -> Self {
        Self {
            account_dir: account_dir.into(),
            output_file: output_file.into(),
            recent_replay_count: None,
        }
    }

    pub fn with_recent_replay_count(mut self, recent_replay_count: Option<usize>) -> Self {
        self.recent_replay_count = recent_replay_count;
        self
    }

    pub fn account_dir(&self) -> &Path {
        &self.account_dir
    }

    pub fn output_file(&self) -> &Path {
        &self.output_file
    }

    pub fn recent_replay_count(&self) -> Option<usize> {
        self.recent_replay_count
    }

    pub fn collect_replay_files(&self) -> Vec<PathBuf> {
        DetailedReplayAnalyzer::collect_cache_replay_files(
            &self.account_dir,
            self.recent_replay_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateCacheSummary {
    scanned_replays: usize,
    output_file: PathBuf,
    entries: Vec<CacheReplayEntry>,
    completed: bool,
    timing_report: GenerateCacheTimingReport,
}

impl GenerateCacheSummary {
    pub(super) fn new(
        scanned_replays: usize,
        output_file: PathBuf,
        entries: Vec<CacheReplayEntry>,
        completed: bool,
        timing_report: GenerateCacheTimingReport,
    ) -> Self {
        Self {
            scanned_replays,
            output_file,
            entries,
            completed,
            timing_report,
        }
    }

    pub fn scanned_replays(&self) -> usize {
        self.scanned_replays
    }

    pub fn output_file(&self) -> &Path {
        &self.output_file
    }

    pub fn cache_entries(&self) -> &[CacheReplayEntry] {
        &self.entries
    }

    pub fn into_cache_entries(self) -> Vec<CacheReplayEntry> {
        self.entries
    }

    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn timing_report(&self) -> &GenerateCacheTimingReport {
        &self.timing_report
    }
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

#[derive(Clone, Default)]
pub struct GenerateCacheRuntimeOptions {
    worker_count: Option<usize>,
    stop_controller: Option<Arc<GenerateCacheStopController>>,
    timings_enabled: Option<bool>,
    cache_entry_sink: Option<Arc<dyn CacheEntrySink>>,
    cache_entry_sink_batch_size: Option<usize>,
    existing_detailed_cache_entries: Option<HashMap<String, CacheReplayEntry>>,
    existing_detailed_cache_identities_by_hash: Option<HashMap<String, ReplayCacheFileIdentity>>,
    replay_files: Option<Vec<PathBuf>>,
}

impl std::fmt::Debug for GenerateCacheRuntimeOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GenerateCacheRuntimeOptions")
            .field("worker_count", &self.worker_count)
            .field("stop_controller", &self.stop_controller.is_some())
            .field("timings_enabled", &self.timings_enabled)
            .field("cache_entry_sink", &self.cache_entry_sink.is_some())
            .field(
                "cache_entry_sink_batch_size",
                &self.cache_entry_sink_batch_size,
            )
            .field(
                "existing_detailed_cache_entries",
                &self
                    .existing_detailed_cache_entries
                    .as_ref()
                    .map(HashMap::len),
            )
            .field(
                "existing_detailed_cache_identities_by_hash",
                &self
                    .existing_detailed_cache_identities_by_hash
                    .as_ref()
                    .map(HashMap::len),
            )
            .field(
                "replay_files",
                &self.replay_files.as_ref().map(std::vec::Vec::len),
            )
            .finish()
    }
}

impl GenerateCacheRuntimeOptions {
    pub fn with_worker_count(mut self, worker_count: usize) -> Self {
        self.worker_count = Some(worker_count);
        self
    }

    pub fn with_stop_controller(
        mut self,
        stop_controller: Arc<GenerateCacheStopController>,
    ) -> Self {
        self.stop_controller = Some(stop_controller);
        self
    }

    pub fn with_detailed_report_timings(mut self, enabled: bool) -> Self {
        self.timings_enabled = Some(enabled);
        self
    }

    pub fn with_timings_enabled(mut self, enabled: bool) -> Self {
        self.timings_enabled = Some(enabled);
        self
    }

    pub fn with_cache_entry_sink(mut self, sink: Arc<dyn CacheEntrySink>) -> Self {
        self.cache_entry_sink = Some(sink);
        self
    }

    pub fn with_cache_entry_sink_batch_size(mut self, batch_size: usize) -> Self {
        self.cache_entry_sink_batch_size = Some(batch_size.max(1));
        self
    }

    pub fn with_existing_detailed_cache_entries(
        mut self,
        entries: HashMap<String, CacheReplayEntry>,
    ) -> Self {
        self.existing_detailed_cache_entries = Some(entries);
        self
    }

    pub fn with_existing_detailed_cache_identities_by_hash(
        mut self,
        identities_by_hash: HashMap<String, ReplayCacheFileIdentity>,
    ) -> Self {
        self.existing_detailed_cache_identities_by_hash = Some(identities_by_hash);
        self
    }

    pub fn with_replay_files(mut self, replay_files: Vec<PathBuf>) -> Self {
        self.replay_files = Some(replay_files);
        self
    }

    pub(super) fn timings_enabled(&self) -> bool {
        self.timings_enabled
            .unwrap_or_else(AnalyzerTimingConfig::enabled_from_env)
    }

    pub(super) fn detailed_report_timings_enabled(&self) -> bool {
        self.timings_enabled()
    }

    pub(super) fn resolved_worker_count(&self, total_files: usize) -> usize {
        self.worker_count
            .map(|value| std::cmp::max(1, std::cmp::min(value, total_files)))
            .unwrap_or_else(|| Self::default_worker_count(total_files))
    }

    pub(super) fn default_worker_count(total_files: usize) -> usize {
        std::cmp::max(1, std::cmp::min(Self::half_cpu_worker_cap(), total_files))
    }

    pub(super) fn half_cpu_worker_cap() -> usize {
        let cpu_count = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        std::cmp::max(1, cpu_count / 2)
    }

    pub(super) fn stop_controller(&self) -> Option<Arc<GenerateCacheStopController>> {
        self.stop_controller.clone()
    }

    pub(super) fn cache_entry_sink(&self) -> Option<Arc<dyn CacheEntrySink>> {
        self.cache_entry_sink.clone()
    }

    pub(super) fn cache_entry_sink_batch_size(&self) -> usize {
        self.cache_entry_sink_batch_size
            .unwrap_or(DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE)
            .max(1)
    }

    pub(super) fn existing_detailed_cache_entries(
        &self,
    ) -> Option<HashMap<String, CacheReplayEntry>> {
        self.existing_detailed_cache_entries.clone()
    }

    pub(super) fn existing_detailed_cache_identities_by_hash(
        &self,
    ) -> Option<HashMap<String, ReplayCacheFileIdentity>> {
        self.existing_detailed_cache_identities_by_hash.clone()
    }

    pub(super) fn replay_files(&self) -> Option<Vec<PathBuf>> {
        self.replay_files.clone()
    }
}

#[derive(Debug, Error)]
pub enum GenerateCacheError {
    #[error("account directory does not exist or is not a directory: {0}")]
    InvalidAccountDirectory(PathBuf),
    #[error("failed to load detailed-analysis cache formatting rules: {0}")]
    DetailedAnalysisConfig(String),
    #[error("failed to build rayon thread pool: {0}")]
    ThreadPoolBuildFailed(String),
    #[error("failed to canonicalize cache payload: {0}")]
    CanonicalizeFailed(#[source] serde_json::Error),
    #[error("failed to write cache entries: {0}")]
    CacheEntrySink(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FullAnalysisMode {
    Simple,
    Detailed,
}

impl FullAnalysisMode {
    pub(super) fn is_detailed(self) -> bool {
        self == Self::Detailed
    }

    pub(super) fn stopped_message(self) -> &'static str {
        match self {
            Self::Simple => "Simple analysis stopped after the current work finished.",
            Self::Detailed => "Detailed analysis stopped after the current work finished.",
        }
    }

    pub(super) fn progress_label(self) -> &'static str {
        match self {
            Self::Simple => "Simple analysis",
            Self::Detailed => "Detailed analysis",
        }
    }
}
