use std::sync::{Arc, Mutex};

use crate::ReplayInfo;

pub struct ReplayState {
    selected_replay_file: Arc<Mutex<Option<String>>>,
}

impl ReplayState {
    pub fn new() -> Self {
        Self {
            selected_replay_file: Arc::new(Mutex::new(None)),
        }
    }

    pub fn sync_selected_replay_file_from_replays(&self, replays: &[ReplayInfo]) {
        let selected = replays.first().map(|replay| replay.file.clone());

        if let Ok(mut selected_file) = self.selected_replay_file.lock() {
            match selected_file.as_ref() {
                Some(current) if replays.iter().any(|replay| &replay.file == current) => {}
                _ => {
                    *selected_file = selected;
                }
            }
        }
    }

    pub fn get_current_replay_file(&self) -> Option<String> {
        self.selected_replay_file
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    pub fn set_current_replay_file(&self, filename: Option<&str>) {
        if let Ok(mut selected_file) = self.selected_replay_file.lock() {
            *selected_file = filename.map(ToString::to_string);
        }
    }

    pub fn set_current_replay_file_if_empty(&self, filename: Option<String>) {
        if let Ok(mut selected_file) = self.selected_replay_file.lock()
            && selected_file.is_none()
        {
            *selected_file = filename;
        }
    }

    pub fn clear_replay_cache_slots(&self) {
        if let Ok(mut selected_replay_file) = self.selected_replay_file.lock() {
            *selected_replay_file = None;
        }
    }
}
