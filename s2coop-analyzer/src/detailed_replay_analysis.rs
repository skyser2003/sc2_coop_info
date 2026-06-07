use crate::cache_overall_stats_generator::{AnalysisPlayerStatsSeries, CacheReplayEntry};
use crate::dictionary_data::{CacheGenerationData, UnitAddKillsToJson, UnitNamesJson};
use crate::tauri_replay_analysis_impl::{
    ParsedReplayMessage, PlayerPositions, ReplayReport, ReplayReportDetailData,
    ReplayReportDetailedInput,
};
use indexmap::IndexMap;
use s2protocol_port::{ReplayEvent, TrackerEvent};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
mod analysis_constants;
mod analysis_inputs;
mod analysis_model;
mod base_parse;
mod cache_generation;
mod cache_parallel;
mod cache_progress;
mod cache_runtime;
mod cache_sink;
mod detailed_report;
mod replay_event_handlers;
mod timing;

use crate::stats_counter_core::{
    ReplayDroneCommandEventKind, ReplayDroneIdentifierCore, ReplayStatsCounterCore,
    StatsCounterDictionaries,
};
use analysis_constants::CUSTOM_KILL_ICON_KEYS;
use analysis_inputs::{
    FillUnitKillsAndIconsInput, ReplayMutatorIdentificationInput, UnitBornOrInitHandlerInput,
    UnitDiedDetailHandlerInput, UnitDiedKillStatsHandlerInput, UnitOwnerChangeHandlerInput,
    UnitStats, UnitTypeChangeHandlerInput, UpgradeEventHandlerInput,
};
pub use analysis_model::{
    DetailedReplayAnalysisError, DetailedReplayAnalysisResult, ProtocolBuildValue,
    ReplayAnalysisResources, ReplayBuildInfo, ReplayCacheFileIdentity, ReplayFileIdentity,
};
use analysis_model::{
    ReplayAnalysisSets, ReplayBaseParse, ReplayBaseParseError, ReplayBaseParseFilters,
    ReplayBaseParseOptions, ReplayCacheContext, ReplayDetailedEventCollection,
    ReplayDetailedEventCollector, ReplayDetailedParseContext, ReplayEventKind, ReplayFileDigest,
    ReplayMutatorParseContext, ReplayNumericValue, ReplayParsedContext, ReplayParsedInputBundle,
    TimedDetailedReplayReport, TimedReplayEntryParse,
};
pub use cache_parallel::{
    ReplayCacheParallelMapResult, ReplayCacheParallelParseOptions, ReplayCacheParseMode,
    ReplayCacheParsedEntry,
};
pub use cache_runtime::{
    GenerateCacheConfig, GenerateCacheError, GenerateCacheRuntimeOptions,
    GenerateCacheStopController, GenerateCacheSummary,
};
pub use cache_sink::{CacheEntrySink, CacheEntrySinkError, CacheReplayCheck};
use replay_event_handlers::{
    IdentifiedWavesMap, ReplayEventHandlers, ReplayMapAnalysisFlags, ReplayPlayerIdSet,
    StatsCounterTarget, UnitBornOrInitEventFields, UnitBornOrInitUnitIds, UnitDiedEventFields,
    UnitEventPosition, UnitStateMap, UnitTypeChangeEventFields, UnitTypeCountMap, WaveUnitsState,
};
use timing::{
    AnalyzerTimingConfig, ReplayAnalysisNoopTimingCollector, ReplayAnalysisTiming,
    ReplayAnalysisTimingCollector, ReplayReportTimingSpan,
};
pub use timing::{DetailedReplayReportTiming, GenerateCacheTimingReport, ReplayTiming};

pub const DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE: usize = 100;

pub struct DetailedReplayAnalyzer;

impl DetailedReplayAnalyzer {
    fn event_gameloop(event: &ReplayEvent) -> i64 {
        event._gameloop()
    }

    fn event_control_id(event: &ReplayEvent) -> Option<i64> {
        match event {
            ReplayEvent::Game(event) => event.m_control_id,
            ReplayEvent::Tracker(_) => None,
        }
    }

    fn event_event_type(event: &ReplayEvent) -> Option<i64> {
        match event {
            ReplayEvent::Game(event) => event.m_event_type,
            ReplayEvent::Tracker(_) => None,
        }
    }

    fn event_user_id(event: &ReplayEvent) -> Option<i64> {
        match event {
            ReplayEvent::Game(event) => event.user_id,
            ReplayEvent::Tracker(_) => None,
        }
    }

    fn difficulty_name(code: i64) -> &'static str {
        match code {
            1 => "Casual",
            2 => "Normal",
            3 => "Hard",
            4 => "Brutal",
            5 => "Custom",
            6 => "Cheater",
            _ => "Unknown",
        }
    }

    fn region_name(code: i64) -> &'static str {
        match code {
            1 => "NA",
            2 => "EU",
            3 => "KR",
            5 => "CN",
            98 => "PTR",
            _ => "",
        }
    }

    fn format_duration(seconds: f64) -> String {
        if !seconds.is_finite() || seconds <= 0.0 {
            return "00:00".to_string();
        }

        let total = seconds.floor() as u64;
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let secs = total % 60;
        if hours > 0 {
            format!("{hours:02}:{minutes:02}:{secs:02}")
        } else {
            format!("{minutes:02}:{secs:02}")
        }
    }

    fn valid_protocol_mapping(build: i64) -> Option<i64> {
        match build {
            81102 => Some(81433),
            80871 => Some(81433),
            76811 => Some(76114),
            80188 => Some(78285),
            79998 => Some(78285),
            81433 => Some(83830),
            84643 => Some(83830),
            _ => None,
        }
    }

    fn supported_legacy_protocol(build: i64) -> bool {
        matches!(build, 76114 | 78285 | 83830)
    }

    fn cache_handle_id(handle: &str) -> String {
        let tail = handle.rsplit('/').next().unwrap_or("");
        tail.split('.').next().unwrap_or("").to_string()
    }

    fn mutator_from_button(button: i64, panel: i64, mutators: &[String]) -> Option<String> {
        let idx = (button - 41) / 3 + (panel - 1) * 15;
        if idx < 0 {
            return None;
        }
        let Ok(index) = usize::try_from(idx) else {
            return None;
        };
        mutators.get(index).cloned()
    }

    fn identify_mutators_for_replay(
        input: ReplayMutatorIdentificationInput<'_>,
    ) -> (Vec<String>, bool) {
        let ReplayMutatorIdentificationInput {
            event_collection,
            mutators_all,
            mutators_ui,
            mutator_ids,
            cached_mutators,
            extension,
            mm,
            mutator_context,
        } = input;
        let mut mutators = Vec::new();
        let mut weekly = false;

        if mm && let Some(collection) = event_collection {
            for mutator_key in &collection.mm_mutator_keys {
                if mutator_ids.contains_key(mutator_key) {
                    mutators.push(mutator_key.clone());
                }
            }
        }

        if extension && let Some(context) = mutator_context {
            for handle in &context.cache_handles {
                let cached = DetailedReplayAnalyzer::cache_handle_id(handle);
                if cached.is_empty() {
                    continue;
                }
                if let Some(mutator_id) = cached_mutators.get(&cached) {
                    mutators.push(mutator_id.clone());
                    weekly = true;
                }
            }
        }

        if !extension
            && let Some(context) = mutator_context
            && context.brutal_plus_difficulty > 0
        {
            for key in &context.retry_mutation_indexes {
                if *key <= 0 {
                    continue;
                }
                if let Ok(index) = usize::try_from(*key - 1)
                    && let Some(mutator) = mutators_all.get(index)
                {
                    mutators.push(mutator.clone());
                }
            }
        }

        if extension && let Some(collection) = event_collection {
            let mut panel = 1_i64;
            for action in &collection.extension_actions {
                let action = *action;
                if (41..=83).contains(&action)
                    && let Some(new_mutator) =
                        DetailedReplayAnalyzer::mutator_from_button(action, panel, mutators_ui)
                {
                    if !mutators.contains(&new_mutator) || new_mutator == "Random" {
                        mutators.push(new_mutator);
                    } else if new_mutator != "Random"
                        && let Some(position) =
                            mutators.iter().position(|value| value == &new_mutator)
                    {
                        mutators.remove(position);
                    }
                }

                if action == 123 && panel > 1 {
                    panel -= 1;
                }
                if action == 124 && panel < 4 {
                    panel += 1;
                }

                if (88..=106).contains(&action)
                    && let Ok(index) = usize::try_from((action - 88) / 2)
                    && index < mutators.len()
                {
                    mutators.remove(index);
                }
            }
        }

        (
            mutators
                .into_iter()
                .map(|mutator| {
                    mutator
                        .replace("Heroes from the Storm (old)", "Heroes from the Storm")
                        .replace("Extreme Caution", "Afraid of the Dark")
                })
                .collect(),
            weekly,
        )
    }
}
