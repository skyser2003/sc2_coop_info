use super::super::ReplayEventKind;
use super::{
    AnalyzerTimingConfig, DetailedReplayReportTiming, GenerateCacheTimingReport,
    ReplayEntryParseTiming, ReplayReportTimingSpan,
};
use s2protocol_port::ReplayParseTiming;
use std::time::Duration;

pub(super) struct GenerateCacheTimingReportFormatter;

impl GenerateCacheTimingReportFormatter {
    pub(super) fn format_amdahl_summary(report: &GenerateCacheTimingReport) -> String {
        if !report.enabled {
            return format!(
                "Amdahl timings disabled; set {}=1 to enable.",
                AnalyzerTimingConfig::env_var_name()
            );
        }

        let max_speedup = report
            .amdahl_max_speedup_from_serial_fraction()
            .map(|value| format!("{value:.2}x"))
            .unwrap_or_else(|| "unbounded".to_string());
        let merge_and_sort = report.merge_entries + report.sort_entries;
        let candidate_other = Self::saturating_duration_sub_all(
            report.collect_candidates_worker,
            &[
                report.collect_candidates_hash_lookup,
                report.collect_candidates_priority,
            ],
        );
        let replay_other = Self::saturating_duration_sub_all(
            report.replay_analysis_worker,
            &[
                report.replay_analysis_parse_detailed,
                report.replay_analysis_parse_basic_fallback,
                report.replay_analysis_detailed_report,
                report.replay_analysis_temp_entry_write,
                report.replay_analysis_progress_record,
            ],
        );

        let mut output = format!(
            concat!(
                "Amdahl timings:\n",
                "  total={:.3}s workers={} files={} candidates={} pending={} reused={} analyzed={}\n",
                "  serial_wall_estimate={:.3}s ({:.1}%) parallelizable_wall={:.3}s ({:.1}%) max_speedup_from_this_serial_fraction={}\n",
                "  phases: collect_files={:.3}s resolve_handles={:.3}s load_cache={:.3}s build_pool={:.3}s build_canonical_pool={:.3}s collect_candidates_parallel={:.3}s replay_analysis_parallel={:.3}s collect_results={:.3}s merge_sort={:.3}s canonicalize_total={:.3}s write_file={:.3}s"
            ),
            Self::duration_seconds(report.total),
            report.worker_count,
            report.total_replay_files,
            report.candidate_count,
            report.pending_candidate_count,
            report.reused_candidate_count,
            report.analyzed_entry_count,
            Self::duration_seconds(report.serial_wall_estimate()),
            report.serial_wall_fraction() * 100.0,
            Self::duration_seconds(report.parallelizable_wall_time()),
            report.parallelizable_wall_fraction() * 100.0,
            max_speedup,
            Self::duration_seconds(report.collect_replay_files),
            Self::duration_seconds(report.resolve_main_handles),
            Self::duration_seconds(report.load_existing_cache),
            Self::duration_seconds(report.build_thread_pool),
            Self::duration_seconds(report.build_canonicalize_thread_pool),
            Self::duration_seconds(report.collect_candidates_parallel),
            Self::duration_seconds(report.replay_analysis_parallel),
            Self::duration_seconds(report.collect_analyzed_entries),
            Self::duration_seconds(merge_and_sort),
            Self::duration_seconds(report.canonicalize_entries),
            Self::duration_seconds(report.write_entries),
        );

        output.push_str(&format!(
            concat!(
                "\n  parallel core use: ",
                "collect_candidates worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "replay_analysis worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "simple_analysis worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%; ",
                "canonicalize_json workers={} worker_time={:.3}s effective_cores={:.2} capacity_eff={:.1}%"
            ),
            Self::duration_seconds(report.collect_candidates_worker),
            Self::effective_cores(
                report.collect_candidates_worker,
                report.collect_candidates_parallel
            ),
            Self::core_efficiency_percent(
                report.collect_candidates_worker,
                report.collect_candidates_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.replay_analysis_worker),
            Self::effective_cores(report.replay_analysis_worker, report.replay_analysis_parallel),
            Self::core_efficiency_percent(
                report.replay_analysis_worker,
                report.replay_analysis_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.simple_analysis_worker),
            Self::effective_cores(report.simple_analysis_worker, report.simple_analysis_parallel),
            Self::core_efficiency_percent(
                report.simple_analysis_worker,
                report.simple_analysis_parallel,
                report.worker_count
            ),
            report.canonicalize_worker_count,
            Self::duration_seconds(report.canonicalize_entries_worker),
            Self::effective_cores(
                report.canonicalize_entries_worker,
                report.canonicalize_entries_parallel
            ),
            Self::core_efficiency_percent(
                report.canonicalize_entries_worker,
                report.canonicalize_entries_parallel,
                report.canonicalize_worker_count
            ),
        ));

        output.push_str(&format!(
            concat!(
                "\n  candidate parts: ",
                "hash_lookup={:.3}s capacity_eff={:.1}% ",
                "priority={:.3}s capacity_eff={:.1}% ",
                "other={:.3}s capacity_eff={:.1}%"
            ),
            Self::duration_seconds(report.collect_candidates_hash_lookup),
            Self::core_efficiency_percent(
                report.collect_candidates_hash_lookup,
                report.collect_candidates_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.collect_candidates_priority),
            Self::core_efficiency_percent(
                report.collect_candidates_priority,
                report.collect_candidates_parallel,
                report.worker_count
            ),
            Self::duration_seconds(candidate_other),
            Self::core_efficiency_percent(
                candidate_other,
                report.collect_candidates_parallel,
                report.worker_count
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
            Self::duration_seconds(report.replay_analysis_parse_detailed),
            Self::core_efficiency_percent(
                report.replay_analysis_parse_detailed,
                report.replay_analysis_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.replay_analysis_detailed_report),
            Self::core_efficiency_percent(
                report.replay_analysis_detailed_report,
                report.replay_analysis_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.replay_analysis_parse_basic_fallback),
            Self::core_efficiency_percent(
                report.replay_analysis_parse_basic_fallback,
                report.replay_analysis_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.replay_analysis_temp_entry_write),
            Self::core_efficiency_percent(
                report.replay_analysis_temp_entry_write,
                report.replay_analysis_parallel,
                report.worker_count
            ),
            Self::duration_seconds(report.replay_analysis_progress_record),
            Self::core_efficiency_percent(
                report.replay_analysis_progress_record,
                report.replay_analysis_parallel,
                report.worker_count
            ),
            Self::duration_seconds(replay_other),
            Self::core_efficiency_percent(
                replay_other,
                report.replay_analysis_parallel,
                report.worker_count
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
            report.replay_events_decoded_len,
            report.replay_events_decoded_capacity,
            report.replay_events_retained_len,
            report.replay_events_retained_capacity,
            report.replay_analysis_temp_persisted_entries,
            report.replay_analysis_temp_persisted_bytes,
            report.canonicalize_value_count,
            report.canonicalize_payload_bytes,
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
                report
                    .replay_analysis_parse_detailed_breakdown
                    .base
                    .decode_replay_detail
                    .mpq_read_file()
            ),
            Self::duration_seconds(
                report
                    .replay_analysis_parse_detailed_breakdown
                    .base
                    .decode_replay_detail
                    .decode_ordered_events()
            ),
            Self::duration_seconds(
                report
                    .replay_analysis_detailed_report_breakdown
                    .events_total()
            ),
            Self::duration_seconds(
                report
                    .replay_analysis_detailed_report_breakdown
                    .unit_born_or_init()
            ),
            Self::duration_seconds(
                report
                    .replay_analysis_detailed_report_breakdown
                    .unit_died_detail()
            ),
            Self::duration_seconds(report.replay_analysis_report_to_cache_entry),
            Self::duration_seconds(
                report
                    .replay_analysis_parse_detailed_breakdown
                    .base
                    .hash_file
            ),
            Self::usize_percent(
                report.replay_events_retained_len,
                report.replay_events_retained_capacity
            ),
        ));

        output.push('\n');
        output.push_str(&Self::format_parse_timing_breakdown(
            "parse_detailed parts",
            &report.replay_analysis_parse_detailed_breakdown,
            report.replay_analysis_parallel,
            report.worker_count,
        ));

        if report
            .replay_analysis_detailed_report_breakdown
            .has_timings()
        {
            output.push('\n');
            output.push_str(&Self::format_detailed_report_timing_breakdown(
                "detailed_report parts",
                &report.replay_analysis_detailed_report_breakdown,
                report.replay_analysis_parallel,
                report.worker_count,
            ));
            output.push_str(&format!(
                "\n  detailed_report conversion: report_to_cache_entry={:.3}s capacity_eff={:.1}%",
                Self::duration_seconds(report.replay_analysis_report_to_cache_entry),
                Self::core_efficiency_percent(
                    report.replay_analysis_report_to_cache_entry,
                    report.replay_analysis_parallel,
                    report.worker_count
                ),
            ));
        }

        output.push('\n');
        output.push_str(&Self::format_parse_timing_breakdown(
            "parse_basic_fallback parts",
            &report.replay_analysis_parse_basic_fallback_breakdown,
            report.replay_analysis_parallel,
            report.worker_count,
        ));

        if report.simple_analysis_worker > Duration::ZERO {
            output.push('\n');
            output.push_str(&Self::format_parse_timing_breakdown(
                "simple_parse parts",
                &report.simple_analysis_parse_breakdown,
                report.simple_analysis_parallel,
                report.worker_count,
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
            Self::duration_seconds(report.canonicalize_entries_parallel),
            Self::duration_seconds(report.canonicalize_entries_worker),
            Self::core_efficiency_percent(
                report.canonicalize_entries_worker,
                report.canonicalize_entries_parallel,
                report.canonicalize_worker_count
            ),
            Self::duration_seconds(report.canonicalize_to_json_value_worker),
            Self::core_efficiency_percent(
                report.canonicalize_to_json_value_worker,
                report.canonicalize_entries_parallel,
                report.canonicalize_worker_count
            ),
            Self::duration_seconds(report.canonicalize_json_value_worker),
            Self::core_efficiency_percent(
                report.canonicalize_json_value_worker,
                report.canonicalize_entries_parallel,
                report.canonicalize_worker_count
            ),
            Self::duration_seconds(report.canonicalize_serialize_payload),
            Self::duration_seconds(report.canonicalize_deserialize_payload),
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

    pub(super) fn duration_fraction(part: Duration, total: Duration) -> f64 {
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
