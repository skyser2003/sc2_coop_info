use rayon::prelude::*;
use s2coop_analyzer::detailed_replay_analysis::{DetailedReplayAnalyzer, ReplayAnalysisResources};
use serde::Deserialize;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use tauri::{State, Wry};

use crate::{
    AppSettings, BackendState, ConfigChatPayload, ConfigReplayVisualPayload, ConfigReplaysPayload,
    OverlayActionResponse, PathManagerOps, ReplayAnalysis, ReplayAnalysisOps, ReplayCacheDatabase,
    ReplayCacheDifficultyFilter, ReplayCacheGameSortKey, ReplayCacheGamesPageQuery,
    ReplayCachePage, ReplayCacheSortDirection, ReplayInfo, TauriOverlayOps, overlay_info,
};

use super::DEFAULT_CONFIG_ROWS_PER_PAGE;

pub struct ReplayCommands;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReplaysPageRequest {
    limit: Option<usize>,
    page: Option<usize>,
    rows_per_page: Option<usize>,
    search: Option<String>,
    sort_key: Option<String>,
    sort_direction: Option<String>,
    difficulty_filters: Option<Vec<String>>,
    include_normal_games: Option<bool>,
    include_mutation_games: Option<bool>,
}

struct GamesPageRows {
    rows: Vec<ReplayInfo>,
    total_rows: usize,
}

impl GamesPageRows {
    fn new(rows: Vec<ReplayInfo>, total_rows: usize) -> Self {
        Self { rows, total_rows }
    }

    fn into_parts(self) -> (Vec<ReplayInfo>, usize) {
        (self.rows, self.total_rows)
    }
}

struct GamesPageReplayOps;

impl GamesPageReplayOps {
    fn load_page(
        database: &ReplayCacheDatabase,
        query: &ReplayCacheGamesPageQuery,
        settings: &AppSettings,
        resources: Option<&ReplayAnalysisResources>,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Result<GamesPageRows, String> {
        let cached_files = database
            .load_cached_files()
            .map_err(|error| error.to_string())?;
        let transient_rows = Self::load_transient_rows(
            settings,
            resources,
            &cached_files,
            query,
            main_names,
            main_handles,
        );
        if transient_rows.is_empty() {
            return Self::load_cached_page(database, query, resources, main_names, main_handles);
        }

        Self::load_merged_page(
            database,
            query,
            resources,
            main_names,
            main_handles,
            transient_rows,
        )
    }

    fn load_cached_page(
        database: &ReplayCacheDatabase,
        query: &ReplayCacheGamesPageQuery,
        resources: Option<&ReplayAnalysisResources>,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Result<GamesPageRows, String> {
        let page = database
            .load_summary_entries_page(query)
            .map_err(|error| error.to_string())?;
        let (entries, total_rows) = page.into_rows_and_total();
        let rows = Self::replays_from_cache_entries(entries, resources, main_names, main_handles);
        Ok(GamesPageRows::new(rows, total_rows))
    }

    fn load_merged_page(
        database: &ReplayCacheDatabase,
        query: &ReplayCacheGamesPageQuery,
        resources: Option<&ReplayAnalysisResources>,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
        transient_rows: Vec<ReplayInfo>,
    ) -> Result<GamesPageRows, String> {
        let requested_offset = query.page().offset();
        let requested_limit = query.page().limit();
        let transient_count = transient_rows.len();
        let cached_offset = requested_offset.saturating_sub(transient_count);
        let cached_limit = requested_limit.saturating_add(transient_count);
        let cached_query =
            query.with_page(ReplayCachePage::from_offset(cached_offset, cached_limit));
        let cached_page = database
            .load_summary_entries_page(&cached_query)
            .map_err(|error| error.to_string())?;
        let (cached_entries, cached_total_rows) = cached_page.into_rows_and_total();
        let mut combined =
            Self::replays_from_cache_entries(cached_entries, resources, main_names, main_handles);
        combined.extend(transient_rows);
        Self::sort_rows(&mut combined, query);
        let page_offset = requested_offset.saturating_sub(cached_offset);
        let rows = combined
            .into_iter()
            .skip(page_offset)
            .take(requested_limit)
            .collect::<Vec<_>>();
        Ok(GamesPageRows::new(
            rows,
            cached_total_rows.saturating_add(transient_count),
        ))
    }

    fn replays_from_cache_entries(
        entries: Vec<s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry>,
        resources: Option<&ReplayAnalysisResources>,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let dictionary = resources.map(ReplayAnalysisResources::dictionary_data);
        entries
            .iter()
            .map(|entry| {
                dictionary
                    .map(|dictionary| {
                        ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                            entry, dictionary,
                        )
                    })
                    .unwrap_or_else(|| ReplayAnalysisOps::replay_info_from_cache_entry(entry))
                    .oriented_for_main_identity(main_names, main_handles)
            })
            .collect()
    }

    fn load_transient_rows(
        settings: &AppSettings,
        resources: Option<&ReplayAnalysisResources>,
        cached_files: &HashSet<String>,
        query: &ReplayCacheGamesPageQuery,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Vec<ReplayInfo> {
        let Some(resources) = resources else {
            return Vec::new();
        };
        let Some(root) = settings.resolve_replay_root() else {
            return Vec::new();
        };
        let paths = ReplayAnalysis::collect_replay_paths(&root, 0)
            .into_iter()
            .filter(|path| DetailedReplayAnalyzer::is_games_tab_custom_replay_path(path))
            .filter(|path| Self::path_is_not_cached(path, cached_files))
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Vec::new();
        }

        let worker_count = AppSettings::simple_analysis_worker_threads()
            .max(1)
            .min(paths.len().max(1));
        let parsed_rows = match rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
        {
            Ok(pool) => pool.install(|| {
                paths
                    .par_iter()
                    .filter_map(|path| {
                        Self::parse_transient_row(path, resources, main_names, main_handles)
                    })
                    .collect::<Vec<_>>()
            }),
            Err(error) => {
                crate::sco_warn!(
                    "[SCO/games] failed to build transient replay parse pool: {error}"
                );
                paths
                    .iter()
                    .filter_map(|path| {
                        Self::parse_transient_row(path, resources, main_names, main_handles)
                    })
                    .collect::<Vec<_>>()
            }
        };

        let mut rows = parsed_rows
            .into_iter()
            .filter(|replay| Self::matches_query(replay, query))
            .collect::<Vec<_>>();
        Self::sort_rows(&mut rows, query);
        rows
    }

    fn path_is_not_cached(path: &Path, cached_files: &HashSet<String>) -> bool {
        let file = path.to_string_lossy().to_string();
        !file.is_empty() && !cached_files.contains(&file)
    }

    fn parse_transient_row(
        path: &Path,
        resources: &ReplayAnalysisResources,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Option<ReplayInfo> {
        let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ReplayAnalysis::summarize_replay_with_cache_entry_with_resources(path, resources)
        }));
        let (replay, _cache_entry) = parsed.ok().flatten()?;
        let replay = replay.oriented_for_main_identity(main_names, main_handles);
        if replay.result() == "Unparsed" {
            return None;
        }
        if replay.main_commander().trim().is_empty() && replay.ally_commander().trim().is_empty() {
            return None;
        }
        Some(replay)
    }

    fn matches_query(replay: &ReplayInfo, query: &ReplayCacheGamesPageQuery) -> bool {
        Self::matches_mode_filter(replay, query)
            && Self::matches_difficulty_filter(replay, query.difficulty_filters())
            && Self::matches_search(replay, query.search())
    }

    fn matches_mode_filter(replay: &ReplayInfo, query: &ReplayCacheGamesPageQuery) -> bool {
        let is_mutation = replay.weekly() || !replay.mutators().is_empty();
        if !query.include_normal_games() && !query.include_mutation_games() {
            return false;
        }
        if !query.include_normal_games() {
            return is_mutation;
        }
        if !query.include_mutation_games() {
            return !is_mutation;
        }
        true
    }

    fn matches_difficulty_filter(
        replay: &ReplayInfo,
        filters: &[ReplayCacheDifficultyFilter],
    ) -> bool {
        if filters.is_empty() {
            return false;
        }
        let all_filters = ReplayCacheDifficultyFilter::all();
        if all_filters
            .iter()
            .all(|filter| filters.iter().any(|value| value == filter))
        {
            return true;
        }

        filters
            .iter()
            .any(|filter| Self::matches_single_difficulty_filter(replay, *filter))
    }

    fn matches_single_difficulty_filter(
        replay: &ReplayInfo,
        filter: ReplayCacheDifficultyFilter,
    ) -> bool {
        if let Some(level) = filter.brutal_plus_level() {
            return replay.brutal_plus() == level as u64;
        }

        let difficulty = replay.difficulty().trim().to_ascii_lowercase();
        if filter == ReplayCacheDifficultyFilter::Brutal {
            return replay.brutal_plus() == 0
                && !matches!(difficulty.as_str(), "casual" | "normal" | "hard");
        }

        filter
            .regular_label()
            .is_some_and(|label| replay.brutal_plus() == 0 && difficulty == label)
    }

    fn matches_search(replay: &ReplayInfo, search: &str) -> bool {
        let search = search.trim().to_ascii_lowercase();
        if search.is_empty() {
            return true;
        }

        Self::contains_search(replay.file(), &search)
            || Self::contains_search(replay.result(), &search)
            || Self::contains_search(replay.map(), &search)
            || Self::contains_search(replay.difficulty(), &search)
            || Self::contains_search(replay.enemy(), &search)
            || Self::contains_search(replay.slot1().name(), &search)
            || Self::contains_search(replay.slot2().name(), &search)
            || Self::contains_search(replay.slot1().commander(), &search)
            || Self::contains_search(replay.slot2().commander(), &search)
            || replay
                .mutators()
                .iter()
                .any(|mutator| Self::contains_search(mutator, &search))
    }

    fn contains_search(value: &str, search: &str) -> bool {
        value.to_ascii_lowercase().contains(search)
    }

    fn sort_rows(rows: &mut [ReplayInfo], query: &ReplayCacheGamesPageQuery) {
        rows.sort_by(|left, right| Self::compare_rows(left, right, query));
    }

    fn compare_rows(
        left: &ReplayInfo,
        right: &ReplayInfo,
        query: &ReplayCacheGamesPageQuery,
    ) -> Ordering {
        let primary = Self::compare_primary(left, right, query);
        if primary != Ordering::Equal {
            return primary;
        }

        let time_direction = if query.sort_key() == ReplayCacheGameSortKey::Time {
            query.sort_direction()
        } else {
            ReplayCacheSortDirection::Desc
        };
        let date_order = Self::apply_direction(left.date().cmp(&right.date()), time_direction);
        if date_order != Ordering::Equal {
            return date_order;
        }
        Self::apply_direction(left.file().cmp(right.file()), time_direction)
    }

    fn compare_primary(
        left: &ReplayInfo,
        right: &ReplayInfo,
        query: &ReplayCacheGamesPageQuery,
    ) -> Ordering {
        match query.sort_key() {
            ReplayCacheGameSortKey::Length => Self::apply_direction(
                left.accurate_length()
                    .partial_cmp(&right.accurate_length())
                    .unwrap_or(Ordering::Equal),
                query.sort_direction(),
            ),
            ReplayCacheGameSortKey::Time => {
                Self::apply_direction(left.date().cmp(&right.date()), query.sort_direction())
            }
            _ => Self::apply_direction(
                Self::sort_text(left, query.sort_key())
                    .cmp(&Self::sort_text(right, query.sort_key())),
                query.sort_direction(),
            ),
        }
    }

    fn sort_text(replay: &ReplayInfo, sort_key: ReplayCacheGameSortKey) -> String {
        match sort_key {
            ReplayCacheGameSortKey::Map => replay.map().to_ascii_lowercase(),
            ReplayCacheGameSortKey::Result => replay.result().to_ascii_lowercase(),
            ReplayCacheGameSortKey::PlayerOne => {
                format!("{} {}", replay.slot1().name(), replay.slot1().commander())
                    .to_ascii_lowercase()
            }
            ReplayCacheGameSortKey::PlayerTwo => {
                format!("{} {}", replay.slot2().name(), replay.slot2().commander())
                    .to_ascii_lowercase()
            }
            ReplayCacheGameSortKey::Enemy => replay.enemy().to_ascii_lowercase(),
            ReplayCacheGameSortKey::Difficulty => replay.difficulty().to_ascii_lowercase(),
            ReplayCacheGameSortKey::Mutators => replay.mutators().join(" ").to_ascii_lowercase(),
            ReplayCacheGameSortKey::Actions => replay.file().to_ascii_lowercase(),
            ReplayCacheGameSortKey::Length | ReplayCacheGameSortKey::Time => String::new(),
        }
    }

    fn apply_direction(ordering: Ordering, direction: ReplayCacheSortDirection) -> Ordering {
        match direction {
            ReplayCacheSortDirection::Asc => ordering,
            ReplayCacheSortDirection::Desc => ordering.reverse(),
        }
    }
}

#[tauri::command]
pub async fn config_replays_get(
    app: tauri::AppHandle<Wry>,
    request: Option<ConfigReplaysPageRequest>,
    state: State<'_, BackendState>,
) -> Result<ConfigReplaysPayload, String> {
    ReplayCommands::config_replays_get(app, request, state).await
}

#[tauri::command]
pub async fn config_replay_show(
    app: tauri::AppHandle<Wry>,
    file: Option<String>,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    ReplayCommands::config_replay_show(app, file, state).await
}

#[tauri::command]
pub async fn config_replay_chat(
    app: tauri::AppHandle<Wry>,
    file: String,
    state: State<'_, BackendState>,
) -> Result<ConfigChatPayload, String> {
    ReplayCommands::config_replay_chat(app, file, state).await
}

#[tauri::command]
pub async fn config_replay_visual(
    app: tauri::AppHandle<Wry>,
    file: String,
    state: State<'_, BackendState>,
) -> Result<ConfigReplayVisualPayload, String> {
    ReplayCommands::config_replay_visual(app, file, state).await
}

#[tauri::command]
pub async fn config_replay_move(
    app: tauri::AppHandle<Wry>,
    delta: i64,
    state: State<'_, BackendState>,
) -> Result<OverlayActionResponse, String> {
    ReplayCommands::config_replay_move(app, delta, state).await
}

impl ReplayCommands {
    pub async fn config_replays_get(
        _app: tauri::AppHandle<Wry>,
        request: Option<ConfigReplaysPageRequest>,
        state: State<'_, BackendState>,
    ) -> Result<ConfigReplaysPayload, String> {
        let request = request.unwrap_or_default();
        let page = request.page.unwrap_or(1).max(1);
        let rows_per_page = request
            .rows_per_page
            .or(request.limit)
            .unwrap_or(DEFAULT_CONFIG_ROWS_PER_PAGE)
            .max(1);
        let search = request.search.unwrap_or_default();
        let sort_key = ReplayCacheGameSortKey::from_query_value(request.sort_key.as_deref());
        let sort_direction = ReplayCacheSortDirection::from_query_value(
            request.sort_direction.as_deref(),
            ReplayCacheSortDirection::Desc,
        );
        let difficulty_filters = request
            .difficulty_filters
            .unwrap_or_else(|| {
                ReplayCacheDifficultyFilter::all()
                    .into_iter()
                    .map(|value| format!("{value:?}"))
                    .collect()
            })
            .iter()
            .filter_map(|value| ReplayCacheDifficultyFilter::from_query_value(value))
            .collect::<Vec<_>>();
        let include_normal_games = request.include_normal_games.unwrap_or(true);
        let include_mutation_games = request.include_mutation_games.unwrap_or(true);
        let path =
            format!("/config/replays?page={page}&rows_per_page={rows_per_page}&search={search}");
        state.log_request("get", &path, &None);

        let query = ReplayCacheGamesPageQuery::new(
            ReplayCachePage::new(page, rows_per_page),
            search,
            sort_key,
            sort_direction,
            difficulty_filters,
            include_normal_games,
            include_mutation_games,
        );
        let replay_state = state.get_replay_state();
        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let resources = state.replay_analysis_resources().ok();
        let dictionary = state.dictionary_data().ok();
        let settings = state.read_settings_memory();

        let (replays, total_replays, selected_replay_file) =
            tauri::async_runtime::spawn_blocking(move || {
                let cache_path = PathManagerOps::get_cache_path();
                let database = ReplayCacheDatabase::open_for_cache_path(&cache_path)
                    .map_err(|error| error.to_string())?;
                let page_rows = GamesPageReplayOps::load_page(
                    &database,
                    &query,
                    &settings,
                    resources.as_deref(),
                    &main_names,
                    &main_handles,
                )?;
                let (replays, total_replays) = page_rows.into_parts();
                let selected_replay_file = replay_state
                    .lock()
                    .ok()
                    .and_then(|state| state.get_current_replay_file());

                Ok::<_, String>((replays, total_replays, selected_replay_file))
            })
            .await
            .map_err(|error| format!("Failed to load /config/replays: {error}"))??;

        Ok(ConfigReplaysPayload {
            status: "ok",
            replays: replays
                .into_iter()
                .map(|replay| {
                    dictionary
                        .as_deref()
                        .map(|dictionary| replay.as_games_row_payload_with_dictionary(dictionary))
                        .unwrap_or_else(|| replay.as_games_row_payload())
                })
                .collect(),
            total_replays,
            selected_replay_file,
        })
    }

    pub async fn config_replay_show(
        app: tauri::AppHandle<Wry>,
        file: Option<String>,
        state: State<'_, BackendState>,
    ) -> Result<OverlayActionResponse, String> {
        let body = Some(TauriOverlayOps::to_json_value(
            serde_json::json!({ "file": file }),
        ));
        state.log_request("post", "/config/replays/show", &body);
        let requested = body
            .as_ref()
            .and_then(|payload| payload.get("file"))
            .and_then(Value::as_str);
        Ok(overlay_info::OverlayInfoOps::replay_show_for_window(
            &app, &state, requested,
        ))
    }

    pub async fn config_replay_chat(
        _app: tauri::AppHandle<Wry>,
        file: String,
        state: State<'_, BackendState>,
    ) -> Result<ConfigChatPayload, String> {
        let body = Some(TauriOverlayOps::to_json_value(
            serde_json::json!({ "file": file }),
        ));
        state.log_request("post", "/config/replays/chat", &body);
        let requested_file = body
            .as_ref()
            .and_then(|payload| payload.get("file"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let dictionary = state.dictionary_data().ok();
        let resources = state.replay_analysis_resources().ok();
        let chat = tauri::async_runtime::spawn_blocking(move || {
            TauriOverlayOps::replay_chat_payload_from_slots(
                main_names,
                main_handles,
                &requested_file,
                dictionary,
                resources,
            )
        })
        .await
        .map_err(|error| format!("Failed to load /config/replays/chat: {error}"))??;
        Ok(ConfigChatPayload { status: "ok", chat })
    }

    pub async fn config_replay_visual(
        _app: tauri::AppHandle<Wry>,
        file: String,
        state: State<'_, BackendState>,
    ) -> Result<ConfigReplayVisualPayload, String> {
        let body = Some(TauriOverlayOps::to_json_value(
            serde_json::json!({ "file": file }),
        ));
        state.log_request("post", "/config/replays/visual", &body);
        let requested_file = body
            .as_ref()
            .and_then(|payload| payload.get("file"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let main_names = state.configured_main_names();
        let main_handles = state.configured_main_handles();
        let dictionary = state.dictionary_data()?;
        let resources = state.replay_analysis_resources()?;
        let visual = tauri::async_runtime::spawn_blocking(move || {
            TauriOverlayOps::replay_visual_payload_from_slots(
                main_names,
                main_handles,
                &requested_file,
                dictionary,
                resources,
            )
        })
        .await
        .map_err(|error| format!("Failed to load /config/replays/visual: {error}"))??;
        Ok(ConfigReplayVisualPayload {
            status: "ok",
            visual,
        })
    }

    pub async fn config_replay_move(
        app: tauri::AppHandle<Wry>,
        delta: i64,
        state: State<'_, BackendState>,
    ) -> Result<OverlayActionResponse, String> {
        let body = Some(TauriOverlayOps::to_json_value(
            serde_json::json!({ "delta": delta }),
        ));
        state.log_request("post", "/config/replays/move", &body);
        let delta = body
            .as_ref()
            .and_then(|payload| payload.get("delta"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        Ok(overlay_info::OverlayInfoOps::replay_move_window(
            &app, &state, delta,
        ))
    }
}
