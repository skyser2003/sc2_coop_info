use log::Level;
use sco_tauri_overlay::TestHelperOps;
use sco_tauri_overlay::{AppSettings, LoggingOps, TauriOverlayOps};
use serde_json::Value;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

struct LoggingTestDir {
    path: PathBuf,
}

impl LoggingTestDir {
    fn new(name: &str) -> Self {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "sco-tauri-overlay-{name}-{}-{now_nanos}",
            process::id()
        ));
        fs::create_dir_all(&path).expect("test log directory should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LoggingTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn sanitize_settings_value_removes_deleted_overlay_settings() {
    let sanitized = AppSettings::sanitize_settings_value(json!({
        "enable_logging": true,
        "fast_expand": true,
        "force_hide_overlay": true,
        "show_session": true,
    }));

    assert_eq!(
        sanitized.get("enable_logging").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        sanitized.get("show_session").and_then(Value::as_bool),
        Some(true)
    );
    assert!(sanitized.get("fast_expand").is_none());
    assert!(sanitized.get("force_hide_overlay").is_none());
}

fn logs_file_path_from_settings_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("logs.txt")
}

#[test]
fn logging_setting_respects_boolean_flag() {
    assert!(
        AppSettings::merge_settings_with_defaults(json!({
            "enable_logging": true,
        }))
        .enable_logging()
    );
    assert!(
        !AppSettings::merge_settings_with_defaults(json!({
            "enable_logging": false,
        }))
        .enable_logging()
    );
    assert!(AppSettings::merge_settings_with_defaults(json!({})).enable_logging());
}

#[test]
fn pretty_env_logger_defaults_to_trace_for_development_info_for_deployment_and_color() {
    assert_eq!(LoggingOps::default_filter_directive_for(true), "trace");
    assert_eq!(LoggingOps::default_filter_directive_for(false), "info");
    assert_eq!(LoggingOps::default_log_style_directive(), "always");
}

#[test]
fn formatted_log_line_includes_level_target_and_message() {
    let line =
        LoggingOps::format_record_line("2026-05-27T00:00:00.000Z", Level::Warn, "sco.test", "msg");

    assert_eq!(line, "2026-05-27T00:00:00.000Z WARN  sco.test - msg");
}

#[test]
fn logs_file_path_stays_next_to_settings_file() {
    let settings_path = TestHelperOps::test_config_path("settings.json");
    let path = logs_file_path_from_settings_path(Path::new(&settings_path));

    assert_eq!(path, TestHelperOps::test_config_path("logs.txt"));
}

#[test]
fn rolling_log_path_preserves_file_extension() {
    let log_path = PathBuf::from("logs.txt");

    assert_eq!(
        LoggingOps::rolling_log_file_path(&log_path, 1),
        PathBuf::from("logs.1.txt")
    );
    assert_eq!(
        LoggingOps::rolling_log_file_path(&log_path, 2),
        PathBuf::from("logs.2.txt")
    );
}

#[test]
fn append_line_rotates_current_log_when_write_would_exceed_limit() {
    let test_dir = LoggingTestDir::new("rotate-current");
    let log_path = test_dir.path().join("logs.txt");
    let archive_path = LoggingOps::rolling_log_file_path(&log_path, 1);

    fs::write(&log_path, "current\n").expect("current log should be seeded");
    OpenOptions::new()
        .write(true)
        .open(&log_path)
        .expect("current log should open for sizing")
        .set_len(LoggingOps::max_log_file_bytes())
        .expect("current log should resize");

    LoggingOps::append_line_to_path(&log_path, "next").expect("log append should rotate");

    assert_eq!(
        fs::read_to_string(&log_path).expect("current log should be readable"),
        "next\n"
    );
    assert_eq!(
        fs::metadata(&archive_path)
            .expect("first archive should exist")
            .len(),
        LoggingOps::max_log_file_bytes()
    );
}

#[test]
fn append_line_keeps_only_current_log_and_two_archives() {
    let test_dir = LoggingTestDir::new("retain-three");
    let log_path = test_dir.path().join("logs.txt");
    let first_archive_path = LoggingOps::rolling_log_file_path(&log_path, 1);
    let second_archive_path = LoggingOps::rolling_log_file_path(&log_path, 2);

    fs::write(&log_path, "current\n").expect("current log should be seeded");
    OpenOptions::new()
        .write(true)
        .open(&log_path)
        .expect("current log should open for sizing")
        .set_len(LoggingOps::max_log_file_bytes())
        .expect("current log should resize");
    fs::write(&first_archive_path, "previous\n").expect("first archive should be seeded");
    fs::write(&second_archive_path, "oldest\n").expect("second archive should be seeded");

    LoggingOps::append_line_to_path(&log_path, "next").expect("log append should rotate");

    assert_eq!(
        fs::read_to_string(&log_path).expect("current log should be readable"),
        "next\n"
    );
    assert_eq!(
        fs::read_to_string(&second_archive_path).expect("second archive should be readable"),
        "previous\n"
    );
    assert_eq!(LoggingOps::max_rolling_log_files(), 3);
    assert!(!LoggingOps::rolling_log_file_path(&log_path, 3).exists());
}

#[test]
fn session_counter_delta_only_tracks_victory_and_defeat() {
    assert_eq!(TauriOverlayOps::session_counter_delta("Victory"), (1, 0));
    assert_eq!(TauriOverlayOps::session_counter_delta("defeat"), (0, 1));
    assert_eq!(TauriOverlayOps::session_counter_delta("Unknown"), (0, 0));
}
