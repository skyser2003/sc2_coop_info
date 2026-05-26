use std::sync::{Arc, Mutex};

pub struct ReplayState {
    selected_replay_file: Arc<Mutex<Option<String>>>,
}

impl ReplayState {
    pub fn new() -> Self {
        Self {
            selected_replay_file: Arc::new(Mutex::new(None)),
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

    pub fn clear_selected_replay_file(&self) {
        if let Ok(mut selected_replay_file) = self.selected_replay_file.lock() {
            *selected_replay_file = None;
        }
    }
}
