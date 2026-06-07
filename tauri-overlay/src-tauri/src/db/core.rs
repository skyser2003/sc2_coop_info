use chrono::{Local, LocalResult, TimeZone, Utc};
use rusqlite::{Connection, ErrorCode, Row};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CacheReplayEntry, ProtocolBuildValue,
};
use s2coop_analyzer::detailed_replay_analysis::{
    CacheEntrySink, CacheEntrySinkError, CacheReplayCheck,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::array_json::ReplayCacheArrayJson;
use crate::replay_analysis::{ReplayAnalysis, ReplayAnalysisOps};

mod connection;
mod query_types;

pub use query_types::{
    REPLAY_CACHE_CHILD_TABLES, REPLAY_CACHE_ENTRY_RECORD_COLUMNS, ReplayCacheDifficultyFilter,
    ReplayCacheEntryQuery, ReplayCacheEntryRecordQuery, ReplayCacheGameSortKey,
    ReplayCacheGamesPageQuery, ReplayCachePage, ReplayCachePageResult, ReplayCachePlayerNote,
    ReplayCachePlayerSortKey, ReplayCachePlayersPageQuery, ReplayCacheReadScope,
    ReplayCacheSortDirection, ReplayCacheStatisticsPayload, ReplayCacheStatsDifficultyExclusion,
    ReplayCacheStatsQuery,
};

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(60);
const SQLITE_LOCK_RETRY_WINDOW: Duration = Duration::from_secs(120);
const SQLITE_LOCK_RETRY_DELAY: Duration = Duration::from_millis(100);
pub const REPLAY_CACHE_QUERY_BATCH_SIZE: usize = 900;

pub struct ReplayCacheSqlBatch;

impl ReplayCacheSqlBatch {
    pub fn chunks<T>(values: &[T]) -> impl Iterator<Item = &[T]> {
        values
            .chunks(REPLAY_CACHE_QUERY_BATCH_SIZE)
            .filter(|chunk| !chunk.is_empty())
    }

    pub fn in_placeholders(value_count: usize) -> String {
        Self::repeat_placeholder("?", value_count)
    }

    pub fn values_placeholders(value_count: usize) -> String {
        Self::repeat_placeholder("(?)", value_count)
    }

    fn repeat_placeholder(placeholder: &'static str, value_count: usize) -> String {
        std::iter::repeat_n(placeholder, value_count)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub struct ReplayCacheStatsFactOps;

impl ReplayCacheStatsFactOps {
    pub fn normalized_commander_name(commander: &str) -> String {
        let trimmed = commander.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "abathur" => "Abathur",
            "alarak" => "Alarak",
            "artanis" => "Artanis",
            "dehaka" => "Dehaka",
            "fenix" => "Fenix",
            "han & horner" | "han and horner" | "hanhorner" => "Han & Horner",
            "karax" => "Karax",
            "kerrigan" => "Kerrigan",
            "mengsk" | "arcturus mengsk" => "Mengsk",
            "nova" => "Nova",
            "raynor" => "Raynor",
            "stukov" => "Stukov",
            "swann" => "Swann",
            "tychus" => "Tychus",
            "vorazun" => "Vorazun",
            "zagara" => "Zagara",
            "zeratul" => "Zeratul",
            "stetmann" => "Stetmann",
            _ => trimmed,
        }
        .to_string()
    }

    pub fn normalized_handle_key(handle: &str) -> String {
        ReplayAnalysis::normalized_handle_key(handle)
    }

    pub fn normalized_handle_key_sql(column: &str) -> String {
        format!(
            "
            CASE
                WHEN INSTR(LOWER(TRIM(COALESCE({column}, ''))), '-s2-') > 0
                THEN LOWER(TRIM(COALESCE({column}, '')))
                ELSE ''
            END
            "
        )
    }

    pub fn normalized_commander_sql(column: &str) -> String {
        format!(
            "
            CASE LOWER(TRIM(COALESCE({column}, '')))
                WHEN 'abathur' THEN 'Abathur'
                WHEN 'alarak' THEN 'Alarak'
                WHEN 'artanis' THEN 'Artanis'
                WHEN 'dehaka' THEN 'Dehaka'
                WHEN 'fenix' THEN 'Fenix'
                WHEN 'han & horner' THEN 'Han & Horner'
                WHEN 'han and horner' THEN 'Han & Horner'
                WHEN 'hanhorner' THEN 'Han & Horner'
                WHEN 'karax' THEN 'Karax'
                WHEN 'kerrigan' THEN 'Kerrigan'
                WHEN 'mengsk' THEN 'Mengsk'
                WHEN 'arcturus mengsk' THEN 'Mengsk'
                WHEN 'nova' THEN 'Nova'
                WHEN 'raynor' THEN 'Raynor'
                WHEN 'stukov' THEN 'Stukov'
                WHEN 'swann' THEN 'Swann'
                WHEN 'tychus' THEN 'Tychus'
                WHEN 'vorazun' THEN 'Vorazun'
                WHEN 'zagara' THEN 'Zagara'
                WHEN 'zeratul' THEN 'Zeratul'
                WHEN 'stetmann' THEN 'Stetmann'
                ELSE TRIM(COALESCE({column}, ''))
            END
            "
        )
    }

    pub fn unit_count_fact_columns(value: &CacheCountValue) -> (i64, i64) {
        match value {
            CacheCountValue::Count(value) => (0, *value),
            CacheCountValue::Hidden(_) => (1, 0),
        }
    }
}

#[derive(Debug)]
pub enum ReplayCacheDbError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    JsonArray {
        context: &'static str,
        source: serde_json::Error,
    },
    Sqlite {
        path: PathBuf,
        source: rusqlite::Error,
    },
    UnsupportedSchema {
        path: PathBuf,
        version: i32,
        supported: i32,
    },
}

impl Display for ReplayCacheDbError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "cache database io error '{}': {source}",
                    path.display()
                )
            }
            Self::Json { path, source } => {
                write!(
                    formatter,
                    "cache database json import error '{}': {source}",
                    path.display()
                )
            }
            Self::JsonArray { context, source } => {
                write!(
                    formatter,
                    "cache database json array error '{context}': {source}"
                )
            }
            Self::Sqlite { path, source } => {
                write!(
                    formatter,
                    "cache database sqlite error '{}': {source}",
                    path.display()
                )
            }
            Self::UnsupportedSchema {
                path,
                version,
                supported,
            } => {
                write!(
                    formatter,
                    "cache database '{}' has unsupported schema version {version}; supported version is {supported}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ReplayCacheDbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::JsonArray { source, .. } => Some(source),
            Self::Sqlite { source, .. } => Some(source),
            Self::UnsupportedSchema { .. } => None,
        }
    }
}

impl ReplayCacheDbError {
    pub fn is_sqlite_lock(&self) -> bool {
        match self {
            Self::Sqlite { source, .. } => matches!(
                source.sqlite_error_code(),
                Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
            ),
            _ => false,
        }
    }
}

pub struct ReplayCacheEntrySql;

impl ReplayCacheEntrySql {
    pub const DELETE_ALL: &'static str = "DELETE FROM replay_cache_entries";
    pub const DELETE_BY_FILE_EXCEPT_HASH: &'static str =
        "DELETE FROM replay_cache_entries WHERE file = ?1 AND hash <> ?2";
    pub const SELECT_BY_HASH: &'static str = "
        SELECT
            id,
            hash,
            file,
            file_name,
            date_text,
            date_seconds,
            detailed_analysis,
            result,
            map_name,
            difficulty_p1,
            difficulty_p2,
            ext_difficulty,
            brutal_plus,
            extension,
            weekly,
            region,
            length_ingame_seconds,
            length_realtime_kind,
            length_realtime_int,
            length_realtime_float,
            form_length_realtime,
            replay_build,
            protocol_build_kind,
            protocol_build_int,
            protocol_build_text,
            comp,
            enemy_race,
            has_amon_units,
            has_bonus,
            has_player_stats,
            mutator_values,
            bonus_values,
            updated_at_seconds
        FROM replay_cache_entries
        WHERE hash = ?1
    ";
    pub const SELECT_FILES: &'static str = "SELECT file FROM replay_cache_entries";
}

#[derive(Debug)]
pub struct ReplayCacheEntryRecord {
    pub id: i64,
    pub hash: String,
    pub file: String,
    pub file_name: String,
    pub date_text: String,
    pub date_seconds: u64,
    pub detailed_analysis: bool,
    pub result: String,
    pub map_name: String,
    pub difficulty_p1: String,
    pub difficulty_p2: String,
    pub ext_difficulty: String,
    pub brutal_plus: u32,
    pub extension: bool,
    pub weekly: bool,
    pub region: String,
    pub length_ingame_seconds: u64,
    pub length_realtime: CacheNumericValue,
    pub form_length_realtime: String,
    pub replay_build: u32,
    pub protocol_build: ProtocolBuildValue,
    pub comp: Option<String>,
    pub enemy_race: Option<String>,
    pub has_amon_units: bool,
    pub has_bonus: bool,
    pub has_player_stats: bool,
    pub mutator_values: String,
    pub bonus_values: String,
    pub updated_at_seconds: u64,
}

impl ReplayCacheEntryRecord {
    pub fn from_entry(entry: &CacheReplayEntry) -> Result<Self, ReplayCacheDbError> {
        let date_seconds =
            ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date).unwrap_or(0);
        Ok(Self {
            id: 0,
            hash: entry.hash.clone(),
            file: entry.file.clone(),
            file_name: ReplayCacheFileName::from_replay_file(&entry.file).into_string(),
            date_text: entry.date.clone(),
            date_seconds,
            detailed_analysis: entry.detailed_analysis,
            result: entry.result.clone(),
            map_name: entry.map_name.clone(),
            difficulty_p1: entry.difficulty.0.clone(),
            difficulty_p2: entry.difficulty.1.clone(),
            ext_difficulty: entry.ext_difficulty.clone(),
            brutal_plus: entry.brutal_plus,
            extension: entry.extension,
            weekly: entry.weekly,
            region: entry.region.clone(),
            length_ingame_seconds: entry.length,
            length_realtime: entry.accurate_length.clone(),
            form_length_realtime: entry.form_alength.clone(),
            replay_build: entry.build.replay_build(),
            protocol_build: entry.build.protocol_build().clone(),
            comp: entry.comp.clone(),
            enemy_race: entry.enemy_race.clone(),
            has_amon_units: entry.amon_units.is_some(),
            has_bonus: entry.bonus.is_some(),
            has_player_stats: entry.player_stats.is_some(),
            mutator_values: ReplayCacheArrayJson::encode_strings(&entry.mutators)?,
            bonus_values: ReplayCacheArrayJson::encode_strings(
                entry.bonus.as_deref().unwrap_or(&[]),
            )?,
            updated_at_seconds: ReplayCacheDatabase::now_seconds(),
        })
    }

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            hash: row.get("hash")?,
            file: row.get("file")?,
            file_name: row.get("file_name")?,
            date_text: row.get("date_text")?,
            date_seconds: Self::i64_to_u64(row.get("date_seconds")?),
            detailed_analysis: Self::i64_to_bool(row.get("detailed_analysis")?),
            result: row.get("result")?,
            map_name: row.get("map_name")?,
            difficulty_p1: row.get("difficulty_p1")?,
            difficulty_p2: row.get("difficulty_p2")?,
            ext_difficulty: row.get("ext_difficulty")?,
            brutal_plus: Self::i64_to_u32(row.get("brutal_plus")?),
            extension: Self::i64_to_bool(row.get("extension")?),
            weekly: Self::i64_to_bool(row.get("weekly")?),
            region: row.get("region")?,
            length_ingame_seconds: Self::i64_to_u64(row.get("length_ingame_seconds")?),
            length_realtime: Self::cache_numeric_from_columns(
                row.get::<_, String>("length_realtime_kind")?.as_str(),
                row.get("length_realtime_int")?,
                row.get("length_realtime_float")?,
            ),
            form_length_realtime: row.get("form_length_realtime")?,
            replay_build: Self::i64_to_u32(row.get("replay_build")?),
            protocol_build: Self::protocol_build_from_columns(
                row.get::<_, String>("protocol_build_kind")?.as_str(),
                row.get("protocol_build_int")?,
                row.get("protocol_build_text")?,
            ),
            comp: row.get("comp")?,
            enemy_race: row.get("enemy_race")?,
            has_amon_units: Self::i64_to_bool(row.get("has_amon_units")?),
            has_bonus: Self::i64_to_bool(row.get("has_bonus")?),
            has_player_stats: Self::i64_to_bool(row.get("has_player_stats")?),
            mutator_values: row.get("mutator_values")?,
            bonus_values: row.get("bonus_values")?,
            updated_at_seconds: Self::i64_to_u64(row.get("updated_at_seconds")?),
        })
    }

    pub fn bool_to_i64(value: bool) -> i64 {
        if value { 1 } else { 0 }
    }

    pub fn i64_to_bool(value: i64) -> bool {
        value != 0
    }

    pub fn u64_to_i64(value: u64) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }

    pub fn i64_to_u64(value: i64) -> u64 {
        u64::try_from(value).unwrap_or_default()
    }

    pub fn i64_to_u32(value: i64) -> u32 {
        u32::try_from(value).unwrap_or_default()
    }

    pub fn cache_numeric_columns(
        value: &CacheNumericValue,
    ) -> (&'static str, Option<i64>, Option<f64>) {
        match value {
            CacheNumericValue::Integer(value) => ("integer", Some(Self::u64_to_i64(*value)), None),
            CacheNumericValue::Float(value) => ("float", None, Some(*value)),
        }
    }

    pub fn cache_numeric_from_columns(
        kind: &str,
        int_value: Option<i64>,
        float_value: Option<f64>,
    ) -> CacheNumericValue {
        if kind == "float" {
            CacheNumericValue::Float(
                float_value.unwrap_or_else(|| int_value.map_or(0.0, |value| value as f64)),
            )
        } else {
            CacheNumericValue::Integer(int_value.map(Self::i64_to_u64).unwrap_or_default())
        }
    }

    pub fn protocol_build_columns(
        value: &ProtocolBuildValue,
    ) -> (&'static str, Option<i64>, Option<String>) {
        match value {
            ProtocolBuildValue::Int(value) => ("integer", Some(i64::from(*value)), None),
            ProtocolBuildValue::Str(value) => ("text", None, Some(value.clone())),
        }
    }

    pub fn protocol_build_from_columns(
        kind: &str,
        int_value: Option<i64>,
        text_value: Option<String>,
    ) -> ProtocolBuildValue {
        if kind == "text" {
            ProtocolBuildValue::Str(text_value.unwrap_or_default())
        } else {
            ProtocolBuildValue::Int(int_value.map(Self::i64_to_u32).unwrap_or_default())
        }
    }
}

#[derive(Debug)]
pub struct ReplayCacheFileName {
    value: String,
}

impl ReplayCacheFileName {
    pub fn from_replay_file(file: &str) -> Self {
        let value = file
            .rsplit(['/', '\\'])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(file)
            .to_string();
        Self { value }
    }

    pub fn into_string(self) -> String {
        self.value
    }
}

pub struct ReplayCacheDatabase {
    pub cache_path: PathBuf,
    legacy_cache_path: PathBuf,
    pub db_path: PathBuf,
    pub connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteReplayCacheEntrySink {
    cache_path: PathBuf,
}

impl SqliteReplayCacheEntrySink {
    pub fn new(cache_path: impl Into<PathBuf>) -> Self {
        Self {
            cache_path: cache_path.into(),
        }
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }
}

impl CacheEntrySink for SqliteReplayCacheEntrySink {
    fn write_entries(&self, entries: &[CacheReplayEntry]) -> Result<usize, CacheEntrySinkError> {
        ReplayCacheDatabase::open_for_cache_path(&self.cache_path)
            .and_then(|mut database| database.upsert_entries_preserving_detailed(entries))
            .map_err(|error| CacheEntrySinkError::new(error.to_string()))
    }

    fn write_checks(&self, checks: &[CacheReplayCheck]) -> Result<usize, CacheEntrySinkError> {
        ReplayCacheDatabase::open_for_cache_path(&self.cache_path)
            .and_then(|mut database| database.upsert_unsaved_replay_checks(checks))
            .map_err(|error| CacheEntrySinkError::new(error.to_string()))
    }
}
