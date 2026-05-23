use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::shared_types::ReplayScanProgressPayload;

#[derive(Debug)]
pub struct ReplayScanProgress {
    total: AtomicU64,
    cache_hits: AtomicU64,
    to_parse: AtomicU64,
    newly_parsed: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    parse_skipped: AtomicU64,
    started_at_ms: AtomicU64,
    elapsed_ms: AtomicU64,
    stage: Mutex<String>,
    status: Mutex<String>,
}

impl Default for ReplayScanProgress {
    fn default() -> Self {
        Self {
            stage: Mutex::new("idle".to_string()),
            status: Mutex::new("Idle".to_string()),
            total: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            to_parse: AtomicU64::new(0),
            newly_parsed: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            parse_skipped: AtomicU64::new(0),
            started_at_ms: AtomicU64::new(0),
            elapsed_ms: AtomicU64::new(0),
        }
    }
}

impl ReplayScanProgress {
    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }

    pub fn set_total(&self, value: u64) {
        self.total.store(value, Ordering::Release);
    }

    pub fn set_cache_hits(&self, value: u64) {
        self.cache_hits.store(value, Ordering::Release);
    }

    pub fn set_to_parse(&self, value: u64) {
        self.to_parse.store(value, Ordering::Release);
    }

    pub fn increment_newly_parsed(&self) {
        self.newly_parsed.fetch_add(1, Ordering::AcqRel);
    }

    pub fn increment_completed(&self) {
        self.completed.fetch_add(1, Ordering::AcqRel);
    }

    pub fn set_failed(&self, value: u64) {
        self.failed.store(value, Ordering::Release);
    }

    pub fn increment_failed(&self) {
        self.failed.fetch_add(1, Ordering::AcqRel);
    }

    pub fn set_parse_skipped(&self, value: u64) {
        self.parse_skipped.store(value, Ordering::Release);
    }

    pub fn reset(&self, stage: &str) {
        self.total.store(0, Ordering::Release);
        self.cache_hits.store(0, Ordering::Release);
        self.to_parse.store(0, Ordering::Release);
        self.newly_parsed.store(0, Ordering::Release);
        self.completed.store(0, Ordering::Release);
        self.failed.store(0, Ordering::Release);
        self.parse_skipped.store(0, Ordering::Release);
        self.started_at_ms
            .store(Self::now_millis(), Ordering::Release);
        self.elapsed_ms.store(0, Ordering::Release);
        if let Ok(mut value) = self.stage.lock() {
            *value = stage.to_string();
        }
        if let Ok(mut value) = self.status.lock() {
            *value = "Parsing".to_string();
        }
    }

    pub fn set_stage(&self, stage: &str) {
        if let Ok(mut value) = self.stage.lock() {
            *value = stage.to_string();
        }
    }

    pub fn set_status(&self, status: &str) {
        if let Ok(mut value) = self.status.lock() {
            *value = status.to_string();
        }
        if status == "Completed" {
            let started_at = self.started_at_ms.load(Ordering::Acquire);
            if started_at > 0 {
                let elapsed = Self::now_millis().saturating_sub(started_at);
                self.elapsed_ms.store(elapsed, Ordering::Release);
            }
        }
    }

    pub fn set_counts(&self, total: u64, completed: u64) {
        let bounded_completed = completed.min(total);
        self.total.store(total, Ordering::Release);
        self.completed.store(bounded_completed, Ordering::Release);
        self.to_parse
            .store(total.saturating_sub(bounded_completed), Ordering::Release);
        self.cache_hits.store(0, Ordering::Release);
        self.newly_parsed.store(0, Ordering::Release);
        self.failed.store(0, Ordering::Release);
        self.parse_skipped.store(0, Ordering::Release);
    }

    pub fn as_payload(&self) -> ReplayScanProgressPayload {
        let stage = self
            .stage
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        let status = self
            .status
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "Parsing".to_string());
        let total = self.total.load(Ordering::Acquire);
        let cache_hits = self.cache_hits.load(Ordering::Acquire);
        let to_parse = self.to_parse.load(Ordering::Acquire);
        let newly_parsed = self.newly_parsed.load(Ordering::Acquire);
        let completed = self.completed.load(Ordering::Acquire);
        let failed = self.failed.load(Ordering::Acquire);
        let parse_skipped = self.parse_skipped.load(Ordering::Acquire);
        let started_at = self.started_at_ms.load(Ordering::Acquire);
        let stored_elapsed = self.elapsed_ms.load(Ordering::Acquire);
        let elapsed_ms = if status == "Parsing" && started_at > 0 {
            Self::now_millis().saturating_sub(started_at)
        } else {
            stored_elapsed
        };
        let effective_total = if total > 0 {
            total
        } else {
            cache_hits.saturating_add(to_parse)
        };
        ReplayScanProgressPayload {
            stage,
            status: status.clone(),
            parsing_status: status,
            total: effective_total,
            total_replay_files: effective_total,
            cache_hits,
            files_already_cached: cache_hits,
            to_parse,
            completed,
            newly_parsed,
            newly_parsed_files: newly_parsed,
            failed,
            parse_failed_files: failed,
            parse_skipped,
            parse_skipped_files: parse_skipped,
            elapsed_ms,
            total_time_taken_ms: elapsed_ms,
        }
    }
}
