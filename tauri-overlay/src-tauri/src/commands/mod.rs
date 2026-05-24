// Tauri command macros require module-scope adapter functions; command bodies live on structs.
const DEFAULT_CONFIG_ROWS_PER_PAGE: usize = 20;

pub mod config;
pub mod replays;
pub mod stats;
pub mod system;
