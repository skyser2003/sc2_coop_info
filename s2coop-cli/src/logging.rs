use env_logger::{Builder, Env};

pub struct CliLoggingOps;

const RUST_LOG_ENV_VAR: &str = "RUST_LOG";
const DEFAULT_LOG_DIRECTIVE: &str = "info";

impl CliLoggingOps {
    pub fn default_filter_directive() -> &'static str {
        DEFAULT_LOG_DIRECTIVE
    }

    pub fn initialize_env_logger() {
        let env = Env::default()
            .filter(RUST_LOG_ENV_VAR)
            .default_filter_or(Self::default_filter_directive());
        let mut builder = Builder::from_env(env);
        builder.format_timestamp_millis();
        if let Err(error) = builder.try_init() {
            eprintln!("[s2coop-cli/log] failed to initialize logger: {error}");
        }
    }
}
