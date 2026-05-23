use super::{DetailedReplayAnalyzer, ReplayEventKind};
use crate::cache_overall_stats_generator::CanonicalCachePayloadTiming;
use s2protocol_port::{ReplayDetails, ReplayInitData, ReplayParseTiming};
use std::time::{Duration, Instant};

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

const REPLAY_EVENT_KIND_TIMING_COUNT_NAMES: [&str; 13] = [
    "count.game_user_leave",
    "count.game_selection_delta",
    "count.game_trigger_dialog_control",
    "count.game_command",
    "count.game_command_update_target_unit",
    "count.tracker_player_stats",
    "count.tracker_upgrade",
    "count.tracker_unit_born",
    "count.tracker_unit_init",
    "count.tracker_unit_type_change",
    "count.tracker_unit_owner_change",
    "count.tracker_unit_died",
    "count.other",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReplayReportTimingSpan {
    Setup,
    EventGameUserLeave,
    EventDroneCommand,
    EventPlayerStats,
    EventUpgrade,
    EventUnitBornOrInit,
    EventUnitInitArchon,
    EventUnitIdLookup,
    EventUnitTypeChange,
    EventUnitOwnerChange,
    EventUnitDiedKillStats,
    EventUnitDiedDetail,
    EventsTotal,
    PostPlayerOverridesMessages,
    PostPlayerStats,
    PostBonusComp,
    PostCustomKillIcons,
    PostMainUnitsIcons,
    PostAllyUnitsIcons,
    PostKillbotIcons,
    PostAmonUnits,
    PostReportBuild,
}

const REPLAY_REPORT_TIMING_SPAN_COUNT: usize = 22;
const REPLAY_REPORT_TIMING_SPANS: [ReplayReportTimingSpan; REPLAY_REPORT_TIMING_SPAN_COUNT] = [
    ReplayReportTimingSpan::Setup,
    ReplayReportTimingSpan::EventGameUserLeave,
    ReplayReportTimingSpan::EventDroneCommand,
    ReplayReportTimingSpan::EventPlayerStats,
    ReplayReportTimingSpan::EventUpgrade,
    ReplayReportTimingSpan::EventUnitBornOrInit,
    ReplayReportTimingSpan::EventUnitInitArchon,
    ReplayReportTimingSpan::EventUnitIdLookup,
    ReplayReportTimingSpan::EventUnitTypeChange,
    ReplayReportTimingSpan::EventUnitOwnerChange,
    ReplayReportTimingSpan::EventUnitDiedKillStats,
    ReplayReportTimingSpan::EventUnitDiedDetail,
    ReplayReportTimingSpan::EventsTotal,
    ReplayReportTimingSpan::PostPlayerOverridesMessages,
    ReplayReportTimingSpan::PostPlayerStats,
    ReplayReportTimingSpan::PostBonusComp,
    ReplayReportTimingSpan::PostCustomKillIcons,
    ReplayReportTimingSpan::PostMainUnitsIcons,
    ReplayReportTimingSpan::PostAllyUnitsIcons,
    ReplayReportTimingSpan::PostKillbotIcons,
    ReplayReportTimingSpan::PostAmonUnits,
    ReplayReportTimingSpan::PostReportBuild,
];

impl ReplayReportTimingSpan {
    fn index(self) -> usize {
        match self {
            Self::Setup => 0,
            Self::EventGameUserLeave => 1,
            Self::EventDroneCommand => 2,
            Self::EventPlayerStats => 3,
            Self::EventUpgrade => 4,
            Self::EventUnitBornOrInit => 5,
            Self::EventUnitInitArchon => 6,
            Self::EventUnitIdLookup => 7,
            Self::EventUnitTypeChange => 8,
            Self::EventUnitOwnerChange => 9,
            Self::EventUnitDiedKillStats => 10,
            Self::EventUnitDiedDetail => 11,
            Self::EventsTotal => 12,
            Self::PostPlayerOverridesMessages => 13,
            Self::PostPlayerStats => 14,
            Self::PostBonusComp => 15,
            Self::PostCustomKillIcons => 16,
            Self::PostMainUnitsIcons => 17,
            Self::PostAllyUnitsIcons => 18,
            Self::PostKillbotIcons => 19,
            Self::PostAmonUnits => 20,
            Self::PostReportBuild => 21,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::EventGameUserLeave => "event.game_user_leave",
            Self::EventDroneCommand => "event.drone_command",
            Self::EventPlayerStats => "event.player_stats",
            Self::EventUpgrade => "event.upgrade",
            Self::EventUnitBornOrInit => "event.unit_born_or_init",
            Self::EventUnitInitArchon => "event.unit_init_archon",
            Self::EventUnitIdLookup => "event.unit_id_lookup",
            Self::EventUnitTypeChange => "event.unit_type_change",
            Self::EventUnitOwnerChange => "event.unit_owner_change",
            Self::EventUnitDiedKillStats => "event.unit_died_kill_stats",
            Self::EventUnitDiedDetail => "event.unit_died_detail",
            Self::EventsTotal => "events.total",
            Self::PostPlayerOverridesMessages => "post.player_overrides_messages",
            Self::PostPlayerStats => "post.player_stats",
            Self::PostBonusComp => "post.bonus_comp",
            Self::PostCustomKillIcons => "post.custom_kill_icons",
            Self::PostMainUnitsIcons => "post.main_units_icons",
            Self::PostAllyUnitsIcons => "post.ally_units_icons",
            Self::PostKillbotIcons => "post.killbot_icons",
            Self::PostAmonUnits => "post.amon_units",
            Self::PostReportBuild => "post.report_build",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailedReplayReportTiming {
    total: Duration,
    spans: [Duration; REPLAY_REPORT_TIMING_SPAN_COUNT],
    event_counts: [usize; 13],
    events_input_count: usize,
}

impl Default for DetailedReplayReportTiming {
    fn default() -> Self {
        Self {
            total: Duration::ZERO,
            spans: [Duration::ZERO; REPLAY_REPORT_TIMING_SPAN_COUNT],
            event_counts: [0; 13],
            events_input_count: 0,
        }
    }
}

impl DetailedReplayReportTiming {
    fn new(
        total: Duration,
        spans: [Duration; REPLAY_REPORT_TIMING_SPAN_COUNT],
        event_counts: [usize; 13],
        events_input_count: usize,
    ) -> Self {
        Self {
            total,
            spans,
            event_counts,
            events_input_count,
        }
    }

    pub(super) fn add(&mut self, other: &Self) {
        self.total += other.total;
        for (target, incoming) in self.spans.iter_mut().zip(other.spans.iter()) {
            *target += *incoming;
        }
        for (target, incoming) in self.event_counts.iter_mut().zip(other.event_counts.iter()) {
            *target += *incoming;
        }
        self.events_input_count += other.events_input_count;
    }

    fn span(&self, span: ReplayReportTimingSpan) -> Duration {
        self.spans[span.index()]
    }

    fn event_count(&self, event_kind: ReplayEventKind) -> usize {
        self.event_counts[event_kind.timing_count_index()]
    }

    fn post_processing_total(&self) -> Duration {
        [
            ReplayReportTimingSpan::PostPlayerOverridesMessages,
            ReplayReportTimingSpan::PostPlayerStats,
            ReplayReportTimingSpan::PostBonusComp,
            ReplayReportTimingSpan::PostCustomKillIcons,
            ReplayReportTimingSpan::PostMainUnitsIcons,
            ReplayReportTimingSpan::PostAllyUnitsIcons,
            ReplayReportTimingSpan::PostKillbotIcons,
            ReplayReportTimingSpan::PostAmonUnits,
            ReplayReportTimingSpan::PostReportBuild,
        ]
        .iter()
        .fold(Duration::ZERO, |total, span| total + self.span(*span))
    }

    pub fn has_timings(&self) -> bool {
        self.total > Duration::ZERO || self.events_input_count > 0
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn setup(&self) -> Duration {
        self.span(ReplayReportTimingSpan::Setup)
    }

    pub fn events_total(&self) -> Duration {
        self.span(ReplayReportTimingSpan::EventsTotal)
    }

    pub fn player_stats(&self) -> Duration {
        self.span(ReplayReportTimingSpan::EventPlayerStats)
    }

    pub fn unit_born_or_init(&self) -> Duration {
        self.span(ReplayReportTimingSpan::EventUnitBornOrInit)
    }

    pub fn unit_type_change(&self) -> Duration {
        self.span(ReplayReportTimingSpan::EventUnitTypeChange)
    }

    pub fn unit_died_detail(&self) -> Duration {
        self.span(ReplayReportTimingSpan::EventUnitDiedDetail)
    }

    pub fn unit_id_lookup(&self) -> Duration {
        self.span(ReplayReportTimingSpan::EventUnitIdLookup)
    }

    pub fn report_build(&self) -> Duration {
        self.span(ReplayReportTimingSpan::PostReportBuild)
    }

    pub fn events_input_count(&self) -> usize {
        self.events_input_count
    }
}

#[derive(Debug)]
pub(super) struct ReplayAnalysisTimingCollector {
    label: String,
    started: Instant,
    spans: [Duration; REPLAY_REPORT_TIMING_SPAN_COUNT],
    event_counts: [usize; 13],
    events_input_count: usize,
}

#[derive(Debug, Default)]
pub(super) struct ReplayAnalysisNoopTimingCollector;

pub(super) trait ReplayAnalysisTiming {
    type SpanStart;

    fn new(label: &str) -> Self;
    fn start(&self) -> Self::SpanStart;
    fn finish(&mut self, span: ReplayReportTimingSpan, started: Self::SpanStart);
    fn increment_event_kind(&mut self, event_kind: ReplayEventKind);
    fn add_events_input_count(&mut self, value: usize);
    fn breakdown(&self) -> DetailedReplayReportTiming;
    fn print(&self);
}

impl ReplayAnalysisTimingCollector {
    fn verbose_print_enabled_from_env() -> bool {
        AnalyzerTimingConfig::verbose_from_env()
    }
}

impl ReplayAnalysisTiming for ReplayAnalysisTimingCollector {
    type SpanStart = Instant;

    fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            started: Instant::now(),
            spans: [Duration::ZERO; REPLAY_REPORT_TIMING_SPAN_COUNT],
            event_counts: [0; 13],
            events_input_count: 0,
        }
    }

    #[inline(always)]
    fn start(&self) -> Self::SpanStart {
        Instant::now()
    }

    #[inline(always)]
    fn finish(&mut self, span: ReplayReportTimingSpan, started: Self::SpanStart) {
        let elapsed = started.elapsed();
        self.spans[span.index()] += elapsed;
    }

    #[inline(always)]
    fn increment_event_kind(&mut self, event_kind: ReplayEventKind) {
        self.event_counts[event_kind.timing_count_index()] += 1;
    }

    #[inline(always)]
    fn add_events_input_count(&mut self, value: usize) {
        self.events_input_count += value;
    }

    fn breakdown(&self) -> DetailedReplayReportTiming {
        DetailedReplayReportTiming::new(
            self.started.elapsed(),
            self.spans,
            self.event_counts,
            self.events_input_count,
        )
    }

    fn print(&self) {
        if !Self::verbose_print_enabled_from_env() {
            return;
        }

        eprintln!(
            "[s2coop timing] analyze_replay_file_impl label=\"{}\" total={:.3}ms",
            self.label,
            self.started.elapsed().as_secs_f64() * 1000.0
        );
        for span in REPLAY_REPORT_TIMING_SPANS {
            let duration = self.spans[span.index()];
            if duration > Duration::ZERO {
                eprintln!(
                    "[s2coop timing] span.{}={:.3}ms",
                    span.label(),
                    duration.as_secs_f64() * 1000.0
                );
            }
        }
        eprintln!(
            "[s2coop timing] count.events_input={}",
            self.events_input_count
        );
        for (index, count) in self.event_counts.iter().enumerate() {
            if *count > 0 {
                let name = REPLAY_EVENT_KIND_TIMING_COUNT_NAMES[index];
                eprintln!("[s2coop timing] {name}={count}");
            }
        }
    }
}

impl ReplayAnalysisTiming for ReplayAnalysisNoopTimingCollector {
    type SpanStart = ();

    #[inline(always)]
    fn new(_label: &str) -> Self {
        Self
    }

    #[inline(always)]
    fn start(&self) -> Self::SpanStart {}

    #[inline(always)]
    fn finish(&mut self, _span: ReplayReportTimingSpan, _started: Self::SpanStart) {}

    #[inline(always)]
    fn increment_event_kind(&mut self, _event_kind: ReplayEventKind) {}

    #[inline(always)]
    fn add_events_input_count(&mut self, _value: usize) {}

    #[inline(always)]
    fn breakdown(&self) -> DetailedReplayReportTiming {
        DetailedReplayReportTiming::default()
    }

    #[inline(always)]
    fn print(&self) {}
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
        Self::duration_fraction(self.serial_wall_estimate(), self.total)
    }

    pub fn parallelizable_wall_fraction(&self) -> f64 {
        Self::duration_fraction(self.parallelizable_wall_time(), self.total)
    }

    pub fn amdahl_max_speedup_from_serial_fraction(&self) -> Option<f64> {
        let serial_fraction = self.serial_wall_fraction();
        (serial_fraction > 0.0).then_some(1.0 / serial_fraction)
    }

    pub fn format_amdahl_summary(&self) -> String {
        if !self.enabled {
            return format!(
                "Amdahl timings disabled; set {}=1 to enable.",
                AnalyzerTimingConfig::env_var_name()
            );
        }

        let max_speedup = self
            .amdahl_max_speedup_from_serial_fraction()
            .map(|value| format!("{value:.2}x"))
            .unwrap_or_else(|| "unbounded".to_string());
        let merge_and_sort = self.merge_entries + self.sort_entries;
        let candidate_other = Self::saturating_duration_sub_all(
            self.collect_candidates_worker,
            &[
                self.collect_candidates_hash_lookup,
                self.collect_candidates_priority,
            ],
        );
        let replay_other = Self::saturating_duration_sub_all(
            self.replay_analysis_worker,
            &[
                self.replay_analysis_parse_detailed,
                self.replay_analysis_parse_basic_fallback,
                self.replay_analysis_detailed_report,
                self.replay_analysis_temp_entry_write,
                self.replay_analysis_progress_record,
            ],
        );

        let mut output = format!(
            concat!(
                "Amdahl timings:\n",
                "  total={:.3}s workers={} files={} candidates={} pending={} reused={} analyzed={}\n",
                "  serial_wall_estimate={:.3}s ({:.1}%) parallelizable_wall={:.3}s ({:.1}%) max_speedup_from_this_serial_fraction={}\n",
                "  phases: collect_files={:.3}s resolve_handles={:.3}s load_cache={:.3}s build_pool={:.3}s build_canonical_pool={:.3}s collect_candidates_parallel={:.3}s replay_analysis_parallel={:.3}s collect_results={:.3}s merge_sort={:.3}s canonicalize_total={:.3}s write_file={:.3}s"
            ),
            Self::duration_seconds(self.total),
            self.worker_count,
            self.total_replay_files,
            self.candidate_count,
            self.pending_candidate_count,
            self.reused_candidate_count,
            self.analyzed_entry_count,
            Self::duration_seconds(self.serial_wall_estimate()),
            self.serial_wall_fraction() * 100.0,
            Self::duration_seconds(self.parallelizable_wall_time()),
            self.parallelizable_wall_fraction() * 100.0,
            max_speedup,
            Self::duration_seconds(self.collect_replay_files),
            Self::duration_seconds(self.resolve_main_handles),
            Self::duration_seconds(self.load_existing_cache),
            Self::duration_seconds(self.build_thread_pool),
            Self::duration_seconds(self.build_canonicalize_thread_pool),
            Self::duration_seconds(self.collect_candidates_parallel),
            Self::duration_seconds(self.replay_analysis_parallel),
            Self::duration_seconds(self.collect_analyzed_entries),
            Self::duration_seconds(merge_and_sort),
            Self::duration_seconds(self.canonicalize_entries),
            Self::duration_seconds(self.write_entries),
        );

        output.push_str(&format!(
            concat!(
                "\n  parallel core use: ",
                "collect_candidates worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "replay_analysis worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "simple_analysis worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "canonicalize_json workers={} worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%"
            ),
            Self::duration_seconds(self.collect_candidates_worker),
            Self::effective_cores(
                self.collect_candidates_worker,
                self.collect_candidates_parallel
            ),
            Self::core_efficiency_percent(
                self.collect_candidates_worker,
                self.collect_candidates_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_worker),
            Self::effective_cores(self.replay_analysis_worker, self.replay_analysis_parallel),
            Self::core_efficiency_percent(
                self.replay_analysis_worker,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.simple_analysis_worker),
            Self::effective_cores(self.simple_analysis_worker, self.simple_analysis_parallel),
            Self::core_efficiency_percent(
                self.simple_analysis_worker,
                self.simple_analysis_parallel,
                self.worker_count
            ),
            self.canonicalize_worker_count,
            Self::duration_seconds(self.canonicalize_entries_worker),
            Self::effective_cores(
                self.canonicalize_entries_worker,
                self.canonicalize_entries_parallel
            ),
            Self::core_efficiency_percent(
                self.canonicalize_entries_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
        ));

        output.push_str(&format!(
            concat!(
                "\n  candidate parts: ",
                "hash_lookup={:.3}s capacity_eff={:.1}% ",
                "priority={:.3}s capacity_eff={:.1}% ",
                "other={:.3}s capacity_eff={:.1}%"
            ),
            Self::duration_seconds(self.collect_candidates_hash_lookup),
            Self::core_efficiency_percent(
                self.collect_candidates_hash_lookup,
                self.collect_candidates_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.collect_candidates_priority),
            Self::core_efficiency_percent(
                self.collect_candidates_priority,
                self.collect_candidates_parallel,
                self.worker_count
            ),
            Self::duration_seconds(candidate_other),
            Self::core_efficiency_percent(
                candidate_other,
                self.collect_candidates_parallel,
                self.worker_count
            ),
        ));

        output.push_str(&format!(
            concat!(
                "\n  replay parts: ",
                "parse_detailed={:.3}s capacity_eff={:.1}% ",
                "detailed_report={:.3}s capacity_eff={:.1}% ",
                "parse_basic_fallback={:.3}s capacity_eff={:.1}% ",
                "temp_entry_write={:.3}s capacity_eff={:.1}% ",
                "progress_record={:.3}s capacity_eff={:.1}% ",
                "other={:.3}s capacity_eff={:.1}%"
            ),
            Self::duration_seconds(self.replay_analysis_parse_detailed),
            Self::core_efficiency_percent(
                self.replay_analysis_parse_detailed,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_detailed_report),
            Self::core_efficiency_percent(
                self.replay_analysis_detailed_report,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_parse_basic_fallback),
            Self::core_efficiency_percent(
                self.replay_analysis_parse_basic_fallback,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_temp_entry_write),
            Self::core_efficiency_percent(
                self.replay_analysis_temp_entry_write,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(self.replay_analysis_progress_record),
            Self::core_efficiency_percent(
                self.replay_analysis_progress_record,
                self.replay_analysis_parallel,
                self.worker_count
            ),
            Self::duration_seconds(replay_other),
            Self::core_efficiency_percent(
                replay_other,
                self.replay_analysis_parallel,
                self.worker_count
            ),
        ));

        output.push_str(&format!(
            concat!(
                "\n  allocation counters: ",
                "events_decoded_len={} events_decoded_capacity={} ",
                "events_retained_len={} events_retained_capacity={} ",
                "temp_persisted_entries={} temp_persisted_bytes={} ",
                "canonical_values={} canonical_payload_bytes={}"
            ),
            self.replay_events_decoded_len,
            self.replay_events_decoded_capacity,
            self.replay_events_retained_len,
            self.replay_events_retained_capacity,
            self.replay_analysis_temp_persisted_entries,
            self.replay_analysis_temp_persisted_bytes,
            self.canonicalize_value_count,
            self.canonicalize_payload_bytes,
        ));

        output.push_str(&format!(
            concat!(
                "\n  hotspot hints: ",
                "mpq_read_file={:.3}s ordered_decode={:.3}s ",
                "detailed_event_loop={:.3}s detailed_unit_born_or_init={:.3}s ",
                "detailed_unit_died_detail={:.3}s report_to_cache_entry={:.3}s hash_file={:.3}s ",
                "retained_event_capacity_eff={:.1}%"
            ),
            Self::duration_seconds(
                self.replay_analysis_parse_detailed_breakdown
                    .base
                    .decode_replay_detail
                    .mpq_read_file()
            ),
            Self::duration_seconds(
                self.replay_analysis_parse_detailed_breakdown
                    .base
                    .decode_replay_detail
                    .decode_ordered_events()
            ),
            Self::duration_seconds(
                self.replay_analysis_detailed_report_breakdown
                    .events_total()
            ),
            Self::duration_seconds(
                self.replay_analysis_detailed_report_breakdown
                    .unit_born_or_init()
            ),
            Self::duration_seconds(
                self.replay_analysis_detailed_report_breakdown
                    .unit_died_detail()
            ),
            Self::duration_seconds(self.replay_analysis_report_to_cache_entry),
            Self::duration_seconds(self.replay_analysis_parse_detailed_breakdown.base.hash_file),
            Self::usize_percent(
                self.replay_events_retained_len,
                self.replay_events_retained_capacity
            ),
        ));

        output.push('\n');
        output.push_str(&Self::format_parse_timing_breakdown(
            "parse_detailed parts",
            &self.replay_analysis_parse_detailed_breakdown,
            self.replay_analysis_parallel,
            self.worker_count,
        ));

        if self.replay_analysis_detailed_report_breakdown.has_timings() {
            output.push('\n');
            output.push_str(&Self::format_detailed_report_timing_breakdown(
                "detailed_report parts",
                &self.replay_analysis_detailed_report_breakdown,
                self.replay_analysis_parallel,
                self.worker_count,
            ));
            output.push_str(&format!(
                "\n  detailed_report conversion: report_to_cache_entry={:.3}s capacity_eff={:.1}%",
                Self::duration_seconds(self.replay_analysis_report_to_cache_entry),
                Self::core_efficiency_percent(
                    self.replay_analysis_report_to_cache_entry,
                    self.replay_analysis_parallel,
                    self.worker_count
                ),
            ));
        }

        output.push('\n');
        output.push_str(&Self::format_parse_timing_breakdown(
            "parse_basic_fallback parts",
            &self.replay_analysis_parse_basic_fallback_breakdown,
            self.replay_analysis_parallel,
            self.worker_count,
        ));

        if self.simple_analysis_worker > Duration::ZERO {
            output.push('\n');
            output.push_str(&Self::format_parse_timing_breakdown(
                "simple_parse parts",
                &self.simple_analysis_parse_breakdown,
                self.simple_analysis_parallel,
                self.worker_count,
            ));
        }

        output.push_str(&format!(
            concat!(
                "\n  canonicalize parts: ",
                "json_parallel_wall={:.3}s json_worker={:.3}s capacity_eff={:.1}% ",
                "to_json={:.3}s capacity_eff={:.1}% ",
                "canonicalize_value={:.3}s capacity_eff={:.1}% ",
                "serialize_payload={:.3}s deserialize_payload={:.3}s"
            ),
            Self::duration_seconds(self.canonicalize_entries_parallel),
            Self::duration_seconds(self.canonicalize_entries_worker),
            Self::core_efficiency_percent(
                self.canonicalize_entries_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
            Self::duration_seconds(self.canonicalize_to_json_value_worker),
            Self::core_efficiency_percent(
                self.canonicalize_to_json_value_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
            Self::duration_seconds(self.canonicalize_json_value_worker),
            Self::core_efficiency_percent(
                self.canonicalize_json_value_worker,
                self.canonicalize_entries_parallel,
                self.canonicalize_worker_count
            ),
            Self::duration_seconds(self.canonicalize_serialize_payload),
            Self::duration_seconds(self.canonicalize_deserialize_payload),
        ));

        output
    }

    fn format_detailed_report_timing_breakdown(
        label: &str,
        timing: &DetailedReplayReportTiming,
        wall_time: Duration,
        worker_count: usize,
    ) -> String {
        format!(
            concat!(
                "  {}: total={:.3}s capacity_eff={:.1}% ",
                "setup={:.3}s capacity_eff={:.1}% ",
                "events_total={:.3}s capacity_eff={:.1}% ",
                "post_total={:.3}s capacity_eff={:.1}% ",
                "report_build={:.3}s capacity_eff={:.1}%\n",
                "  detailed_report events: ",
                "input={} game_command={} game_command_update_target_unit={} ",
                "player_stats={:.3}s count={} ",
                "upgrade={:.3}s count={} ",
                "unit_born_or_init={:.3}s count={} ",
                "unit_type_change={:.3}s count={} ",
                "unit_owner_change={:.3}s count={} ",
                "unit_died_kill_stats={:.3}s ",
                "unit_died_detail={:.3}s count={} ",
                "unit_id_lookup={:.3}s"
            ),
            label,
            Self::duration_seconds(timing.total()),
            Self::core_efficiency_percent(timing.total(), wall_time, worker_count),
            Self::duration_seconds(timing.setup()),
            Self::core_efficiency_percent(timing.setup(), wall_time, worker_count),
            Self::duration_seconds(timing.events_total()),
            Self::core_efficiency_percent(timing.events_total(), wall_time, worker_count),
            Self::duration_seconds(timing.post_processing_total()),
            Self::core_efficiency_percent(timing.post_processing_total(), wall_time, worker_count),
            Self::duration_seconds(timing.report_build()),
            Self::core_efficiency_percent(timing.report_build(), wall_time, worker_count),
            timing.events_input_count(),
            timing.event_count(ReplayEventKind::GameCommand),
            timing.event_count(ReplayEventKind::GameCommandUpdateTargetUnit),
            Self::duration_seconds(timing.player_stats()),
            timing.event_count(ReplayEventKind::TrackerPlayerStats),
            Self::duration_seconds(timing.span(ReplayReportTimingSpan::EventUpgrade)),
            timing.event_count(ReplayEventKind::TrackerUpgrade),
            Self::duration_seconds(timing.unit_born_or_init()),
            timing.event_count(ReplayEventKind::TrackerUnitBorn)
                + timing.event_count(ReplayEventKind::TrackerUnitInit),
            Self::duration_seconds(timing.unit_type_change()),
            timing.event_count(ReplayEventKind::TrackerUnitTypeChange),
            Self::duration_seconds(timing.span(ReplayReportTimingSpan::EventUnitOwnerChange)),
            timing.event_count(ReplayEventKind::TrackerUnitOwnerChange),
            Self::duration_seconds(timing.span(ReplayReportTimingSpan::EventUnitDiedKillStats)),
            Self::duration_seconds(timing.unit_died_detail()),
            timing.event_count(ReplayEventKind::TrackerUnitDied),
            Self::duration_seconds(timing.unit_id_lookup()),
        )
    }

    fn format_parse_timing_breakdown(
        label: &str,
        timing: &ReplayEntryParseTiming,
        wall_time: Duration,
        worker_count: usize,
    ) -> String {
        let mut output = format!(
            concat!(
                "  {}: total={:.3}s ",
                "decode_replay={:.3}s capacity_eff={:.1}% ",
                "extract_fields={:.3}s capacity_eff={:.1}% ",
                "validate_filters={:.3}s capacity_eff={:.1}% ",
                "resolve_build={:.3}s capacity_eff={:.1}% ",
                "map_lookup={:.3}s capacity_eff={:.1}% ",
                "lobby_metadata={:.3}s capacity_eff={:.1}% ",
                "length_events={:.3}s capacity_eff={:.1}% ",
                "identify_mutators={:.3}s capacity_eff={:.1}% ",
                "messages={:.3}s capacity_eff={:.1}% ",
                "hash_file={:.3}s capacity_eff={:.1}% ",
                "file_date={:.3}s capacity_eff={:.1}% ",
                "event_filter={:.3}s capacity_eff={:.1}% ",
                "bundle_projection={:.3}s capacity_eff={:.1}% ",
                "candidate_filter={:.3}s capacity_eff={:.1}% ",
                "cache_entry_projection={:.3}s capacity_eff={:.1}%"
            ),
            label,
            Self::duration_seconds(timing.total),
            Self::duration_seconds(timing.base.decode_replay),
            Self::core_efficiency_percent(timing.base.decode_replay, wall_time, worker_count),
            Self::duration_seconds(timing.base.extract_fields),
            Self::core_efficiency_percent(timing.base.extract_fields, wall_time, worker_count),
            Self::duration_seconds(timing.base.validate_filters),
            Self::core_efficiency_percent(timing.base.validate_filters, wall_time, worker_count),
            Self::duration_seconds(timing.base.resolve_build),
            Self::core_efficiency_percent(timing.base.resolve_build, wall_time, worker_count),
            Self::duration_seconds(timing.base.map_lookup),
            Self::core_efficiency_percent(timing.base.map_lookup, wall_time, worker_count),
            Self::duration_seconds(timing.base.lobby_metadata),
            Self::core_efficiency_percent(timing.base.lobby_metadata, wall_time, worker_count),
            Self::duration_seconds(timing.base.length_events),
            Self::core_efficiency_percent(timing.base.length_events, wall_time, worker_count),
            Self::duration_seconds(timing.base.identify_mutators),
            Self::core_efficiency_percent(timing.base.identify_mutators, wall_time, worker_count),
            Self::duration_seconds(timing.base.collect_messages),
            Self::core_efficiency_percent(timing.base.collect_messages, wall_time, worker_count),
            Self::duration_seconds(timing.base.hash_file),
            Self::core_efficiency_percent(timing.base.hash_file, wall_time, worker_count),
            Self::duration_seconds(timing.base.file_date),
            Self::core_efficiency_percent(timing.base.file_date, wall_time, worker_count),
            Self::duration_seconds(timing.base.detailed_event_filter),
            Self::core_efficiency_percent(
                timing.base.detailed_event_filter,
                wall_time,
                worker_count,
            ),
            Self::duration_seconds(timing.bundle_projection),
            Self::core_efficiency_percent(timing.bundle_projection, wall_time, worker_count),
            Self::duration_seconds(timing.candidate_filter),
            Self::core_efficiency_percent(timing.candidate_filter, wall_time, worker_count),
            Self::duration_seconds(timing.cache_entry_projection),
            Self::core_efficiency_percent(timing.cache_entry_projection, wall_time, worker_count),
        );
        output.push('\n');
        output.push_str(&Self::format_decode_timing_breakdown(
            label,
            &timing.base.decode_replay_detail,
            wall_time,
            worker_count,
        ));
        output
    }

    fn format_decode_timing_breakdown(
        label: &str,
        timing: &ReplayParseTiming,
        wall_time: Duration,
        worker_count: usize,
    ) -> String {
        format!(
            concat!(
                "  {} decode: total={:.3}s mpq_bytes={:.1}MB ",
                "header_read={:.3}s capacity_eff={:.1}% ",
                "header_decode={:.3}s capacity_eff={:.1}% ",
                "protocol={:.3}s capacity_eff={:.1}% ",
                "archive_open={:.3}s capacity_eff={:.1}% ",
                "mpq_open_file={:.3}s capacity_eff={:.1}% ",
                "mpq_read_file={:.3}s capacity_eff={:.1}% ",
                "read_game={:.3}s capacity_eff={:.1}% ",
                "read_tracker={:.3}s capacity_eff={:.1}% ",
                "decode_ordered={:.3}s capacity_eff={:.1}% ",
                "read_details={:.3}s capacity_eff={:.1}% ",
                "decode_details={:.3}s capacity_eff={:.1}% ",
                "read_details_backup={:.3}s capacity_eff={:.1}% ",
                "decode_details_backup={:.3}s capacity_eff={:.1}% ",
                "read_init={:.3}s capacity_eff={:.1}% ",
                "decode_init={:.3}s capacity_eff={:.1}% ",
                "init_fallback={:.3}s capacity_eff={:.1}% ",
                "read_messages={:.3}s capacity_eff={:.1}% ",
                "decode_messages={:.3}s capacity_eff={:.1}% ",
                "read_metadata={:.3}s capacity_eff={:.1}% ",
                "decode_metadata_json={:.3}s capacity_eff={:.1}% ",
                "parse_metadata={:.3}s capacity_eff={:.1}% ",
                "read_attributes={:.3}s capacity_eff={:.1}% ",
                "decode_attributes={:.3}s capacity_eff={:.1}% ",
                "parse_attributes={:.3}s capacity_eff={:.1}%"
            ),
            label,
            Self::duration_seconds(timing.total()),
            timing.mpq_bytes_read() as f64 / (1024.0 * 1024.0),
            Self::duration_seconds(timing.read_header()),
            Self::core_efficiency_percent(timing.read_header(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_header()),
            Self::core_efficiency_percent(timing.decode_header(), wall_time, worker_count),
            Self::duration_seconds(timing.resolve_protocol()),
            Self::core_efficiency_percent(timing.resolve_protocol(), wall_time, worker_count),
            Self::duration_seconds(timing.open_archive()),
            Self::core_efficiency_percent(timing.open_archive(), wall_time, worker_count),
            Self::duration_seconds(timing.mpq_open_file()),
            Self::core_efficiency_percent(timing.mpq_open_file(), wall_time, worker_count),
            Self::duration_seconds(timing.mpq_read_file()),
            Self::core_efficiency_percent(timing.mpq_read_file(), wall_time, worker_count),
            Self::duration_seconds(timing.read_game_events()),
            Self::core_efficiency_percent(timing.read_game_events(), wall_time, worker_count),
            Self::duration_seconds(timing.read_tracker_events()),
            Self::core_efficiency_percent(timing.read_tracker_events(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_ordered_events()),
            Self::core_efficiency_percent(timing.decode_ordered_events(), wall_time, worker_count),
            Self::duration_seconds(timing.read_details()),
            Self::core_efficiency_percent(timing.read_details(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_details()),
            Self::core_efficiency_percent(timing.decode_details(), wall_time, worker_count),
            Self::duration_seconds(timing.read_details_backup()),
            Self::core_efficiency_percent(timing.read_details_backup(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_details_backup()),
            Self::core_efficiency_percent(timing.decode_details_backup(), wall_time, worker_count),
            Self::duration_seconds(timing.read_init_data()),
            Self::core_efficiency_percent(timing.read_init_data(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_init_data()),
            Self::core_efficiency_percent(timing.decode_init_data(), wall_time, worker_count),
            Self::duration_seconds(timing.init_data_fallback()),
            Self::core_efficiency_percent(timing.init_data_fallback(), wall_time, worker_count),
            Self::duration_seconds(timing.read_message_events()),
            Self::core_efficiency_percent(timing.read_message_events(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_message_events()),
            Self::core_efficiency_percent(timing.decode_message_events(), wall_time, worker_count),
            Self::duration_seconds(timing.read_metadata()),
            Self::core_efficiency_percent(timing.read_metadata(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_metadata_json()),
            Self::core_efficiency_percent(timing.decode_metadata_json(), wall_time, worker_count),
            Self::duration_seconds(timing.parse_metadata()),
            Self::core_efficiency_percent(timing.parse_metadata(), wall_time, worker_count),
            Self::duration_seconds(timing.read_attributes()),
            Self::core_efficiency_percent(timing.read_attributes(), wall_time, worker_count),
            Self::duration_seconds(timing.decode_attributes()),
            Self::core_efficiency_percent(timing.decode_attributes(), wall_time, worker_count),
            Self::duration_seconds(timing.parse_attributes()),
            Self::core_efficiency_percent(timing.parse_attributes(), wall_time, worker_count),
        )
    }

    fn duration_fraction(part: Duration, total: Duration) -> f64 {
        let total_seconds = total.as_secs_f64();
        if total_seconds <= 0.0 {
            0.0
        } else {
            part.as_secs_f64() / total_seconds
        }
    }

    fn duration_seconds(duration: Duration) -> f64 {
        duration.as_secs_f64()
    }

    fn usize_percent(part: usize, total: usize) -> f64 {
        if total == 0 {
            0.0
        } else {
            (part as f64 / total as f64) * 100.0
        }
    }

    fn effective_cores(worker_time: Duration, wall_time: Duration) -> f64 {
        let wall_seconds = wall_time.as_secs_f64();
        if wall_seconds <= 0.0 {
            0.0
        } else {
            worker_time.as_secs_f64() / wall_seconds
        }
    }

    fn core_efficiency_percent(
        worker_time: Duration,
        wall_time: Duration,
        worker_count: usize,
    ) -> f64 {
        if worker_count == 0 {
            0.0
        } else {
            (Self::effective_cores(worker_time, wall_time) / worker_count as f64) * 100.0
        }
    }

    fn saturating_duration_sub_all(total: Duration, parts: &[Duration]) -> Duration {
        parts
            .iter()
            .fold(total, |remaining, part| remaining.saturating_sub(*part))
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
