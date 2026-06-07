use super::cache_runtime::FullAnalysisMode;
use super::cache_sink::{
    CacheEntrySink, CacheEntrySinkError, CacheReplayCheck, ReplayCacheEntrySinkBuffer,
};
use crate::cache_overall_stats_generator::CacheReplayEntry;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
use std::time::{Duration, Instant};

pub(super) struct GenerateCacheProgressReporter<'a> {
    logger: Option<&'a (dyn Fn(String) + Send + Sync + 'a)>,
    analysis_label: &'static str,
    total_files: usize,
    report_interval: usize,
    start_time: Instant,
    processed_files: AtomicUsize,
    next_report_target: AtomicUsize,
    cache_entry_sink_buffer: ReplayCacheEntrySinkBuffer,
}

impl<'a> GenerateCacheProgressReporter<'a> {
    pub(super) fn new(
        mode: FullAnalysisMode,
        total_files: usize,
        initial_processed_files: usize,
        logger: Option<&'a (dyn Fn(String) + Send + Sync + 'a)>,
        cache_entry_sink: Option<Arc<dyn CacheEntrySink>>,
        cache_entry_sink_batch_size: usize,
    ) -> Self {
        let report_interval = if total_files <= 10 { 1 } else { 10 };
        let initial_processed_files = initial_processed_files.min(total_files);
        Self {
            logger,
            analysis_label: mode.progress_label(),
            total_files,
            report_interval,
            start_time: Instant::now(),
            processed_files: AtomicUsize::new(initial_processed_files),
            next_report_target: AtomicUsize::new(Self::next_progress_target(
                total_files,
                report_interval,
                initial_processed_files,
            )),
            cache_entry_sink_buffer: ReplayCacheEntrySinkBuffer::new(
                cache_entry_sink,
                cache_entry_sink_batch_size,
            ),
        }
    }

    pub(super) fn log_start(&self) {
        if self.total_files == 0 {
            self.log_completion();
            return;
        }

        self.emit(format!(
            "Starting {}!",
            self.analysis_label.to_ascii_lowercase()
        ));
        self.emit(self.progress_message(self.processed_files.load(AtomicOrdering::Relaxed)));
    }

    pub(super) fn record_processed_file(&self) {
        if self.total_files == 0 {
            return;
        }

        let processed = self.processed_files.fetch_add(1, AtomicOrdering::Relaxed) + 1;

        if processed == self.total_files {
            if self.logger.is_some() {
                self.emit(self.progress_message(processed));
            }
            if let Err(error) = self.flush_cache_entries() {
                self.emit(format!("Warning: failed to write cache entries: {error}"));
            }
            return;
        }

        if self.logger.is_none() {
            return;
        }

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
    }

    pub(super) fn log_completion(&self) {
        self.emit(format!(
            "{} completed! {}/{} | 100%",
            self.analysis_label, self.total_files, self.total_files
        ));
        self.emit(format!(
            "{} completed in {:.0} seconds!",
            self.analysis_label,
            self.start_time.elapsed().as_secs_f64()
        ));
    }

    pub(super) fn add_cache_entry(
        &self,
        entry: &CacheReplayEntry,
    ) -> Result<(), CacheEntrySinkError> {
        self.cache_entry_sink_buffer.add_entry(entry)
    }

    pub(super) fn add_cache_check(
        &self,
        check: CacheReplayCheck,
    ) -> Result<(), CacheEntrySinkError> {
        self.cache_entry_sink_buffer.add_check(check)
    }

    pub(super) fn flush_cache_entries(&self) -> Result<(), CacheEntrySinkError> {
        self.cache_entry_sink_buffer.flush()
    }

    pub(super) fn cache_persisted_entries(&self) -> usize {
        self.cache_entry_sink_buffer.persisted_entries()
    }

    pub(super) fn emit(&self, message: String) {
        if let Some(logger) = self.logger {
            logger(message);
        }
    }

    fn progress_message(&self, processed: usize) -> String {
        let percent = Self::progress_percent(processed, self.total_files);
        if processed >= self.report_interval && processed < self.total_files {
            format!(
                "Estimated remaining time: {}\nRunning... {processed}/{} ({percent}%)",
                Self::format_eta_duration(self.estimate_remaining(processed)),
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

    fn progress_percent(processed: usize, total: usize) -> usize {
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
}
