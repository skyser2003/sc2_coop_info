use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{
    DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE, DetailedReplayAnalyzer, ReplayAnalysisResources,
    ReplayCacheParallelParseOptions, ReplayFileIdentity,
};

use crate::path_manager::PathManagerOps;
use crate::replay_scan_progress::ReplayScanProgress;
use crate::{
    AppSettings, QueuedReplayCacheEntrySink, ReplayCacheDatabase, ReplayCacheWriteQueue,
    ReplayInfo, TauriOverlayOps, UNLIMITED_REPLAY_LIMIT,
};

use super::{
    ParsedReplayBatch, ParsedReplayPathResult, ReplayAnalysis, ReplayAnalysisOps, ScanInFlightGuard,
};

impl ReplayAnalysis {
    pub fn modified_seconds(path: &Path) -> u64 {
        path.metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .map_or(0, TauriOverlayOps::format_date_from_system_time)
    }

    pub fn collect_replay_paths(root: &Path, limit: usize) -> Vec<PathBuf> {
        if !root.exists() || !root.is_dir() {
            return Vec::new();
        }

        let mut stack = vec![root.to_path_buf()];
        let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();

        while let Some(current) = stack.pop() {
            let entries_on_disk = match std::fs::read_dir(&current) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for dir_entry in entries_on_disk.filter_map(Result::ok) {
                let path = dir_entry.path();
                let meta = match dir_entry.metadata() {
                    Ok(value) => value,
                    Err(_) => continue,
                };

                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }

                if !meta.is_file() {
                    continue;
                }

                if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sc2replay"))
                {
                    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                    entries.push((path, modified));
                }
            }
        }

        entries.sort_by(|(_, a), (_, b)| b.cmp(a));
        if limit == 0 {
            entries.into_iter().map(|(path, _)| path).collect()
        } else {
            entries
                .into_iter()
                .take(limit)
                .map(|(path, _)| path)
                .collect()
        }
    }

    pub fn summarize_replay_with_cache_entry(
        path: &Path,
    ) -> Option<(ReplayInfo, Option<CacheReplayEntry>)> {
        let _ = path;
        None
    }

    pub fn summarize_replay_with_cache_entry_with_resources(
        path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Option<(ReplayInfo, Option<CacheReplayEntry>)> {
        let parse_started_at = Instant::now();
        let file_label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>");
        let empty_handles = std::collections::HashSet::new();

        match DetailedReplayAnalyzer::analyze_single_detailed(path, &empty_handles, resources) {
            Ok(result) => {
                let replay = ReplayAnalysisOps::replay_info_from_report_with_dictionary(
                    path,
                    result.report(),
                    resources.dictionary_data(),
                )
                .sanitized();
                let cache_entry = (result.cache_persistable()
                    && !DetailedReplayAnalyzer::is_mm_replay_path(path))
                .then_some(result.into_cache_entry());
                crate::sco_debug!(
                    "[SCO/replay] parsed file='{}' for cache projection in {}ms persistable={}",
                    file_label,
                    parse_started_at.elapsed().as_millis(),
                    cache_entry.is_some()
                );
                Some((replay, cache_entry))
            }
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/replay] cache persistence parse failed for {file_label} in {}ms: {error}",
                    parse_started_at.elapsed().as_millis()
                );
                None
            }
        }
    }

    pub fn summarize_replay(path: &Path) -> ReplayInfo {
        Self::summarize_replay_lightweight(path)
    }

    pub fn summarize_replay_lightweight_with_resources(
        path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> ReplayInfo {
        CacheReplayEntry::parse_basic_with_resources(path, resources)
            .map(|entry| {
                ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                    &entry,
                    resources.dictionary_data(),
                )
                .sanitized()
            })
            .unwrap_or_else(|| ReplayAnalysisOps::unparsed_replay(path))
    }

    pub fn summarize_replay_lightweight(path: &Path) -> ReplayInfo {
        ReplayAnalysisOps::unparsed_replay(path)
    }

    pub fn analyze_replays(limit: usize) -> Vec<ReplayInfo> {
        let settings = AppSettings::from_saved_file();
        let main_names = settings.configured_main_names();
        let main_handles = settings.configured_main_handles();
        Self::load_all_analysis_replays_snapshot(limit, &main_names, &main_handles)
    }

    pub fn analyze_replays_with_identity(
        limit: usize,
        settings: &AppSettings,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        scan_progress: &ReplayScanProgress,
        replay_scan_in_flight: &AtomicBool,
    ) -> Vec<ReplayInfo> {
        let _ = (settings, scan_progress, replay_scan_in_flight);
        Self::load_all_analysis_replays_snapshot(limit, main_names, main_handles)
    }

    pub fn analyze_replays_with_resources(
        limit: usize,
        settings: &AppSettings,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        scan_progress: &ReplayScanProgress,
        replay_scan_in_flight: &AtomicBool,
        resources: &ReplayAnalysisResources,
    ) -> Vec<ReplayInfo> {
        let _scan_guard = match replay_scan_in_flight.compare_exchange(
            false,
            true,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => ScanInFlightGuard {
                flag: replay_scan_in_flight,
            },
            Err(_) => {
                scan_progress.set_stage("busy");
                // When busy, return all cached replays from unified cache
                let replays =
                    Self::load_all_analysis_replays_snapshot(limit, main_names, main_handles);
                return replays;
            }
        };

        scan_progress.reset("starting");
        scan_progress.set_status("Loading cache");

        let scan_started_at = Instant::now();
        crate::sco_info!("[SCO/replay] analyze_replays start limit={limit}");
        scan_progress.set_stage("resolving_replay_root");

        let Some(root) = settings.resolve_replay_root() else {
            crate::sco_warn!("[SCO/replay] Replay root not configured");
            scan_progress.set_status("Completed");
            scan_progress.set_stage("no_replay_root");
            return Vec::new();
        };
        crate::sco_debug!("[SCO/replay] scan root: {}", root.display());

        let cache_path = PathManagerOps::get_cache_path();
        let analyzed_files = ReplayCacheDatabase::open_for_cache_path(&cache_path)
            .and_then(|database| database.load_cached_files())
            .map_err(|error| {
                crate::sco_warn!("[SCO/cache] failed to load cached replay file list: {error}");
                error
            })
            .unwrap_or_default();

        let collect_started_at = Instant::now();
        scan_progress.set_stage("collecting_paths");
        let all_paths = Self::collect_replay_paths(&root, limit);
        let all_paths_len = all_paths.len();
        scan_progress.set_total(all_paths_len as u64);

        // Filter paths to only those not in cache
        let paths_to_parse: Vec<PathBuf> = all_paths
            .into_iter()
            .filter(|path| {
                let path_str = path.to_string_lossy().to_string();
                !analyzed_files.contains(&path_str)
            })
            .collect();

        let paths_to_parse_len = paths_to_parse.len();
        scan_progress.set_to_parse(paths_to_parse_len as u64);
        scan_progress.set_cache_hits((all_paths_len - paths_to_parse_len) as u64);

        crate::sco_info!(
            "[SCO/replay] collected {} path(s) in {}ms, {} already cached, parsing {}",
            all_paths_len,
            collect_started_at.elapsed().as_millis(),
            all_paths_len - paths_to_parse_len,
            paths_to_parse_len
        );

        if paths_to_parse.is_empty() {
            scan_progress.set_status("Completed");
            scan_progress.set_stage("cache_only");
            let mut replays =
                Self::load_all_analysis_replays_snapshot(limit, main_names, main_handles);
            if limit > 0 && replays.len() > limit {
                replays.truncate(limit);
            }
            crate::sco_info!(
                "[SCO/replay] analyze_replays finished from cache in {}ms (total={})",
                scan_started_at.elapsed().as_millis(),
                replays.len()
            );
            return replays;
        }

        scan_progress.set_cache_hits(0);
        scan_progress.set_to_parse(paths_to_parse_len as u64);

        let parse_started_at = Instant::now();
        scan_progress.set_stage("parsing_replays");
        let worker_threads = crate::AppSettings::simple_analysis_worker_threads();
        let progress = scan_progress;
        let cache_writer = ReplayCacheWriteQueue::start(cache_path.clone());
        let parse_options = ReplayCacheParallelParseOptions::simple_saved_cache(worker_threads)
            .with_cache_entry_sink(Arc::new(QueuedReplayCacheEntrySink::new(
                cache_writer.sender(),
            )))
            .with_cache_entry_sink_batch_size(DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE);
        let parsed_results = match DetailedReplayAnalyzer::parse_saved_cache_entries_parallel_map(
            paths_to_parse,
            resources,
            &parse_options,
            |parsed_entry| {
                let (path, cache_entry, panicked) = parsed_entry.into_parts();
                progress.increment_completed();
                if panicked {
                    progress.increment_failed();
                    return ParsedReplayPathResult::new(
                        ReplayAnalysisOps::unparsed_replay(&path),
                        Some(path.to_string_lossy().to_string()),
                    );
                }

                progress.increment_newly_parsed();
                let replay = cache_entry
                    .as_ref()
                    .map(|entry| {
                        ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                            entry,
                            resources.dictionary_data(),
                        )
                        .sanitized()
                    })
                    .unwrap_or_else(|| ReplayAnalysisOps::unparsed_replay(&path))
                    .oriented_for_main_identity(main_names, main_handles);
                ParsedReplayPathResult::new(replay, None)
            },
        ) {
            Ok(result) => result.into_values(),
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/cache] failed to parse simple analysis worker batch: {error}"
                );
                Vec::new()
            }
        };
        drop(parse_options);
        let cache_writer_result = cache_writer.finish();

        let parsed_batch =
            parsed_results
                .into_iter()
                .fold(ParsedReplayBatch::new(), |mut batch, parsed| {
                    let (replay, failed_path) = parsed.into_parts();
                    if let Some(failed_path) = failed_path {
                        batch.push_failure(failed_path);
                    }
                    batch.push_success(replay);
                    batch
                });

        let failed_to_parse = parsed_batch.failed_paths;
        let parsed_replays = parsed_batch.replays;
        let persisted_cache_entries = cache_writer_result.persisted_entries();

        if !failed_to_parse.is_empty() {
            crate::sco_warn!(
                "[SCO/replay] failed to parse {} replay(s): {}",
                failed_to_parse.len(),
                failed_to_parse.join(", ")
            );
        }

        let failed_to_parse = failed_to_parse.len();
        scan_progress.set_failed(failed_to_parse as u64);
        scan_progress.set_parse_skipped(0);

        crate::sco_info!(
            "[SCO/replay] parsed {} replay(s) with rayon in {}ms (threads={worker_threads})",
            parsed_replays.len(),
            parse_started_at.elapsed().as_millis()
        );

        scan_progress.set_stage("finalizing_results");
        scan_progress.set_status("Finalizing results");
        crate::sco_debug!(
            "[SCO/replay] finalizing {} parsed replay result(s) against {} cached replay file(s)",
            parsed_replays.len(),
            analyzed_files.len()
        );

        let mut replay_map = HashMap::<String, ReplayInfo>::new();
        for replay in Self::load_all_analysis_replays_snapshot(
            UNLIMITED_REPLAY_LIMIT,
            main_names,
            main_handles,
        ) {
            let replay_hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(&replay.file));
            if replay_hash.is_empty() {
                continue;
            }
            replay_map.retain(|hash, entry| hash == &replay_hash || entry.file != replay.file);
            match replay_map.get(&replay_hash) {
                Some(existing)
                    if ReplayInfo::should_keep_existing_detailed_variant(
                        existing.is_detailed,
                        replay.is_detailed,
                    ) => {}
                _ => {
                    replay_map.insert(replay_hash, replay);
                }
            }
        }

        for replay in parsed_replays {
            let replay_hash = ReplayFileIdentity::calculate_hash(&PathBuf::from(&replay.file));
            if replay_hash.is_empty() {
                continue;
            }
            replay_map.retain(|hash, cached| hash == &replay_hash || cached.file != replay.file);
            match replay_map.get(&replay_hash) {
                Some(existing)
                    if ReplayInfo::should_keep_existing_detailed_variant(
                        existing.is_detailed,
                        replay.is_detailed,
                    ) => {}
                _ => {
                    replay_map.insert(replay_hash, replay);
                }
            }
        }

        crate::sco_debug!(
            "[SCO/cache] persisted {} simple-analysis cache entr(y/ies) with writer batches of {}",
            persisted_cache_entries,
            DEFAULT_CACHE_ENTRY_SINK_BATCH_SIZE
        );
        if cache_writer_result.failed_batches() > 0 {
            crate::sco_warn!(
                "[SCO/cache] failed to persist {} simple-analysis cache writer batch(es)",
                cache_writer_result.failed_batches()
            );
        }

        let mut all_replays = replay_map.into_values().collect::<Vec<_>>();
        ReplayInfo::sort_replays(&mut all_replays);
        if limit > 0 && all_replays.len() > limit {
            all_replays.truncate(limit);
        }

        scan_progress.set_stage("completed");
        scan_progress.set_status("Completed");
        let unparsed_count = all_replays
            .iter()
            .filter(|replay| replay.result == "Unparsed")
            .count();
        crate::sco_info!(
            "[SCO/replay] analyze_replays finished in {}ms (parsed={}, unparsed={}, cached={})",
            scan_started_at.elapsed().as_millis(),
            all_replays.len() - unparsed_count,
            unparsed_count,
            all_paths_len - paths_to_parse_len
        );

        all_replays
    }
}
