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

    pub fn initialize_env_logger() {
        let mut builder = pretty_env_logger::formatted_timed_builder();
        builder.parse_filters(Self::default_filter_directive());
        builder.parse_write_style(Self::default_log_style_directive());
        builder.parse_default_env();
        if let Err(error) = builder.try_init() {
            eprintln!("[s2coop-cli/log] failed to initialize logger: {error}");
        }
    }
}
