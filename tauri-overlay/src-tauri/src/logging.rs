use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use log::{Level, Record};

use crate::app_settings::AppSettings;
use crate::path_manager::PathManagerOps;

pub struct LoggingOps;

const MAX_LOG_FILE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ROLLING_LOG_FILES: usize = 3;
const DEVELOPMENT_LOG_DIRECTIVE: &str = "trace";
const DEPLOYMENT_LOG_DIRECTIVE: &str = "info";
const DEFAULT_LOG_STYLE_DIRECTIVE: &str = "always";

impl LoggingOps {
    fn logs_file_path() -> PathBuf {
        PathManagerOps::get_log_path()
    }
}

impl LoggingOps {
    pub fn max_log_file_bytes() -> u64 {
        MAX_LOG_FILE_BYTES
    }
}

impl LoggingOps {
    pub fn max_rolling_log_files() -> usize {
        MAX_ROLLING_LOG_FILES
    }
}

impl LoggingOps {
    pub fn default_filter_directive() -> &'static str {
        LoggingOps::default_filter_directive_for(PathManagerOps::is_dev_env())
    }

    pub fn default_filter_directive_for(development_environment: bool) -> &'static str {
        if development_environment {
            DEVELOPMENT_LOG_DIRECTIVE
        } else {
            DEPLOYMENT_LOG_DIRECTIVE
        }
    }

    pub fn default_log_style_directive() -> &'static str {
        DEFAULT_LOG_STYLE_DIRECTIVE
    }

    pub fn format_display_time(hour: u32, minute: u32, second: u32) -> String {
        format!("{hour:02}:{minute:02}:{second:02}")
    }

    fn display_timestamp() -> String {
        Local::now().format("%H:%M:%S").to_string()
    }
}

impl LoggingOps {
    pub fn initialize_env_logger() {
        let mut builder = pretty_env_logger::formatted_timed_builder();
        builder.parse_filters(LoggingOps::default_filter_directive());
        builder.parse_write_style(LoggingOps::default_log_style_directive());
        builder.parse_default_env();
        builder.format(|formatter, record| {
            let timestamp = formatter.timestamp_millis().to_string();
            let line = LoggingOps::format_log_record(&timestamp, record);
            LoggingOps::append_line_if_enabled(&line);
            let display_timestamp = LoggingOps::display_timestamp();

            let level_style = formatter.default_level_style(record.level());
            let level_text = record.level().to_string();
            let mut target_style = formatter.style();
            target_style.set_bold(true);

            writeln!(
                formatter,
                "{display_timestamp} {:<5} {} - {}",
                level_style.value(level_text),
                target_style.value(record.target()),
                record.args()
            )
        });

        if let Err(error) = builder.try_init() {
            eprintln!("[SCO/log] failed to initialize logger: {error}");
        }
    }
}

impl LoggingOps {
    pub fn format_record_line(
        timestamp: &str,
        level: Level,
        target: &str,
        message: &str,
    ) -> String {
        let level_text = level.to_string();
        format!("{timestamp} {level_text:<5} {target} - {message}")
    }

    fn format_log_record(timestamp: &str, record: &Record<'_>) -> String {
        LoggingOps::format_record_line(
            timestamp,
            record.level(),
            record.target(),
            &record.args().to_string(),
        )
    }
}

impl LoggingOps {
    pub fn rolling_log_file_path(path: &Path, index: usize) -> PathBuf {
        if index == 0 {
            return path.to_path_buf();
        }

        let Some(file_name) = path.file_name() else {
            return path.to_path_buf();
        };

        let mut rolling_name = OsString::new();
        if let Some(stem) = path.file_stem() {
            rolling_name.push(stem);
        } else {
            rolling_name.push(file_name);
        }
        rolling_name.push(format!(".{index}"));

        if let Some(extension) = path.extension() {
            rolling_name.push(".");
            rolling_name.push(extension);
        }

        path.with_file_name(rolling_name)
    }
}

impl LoggingOps {
    fn remove_file_if_exists(path: &Path) -> Result<(), String> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
        }
    }
}

impl LoggingOps {
    fn rename_file_if_exists(source: &Path, target: &Path) -> Result<(), String> {
        if !source.exists() {
            return Ok(());
        }

        LoggingOps::remove_file_if_exists(target)?;

        match fs::rename(source, target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "failed to rotate {} to {}: {error}",
                source.display(),
                target.display()
            )),
        }
    }
}

impl LoggingOps {
    fn rotate_logs(path: &Path) -> Result<(), String> {
        let oldest_index = LoggingOps::max_rolling_log_files() - 1;
        let oldest_path = LoggingOps::rolling_log_file_path(path, oldest_index);
        LoggingOps::remove_file_if_exists(&oldest_path)?;

        for index in (1..oldest_index).rev() {
            let source = LoggingOps::rolling_log_file_path(path, index);
            let target = LoggingOps::rolling_log_file_path(path, index + 1);
            LoggingOps::rename_file_if_exists(&source, &target)?;
        }

        let first_archive = LoggingOps::rolling_log_file_path(path, 1);
        LoggingOps::rename_file_if_exists(path, &first_archive)
    }
}

impl LoggingOps {
    fn rotate_logs_if_needed(path: &Path, incoming_bytes: u64) -> Result<(), String> {
        let current_size = match fs::metadata(path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "failed to read log metadata {}: {error}",
                    path.display()
                ));
            }
        };

        if current_size.saturating_add(incoming_bytes) <= LoggingOps::max_log_file_bytes() {
            return Ok(());
        }

        LoggingOps::rotate_logs(path)
    }
}

impl LoggingOps {
    pub fn append_line(message: &str) -> Result<(), String> {
        let path = LoggingOps::logs_file_path();
        LoggingOps::append_line_to_path(&path, message)
    }
}

impl LoggingOps {
    pub fn append_line_to_path(path: &Path, message: &str) -> Result<(), String> {
        let incoming_bytes = u64::try_from(message.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        LoggingOps::rotate_logs_if_needed(path, incoming_bytes)?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("failed to open {}: {error}", path.display()))?;

        writeln!(file, "{message}")
            .map_err(|error| format!("failed to append {}: {error}", path.display()))
    }
}

impl LoggingOps {
    fn file_logging_enabled() -> bool {
        let settings = AppSettings::from_saved_file();
        settings.enable_logging()
    }
}

impl LoggingOps {
    pub fn append_line_if_enabled(message: &str) {
        if !LoggingOps::file_logging_enabled() {
            return;
        }

        if let Err(error) = LoggingOps::append_line(message) {
            eprintln!("[SCO/log] {error}");
        }
    }
}
