use super::*;

impl OverlayInfoOps {
    fn emit_overlay_replay_payload(app: &tauri::AppHandle<Wry>, payload: &OverlayReplayPayload) {
        OverlayInfoOps::sync_overlay_runtime_settings(app);
        let _ = app.emit(OVERLAY_REPLAY_PAYLOAD_EVENT, payload);
        OverlayInfoOps::show_overlay_window(app);
    }
}

impl OverlayInfoOps {
    pub fn emit_replay_to_overlay_from_replay(
        app: &tauri::AppHandle<Wry>,
        replay: &crate::ReplayInfo,
        mark_new_replay: bool,
    ) {
        let state = app.state::<BackendState>();

        let replay = (!replay.is_detailed)
            .then(|| {
                TauriOverlayOps::process_replay_detailed(&state, &PathBuf::from(&replay.file)).1
            })
            .flatten()
            .unwrap_or_else(|| replay.clone());

        let settings = state.read_settings_memory();
        let show_session = settings.show_session();
        let (session_victories, session_defeats) = state.session_counts();
        let payload = OverlayInfoOps::overlay_payload_from_replay(
            &state,
            &replay,
            mark_new_replay,
            show_session,
            session_victories,
            session_defeats,
        );
        OverlayInfoOps::emit_overlay_replay_payload(app, &payload);
    }
}

impl OverlayInfoOps {
    pub fn replay_for_display<'a>(
        replays: &'a [crate::ReplayInfo],
        requested: Option<&str>,
        selected: &Option<String>,
    ) -> Option<&'a crate::ReplayInfo> {
        if let Some(requested_file) = requested.map(str::trim).filter(|value| !value.is_empty()) {
            return replays
                .iter()
                .find(|replay| replay.file == requested_file)
                .or_else(|| {
                    Path::new(requested_file).file_name().and_then(|name| {
                        let file_name = name.to_string_lossy();
                        replays.iter().find(|replay| {
                            Path::new(&replay.file)
                                .file_name()
                                .is_some_and(|current| current == file_name.as_ref())
                        })
                    })
                });
        }

        selected
            .as_deref()
            .and_then(|current| replays.iter().find(|replay| replay.file == current))
            .or_else(|| {
                selected.as_deref().and_then(|current| {
                    Path::new(current).file_name().and_then(|name| {
                        let file_name = name.to_string_lossy();
                        replays.iter().find(|replay| {
                            Path::new(&replay.file)
                                .file_name()
                                .is_some_and(|candidate| candidate == file_name.as_ref())
                        })
                    })
                })
            })
            .or_else(|| replays.first())
    }
}

impl OverlayInfoOps {
    pub fn replay_move_target_index(
        replays: &[crate::ReplayInfo],
        selected: &Option<String>,
        delta: i64,
        replay_data_active: bool,
    ) -> usize {
        if replays.is_empty() || !replay_data_active {
            return 0;
        }

        let mut index = TauriOverlayOps::replay_index_by_file(replays, selected).unwrap_or(0);
        if delta > 0 {
            index = index.saturating_sub(delta as usize);
        } else if delta < 0 {
            let steps = delta.wrapping_abs() as usize;
            index = (index + steps).min(replays.len().saturating_sub(1));
        }

        index
    }
}

impl OverlayInfoOps {
    pub fn replay_move_should_be_ignored(
        current_index: Option<usize>,
        target_index: usize,
        replay_data_active: bool,
    ) -> bool {
        replay_data_active && current_index.is_some_and(|index| index == target_index)
    }
}

impl OverlayInfoOps {
    fn cached_replay_for_display_from_database(
        state: &BackendState,
        requested: Option<&str>,
        selected: &Option<String>,
    ) -> Option<ReplayInfo> {
        let cache_path = PathManagerOps::get_cache_path();
        let database = ReplayCacheDatabase::open_for_cache_path(&cache_path).map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] replay display cache lookup failed: {error}");
            error
        });
        let Ok(database) = database else {
            return None;
        };

        let entry = match requested {
            Some(file) => database.load_entry_by_file(file),
            None => match selected.as_deref() {
                Some(file) => database
                    .load_entry_by_file(file)
                    .and_then(|entry| match entry {
                        Some(entry) => Ok(Some(entry)),
                        None => database.load_latest_entry(),
                    }),
                None => database.load_latest_entry(),
            },
        }
        .map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] replay display row lookup failed: {error}");
            error
        })
        .ok()
        .flatten()?;

        Some(TauriOverlayOps::replay_info_from_cache_entry_for_state(
            state, &entry,
        ))
    }

    fn replay_move_candidate_from_database(
        state: &BackendState,
        delta: i64,
    ) -> Result<Option<ReplayInfo>, String> {
        const CANDIDATE_BATCH_SIZE: usize = 32;

        let cache_path = PathManagerOps::get_cache_path();
        let database = ReplayCacheDatabase::open_for_cache_path(&cache_path).map_err(|error| {
            crate::sco_warn!("[SCO/cache-db] replay move cache lookup failed: {error}");
            error.to_string()
        })?;

        let selected = state.get_current_replay_file();
        let replay_data_active = state.overlay_replay_data_active();
        let mut offset = 0usize;
        loop {
            let entries = database
                .load_navigation_candidates(
                    selected.as_deref(),
                    delta,
                    replay_data_active,
                    offset,
                    CANDIDATE_BATCH_SIZE,
                )
                .map_err(|error| {
                    crate::sco_warn!("[SCO/cache-db] replay move row lookup failed: {error}");
                    error.to_string()
                })?;
            if entries.is_empty() {
                return Ok(None);
            }

            let entries_len = entries.len();
            if let Some(entry) = entries
                .into_iter()
                .find(|entry| Path::new(&entry.file).exists())
            {
                return Ok(Some(
                    TauriOverlayOps::replay_info_from_cache_entry_for_state(state, &entry),
                ));
            }
            offset = offset.saturating_add(entries_len);
        }
    }

    pub fn replay_show_for_window(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        requested: Option<&str>,
    ) -> crate::OverlayActionResponse {
        let requested = requested.map(str::trim).filter(|value| !value.is_empty());
        let selected = state.get_current_replay_file();

        let replay = match OverlayInfoOps::cached_replay_for_display_from_database(
            state, requested, &selected,
        ) {
            Some(replay) => replay,
            None => {
                let Some(requested_file) = requested else {
                    return crate::OverlayActionResponse::failure("No replay selected");
                };
                let requested_path = PathBuf::from(requested_file);
                let (_outcome, parsed) =
                    TauriOverlayOps::process_replay_detailed(state, requested_path.as_path());
                let Some(parsed) = parsed else {
                    return crate::OverlayActionResponse::failure(format!(
                        "Failed to parse replay: {requested_file}"
                    ));
                };
                parsed
            }
        };
        let file = replay.file.clone();

        OverlayInfoOps::emit_replay_to_overlay_from_replay(app, &replay, false);
        state.set_overlay_replay_data_active(true);
        state.set_current_replay_file(Some(&file));

        crate::OverlayActionResponse::success("Replay shown")
    }
}

impl OverlayInfoOps {
    pub fn replay_move_window(
        app: &tauri::AppHandle<Wry>,
        state: &BackendState,
        delta: i64,
    ) -> crate::OverlayActionResponse {
        let replay_data_active = state.overlay_replay_data_active();
        let replay = match OverlayInfoOps::replay_move_candidate_from_database(state, delta) {
            Ok(Some(replay)) => replay,
            Ok(None) if replay_data_active => {
                return crate::OverlayActionResponse::success("Replay move ignored");
            }
            Ok(None) => return crate::OverlayActionResponse::failure("No replays available"),
            Err(error) => return crate::OverlayActionResponse::failure(error),
        };
        let file = replay.file.clone();

        OverlayInfoOps::emit_replay_to_overlay_from_replay(app, &replay, false);
        state.set_overlay_replay_data_active(true);
        state.set_current_replay_file(Some(&file));

        crate::OverlayActionResponse::success("Replay moved")
    }
}
