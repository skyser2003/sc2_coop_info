use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::GenerateCacheError;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    Sqlite {
        path: PathBuf,
        source: rusqlite::Error,
    },
    GenerateCache {
        path: PathBuf,
        source: GenerateCacheError,
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
            Self::Sqlite { path, source } => {
                write!(
                    formatter,
                    "cache database sqlite error '{}': {source}",
                    path.display()
                )
            }
            Self::GenerateCache { path, source } => {
                write!(
                    formatter,
                    "cache database legacy export error '{}': {source}",
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
            Self::Sqlite { source, .. } => Some(source),
            Self::GenerateCache { source, .. } => Some(source),
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

#[derive(Debug)]
struct ReplayCacheEntryRecord {
    hash: String,
    file: String,
    file_name: String,
    date_text: String,
    date_seconds: u64,
    detailed_analysis: bool,
    result: String,
    map_name: String,
    difficulty: String,
    ext_difficulty: String,
    brutal_plus: u32,
    extension: bool,
    weekly: bool,
    region: String,
    payload_json: String,
    updated_at_seconds: u64,
}

impl ReplayCacheEntryRecord {
    fn from_entry(entry: &CacheReplayEntry) -> Result<Self, serde_json::Error> {
        let date_seconds =
            ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date).unwrap_or(0);
        Ok(Self {
            hash: entry.hash.clone(),
            file: entry.file.clone(),
            file_name: ReplayCacheFileName::from_replay_file(&entry.file).into_string(),
            date_text: entry.date.clone(),
            date_seconds,
            detailed_analysis: entry.detailed_analysis,
            result: entry.result.clone(),
            map_name: entry.map_name.clone(),
            difficulty: entry.difficulty.1.clone(),
            ext_difficulty: entry.ext_difficulty.clone(),
            brutal_plus: entry.brutal_plus,
            extension: entry.extension,
            weekly: entry.weekly,
            region: entry.region.clone(),
            payload_json: serde_json::to_string(entry)?,
            updated_at_seconds: ReplayCacheDatabase::now_seconds(),
        })
    }

    fn bool_to_i64(value: bool) -> i64 {
        if value { 1 } else { 0 }
    }

    fn u64_to_i64(value: u64) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }
}

#[derive(Debug)]
struct ReplayCacheFileName {
    value: String,
}

impl ReplayCacheFileName {
    fn from_replay_file(file: &str) -> Self {
        let value = file
            .rsplit(['/', '\\'])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(file)
            .to_string();
        Self { value }
    }

    fn into_string(self) -> String {
        self.value
    }
}

pub struct ReplayCacheDatabase {
    cache_path: PathBuf,
    db_path: PathBuf,
    connection: Connection,
}

impl ReplayCacheDatabase {
    pub fn db_path_for_cache_path(cache_path: &Path) -> PathBuf {
        cache_path.with_extension("sqlite3")
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

        Self::migrate_schema(&mut connection, &db_path)?;
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

    fn migrate_schema(
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

        if schema_version == 0 {
            connection
                .execute_batch(
                    "
                    CREATE TABLE IF NOT EXISTS replay_cache_metadata (
                        key TEXT PRIMARY KEY NOT NULL,
                        value TEXT NOT NULL
                    );

                    CREATE TABLE IF NOT EXISTS replay_cache_entries (
                        hash TEXT PRIMARY KEY NOT NULL,
                        file TEXT NOT NULL,
                        file_name TEXT NOT NULL,
                        date_text TEXT NOT NULL,
                        date_seconds INTEGER NOT NULL,
                        detailed_analysis INTEGER NOT NULL,
                        result TEXT NOT NULL,
                        map_name TEXT NOT NULL,
                        difficulty TEXT NOT NULL,
                        ext_difficulty TEXT NOT NULL,
                        brutal_plus INTEGER NOT NULL,
                        extension INTEGER NOT NULL,
                        weekly INTEGER NOT NULL,
                        region TEXT NOT NULL,
                        payload_json TEXT NOT NULL,
                        updated_at_seconds INTEGER NOT NULL
                    );

                    CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_date
                        ON replay_cache_entries(date_seconds DESC, date_text DESC, file DESC);
                    CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file
                        ON replay_cache_entries(file);
                    CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file_name
                        ON replay_cache_entries(file_name, date_seconds DESC);
                    CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_detailed
                        ON replay_cache_entries(detailed_analysis, date_seconds DESC);
                    PRAGMA user_version = 1;
                    ",
                )
                .map_err(|source| ReplayCacheDbError::Sqlite {
                    path: db_path.to_path_buf(),
                    source,
                })?;
        }

        Ok(())
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
            let imported = self.import_temp_cache_file(&temp_path)?;
            if imported > 0
                && let Err(error) = std::fs::remove_file(&temp_path)
            {
                crate::sco_log!(
                    "[SCO/cache-db] failed to remove imported temp cache '{}': {error}",
                    temp_path.display()
                );
            }
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
        let entries = self.read_legacy_cache_entries()?;
        self.upsert_entries_preserving_detailed(&entries)
    }

    fn import_temp_cache_file(&mut self, temp_path: &Path) -> Result<usize, ReplayCacheDbError> {
        let entries = Self::read_temp_cache_entries(temp_path)?;
        self.upsert_entries_preserving_detailed(&entries)
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

    pub fn upsert_entries_preserving_detailed(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        let mut changed = 0usize;
        for entry in entries {
            if entry.hash.is_empty() {
                continue;
            }
            let record = ReplayCacheEntryRecord::from_entry(entry).map_err(|source| {
                ReplayCacheDbError::Json {
                    path: self.cache_path.clone(),
                    source,
                }
            })?;
            changed = changed.saturating_add(Self::upsert_record(&tx, &record, true, &db_path)?);
        }
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    pub fn replace_entries(
        &mut self,
        entries: &[CacheReplayEntry],
    ) -> Result<usize, ReplayCacheDbError> {
        let db_path = self.db_path.clone();
        let tx = self
            .connection
            .transaction()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        tx.execute("DELETE FROM replay_cache_entries", [])
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        let mut changed = 0usize;
        for entry in entries {
            if entry.hash.is_empty() {
                continue;
            }
            let record = ReplayCacheEntryRecord::from_entry(entry).map_err(|source| {
                ReplayCacheDbError::Json {
                    path: self.cache_path.clone(),
                    source,
                }
            })?;
            changed = changed.saturating_add(Self::upsert_record(&tx, &record, false, &db_path)?);
        }
        tx.commit().map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path,
            source,
        })?;
        Ok(changed)
    }

    fn upsert_record(
        tx: &Transaction<'_>,
        record: &ReplayCacheEntryRecord,
        preserve_detailed: bool,
        db_path: &Path,
    ) -> Result<usize, ReplayCacheDbError> {
        tx.execute(
            "DELETE FROM replay_cache_entries WHERE file = ?1 AND hash <> ?2",
            params![record.file, record.hash],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })?;

        let update_guard = if preserve_detailed {
            "WHERE replay_cache_entries.detailed_analysis = 0 OR excluded.detailed_analysis = 1"
        } else {
            ""
        };
        let sql = format!(
            "
            INSERT INTO replay_cache_entries (
                hash,
                file,
                file_name,
                date_text,
                date_seconds,
                detailed_analysis,
                result,
                map_name,
                difficulty,
                ext_difficulty,
                brutal_plus,
                extension,
                weekly,
                region,
                payload_json,
                updated_at_seconds
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
            )
            ON CONFLICT(hash) DO UPDATE SET
                file = excluded.file,
                file_name = excluded.file_name,
                date_text = excluded.date_text,
                date_seconds = excluded.date_seconds,
                detailed_analysis = excluded.detailed_analysis,
                result = excluded.result,
                map_name = excluded.map_name,
                difficulty = excluded.difficulty,
                ext_difficulty = excluded.ext_difficulty,
                brutal_plus = excluded.brutal_plus,
                extension = excluded.extension,
                weekly = excluded.weekly,
                region = excluded.region,
                payload_json = excluded.payload_json,
                updated_at_seconds = excluded.updated_at_seconds
            {update_guard}
            "
        );
        tx.execute(
            &sql,
            params![
                record.hash,
                record.file,
                record.file_name,
                record.date_text,
                ReplayCacheEntryRecord::u64_to_i64(record.date_seconds),
                ReplayCacheEntryRecord::bool_to_i64(record.detailed_analysis),
                record.result,
                record.map_name,
                record.difficulty,
                record.ext_difficulty,
                i64::from(record.brutal_plus),
                ReplayCacheEntryRecord::bool_to_i64(record.extension),
                ReplayCacheEntryRecord::bool_to_i64(record.weekly),
                record.region,
                record.payload_json,
                ReplayCacheEntryRecord::u64_to_i64(record.updated_at_seconds),
            ],
        )
        .map_err(|source| ReplayCacheDbError::Sqlite {
            path: db_path.to_path_buf(),
            source,
        })
    }

    pub fn load_entries(
        &self,
        query: ReplayCacheEntryQuery,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        match (query.scope(), query.limit()) {
            (ReplayCacheReadScope::All, 0) => self.load_entries_with_sql(
                "
                SELECT payload_json FROM replay_cache_entries
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                ",
            ),
            (ReplayCacheReadScope::All, limit) => self.load_limited_entries_with_sql(
                "
                SELECT payload_json FROM replay_cache_entries
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                LIMIT ?1
                ",
                limit,
            ),
            (ReplayCacheReadScope::DetailedOnly, 0) => self.load_entries_with_sql(
                "
                SELECT payload_json FROM replay_cache_entries
                WHERE detailed_analysis = 1
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                ",
            ),
            (ReplayCacheReadScope::DetailedOnly, limit) => self.load_limited_entries_with_sql(
                "
                SELECT payload_json FROM replay_cache_entries
                WHERE detailed_analysis = 1
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                LIMIT ?1
                ",
                limit,
            ),
        }
    }

    fn load_entries_with_sql(
        &self,
        sql: &str,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map([], |row| Self::entry_from_row(row, 0))
            .map_err(|source| self.sqlite_error(source))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|source| self.sqlite_error(source))?);
        }
        Ok(entries)
    }

    fn load_limited_entries_with_sql(
        &self,
        sql: &str,
        limit: usize,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let safe_limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|source| self.sqlite_error(source))?;
        let rows = statement
            .query_map(params![safe_limit], |row| Self::entry_from_row(row, 0))
            .map_err(|source| self.sqlite_error(source))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|source| self.sqlite_error(source))?);
        }
        Ok(entries)
    }

    fn entry_from_row(row: &Row<'_>, payload_column: usize) -> rusqlite::Result<CacheReplayEntry> {
        let payload_json = row.get::<_, String>(payload_column)?;
        serde_json::from_str::<CacheReplayEntry>(&payload_json).map_err(|source| {
            rusqlite::Error::FromSqlConversionFailure(payload_column, Type::Text, Box::new(source))
        })
    }

    pub fn load_entry_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        self.connection
            .query_row(
                "SELECT payload_json FROM replay_cache_entries WHERE hash = ?1",
                params![hash],
                |row| Self::entry_from_row(row, 0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))
    }

    pub fn load_entry_by_file(
        &self,
        file: &str,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        if let Some(entry) = self.load_entry_by_exact_file(file)? {
            return Ok(Some(entry));
        }

        let file_name = ReplayCacheFileName::from_replay_file(file).into_string();
        if file_name.trim().is_empty() {
            return Ok(None);
        }
        self.connection
            .query_row(
                "
                SELECT payload_json FROM replay_cache_entries
                WHERE file_name = ?1
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                LIMIT 1
                ",
                params![file_name],
                |row| Self::entry_from_row(row, 0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))
    }

    fn load_entry_by_exact_file(
        &self,
        file: &str,
    ) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        self.connection
            .query_row(
                "SELECT payload_json FROM replay_cache_entries WHERE file = ?1",
                params![file],
                |row| Self::entry_from_row(row, 0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))
    }

    pub fn load_latest_entry(&self) -> Result<Option<CacheReplayEntry>, ReplayCacheDbError> {
        self.connection
            .query_row(
                "
                SELECT payload_json FROM replay_cache_entries
                ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
                LIMIT 1
                ",
                [],
                |row| Self::entry_from_row(row, 0),
            )
            .optional()
            .map_err(|source| self.sqlite_error(source))
    }

    pub fn load_entries_by_hash(
        &self,
    ) -> Result<HashMap<String, CacheReplayEntry>, ReplayCacheDbError> {
        Ok(self
            .load_entries(ReplayCacheEntryQuery::all(0))?
            .into_iter()
            .filter(|entry| !entry.hash.is_empty())
            .map(|entry| (entry.hash.clone(), entry))
            .collect())
    }

    pub fn count_entries(&self) -> Result<usize, ReplayCacheDbError> {
        let count = self
            .connection
            .query_row("SELECT COUNT(*) FROM replay_cache_entries", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|source| self.sqlite_error(source))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    pub fn export_to_legacy_json(&self) -> Result<(), ReplayCacheDbError> {
        let entries = self.load_entries(ReplayCacheEntryQuery::all(0))?;
        CacheReplayEntry::write_entries(&entries, &self.cache_path).map_err(|source| {
            ReplayCacheDbError::GenerateCache {
                path: self.cache_path.clone(),
                source,
            }
        })
    }

    fn sqlite_error(&self, source: rusqlite::Error) -> ReplayCacheDbError {
        ReplayCacheDbError::Sqlite {
            path: self.db_path.clone(),
            source,
        }
    }

    fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}
