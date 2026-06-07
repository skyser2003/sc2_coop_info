use crate::TauriOverlayOps;

use super::ReplayAnalysis;

impl ReplayAnalysis {
    pub fn normalized_player_key(name: &str) -> String {
        TauriOverlayOps::sanitize_replay_text(name)
            .trim()
            .to_ascii_lowercase()
    }

    pub fn normalized_handle_key(handle: &str) -> String {
        let normalized = TauriOverlayOps::sanitize_replay_text(handle)
            .trim()
            .to_ascii_lowercase();
        if normalized.contains("-s2-") {
            normalized
        } else {
            String::new()
        }
    }

    pub fn is_main_player_by_name(
        player_name: &str,
        main_names: &std::collections::HashSet<String>,
    ) -> bool {
        if main_names.is_empty() {
            return false;
        }
        let normalized = Self::normalized_player_key(player_name);
        !normalized.is_empty() && main_names.contains(&normalized)
    }

    pub fn is_main_player_by_handle(
        player_handle: &str,
        main_handles: &std::collections::HashSet<String>,
    ) -> bool {
        if main_handles.is_empty() {
            return false;
        }
        let normalized = Self::normalized_handle_key(player_handle);
        !normalized.is_empty() && main_handles.contains(&normalized)
    }

    pub fn is_main_player_identity(
        player_name: &str,
        player_handle: &str,
        main_names: &std::collections::HashSet<String>,
        main_handles: &std::collections::HashSet<String>,
    ) -> bool {
        Self::is_main_player_by_handle(player_handle, main_handles)
            || Self::is_main_player_by_name(player_name, main_names)
    }
}
