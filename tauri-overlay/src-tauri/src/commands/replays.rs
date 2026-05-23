use s2coop_analyzer::detailed_replay_analysis::ReplayAnalysisResources;
use serde::Deserialize;
use serde_json::Value;
use tauri::{State, Wry};

use crate::{
    BackendState, ConfigChatPayload, ConfigReplayVisualPayload, ConfigReplaysPayload,
    OverlayActionResponse, PathManagerOps, ReplayAnalysisOps, ReplayCacheDatabase,
    ReplayCacheDifficultyFilter, ReplayCacheGameSortKey, ReplayCacheGamesPageQuery,
    ReplayCachePage, ReplayCacheSortDirection, TauriOverlayOps, overlay_info,
};

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
            .unwrap_or(300)
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

        let (replays, total_replays, selected_replay_file) =
            tauri::async_runtime::spawn_blocking(move || {
                let cache_path = PathManagerOps::get_cache_path();
                let (entries, total_replays) =
                    ReplayCacheDatabase::open_for_cache_path(&cache_path)
                        .and_then(|database| {
                            let page = database.load_summary_entries_page(&query)?;
                            Ok(page.into_rows_and_total())
                        })
                        .map_err(|error| error.to_string())?;
                let dictionary = resources
                    .as_deref()
                    .map(ReplayAnalysisResources::dictionary_data);
                let replays = entries
                    .iter()
                    .map(|entry| {
                        dictionary
                            .map(|dictionary| {
                                ReplayAnalysisOps::replay_info_from_cache_entry_with_dictionary(
                                    entry, dictionary,
                                )
                            })
                            .unwrap_or_else(|| {
                                ReplayAnalysisOps::replay_info_from_cache_entry(entry)
                            })
                            .oriented_for_main_identity(&main_names, &main_handles)
                    })
                    .collect::<Vec<_>>();
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
