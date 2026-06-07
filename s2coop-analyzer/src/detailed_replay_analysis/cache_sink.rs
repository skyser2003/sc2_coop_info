use crate::cache_overall_stats_generator::CacheReplayEntry;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntrySinkError {
    message: String,
}

impl CacheEntrySinkError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for CacheEntrySinkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CacheEntrySinkError {}

pub trait CacheEntrySink: Send + Sync {
    fn write_entries(&self, entries: &[CacheReplayEntry]) -> Result<usize, CacheEntrySinkError>;

    fn write_checks(&self, checks: &[CacheReplayCheck]) -> Result<usize, CacheEntrySinkError> {
        Ok(checks.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheReplayCheck {
    hash: String,
    file: String,
    modified_seconds: u64,
}

impl CacheReplayCheck {
    pub fn new(hash: impl Into<String>, file: impl Into<String>, modified_seconds: u64) -> Self {
        Self {
            hash: hash.into(),
            file: file.into(),
            modified_seconds,
        }
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn modified_seconds(&self) -> u64 {
        self.modified_seconds
    }
}

pub(super) struct ReplayCacheEntrySinkBuffer {
    sink: Option<Arc<dyn CacheEntrySink>>,
    batch_size: usize,
    pending_entries: std::sync::Mutex<Vec<CacheReplayEntry>>,
    pending_checks: std::sync::Mutex<Vec<CacheReplayCheck>>,
    persisted_entries: AtomicUsize,
}

impl ReplayCacheEntrySinkBuffer {
    pub(super) fn new(sink: Option<Arc<dyn CacheEntrySink>>, batch_size: usize) -> Self {
        Self {
            sink,
            batch_size: batch_size.max(1),
            pending_entries: std::sync::Mutex::new(Vec::new()),
            pending_checks: std::sync::Mutex::new(Vec::new()),
            persisted_entries: AtomicUsize::new(0),
        }
    }

    pub(super) fn add_entry(&self, entry: &CacheReplayEntry) -> Result<(), CacheEntrySinkError> {
        if self.sink.is_none() {
            return Ok(());
        }

        let pending = {
            let mut pending_entries = self
                .pending_entries
                .lock()
                .map_err(|_| CacheEntrySinkError::new("cache entry buffer lock poisoned"))?;
            pending_entries.push(entry.clone());
            if pending_entries.len() < self.batch_size {
                return Ok(());
            }
            std::mem::take(&mut *pending_entries)
        };

        self.write_entries(pending)
    }

    pub(super) fn add_check(&self, check: CacheReplayCheck) -> Result<(), CacheEntrySinkError> {
        if self.sink.is_none() {
            return Ok(());
        }

        let pending = {
            let mut pending_checks = self
                .pending_checks
                .lock()
                .map_err(|_| CacheEntrySinkError::new("cache check buffer lock poisoned"))?;
            pending_checks.push(check);
            if pending_checks.len() < self.batch_size {
                return Ok(());
            }
            std::mem::take(&mut *pending_checks)
        };

        self.write_checks(pending)
    }

    pub(super) fn flush(&self) -> Result<(), CacheEntrySinkError> {
        if self.sink.is_none() {
            return Ok(());
        }

        let pending_entries = {
            let mut pending_entries = self
                .pending_entries
                .lock()
                .map_err(|_| CacheEntrySinkError::new("cache entry buffer lock poisoned"))?;
            std::mem::take(&mut *pending_entries)
        };
        self.write_entries(pending_entries)?;

        let pending_checks = {
            let mut pending_checks = self
                .pending_checks
                .lock()
                .map_err(|_| CacheEntrySinkError::new("cache check buffer lock poisoned"))?;
            std::mem::take(&mut *pending_checks)
        };
        self.write_checks(pending_checks)
    }

    pub(super) fn persisted_entries(&self) -> usize {
        self.persisted_entries.load(AtomicOrdering::Relaxed)
    }

    fn write_entries(&self, entries: Vec<CacheReplayEntry>) -> Result<(), CacheEntrySinkError> {
        if entries.is_empty() {
            return Ok(());
        }
        let Some(sink) = self.sink.as_ref() else {
            return Ok(());
        };
        let changed = sink.write_entries(&entries)?;
        self.persisted_entries
            .fetch_add(changed, AtomicOrdering::Relaxed);
        Ok(())
    }

    fn write_checks(&self, checks: Vec<CacheReplayCheck>) -> Result<(), CacheEntrySinkError> {
        if checks.is_empty() {
            return Ok(());
        }
        let Some(sink) = self.sink.as_ref() else {
            return Ok(());
        };
        let changed = sink.write_checks(&checks)?;
        self.persisted_entries
            .fetch_add(changed, AtomicOrdering::Relaxed);
        Ok(())
    }
}
