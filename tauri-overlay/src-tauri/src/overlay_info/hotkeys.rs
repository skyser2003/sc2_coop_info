use super::*;

impl OverlayInfoOps {
    fn is_valid_hotkey(shortcut: &str) -> bool {
        Shortcut::from_str(shortcut).is_ok()
    }
}

impl OverlayInfoOps {
    pub fn normalize_hotkey(raw: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let normalized: String = raw
            .chars()
            .filter(|value| !value.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();

        let mut blocked = false;
        let canonical = normalized
            .split('+')
            .filter(|token| !token.is_empty())
            .filter_map(|token| {
                let normalized_token = match token {
                    "backspace" | "delete" => {
                        blocked = true;
                        return None;
                    }
                    "control" => "control",
                    "ctrl" => "control",
                    "shift" => "shift",
                    "alt" => "alt",
                    "meta" => "super",
                    "super" => "super",
                    "cmd" => "super",
                    "command" => "super",
                    "win" => "super",
                    "windows" => "super",
                    "commandorcontrol" | "commandorctrl" | "cmdorcontrol" | "cmdorctrl" => {
                        #[cfg(target_os = "macos")]
                        {
                            "super"
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            "control"
                        }
                    }
                    "!" => "1",
                    "@" => "2",
                    "#" => "3",
                    "$" => "4",
                    "%" => "5",
                    "^" => "6",
                    "&" => "7",
                    "*" => "8",
                    "(" => "9",
                    ")" => "0",
                    "_" => "-",
                    "plus" => "=",
                    "+" => "=",
                    "asterisk" => "8",
                    "{" => "[",
                    "}" => "]",
                    "|" => "\\",
                    ":" => ";",
                    "\"" => "'",
                    "<" => ",",
                    ">" => ".",
                    "?" => "/",
                    "~" => "`",
                    other => other,
                };
                Some(normalized_token)
            })
            .collect::<Vec<&str>>()
            .join("+");

        if blocked {
            crate::sco_warn!("[SCO/hotkey] Backspace/Delete cannot be used as global hotkey");
            return None;
        }

        if OverlayInfoOps::is_valid_hotkey(&canonical) {
            return Some(canonical);
        }

        crate::sco_warn!("[SCO/hotkey] Ignoring invalid hotkey '{raw}'");
        None
    }
}

impl OverlayInfoOps {
    fn register_shortcut_action(
        app_handle: &tauri::AppHandle<Wry>,
        shortcut: &Shortcut,
        action: &'static str,
        event_state: ShortcutState,
    ) {
        if event_state != ShortcutState::Pressed {
            return;
        }

        let pressed = shortcut.into_string().to_ascii_lowercase();
        crate::sco_debug!("[SCO/hotkey] Triggered shortcut '{pressed}' => '{action}'");

        match action {
            "overlay_newer" | "overlay_older" | "overlay_player_stats" => {
                let state = app_handle.state::<BackendState>();
                if !state.try_begin_hotkey_action() {
                    crate::sco_debug!(
                        "[SCO/hotkey] Ignoring '{pressed}' because another hotkey action is running"
                    );
                    return;
                }
                let action_name = action.to_string();
                let app_handle = app_handle.clone();
                thread::spawn(move || {
                    let state = app_handle.state::<BackendState>();
                    let _ = OverlayInfoOps::perform_overlay_action(
                        &app_handle,
                        &state,
                        &action_name,
                        None,
                    );
                    state.finish_hotkey_action();
                });
            }
            _ => {
                let state = app_handle.state::<BackendState>();
                let _ = OverlayInfoOps::perform_overlay_action(app_handle, &state, action, None);
            }
        }
    }
}

impl OverlayInfoOps {
    fn register_hotkey_binding(
        app: &tauri::AppHandle<Wry>,
        binding: &ResolvedHotkeyBinding,
    ) -> Result<(), String> {
        let parsed = Shortcut::from_str(binding.shortcut())
            .map_err(|error| format!("Failed to parse hotkey '{}': {error}", binding.shortcut()))?;
        let action = binding.action();
        app.global_shortcut()
            .on_shortcut(parsed, move |app_handle, shortcut, event| {
                OverlayInfoOps::register_shortcut_action(app_handle, shortcut, action, event.state);
            })
            .map_err(|error| {
                format!(
                    "Failed to register hotkey '{}': {error}",
                    binding.shortcut()
                )
            })
    }
}

impl OverlayInfoOps {
    fn unregister_hotkey_binding(
        app: &tauri::AppHandle<Wry>,
        binding: &ResolvedHotkeyBinding,
    ) -> Result<(), String> {
        let parsed = Shortcut::from_str(binding.shortcut())
            .map_err(|error| format!("Failed to parse hotkey '{}': {error}", binding.shortcut()))?;
        if !app.global_shortcut().is_registered(parsed) {
            return Ok(());
        }
        app.global_shortcut().unregister(parsed).map_err(|error| {
            format!(
                "Failed to unregister hotkey '{}': {error}",
                binding.shortcut()
            )
        })
    }
}

impl OverlayInfoOps {
    pub fn register_overlay_hotkeys(app: &tauri::AppHandle<Wry>) -> Result<(), String> {
        let _ = app.global_shortcut().unregister_all();
        let state = app.state::<BackendState>();

        let active_reassign_path = state.active_hotkey_reassign_path();
        let mut registered: HashMap<String, &'static str> = HashMap::new();
        let mut registered_count = 0usize;

        for binding in state.resolved_overlay_hotkey_bindings() {
            if active_reassign_path.as_deref() == Some(binding.path()) {
                crate::sco_debug!(
                    "[SCO/hotkey] Skipping '{}' because it is currently being reassigned",
                    binding.path()
                );
                continue;
            }
            if let Some(existing_action) = registered.get(binding.canonical()) {
                if *existing_action == binding.action() {
                    crate::sco_debug!(
                        "[SCO/hotkey] Duplicate hotkey '{}' for '{}' ignored.",
                        binding.canonical(),
                        binding.action()
                    );
                } else {
                    crate::sco_warn!(
                        "[SCO/hotkey] Hotkey '{}' already bound to '{}', skipping '{}'.",
                        binding.canonical(),
                        existing_action,
                        binding.action()
                    );
                }
                continue;
            }
            crate::sco_debug!(
                "[SCO/hotkey] Registering '{}' for '{}'",
                binding.shortcut(),
                binding.action()
            );
            OverlayInfoOps::register_hotkey_binding(app, &binding)?;
            registered.insert(binding.canonical().to_string(), binding.action());
            registered_count += 1;
        }

        if registered_count == 0 {
            crate::sco_info!("[SCO/hotkey] No overlay hotkeys configured.");
        }

        Ok(())
    }
}

impl OverlayInfoOps {
    pub fn begin_hotkey_reassign(app: &tauri::AppHandle<Wry>, path: &str) -> Result<(), String> {
        let state = app.state::<BackendState>();
        if let Some(previous_path) = state.active_hotkey_reassign_path()
            && previous_path != path
        {
            OverlayInfoOps::end_hotkey_reassign(app, &previous_path)?;
        }

        state.set_active_hotkey_reassign_path(Some(path.to_string()));
        let binding = state
            .resolved_overlay_hotkey_bindings()
            .into_iter()
            .find(|binding| binding.path() == path);
        state.set_active_hotkey_reassign_binding(binding.clone());

        if let Some(binding) = binding {
            OverlayInfoOps::unregister_hotkey_binding(app, &binding)?;
            crate::sco_debug!(
                "[SCO/hotkey] Removed hotkey trigger for '{}' while it is being reassigned",
                path
            );
        }

        Ok(())
    }
}

impl OverlayInfoOps {
    pub fn end_hotkey_reassign(app: &tauri::AppHandle<Wry>, path: &str) -> Result<(), String> {
        let state = app.state::<BackendState>();
        if state.active_hotkey_reassign_path().as_deref() == Some(path) {
            state.set_active_hotkey_reassign_path(None);
        }

        let settings_value = state.read_settings_memory();
        let fallback_binding = state.active_hotkey_reassign_binding();
        let Some(binding) =
            settings_value.hotkey_binding_for_reassign_end(path, fallback_binding.as_ref())
        else {
            state.set_active_hotkey_reassign_binding(None);
            crate::sco_warn!("[SCO/hotkey] '{path}' has no active binding after reassignment");
            return Ok(());
        };

        let bindings = settings_value.resolved_overlay_hotkey_bindings();
        if bindings
            .iter()
            .any(|other| other.path() != binding.path() && other.canonical() == binding.canonical())
        {
            state.set_active_hotkey_reassign_binding(None);
            crate::sco_warn!(
                "[SCO/hotkey] Hotkey '{}' conflicts with another binding, skipping '{}'.",
                binding.canonical(),
                binding.path()
            );
            return Ok(());
        }

        OverlayInfoOps::register_hotkey_binding(app, &binding)?;
        state.set_active_hotkey_reassign_binding(None);
        crate::sco_debug!(
            "[SCO/hotkey] Recreated hotkey trigger for '{}' as '{}'",
            path,
            binding.shortcut()
        );
        Ok(())
    }
}
