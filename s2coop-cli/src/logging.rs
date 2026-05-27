use std::io::Write;

use chrono::Local;

pub struct CliLoggingOps;

const DEFAULT_LOG_DIRECTIVE: &str = "info";
const DEFAULT_LOG_STYLE_DIRECTIVE: &str = "always";

impl CliLoggingOps {
    pub fn default_filter_directive() -> &'static str {
        DEFAULT_LOG_DIRECTIVE
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

    pub fn initialize_env_logger() {
        let mut builder = pretty_env_logger::formatted_timed_builder();
        builder.parse_filters(Self::default_filter_directive());
        builder.parse_write_style(Self::default_log_style_directive());
        builder.parse_default_env();
        builder.format(|formatter, record| {
            let display_timestamp = CliLoggingOps::display_timestamp();
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
            eprintln!("[s2coop-cli/log] failed to initialize logger: {error}");
        }
    }
}
