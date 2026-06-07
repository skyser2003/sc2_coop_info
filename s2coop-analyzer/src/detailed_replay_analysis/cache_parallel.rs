use super::{
    CacheEntrySink, DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, GenerateCacheRuntimeOptions,
    GenerateCacheStopController,
};
use crate::cache_overall_stats_generator::CacheReplayEntry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheParseMode {
    Simple,
    DetailedBase,
}

impl ReplayCacheParseMode {
    pub(super) fn include_events(self) -> bool {
        match self {
            Self::Simple => false,
            Self::DetailedBase => true,
        }
    }
}

#[derive(Clone)]
pub struct ReplayCacheParallelParseOptions {
    mode: ReplayCacheParseMode,
    worker_count: usize,
    stop_controller: Option<Arc<GenerateCacheStopController>>,
    cache_entry_sink: Option<Arc<dyn CacheEntrySink>>,
    cache_entry_sink_batch_size: usize,
}

impl ReplayCacheParallelParseOptions {
    pub fn simple_saved_cache(worker_count: usize) -> Self {
        Self::saved_cache(ReplayCacheParseMode::Simple, worker_count)
    }

    pub fn simple_saved_cache_half_cores() -> Self {
        Self::simple_saved_cache(GenerateCacheRuntimeOptions::half_cpu_worker_cap())
    }

    pub fn saved_cache(mode: ReplayCacheParseMode, worker_count: usize) -> Self {
        Self {
            mode,
            worker_count: worker_count.max(1),
            stop_controller: None,
            cache_entry_sink: None,
            cache_entry_sink_batch_size: DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE,
        }
    }

    pub fn with_stop_controller(
        mut self,
        stop_controller: Option<Arc<GenerateCacheStopController>>,
    ) -> Self {
        self.stop_controller = stop_controller;
        self
    }

    pub fn with_cache_entry_sink(mut self, sink: Arc<dyn CacheEntrySink>) -> Self {
        self.cache_entry_sink = Some(sink);
        self
    }

    pub fn with_cache_entry_sink_batch_size(mut self, batch_size: usize) -> Self {
        self.cache_entry_sink_batch_size = batch_size.max(1);
        self
    }

    pub fn mode(&self) -> ReplayCacheParseMode {
        self.mode
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub(super) fn resolved_worker_count(&self, total_files: usize) -> usize {
        self.worker_count.max(1).min(total_files.max(1))
    }

    pub(super) fn stop_controller(&self) -> Option<Arc<GenerateCacheStopController>> {
        self.stop_controller.clone()
    }

    pub(super) fn cache_entry_sink(&self) -> Option<Arc<dyn CacheEntrySink>> {
        self.cache_entry_sink.clone()
    }

    pub(super) fn cache_entry_sink_batch_size(&self) -> usize {
        self.cache_entry_sink_batch_size
    }
}

#[derive(Debug, Clone)]
pub struct ReplayCacheParsedEntry {
    path: PathBuf,
    entry: Option<CacheReplayEntry>,
    panicked: bool,
}

impl ReplayCacheParsedEntry {
    pub(super) fn new(path: PathBuf, entry: Option<CacheReplayEntry>, panicked: bool) -> Self {
        Self {
            path,
            entry,
            panicked,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entry(&self) -> Option<&CacheReplayEntry> {
        self.entry.as_ref()
    }

    pub fn panicked(&self) -> bool {
        self.panicked
    }

    pub fn into_parts(self) -> (PathBuf, Option<CacheReplayEntry>, bool) {
        (self.path, self.entry, self.panicked)
    }
}

#[derive(Debug, Clone)]
pub struct ReplayCacheParallelMapResult<T> {
    values: Vec<T>,
    completed: bool,
    worker_count: usize,
    persisted_entries: usize,
}

impl<T> ReplayCacheParallelMapResult<T> {
    pub(super) fn new(
        values: Vec<T>,
        completed: bool,
        worker_count: usize,
        persisted_entries: usize,
    ) -> Self {
        Self {
            values,
            completed,
            worker_count,
            persisted_entries,
        }
    }

    pub fn into_values(self) -> Vec<T> {
        self.values
    }

    pub fn values(&self) -> &[T] {
        &self.values
    }

    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn persisted_entries(&self) -> usize {
        self.persisted_entries
    }
}

pub(super) struct ParallelMapResult<T> {
    values: Vec<T>,
    completed: bool,
    worker_count: usize,
}

impl<T> ParallelMapResult<T> {
    pub(super) fn new(values: Vec<T>, completed: bool, worker_count: usize) -> Self {
        Self {
            values,
            completed,
            worker_count,
        }
    }

    pub(super) fn into_values(self) -> Vec<T> {
        self.values
    }

    pub(super) fn values(&self) -> &[T] {
        &self.values
    }

    pub(super) fn completed(&self) -> bool {
        self.completed
    }

    pub(super) fn worker_count(&self) -> usize {
        self.worker_count
    }
}
