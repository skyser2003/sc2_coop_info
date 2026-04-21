use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::path_manager;

use crate::app_settings::AppSettings;

fn logs_file_path() -> Option<PathBuf> {
    let path = path_manager::get_log_path();
    Some(path)
}

pub(crate) fn append_line(message: &str) -> Result<(), String> {
    let Some(path) = logs_file_path() else {
        return Ok(());
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;

    writeln!(file, "{message}")
        .map_err(|error| format!("failed to append {}: {error}", path.display()))
}

fn file_logging_enabled() -> bool {
    let settings = AppSettings::from_saved_file();
    settings.enable_logging()
}

pub(crate) fn append_line_if_enabled(message: &str) {
    if !file_logging_enabled() {
        return;
    }

    if let Err(error) = append_line(message) {
        eprintln!("[SCO/log] {error}");
    }
}

pub(crate) fn log_line(message: &str) {
    eprintln!("{message}");
    append_line_if_enabled(message);
}
