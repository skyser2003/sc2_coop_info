mod cache_timing_format;
mod report_timing;

use super::DetailedReplayAnalyzer;
use crate::cache_overall_stats_generator::CanonicalCachePayloadTiming;
use cache_timing_format::GenerateCacheTimingReportFormatter;
pub use report_timing::DetailedReplayReportTiming;
pub(super) use report_timing::{
    ReplayAnalysisNoopTimingCollector, ReplayAnalysisTiming, ReplayAnalysisTimingCollector,
    ReplayReportTimingSpan,
};
use s2protocol_port::{ReplayDetails, ReplayInitData, ReplayParseTiming};
use std::time::Duration;

const TIMINGS_ENV_VAR: &str = "S2COOP_ANALYZER_TIMINGS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnalyzerTimingConfig;

impl AnalyzerTimingConfig {
    pub(super) fn enabled_from_env() -> bool {
        std::env::var_os(TIMINGS_ENV_VAR)
            .and_then(|value| value.into_string().ok())
            .map(|value| Self::enabled_value(value.as_str()))
            .unwrap_or(false)
    }

    pub(super) fn verbose_from_env() -> bool {
        std::env::var_os(TIMINGS_ENV_VAR)
            .and_then(|value| value.into_string().ok())
            .map(|value| Self::verbose_value(value.as_str()))
            .unwrap_or(false)
    }

    pub(super) fn env_var_name() -> &'static str {
        TIMINGS_ENV_VAR
    }

    fn enabled_value(value: &str) -> bool {
        let trimmed = value.trim();
        !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false")
    }

    fn verbose_value(value: &str) -> bool {
        let trimmed = value.trim();
        trimmed.eq_ignore_ascii_case("verbose")
            || trimmed.eq_ignore_ascii_case("trace")
            || trimmed.eq_ignore_ascii_case("per-replay")
            || trimmed.eq_ignore_ascii_case("per_replay")
    }
}
pub struct ReplayTiming;

impl ReplayTiming {
    pub fn realtime_length_from_replay(
        accurate_length: f64,
        details: &ReplayDetails,
        init_data: &ReplayInitData,
    ) -> f64 {
        DetailedReplayAnalyzer::realtime_length_from_replay(accurate_length, details, init_data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ReplayBaseParseTiming {
    pub(super) total: Duration,
    pub(super) early_filter: Duration,
    pub(super) decode_replay: Duration,
    pub(super) decode_replay_detail: ReplayParseTiming,
    pub(super) events_decoded_len: usize,
    pub(super) events_decoded_capacity: usize,
    pub(super) events_retained_len: usize,
    pub(super) events_retained_capacity: usize,
    pub(super) extract_fields: Duration,
    pub(super) validate_filters: Duration,
    pub(super) resolve_build: Duration,
    pub(super) map_lookup: Duration,
    pub(super) lobby_metadata: Duration,
    pub(super) length_events: Duration,
    pub(super) identify_mutators: Duration,
    pub(super) collect_messages: Duration,
    pub(super) hash_file: Duration,
    pub(super) file_date: Duration,
    pub(super) detailed_event_filter: Duration,
    pub(super) build_base: Duration,
}

impl ReplayBaseParseTiming {
    pub(super) fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    pub(super) fn add(&mut self, other: &Self) {
        self.total += other.total;
        self.early_filter += other.early_filter;
        self.decode_replay += other.decode_replay;
        self.decode_replay_detail.add(&other.decode_replay_detail);
        self.events_decoded_len += other.events_decoded_len;
        self.events_decoded_capacity += other.events_decoded_capacity;
        self.events_retained_len += other.events_retained_len;
        self.events_retained_capacity += other.events_retained_capacity;
        self.extract_fields += other.extract_fields;
        self.validate_filters += other.validate_filters;
        self.resolve_build += other.resolve_build;
        self.map_lookup += other.map_lookup;
        self.lobby_metadata += other.lobby_metadata;
        self.length_events += other.length_events;
        self.identify_mutators += other.identify_mutators;
        self.collect_messages += other.collect_messages;
        self.hash_file += other.hash_file;
        self.file_date += other.file_date;
        self.detailed_event_filter += other.detailed_event_filter;
        self.build_base += other.build_base;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ReplayEntryParseTiming {
    pub(super) total: Duration,
    pub(super) base: ReplayBaseParseTiming,
    pub(super) bundle_projection: Duration,
    pub(super) candidate_filter: Duration,
    pub(super) cache_entry_projection: Duration,
}

impl ReplayEntryParseTiming {
    pub(super) fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    pub(super) fn add(&mut self, other: &Self) {
        self.total += other.total;
        self.base.add(&other.base);
        self.bundle_projection += other.bundle_projection;
        self.candidate_filter += other.candidate_filter;
        self.cache_entry_projection += other.cache_entry_projection;
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GenerateCacheTimingReport {
    pub(super) enabled: bool,
    pub(super) worker_count: usize,
    pub(super) total_replay_files: usize,
    pub(super) candidate_count: usize,
    pub(super) reused_candidate_count: usize,
    pub(super) pending_candidate_count: usize,
    pub(super) analyzed_entry_count: usize,
    pub(super) output_directory_setup: Duration,
    pub(super) collect_replay_files: Duration,
    pub(super) resolve_main_handles: Duration,
    pub(super) load_existing_cache: Duration,
    pub(super) build_thread_pool: Duration,
    pub(super) build_canonicalize_thread_pool: Duration,
    pub(super) collect_candidates_parallel: Duration,
    pub(super) collect_candidates_worker: Duration,
    pub(super) collect_candidates_hash_lookup: Duration,
    pub(super) collect_candidates_priority: Duration,
    pub(super) partition_candidates: Duration,
    pub(super) sort_pending_candidates: Duration,
    pub(super) replay_analysis_parallel: Duration,
    pub(super) replay_analysis_worker: Duration,
    pub(super) replay_analysis_parse_detailed: Duration,
    pub(super) replay_analysis_parse_detailed_breakdown: ReplayEntryParseTiming,
    pub(super) replay_analysis_parse_basic_fallback: Duration,
    pub(super) replay_analysis_parse_basic_fallback_breakdown: ReplayEntryParseTiming,
    pub(super) replay_events_decoded_len: usize,
    pub(super) replay_events_decoded_capacity: usize,
    pub(super) replay_events_retained_len: usize,
    pub(super) replay_events_retained_capacity: usize,
    pub(super) replay_analysis_detailed_report: Duration,
    pub(super) replay_analysis_detailed_report_breakdown: DetailedReplayReportTiming,
    pub(super) replay_analysis_report_to_cache_entry: Duration,
    pub(super) replay_analysis_temp_entry_write: Duration,
    pub(super) replay_analysis_temp_persisted_entries: usize,
    pub(super) replay_analysis_temp_persisted_bytes: usize,
    pub(super) replay_analysis_progress_record: Duration,
    pub(super) collect_analyzed_entries: Duration,
    pub(super) merge_entries: Duration,
    pub(super) sort_entries: Duration,
    pub(super) cleanup_temp_file: Duration,
    pub(super) simple_analysis_parallel: Duration,
    pub(super) simple_analysis_worker: Duration,
    pub(super) simple_analysis_parse: Duration,
    pub(super) simple_analysis_parse_breakdown: ReplayEntryParseTiming,
    pub(super) canonicalize_entries: Duration,
    pub(super) canonicalize_worker_count: usize,
    pub(super) canonicalize_entries_parallel: Duration,
    pub(super) canonicalize_entries_worker: Duration,
    pub(super) canonicalize_to_json_value_worker: Duration,
    pub(super) canonicalize_json_value_worker: Duration,
    pub(super) canonicalize_serialize_payload: Duration,
    pub(super) canonicalize_deserialize_payload: Duration,
    pub(super) canonicalize_value_count: usize,
    pub(super) canonicalize_payload_bytes: usize,
    pub(super) write_entries: Duration,
    pub(super) total: Duration,
}

impl GenerateCacheTimingReport {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub(super) fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    pub(super) fn apply_canonical_payload_timing(&mut self, timing: CanonicalCachePayloadTiming) {
        self.canonicalize_worker_count = timing.worker_count();
        self.canonicalize_entries_parallel = timing.canonicalize_entries_parallel();
        self.canonicalize_entries_worker = timing.canonicalize_entries_worker();
        self.canonicalize_to_json_value_worker = timing.to_json_value_worker();
        self.canonicalize_json_value_worker = timing.canonicalize_json_value_worker();
        self.canonicalize_serialize_payload = timing.serialize_payload();
        self.canonicalize_deserialize_payload = timing.deserialize_payload();
        self.canonicalize_value_count = timing.canonical_value_count();
        self.canonicalize_payload_bytes = timing.payload_bytes();
    }

    pub(super) fn add_candidate_collection_timing(
        &mut self,
        timing: &CandidateReplayCollectionTiming,
    ) {
        self.collect_candidates_worker += timing.total();
        self.collect_candidates_hash_lookup += timing.hash_lookup();
        self.collect_candidates_priority += timing.priority();
    }

    pub(super) fn add_replay_analysis_timing(&mut self, timing: &CandidateReplayAnalysisTiming) {
        self.replay_analysis_worker += timing.total();
        self.replay_analysis_parse_detailed += timing.parse_detailed();
        self.replay_analysis_parse_detailed_breakdown
            .add(timing.parse_detailed_breakdown());
        self.replay_events_decoded_len += timing.parse_detailed_breakdown().base.events_decoded_len;
        self.replay_events_decoded_capacity += timing
            .parse_detailed_breakdown()
            .base
            .events_decoded_capacity;
        self.replay_events_retained_len +=
            timing.parse_detailed_breakdown().base.events_retained_len;
        self.replay_events_retained_capacity += timing
            .parse_detailed_breakdown()
            .base
            .events_retained_capacity;
        self.replay_analysis_parse_basic_fallback += timing.parse_basic_fallback();
        self.replay_analysis_parse_basic_fallback_breakdown
            .add(timing.parse_basic_fallback_breakdown());
        self.replay_analysis_detailed_report += timing.detailed_report();
        self.replay_analysis_detailed_report_breakdown
            .add(timing.detailed_report_breakdown());
        self.replay_analysis_report_to_cache_entry += timing.report_to_cache_entry();
        self.replay_analysis_temp_entry_write += timing.temp_entry_write();
        self.replay_analysis_progress_record += timing.progress_record();
    }

    pub(super) fn add_simple_analysis_timing(&mut self, timing: &CandidateReplayAnalysisTiming) {
        self.simple_analysis_worker += timing.total();
        self.simple_analysis_parse += timing.parse_simple();
        self.simple_analysis_parse_breakdown
            .add(timing.parse_simple_breakdown());
    }

    pub(super) fn set_temp_persist_stats(&mut self, entries: usize, bytes: usize) {
        self.replay_analysis_temp_persisted_entries = entries;
        self.replay_analysis_temp_persisted_bytes = bytes;
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn total_replay_files(&self) -> usize {
        self.total_replay_files
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_count
    }

    pub fn reused_candidate_count(&self) -> usize {
        self.reused_candidate_count
    }

    pub fn pending_candidate_count(&self) -> usize {
        self.pending_candidate_count
    }

    pub fn analyzed_entry_count(&self) -> usize {
        self.analyzed_entry_count
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn output_directory_setup(&self) -> Duration {
        self.output_directory_setup
    }

    pub fn collect_replay_files(&self) -> Duration {
        self.collect_replay_files
    }

    pub fn resolve_main_handles(&self) -> Duration {
        self.resolve_main_handles
    }

    pub fn load_existing_cache(&self) -> Duration {
        self.load_existing_cache
    }

    pub fn build_thread_pool(&self) -> Duration {
        self.build_thread_pool
    }

    pub fn build_canonicalize_thread_pool(&self) -> Duration {
        self.build_canonicalize_thread_pool
    }

    pub fn collect_candidates_parallel(&self) -> Duration {
        self.collect_candidates_parallel
    }

    pub fn collect_candidates_worker(&self) -> Duration {
        self.collect_candidates_worker
    }

    pub fn collect_candidates_hash_lookup(&self) -> Duration {
        self.collect_candidates_hash_lookup
    }

    pub fn collect_candidates_priority(&self) -> Duration {
        self.collect_candidates_priority
    }

    pub fn partition_candidates(&self) -> Duration {
        self.partition_candidates
    }

    pub fn sort_pending_candidates(&self) -> Duration {
        self.sort_pending_candidates
    }

    pub fn replay_analysis_parallel(&self) -> Duration {
        self.replay_analysis_parallel
    }

    pub fn replay_analysis_worker(&self) -> Duration {
        self.replay_analysis_worker
    }

    pub fn replay_analysis_parse_detailed(&self) -> Duration {
        self.replay_analysis_parse_detailed
    }

    pub fn replay_analysis_parse_basic_fallback(&self) -> Duration {
        self.replay_analysis_parse_basic_fallback
    }

    pub fn replay_analysis_detailed_report(&self) -> Duration {
        self.replay_analysis_detailed_report
    }

    pub fn replay_analysis_detailed_report_breakdown(&self) -> &DetailedReplayReportTiming {
        &self.replay_analysis_detailed_report_breakdown
    }

    pub fn replay_analysis_report_to_cache_entry(&self) -> Duration {
        self.replay_analysis_report_to_cache_entry
    }

    pub fn replay_events_decoded_len(&self) -> usize {
        self.replay_events_decoded_len
    }

    pub fn replay_events_decoded_capacity(&self) -> usize {
        self.replay_events_decoded_capacity
    }

    pub fn replay_events_retained_len(&self) -> usize {
        self.replay_events_retained_len
    }

    pub fn replay_events_retained_capacity(&self) -> usize {
        self.replay_events_retained_capacity
    }

    pub fn replay_analysis_temp_entry_write(&self) -> Duration {
        self.replay_analysis_temp_entry_write
    }

    pub fn replay_analysis_progress_record(&self) -> Duration {
        self.replay_analysis_progress_record
    }

    pub fn collect_analyzed_entries(&self) -> Duration {
        self.collect_analyzed_entries
    }

    pub fn merge_entries(&self) -> Duration {
        self.merge_entries
    }

    pub fn sort_entries(&self) -> Duration {
        self.sort_entries
    }

    pub fn cleanup_temp_file(&self) -> Duration {
        self.cleanup_temp_file
    }

    pub fn simple_analysis_parallel(&self) -> Duration {
        self.simple_analysis_parallel
    }

    pub fn simple_analysis_worker(&self) -> Duration {
        self.simple_analysis_worker
    }

    pub fn simple_analysis_parse(&self) -> Duration {
        self.simple_analysis_parse
    }

    pub fn canonicalize_entries(&self) -> Duration {
        self.canonicalize_entries
    }

    pub fn canonicalize_worker_count(&self) -> usize {
        self.canonicalize_worker_count
    }

    pub fn canonicalize_entries_parallel(&self) -> Duration {
        self.canonicalize_entries_parallel
    }

    pub fn canonicalize_entries_worker(&self) -> Duration {
        self.canonicalize_entries_worker
    }

    pub fn canonicalize_to_json_value_worker(&self) -> Duration {
        self.canonicalize_to_json_value_worker
    }

    pub fn canonicalize_json_value_worker(&self) -> Duration {
        self.canonicalize_json_value_worker
    }

    pub fn canonicalize_serialize_payload(&self) -> Duration {
        self.canonicalize_serialize_payload
    }

    pub fn canonicalize_deserialize_payload(&self) -> Duration {
        self.canonicalize_deserialize_payload
    }

    pub fn write_entries(&self) -> Duration {
        self.write_entries
    }

    pub fn parallelizable_wall_time(&self) -> Duration {
        self.collect_candidates_parallel
            + self.replay_analysis_parallel
            + self.simple_analysis_parallel
            + self.canonicalize_entries_parallel
    }

    pub fn serial_wall_estimate(&self) -> Duration {
        self.total.saturating_sub(self.parallelizable_wall_time())
    }

    pub fn serial_wall_fraction(&self) -> f64 {
        GenerateCacheTimingReportFormatter::duration_fraction(
            self.serial_wall_estimate(),
            self.total,
        )
    }

    pub fn parallelizable_wall_fraction(&self) -> f64 {
        GenerateCacheTimingReportFormatter::duration_fraction(
            self.parallelizable_wall_time(),
            self.total,
        )
    }

    pub fn amdahl_max_speedup_from_serial_fraction(&self) -> Option<f64> {
        let serial_fraction = self.serial_wall_fraction();
        (serial_fraction > 0.0).then_some(1.0 / serial_fraction)
    }

    pub fn format_amdahl_summary(&self) -> String {
        GenerateCacheTimingReportFormatter::format_amdahl_summary(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CandidateReplayCollectionTiming {
    pub(super) total: Duration,
    pub(super) hash_lookup: Duration,
    pub(super) priority: Duration,
}

impl CandidateReplayCollectionTiming {
    pub(super) fn new(total: Duration, hash_lookup: Duration, priority: Duration) -> Self {
        Self {
            total,
            hash_lookup,
            priority,
        }
    }

    fn total(&self) -> Duration {
        self.total
    }

    fn hash_lookup(&self) -> Duration {
        self.hash_lookup
    }

    fn priority(&self) -> Duration {
        self.priority
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CandidateReplayAnalysisTiming {
    pub(super) total: Duration,
    pub(super) parse_simple: Duration,
    pub(super) parse_simple_breakdown: ReplayEntryParseTiming,
    pub(super) parse_detailed: Duration,
    pub(super) parse_detailed_breakdown: ReplayEntryParseTiming,
    pub(super) parse_basic_fallback: Duration,
    pub(super) parse_basic_fallback_breakdown: ReplayEntryParseTiming,
    pub(super) detailed_report: Duration,
    pub(super) detailed_report_breakdown: DetailedReplayReportTiming,
    pub(super) report_to_cache_entry: Duration,
    pub(super) temp_entry_write: Duration,
    pub(super) progress_record: Duration,
}

impl CandidateReplayAnalysisTiming {
    pub(super) fn finish(mut self, total: Duration) -> Self {
        self.total = total;
        self
    }

    pub(super) fn add_temp_entry_write(&mut self, duration: Duration) {
        self.temp_entry_write += duration;
    }

    pub(super) fn add_progress_record(&mut self, duration: Duration) {
        self.progress_record += duration;
    }

    pub(super) fn add_report_to_cache_entry(&mut self, duration: Duration) {
        self.report_to_cache_entry += duration;
    }

    fn total(&self) -> Duration {
        self.total
    }

    fn parse_simple(&self) -> Duration {
        self.parse_simple
    }

    fn parse_simple_breakdown(&self) -> &ReplayEntryParseTiming {
        &self.parse_simple_breakdown
    }

    fn parse_detailed(&self) -> Duration {
        self.parse_detailed
    }

    fn parse_detailed_breakdown(&self) -> &ReplayEntryParseTiming {
        &self.parse_detailed_breakdown
    }

    fn parse_basic_fallback(&self) -> Duration {
        self.parse_basic_fallback
    }

    fn parse_basic_fallback_breakdown(&self) -> &ReplayEntryParseTiming {
        &self.parse_basic_fallback_breakdown
    }

    fn detailed_report(&self) -> Duration {
        self.detailed_report
    }

    fn detailed_report_breakdown(&self) -> &DetailedReplayReportTiming {
        &self.detailed_report_breakdown
    }

    fn report_to_cache_entry(&self) -> Duration {
        self.report_to_cache_entry
    }

    fn temp_entry_write(&self) -> Duration {
        self.temp_entry_write
    }

    fn progress_record(&self) -> Duration {
        self.progress_record
    }
}
