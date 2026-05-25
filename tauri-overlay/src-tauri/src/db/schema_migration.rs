use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use std::time::{Duration, Instant};

use super::core::{ReplayCacheDatabase, ReplayCacheDbError};

const CURRENT_SCHEMA_VERSION: i32 = 2;

impl ReplayCacheDatabase {
    pub(super) fn initialize_schema(
        connection: &mut Connection,
        db_path: &Path,
        db_existed: bool,
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

        let should_log_version_update = db_existed && schema_version < CURRENT_SCHEMA_VERSION;
        let version_update_started_at = Instant::now();
        let result = Self::apply_current_schema(connection, db_path, schema_version);
        if should_log_version_update {
            Self::log_schema_version_update(
                schema_version,
                result.as_ref(),
                version_update_started_at.elapsed(),
                db_path,
            );
        }
        result
    }

    fn apply_current_schema(
        connection: &mut Connection,
        db_path: &Path,
        schema_version: i32,
    ) -> Result<(), ReplayCacheDbError> {
        Self::create_current_schema(connection, db_path)?;
        Self::drop_obsolete_indexes(connection, db_path)?;
        Self::migrate_schema(connection, db_path, schema_version)?;
        Self::create_current_indexes(connection, db_path)?;
        Self::set_current_schema_version(connection, db_path)?;
        Ok(())
    }

    fn log_schema_version_update(
        from_version: i32,
        result: Result<&(), &ReplayCacheDbError>,
        elapsed: Duration,
        db_path: &Path,
    ) {
        match result {
            Ok(_) => {
                crate::sco_log!(
                    "[SCO/cache-db] schema version update completed from={} to={} path='{}' elapsed={}ms",
                    from_version,
                    CURRENT_SCHEMA_VERSION,
                    db_path.display(),
                    elapsed.as_millis()
                );
            }
            Err(error) => {
                crate::sco_log!(
                    "[SCO/cache-db] schema version update failed from={} to={} path='{}' elapsed={}ms error={}",
                    from_version,
                    CURRENT_SCHEMA_VERSION,
                    db_path.display(),
                    elapsed.as_millis(),
                    error
                );
            }
        }
    }

    fn migrate_schema(
        connection: &mut Connection,
        db_path: &Path,
        schema_version: i32,
    ) -> Result<(), ReplayCacheDbError> {
        if schema_version < 2 {
            Self::migrate_schema_to_v2(connection, db_path)?;
        }
        Ok(())
    }

    fn migrate_schema_to_v2(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        Self::remove_obsolete_schema_parts(connection, db_path)
    }

    fn set_current_schema_version(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        connection
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })
    }

    fn remove_obsolete_schema_parts(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        Self::drop_column_if_present(
            connection,
            db_path,
            "replay_cache_amon_units",
            "created_hidden",
        )?;
        Self::drop_column_if_present(
            connection,
            db_path,
            "replay_cache_amon_units",
            "lost_hidden",
        )?;
        Self::drop_column_if_present(
            connection,
            db_path,
            "replay_cache_stats_players",
            "player_handle",
        )?;
        Self::drop_column_if_present(
            connection,
            db_path,
            "replay_cache_stats_players",
            "player_kills",
        )?;
        Self::drop_column_if_present(
            connection,
            db_path,
            "replay_cache_stats_player_units",
            "player_handle",
        )?;
        Ok(())
    }

    fn drop_column_if_present(
        connection: &mut Connection,
        db_path: &Path,
        table_name: &'static str,
        column_name: &'static str,
    ) -> Result<(), ReplayCacheDbError> {
        let column_exists = connection
            .query_row(
                "SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2 LIMIT 1",
                [table_name, column_name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?
            .is_some();
        if !column_exists {
            return Ok(());
        }

        let sql = format!("ALTER TABLE {table_name} DROP COLUMN {column_name};");
        connection
            .execute_batch(&sql)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        Ok(())
    }

    fn drop_obsolete_indexes(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        connection
            .execute_batch(
                "
                DROP INDEX IF EXISTS idx_replay_cache_stats_players_handle;
                DROP INDEX IF EXISTS idx_replay_cache_stats_player_units_unit;
                DROP INDEX IF EXISTS idx_replay_cache_amon_units_unit;
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })
    }
}
