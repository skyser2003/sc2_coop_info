use super::*;

impl ReplayCacheDatabase {
    pub fn retry_sqlite_lock<T>(
        mut operation: impl FnMut() -> Result<T, ReplayCacheDbError>,
    ) -> Result<T, ReplayCacheDbError> {
        let started_at = Instant::now();
        loop {
            match operation() {
                Ok(value) => return Ok(value),
                Err(error)
                    if error.is_sqlite_lock()
                        && started_at.elapsed() < SQLITE_LOCK_RETRY_WINDOW =>
                {
                    thread::sleep(SQLITE_LOCK_RETRY_DELAY);
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn db_path_for_cache_path(cache_path: &Path) -> PathBuf {
        if cache_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sqlite3"))
        {
            cache_path.to_path_buf()
        } else {
            cache_path.with_extension("sqlite3")
        }
    }

    pub fn legacy_json_path_for_cache_path(cache_path: &Path) -> PathBuf {
        if cache_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sqlite3"))
        {
            cache_path.with_extension("json")
        } else {
            cache_path.to_path_buf()
        }
    }

    pub fn legacy_temp_jsonl_path_for_cache_path(cache_path: &Path) -> PathBuf {
        Self::legacy_json_path_for_cache_path(cache_path).with_extension("temp.jsonl")
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

    pub fn sqlite_contains_pattern(value: &str) -> String {
        let mut pattern = String::with_capacity(value.len() + 2);
        pattern.push('%');
        for ch in value.trim().to_ascii_lowercase().chars() {
            match ch {
                '%' | '_' | '\\' => {
                    pattern.push('\\');
                    pattern.push(ch);
                }
                _ => pattern.push(ch),
            }
        }
        pattern.push('%');
        pattern
    }

    pub fn usize_to_i64(value: usize) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }

    pub fn open_for_cache_path(cache_path: &Path) -> Result<Self, ReplayCacheDbError> {
        Self::retry_sqlite_lock(|| Self::open_for_cache_path_once(cache_path))
    }

    fn open_for_cache_path_once(cache_path: &Path) -> Result<Self, ReplayCacheDbError> {
        let db_path = Self::db_path_for_cache_path(cache_path);
        let legacy_cache_path = Self::legacy_json_path_for_cache_path(cache_path);
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
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
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
            .pragma_update(None, "synchronous", "NORMAL")
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

        Self::initialize_schema(&mut connection, &db_path, db_existed)?;
        let mut database = Self {
            cache_path: cache_path.to_path_buf(),
            legacy_cache_path,
            db_path,
            connection,
        };
        if let Err(error) = database.import_legacy_cache_if_needed(db_existed) {
            crate::sco_warn!("[SCO/cache-db] legacy cache import skipped: {error}");
        }
        database.backfill_statistics_fact_tables_if_needed()?;
        Ok(database)
    }

    fn backfill_statistics_fact_tables_if_needed(&self) -> Result<(), ReplayCacheDbError> {
        let should_backfill = self
            .connection
            .query_row(
                "
                SELECT
                    EXISTS(SELECT 1 FROM replay_cache_entries LIMIT 1)
                    AND (
                        NOT EXISTS(SELECT 1 FROM replay_cache_stats_players LIMIT 1)
                        OR NOT EXISTS(SELECT 1 FROM replay_cache_stats_player_units LIMIT 1)
                    )
                ",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|source| self.sqlite_error(source))?;
        if should_backfill == 0 {
            return Ok(());
        }

        let started_at = Instant::now();
        let commander_sql = ReplayCacheStatsFactOps::normalized_commander_sql("p.commander");
        let handle_key_sql = ReplayCacheStatsFactOps::normalized_handle_key_sql("p.player_handle");
        let sql = format!(
            "
            INSERT OR IGNORE INTO replay_cache_stats_players (
                replay_id,
                pid,
                player_handle_key,
                commander
            )
            SELECT
                replay_id,
                pid,
                player_handle_key,
                commander
            FROM (
                SELECT
                    p.replay_id,
                    p.pid,
                    {handle_key_sql} AS player_handle_key,
                    {commander_sql} AS commander
                FROM replay_cache_players p
                WHERE p.pid IN (1, 2)
                    AND TRIM(COALESCE(p.commander, '')) <> ''
            ) players
            WHERE commander <> '';

            INSERT OR IGNORE INTO replay_cache_stats_player_units (
                replay_id,
                pid,
                player_handle_key,
                commander,
                player_kills,
                unit_name,
                created_hidden,
                created_count,
                lost_hidden,
                lost_count,
                kills
            )
            SELECT
                units.replay_id,
                units.pid,
                players.player_handle_key,
                players.commander,
                players.player_kills,
                units.unit_name,
                CASE WHEN units.created_kind = 'hidden' THEN 1 ELSE 0 END AS created_hidden,
                CASE
                    WHEN units.created_kind = 'hidden' THEN 0
                    ELSE COALESCE(units.created_count, 0)
                END AS created_count,
                CASE WHEN units.lost_kind = 'hidden' THEN 1 ELSE 0 END AS lost_hidden,
                CASE
                    WHEN units.lost_kind = 'hidden' THEN 0
                    ELSE COALESCE(units.lost_count, 0)
                END AS lost_count,
                COALESCE(units.kills, 0) AS kills
            FROM replay_cache_player_units units
            INNER JOIN (
                SELECT
                    p.replay_id,
                    p.pid,
                    {handle_key_sql} AS player_handle_key,
                    {commander_sql} AS commander,
                    COALESCE(p.kills, 0) AS player_kills
                FROM replay_cache_players p
                WHERE p.pid IN (1, 2)
                    AND TRIM(COALESCE(p.commander, '')) <> ''
            ) players
                ON players.replay_id = units.replay_id
                AND players.pid = units.pid
            WHERE players.commander <> '';
            "
        );
        self.connection
            .execute_batch(&sql)
            .map_err(|source| self.sqlite_error(source))?;
        crate::sco_info!(
            "[SCO/cache-db] statistics fact table backfill elapsed={}ms",
            started_at.elapsed().as_millis()
        );
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

        let temp_path = Self::legacy_temp_jsonl_path_for_cache_path(&self.cache_path);
        if temp_path.exists() {
            self.import_temp_cache_file(&temp_path)?;
        }

        Ok(())
    }

    fn should_import_legacy_json(&self, db_existed: bool) -> bool {
        if !self.legacy_cache_path.exists() {
            return false;
        }
        if !db_existed {
            return true;
        }

        let Ok(json_modified) = self
            .legacy_cache_path
            .metadata()
            .and_then(|meta| meta.modified())
        else {
            return false;
        };
        let Ok(db_modified) = self.db_path.metadata().and_then(|meta| meta.modified()) else {
            return true;
        };
        json_modified > db_modified
    }

    pub fn import_legacy_cache_file(&mut self) -> Result<usize, ReplayCacheDbError> {
        crate::sco_info!(
            "[SCO/cache-db] starting legacy JSON cache import from '{}'",
            self.legacy_cache_path.display()
        );
        let mut entries = self.read_legacy_cache_entries()?;
        let entry_count = entries.len();
        Self::normalize_legacy_cache_dates_to_utc(&mut entries);
        let changed = self.upsert_entries_preserving_detailed(&entries)?;
        Self::remove_imported_legacy_file(&self.legacy_cache_path)?;
        crate::sco_info!(
            "[SCO/cache-db] completed legacy JSON cache import from '{}' (entries={}, changed={})",
            self.legacy_cache_path.display(),
            entry_count,
            changed
        );
        Ok(changed)
    }

    fn import_temp_cache_file(&mut self, temp_path: &Path) -> Result<usize, ReplayCacheDbError> {
        crate::sco_info!(
            "[SCO/cache-db] starting legacy JSONL temp cache import from '{}'",
            temp_path.display()
        );
        let mut entries = Self::read_temp_cache_entries(temp_path)?;
        let entry_count = entries.len();
        Self::normalize_legacy_cache_dates_to_utc(&mut entries);
        let changed = self.upsert_entries_preserving_detailed(&entries)?;
        Self::remove_imported_legacy_file(temp_path)?;
        crate::sco_info!(
            "[SCO/cache-db] completed legacy JSONL temp cache import from '{}' (entries={}, changed={})",
            temp_path.display(),
            entry_count,
            changed
        );
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
        let payload =
            std::fs::read(&self.legacy_cache_path).map_err(|source| ReplayCacheDbError::Io {
                path: self.legacy_cache_path.clone(),
                source,
            })?;
        serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).map_err(|source| {
            ReplayCacheDbError::Json {
                path: self.legacy_cache_path.clone(),
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
                    crate::sco_warn!(
                        "[SCO/cache-db] ignored malformed temp cache row '{}': {source}",
                        temp_path.display()
                    );
                }
            }
        }
        Ok(entries)
    }

    pub fn count_value_columns(value: &CacheCountValue) -> (&'static str, Option<i64>) {
        match value {
            CacheCountValue::Count(value) => ("count", Some(*value)),
            CacheCountValue::Hidden(_) => ("hidden", None),
        }
    }

    pub fn count_value_from_kind_and_count(kind: String, count: Option<i64>) -> CacheCountValue {
        if kind == "hidden" {
            CacheCountValue::Hidden("-".to_string())
        } else {
            CacheCountValue::Count(count.unwrap_or_default())
        }
    }

    pub fn sqlite_error(&self, source: rusqlite::Error) -> ReplayCacheDbError {
        ReplayCacheDbError::Sqlite {
            path: self.db_path.clone(),
            source,
        }
    }

    pub fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}
