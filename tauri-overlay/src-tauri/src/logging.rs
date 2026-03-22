use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static FILE_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn refresh_from_settings(settings: &Value) {
    FILE_LOGGING_ENABLED.store(
        crate::logging_enabled_from_settings(settings),
        Ordering::Release,
    );
}

pub(crate) fn logs_file_path_from_settings_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("Logs.txt")
}

fn logs_file_path() -> Option<PathBuf> {
    let settings_path = crate::overlay_info::locate_settings_file()?;
    Some(logs_file_path_from_settings_path(&settings_path))
}

fn append_line(message: &str) -> Result<(), String> {
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

pub(crate) fn append_line_if_enabled(message: &str) {
    if !FILE_LOGGING_ENABLED.load(Ordering::Acquire) {
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
