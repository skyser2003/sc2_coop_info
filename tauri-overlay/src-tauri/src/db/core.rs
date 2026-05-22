use chrono::{Local, LocalResult, TimeZone, Utc};
use rusqlite::{Connection, Row};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CacheReplayEntry, ProtocolBuildValue,
};
use s2coop_analyzer::detailed_replay_analysis::{CacheEntrySink, CacheEntrySinkError};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::array_json::ReplayCacheArrayJson;
use crate::replay_analysis::ReplayAnalysisOps;

const CURRENT_SCHEMA_VERSION: i32 = 1;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheReadScope {
    All,
    DetailedOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayCacheEntryQuery {
    scope: ReplayCacheReadScope,
    limit: usize,
}

impl ReplayCacheEntryQuery {
    pub fn all(limit: usize) -> Self {
        Self {
            scope: ReplayCacheReadScope::All,
            limit,
        }
    }

    pub fn detailed_only(limit: usize) -> Self {
        Self {
            scope: ReplayCacheReadScope::DetailedOnly,
            limit,
        }
    }

    fn scope(&self) -> ReplayCacheReadScope {
        self.scope
    }

    fn limit(&self) -> usize {
        self.limit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReplayCacheTable {
    Players,
    PlayerUnits,
    PlayerIcons,
    PlayerIconOrders,
    Messages,
    AmonUnits,
    PlayerStatSeries,
}

pub(super) const REPLAY_CACHE_CHILD_TABLES: [ReplayCacheTable; 7] = [
    ReplayCacheTable::PlayerUnits,
    ReplayCacheTable::PlayerIconOrders,
    ReplayCacheTable::PlayerIcons,
    ReplayCacheTable::Messages,
    ReplayCacheTable::AmonUnits,
    ReplayCacheTable::PlayerStatSeries,
    ReplayCacheTable::Players,
];

impl ReplayCacheTable {
    pub(super) fn delete_by_replay_id_sql(self) -> &'static str {
        match self {
            Self::Players => "DELETE FROM replay_cache_players WHERE replay_id = ?1",
            Self::PlayerUnits => "DELETE FROM replay_cache_player_units WHERE replay_id = ?1",
            Self::PlayerIcons => "DELETE FROM replay_cache_player_icons WHERE replay_id = ?1",
            Self::PlayerIconOrders => {
                "DELETE FROM replay_cache_player_icon_orders WHERE replay_id = ?1"
            }
            Self::Messages => "DELETE FROM replay_cache_messages WHERE replay_id = ?1",
            Self::AmonUnits => "DELETE FROM replay_cache_amon_units WHERE replay_id = ?1",
            Self::PlayerStatSeries => {
                "DELETE FROM replay_cache_player_stat_series WHERE replay_id = ?1"
            }
        }
    }
}

const REPLAY_CACHE_ENTRY_RECORD_COLUMNS: &str = "
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
";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReplayCacheEntryRecordQuery {
    All,
    AllLimited,
    DetailedOnly,
    DetailedOnlyLimited,
}

impl ReplayCacheEntryRecordQuery {
    pub(super) fn from_entry_query(query: ReplayCacheEntryQuery) -> Self {
        match (query.scope(), query.limit()) {
            (ReplayCacheReadScope::All, 0) => Self::All,
            (ReplayCacheReadScope::All, _) => Self::AllLimited,
            (ReplayCacheReadScope::DetailedOnly, 0) => Self::DetailedOnly,
            (ReplayCacheReadScope::DetailedOnly, _) => Self::DetailedOnlyLimited,
        }
    }

    pub(super) fn limit(self, query: ReplayCacheEntryQuery) -> Option<i64> {
        match self {
            Self::All | Self::DetailedOnly => None,
            Self::AllLimited | Self::DetailedOnlyLimited => {
                Some(i64::try_from(query.limit()).unwrap_or(i64::MAX))
            }
        }
    }

    pub(super) fn sql(self) -> String {
        let filter = match self {
            Self::DetailedOnly | Self::DetailedOnlyLimited => "WHERE detailed_analysis = 1",
            Self::All | Self::AllLimited => "",
        };
        let limit = match self {
            Self::AllLimited | Self::DetailedOnlyLimited => "LIMIT ?1",
            Self::All | Self::DetailedOnly => "",
        };
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            {filter}
            ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
            {limit}
            "
        )
    }
}

pub(super) struct ReplayCacheEntrySql;

impl ReplayCacheEntrySql {
    pub(super) const DELETE_ALL: &'static str = "DELETE FROM replay_cache_entries";
    pub(super) const DELETE_BY_FILE_EXCEPT_HASH: &'static str =
        "DELETE FROM replay_cache_entries WHERE file = ?1 AND hash <> ?2";
    pub(super) const SELECT_DETAILED_BY_HASH: &'static str =
        "SELECT detailed_analysis FROM replay_cache_entries WHERE hash = ?1";
    pub(super) const SELECT_ID_BY_HASH: &'static str =
        "SELECT id FROM replay_cache_entries WHERE hash = ?1";
    pub(super) const SELECT_BY_HASH: &'static str = "
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
    pub(super) const SELECT_BY_ID: &'static str = "
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
        WHERE id = ?1
    ";
    pub(super) const SELECT_IDS_PAGE: &'static str = "
        SELECT id FROM replay_cache_entries
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT ?1 OFFSET ?2
    ";
    pub(super) const SELECT_NEWER_IDS: &'static str = "
        SELECT id FROM replay_cache_entries
        WHERE
            date_seconds > ?1 OR
            (date_seconds = ?1 AND date_text > ?2) OR
            (date_seconds = ?1 AND date_text = ?2 AND file > ?3) OR
            (date_seconds = ?1 AND date_text = ?2 AND file = ?3 AND hash > ?4)
        ORDER BY date_seconds ASC, date_text ASC, file ASC, hash ASC
        LIMIT ?5 OFFSET ?6
    ";
    pub(super) const SELECT_OLDER_IDS: &'static str = "
        SELECT id FROM replay_cache_entries
        WHERE
            date_seconds < ?1 OR
            (date_seconds = ?1 AND date_text < ?2) OR
            (date_seconds = ?1 AND date_text = ?2 AND file < ?3) OR
            (date_seconds = ?1 AND date_text = ?2 AND file = ?3 AND hash < ?4)
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT ?5 OFFSET ?6
    ";
    pub(super) const SELECT_ID_BY_EXACT_FILE: &'static str =
        "SELECT id FROM replay_cache_entries WHERE file = ?1";
    pub(super) const SELECT_ID_BY_FILE_NAME: &'static str = "
        SELECT id FROM replay_cache_entries
        WHERE file_name = ?1
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT 1
    ";
    pub(super) const SELECT_LATEST_ID: &'static str = "
        SELECT id FROM replay_cache_entries
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT 1
    ";
    pub(super) const SELECT_FILES: &'static str = "SELECT file FROM replay_cache_entries";
}

#[derive(Debug)]
pub(super) struct ReplayCacheEntryRecord {
    pub(super) id: i64,
    pub(super) hash: String,
    pub(super) file: String,
    pub(super) file_name: String,
    pub(super) date_text: String,
    pub(super) date_seconds: u64,
    pub(super) detailed_analysis: bool,
    pub(super) result: String,
    pub(super) map_name: String,
    pub(super) difficulty_p1: String,
    pub(super) difficulty_p2: String,
    pub(super) ext_difficulty: String,
    pub(super) brutal_plus: u32,
    pub(super) extension: bool,
    pub(super) weekly: bool,
    pub(super) region: String,
    pub(super) length_ingame_seconds: u64,
    pub(super) length_realtime: CacheNumericValue,
    pub(super) form_length_realtime: String,
    pub(super) replay_build: u32,
    pub(super) protocol_build: ProtocolBuildValue,
    pub(super) comp: Option<String>,
    pub(super) enemy_race: Option<String>,
    pub(super) has_amon_units: bool,
    pub(super) has_bonus: bool,
    pub(super) has_player_stats: bool,
    pub(super) mutator_values: String,
    pub(super) bonus_values: String,
    pub(super) updated_at_seconds: u64,
}

impl ReplayCacheEntryRecord {
    pub(super) fn from_entry(entry: &CacheReplayEntry) -> Result<Self, ReplayCacheDbError> {
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

    pub(super) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
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

    pub(super) fn bool_to_i64(value: bool) -> i64 {
        if value { 1 } else { 0 }
    }

    pub(super) fn i64_to_bool(value: i64) -> bool {
        value != 0
    }

    pub(super) fn u64_to_i64(value: u64) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }

    pub(super) fn i64_to_u64(value: i64) -> u64 {
        u64::try_from(value).unwrap_or_default()
    }

    pub(super) fn i64_to_u32(value: i64) -> u32 {
        u32::try_from(value).unwrap_or_default()
    }

    pub(super) fn cache_numeric_columns(
        value: &CacheNumericValue,
    ) -> (&'static str, Option<i64>, Option<f64>) {
        match value {
            CacheNumericValue::Integer(value) => ("integer", Some(Self::u64_to_i64(*value)), None),
            CacheNumericValue::Float(value) => ("float", None, Some(*value)),
        }
    }

    pub(super) fn cache_numeric_from_columns(
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

    pub(super) fn protocol_build_columns(
        value: &ProtocolBuildValue,
    ) -> (&'static str, Option<i64>, Option<String>) {
        match value {
            ProtocolBuildValue::Int(value) => ("integer", Some(i64::from(*value)), None),
            ProtocolBuildValue::Str(value) => ("text", None, Some(value.clone())),
        }
    }

    pub(super) fn protocol_build_from_columns(
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
pub(super) struct ReplayCacheFileName {
    value: String,
}

impl ReplayCacheFileName {
    pub(super) fn from_replay_file(file: &str) -> Self {
        let value = file
            .rsplit(['/', '\\'])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(file)
            .to_string();
        Self { value }
    }

    pub(super) fn into_string(self) -> String {
        self.value
    }
}

pub struct ReplayCacheDatabase {
    pub(super) cache_path: PathBuf,
    pub(super) db_path: PathBuf,
    pub(super) connection: Connection,
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
}

impl ReplayCacheDatabase {
    pub fn db_path_for_cache_path(cache_path: &Path) -> PathBuf {
        cache_path.with_extension("sqlite3")
    }

    pub fn db_related_paths_for_cache_path(cache_path: &Path) -> Vec<PathBuf> {
        let db_path = Self::db_path_for_cache_path(cache_path);
        vec![
            db_path.clone(),
            Self::path_with_suffix(&db_path, "-wal"),
            Self::path_with_suffix(&db_path, "-shm"),
            Self::path_with_suffix(&db_path, "-journal"),
        ]
    }

    fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        PathBuf::from(value)
    }

    pub fn open_for_cache_path(cache_path: &Path) -> Result<Self, ReplayCacheDbError> {
        let db_path = Self::db_path_for_cache_path(cache_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ReplayCacheDbError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let db_existed = db_path.exists();
        let mut connection =
            Connection::open(&db_path).map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;

        Self::initialize_schema(&mut connection, &db_path)?;
        let mut database = Self {
            cache_path: cache_path.to_path_buf(),
            db_path,
            connection,
        };
        if let Err(error) = database.import_legacy_cache_if_needed(db_existed) {
            crate::sco_log!("[SCO/cache-db] legacy cache import skipped: {error}");
        }
        Ok(database)
    }

    fn initialize_schema(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let schema_version = connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;

        if schema_version > CURRENT_SCHEMA_VERSION {
            return Err(ReplayCacheDbError::UnsupportedSchema {
                path: db_path.to_path_buf(),
                version: schema_version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }

        Self::create_current_schema(connection, db_path)
    }

    fn create_current_schema(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS replay_cache_entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    hash TEXT UNIQUE NOT NULL,
                    file TEXT NOT NULL,
                    file_name TEXT NOT NULL,
                    date_text TEXT NOT NULL,
                    date_seconds INTEGER NOT NULL,
                    detailed_analysis INTEGER NOT NULL,
                    result TEXT NOT NULL,
                    map_name TEXT NOT NULL,
                    difficulty_p1 TEXT NOT NULL,
                    difficulty_p2 TEXT NOT NULL,
                    ext_difficulty TEXT NOT NULL,
                    brutal_plus INTEGER NOT NULL,
                    extension INTEGER NOT NULL,
                    weekly INTEGER NOT NULL,
                    region TEXT NOT NULL,
                    length_ingame_seconds INTEGER NOT NULL,
                    length_realtime_kind TEXT NOT NULL CHECK(length_realtime_kind IN ('integer', 'float')),
                    length_realtime_int INTEGER,
                    length_realtime_float REAL,
                    form_length_realtime TEXT NOT NULL,
                    replay_build INTEGER NOT NULL,
                    protocol_build_kind TEXT NOT NULL CHECK(protocol_build_kind IN ('integer', 'text')),
                    protocol_build_int INTEGER,
                    protocol_build_text TEXT,
                    comp TEXT,
                    enemy_race TEXT,
                    has_amon_units INTEGER NOT NULL,
                    has_bonus INTEGER NOT NULL,
                    has_player_stats INTEGER NOT NULL,
                    mutator_values TEXT NOT NULL CHECK(json_valid(mutator_values)),
                    bonus_values TEXT NOT NULL CHECK(json_valid(bonus_values)),
                    updated_at_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS replay_cache_players (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    apm INTEGER,
                    commander TEXT,
                    commander_level INTEGER,
                    commander_mastery_level INTEGER,
                    handle TEXT,
                    kills INTEGER,
                    name TEXT,
                    observer INTEGER,
                    prestige INTEGER,
                    prestige_name TEXT,
                    race TEXT,
                    result TEXT,
                    has_masteries INTEGER NOT NULL,
                    has_icons INTEGER NOT NULL,
                    has_units INTEGER NOT NULL,
                    mastery_values TEXT NOT NULL CHECK(json_valid(mastery_values)),
                    PRIMARY KEY (replay_id, pid)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_units (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    unit_name TEXT NOT NULL,
                    created_kind TEXT NOT NULL CHECK(created_kind IN ('count', 'hidden')),
                    created_count INTEGER,
                    lost_kind TEXT NOT NULL CHECK(lost_kind IN ('count', 'hidden')),
                    lost_count INTEGER,
                    kills INTEGER NOT NULL,
                    fraction REAL NOT NULL,
                    PRIMARY KEY (replay_id, pid, unit_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_icons (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    icon_name TEXT NOT NULL,
                    icon_kind TEXT NOT NULL CHECK(icon_kind IN ('count', 'order')),
                    count_value INTEGER,
                    PRIMARY KEY (replay_id, pid, icon_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_icon_orders (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    icon_name TEXT NOT NULL,
                    order_values TEXT NOT NULL CHECK(json_valid(order_values)),
                    PRIMARY KEY (replay_id, pid, icon_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_messages (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    message_index INTEGER NOT NULL,
                    player INTEGER NOT NULL,
                    time REAL NOT NULL,
                    text TEXT NOT NULL,
                    PRIMARY KEY (replay_id, message_index)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_amon_units (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    unit_name TEXT NOT NULL,
                    created_kind TEXT NOT NULL CHECK(created_kind IN ('count', 'hidden')),
                    created_count INTEGER,
                    created_hidden TEXT,
                    lost_kind TEXT NOT NULL CHECK(lost_kind IN ('count', 'hidden')),
                    lost_count INTEGER,
                    lost_hidden TEXT,
                    kills INTEGER NOT NULL,
                    fraction REAL NOT NULL,
                    PRIMARY KEY (replay_id, unit_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_stat_series (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    name TEXT NOT NULL,
                    supply_values TEXT NOT NULL CHECK(json_valid(supply_values)),
                    mining_values TEXT NOT NULL CHECK(json_valid(mining_values)),
                    army_values TEXT NOT NULL CHECK(json_valid(army_values)),
                    killed_values TEXT NOT NULL CHECK(json_valid(killed_values)),
                    PRIMARY KEY (replay_id, pid)
                );

                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_date
                    ON replay_cache_entries(date_seconds DESC, date_text DESC, file DESC, hash DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file
                    ON replay_cache_entries(file);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file_name
                    ON replay_cache_entries(file_name, date_seconds DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_detailed
                    ON replay_cache_entries(detailed_analysis, date_seconds DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_games_tab
                    ON replay_cache_entries(date_seconds DESC, result, difficulty_p1, difficulty_p2, map_name);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_handle
                    ON replay_cache_players(handle, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_commander
                    ON replay_cache_players(commander, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_player_units_unit
                    ON replay_cache_player_units(unit_name, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_amon_units_unit
                    ON replay_cache_amon_units(unit_name, replay_id);

                PRAGMA user_version = 1;
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })
    }

    fn import_legacy_cache_if_needed(
        &mut self,
        db_existed: bool,
    ) -> Result<(), ReplayCacheDbError> {
        let import_json = self.should_import_legacy_json(db_existed);
        if import_json {
            self.import_legacy_cache_file()?;
        }

        let temp_path = Self::temp_cache_path(&self.cache_path);
        if temp_path.exists() {
            self.import_temp_cache_file(&temp_path)?;
        }

        Ok(())
    }

    fn should_import_legacy_json(&self, db_existed: bool) -> bool {
        if !self.cache_path.exists() {
            return false;
        }
        if !db_existed {
            return true;
        }

        let Ok(json_modified) = self.cache_path.metadata().and_then(|meta| meta.modified()) else {
            return false;
        };
        let Ok(db_modified) = self.db_path.metadata().and_then(|meta| meta.modified()) else {
            return true;
        };
        json_modified > db_modified
    }

    fn temp_cache_path(cache_path: &Path) -> PathBuf {
        cache_path.with_extension("temp.jsonl")
    }

    pub fn import_legacy_cache_file(&mut self) -> Result<usize, ReplayCacheDbError> {
        let mut entries = self.read_legacy_cache_entries()?;
        Self::normalize_legacy_cache_dates_to_utc(&mut entries);
        let changed = self.upsert_entries_preserving_detailed(&entries)?;
        Self::remove_imported_legacy_file(&self.cache_path)?;
        Ok(changed)
    }

    fn import_temp_cache_file(&mut self, temp_path: &Path) -> Result<usize, ReplayCacheDbError> {
        let mut entries = Self::read_temp_cache_entries(temp_path)?;
        Self::normalize_legacy_cache_dates_to_utc(&mut entries);
        let changed = self.upsert_entries_preserving_detailed(&entries)?;
        Self::remove_imported_legacy_file(temp_path)?;
        Ok(changed)
    }

    fn remove_imported_legacy_file(path: &Path) -> Result<(), ReplayCacheDbError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ReplayCacheDbError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    fn read_legacy_cache_entries(&self) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let payload = std::fs::read(&self.cache_path).map_err(|source| ReplayCacheDbError::Io {
            path: self.cache_path.clone(),
            source,
        })?;
        serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).map_err(|source| {
            ReplayCacheDbError::Json {
                path: self.cache_path.clone(),
                source,
            }
        })
    }

    fn normalize_legacy_cache_dates_to_utc(entries: &mut [CacheReplayEntry]) {
        for entry in entries {
            if let Some(date_text) = Self::legacy_local_date_to_utc_text(&entry.date) {
                entry.date = date_text;
            }
        }
    }

    fn legacy_local_date_to_utc_text(value: &str) -> Option<String> {
        let parts = value
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 3 {
            return None;
        }

        let year = parts.first()?.parse::<i32>().ok()?;
        let month = parts.get(1)?.parse::<u32>().ok()?;
        let day = parts.get(2)?.parse::<u32>().ok()?;
        let hour = parts
            .get(3)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        let minute = parts
            .get(4)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        let second = parts
            .get(5)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);

        let local_datetime = match Local.with_ymd_and_hms(year, month, day, hour, minute, second) {
            LocalResult::Single(value) => value,
            LocalResult::Ambiguous(earliest, _) => earliest,
            LocalResult::None => return None,
        };
        Some(
            local_datetime
                .with_timezone(&Utc)
                .format("%Y:%m:%d:%H:%M:%S")
                .to_string(),
        )
    }

    fn read_temp_cache_entries(
        temp_path: &Path,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let content =
            std::fs::read_to_string(temp_path).map_err(|source| ReplayCacheDbError::Io {
                path: temp_path.to_path_buf(),
                source,
            })?;
        let mut entries = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<CacheReplayEntry>(trimmed) {
                Ok(entry) if !entry.hash.is_empty() => entries.push(entry),
                Ok(_) => {}
                Err(source) => {
                    crate::sco_log!(
                        "[SCO/cache-db] ignored malformed temp cache row '{}': {source}",
                        temp_path.display()
                    );
                }
            }
        }
        Ok(entries)
    }

    pub(super) fn count_value_columns(value: &CacheCountValue) -> (&'static str, Option<i64>) {
        match value {
            CacheCountValue::Count(value) => ("count", Some(*value)),
            CacheCountValue::Hidden(_) => ("hidden", None),
        }
    }

    pub(super) fn count_value_columns_with_hidden(
        value: &CacheCountValue,
    ) -> (&'static str, Option<i64>, Option<String>) {
        match value {
            CacheCountValue::Count(value) => ("count", Some(*value), None),
            CacheCountValue::Hidden(value) => ("hidden", None, Some(value.clone())),
        }
    }

    pub(super) fn count_value_from_kind_and_count(
        kind: String,
        count: Option<i64>,
    ) -> CacheCountValue {
        if kind == "hidden" {
            CacheCountValue::Hidden("-".to_string())
        } else {
            CacheCountValue::Count(count.unwrap_or_default())
        }
    }

    pub(super) fn count_value_from_columns(
        kind: String,
        count: Option<i64>,
        hidden: Option<String>,
    ) -> CacheCountValue {
        if kind == "hidden" {
            CacheCountValue::Hidden(hidden.unwrap_or_default())
        } else {
            CacheCountValue::Count(count.unwrap_or_default())
        }
    }

    pub(super) fn sqlite_error(&self, source: rusqlite::Error) -> ReplayCacheDbError {
        ReplayCacheDbError::Sqlite {
            path: self.db_path.clone(),
            source,
        }
    }

    pub(super) fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}
