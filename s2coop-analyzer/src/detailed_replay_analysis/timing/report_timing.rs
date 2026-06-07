use super::super::ReplayEventKind;
use super::AnalyzerTimingConfig;
use std::time::{Duration, Instant};

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
pub(in crate::detailed_replay_analysis) enum ReplayReportTimingSpan {
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

    pub(in crate::detailed_replay_analysis) fn add(&mut self, other: &Self) {
        self.total += other.total;
        for (target, incoming) in self.spans.iter_mut().zip(other.spans.iter()) {
            *target += *incoming;
        }
        for (target, incoming) in self.event_counts.iter_mut().zip(other.event_counts.iter()) {
            *target += *incoming;
        }
        self.events_input_count += other.events_input_count;
    }

    pub(super) fn span(&self, span: ReplayReportTimingSpan) -> Duration {
        self.spans[span.index()]
    }

    pub(super) fn event_count(&self, event_kind: ReplayEventKind) -> usize {
        self.event_counts[event_kind.timing_count_index()]
    }

    pub(super) fn post_processing_total(&self) -> Duration {
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
pub(in crate::detailed_replay_analysis) struct ReplayAnalysisTimingCollector {
    label: String,
    started: Instant,
    spans: [Duration; REPLAY_REPORT_TIMING_SPAN_COUNT],
    event_counts: [usize; 13],
    events_input_count: usize,
}

#[derive(Debug, Default)]
pub(in crate::detailed_replay_analysis) struct ReplayAnalysisNoopTimingCollector;

pub(in crate::detailed_replay_analysis) trait ReplayAnalysisTiming {
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

        log::debug!(
            "[s2coop timing] analyze_replay_file_impl label=\"{}\" total={:.3}ms",
            self.label,
            self.started.elapsed().as_secs_f64() * 1000.0
        );
        for span in REPLAY_REPORT_TIMING_SPANS {
            let duration = self.spans[span.index()];
            if duration > Duration::ZERO {
                log::trace!(
                    "[s2coop timing] span.{}={:.3}ms",
                    span.label(),
                    duration.as_secs_f64() * 1000.0
                );
            }
        }
        log::trace!(
            "[s2coop timing] count.events_input={}",
            self.events_input_count
        );
        for (index, count) in self.event_counts.iter().enumerate() {
            if *count > 0 {
                let name = REPLAY_EVENT_KIND_TIMING_COUNT_NAMES[index];
                log::trace!("[s2coop timing] {name}={count}");
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
