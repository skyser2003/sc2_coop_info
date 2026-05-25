use rusqlite::Connection;
use std::path::Path;

use super::core::{ReplayCacheDatabase, ReplayCacheDbError};

impl ReplayCacheDatabase {
    pub(super) fn create_current_schema(
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

                CREATE TABLE IF NOT EXISTS replay_cache_unsaved_replay_checks (
                    hash TEXT PRIMARY KEY NOT NULL,
                    file TEXT NOT NULL,
                    file_name TEXT NOT NULL,
                    file_modified_seconds INTEGER NOT NULL,
                    updated_at_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS replay_player_infos (
                    handle TEXT PRIMARY KEY NOT NULL,
                    wins INTEGER NOT NULL,
                    losses INTEGER NOT NULL,
                    average_apm REAL NOT NULL,
                    latest_commander TEXT NOT NULL,
                    commander_frequency REAL NOT NULL,
                    kill_ratio REAL NOT NULL,
                    latest_played_time INTEGER NOT NULL,
                    updated_at_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS replay_cache_players (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    player_name TEXT NOT NULL,
                    apm INTEGER,
                    commander TEXT,
                    commander_level INTEGER,
                    commander_mastery_level INTEGER,
                    player_handle TEXT NOT NULL REFERENCES replay_player_infos(handle) ON UPDATE CASCADE,
                    kills INTEGER,
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

                CREATE TABLE IF NOT EXISTS replay_cache_weeklies (
                    replay_id INTEGER PRIMARY KEY REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    result TEXT NOT NULL,
                    map_name TEXT NOT NULL,
                    difficulty TEXT NOT NULL,
                    brutal_plus INTEGER NOT NULL,
                    mutator_values TEXT NOT NULL CHECK(json_valid(mutator_values))
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

                CREATE TABLE IF NOT EXISTS replay_cache_stats_players (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    player_handle_key TEXT NOT NULL,
                    commander TEXT NOT NULL,
                    PRIMARY KEY (replay_id, pid)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_stats_player_units (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    player_handle_key TEXT NOT NULL,
                    commander TEXT NOT NULL,
                    player_kills INTEGER NOT NULL,
                    unit_name TEXT NOT NULL,
                    created_hidden INTEGER NOT NULL,
                    created_count INTEGER NOT NULL,
                    lost_hidden INTEGER NOT NULL,
                    lost_count INTEGER NOT NULL,
                    kills INTEGER NOT NULL,
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
                    lost_kind TEXT NOT NULL CHECK(lost_kind IN ('count', 'hidden')),
                    lost_count INTEGER,
                    kills INTEGER NOT NULL,
                    fraction REAL NOT NULL,
                    PRIMARY KEY (replay_id, unit_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_stat_series (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    player_handle TEXT NOT NULL REFERENCES replay_player_infos(handle) ON UPDATE CASCADE,
                    supply_values TEXT NOT NULL CHECK(json_valid(supply_values)),
                    mining_values TEXT NOT NULL CHECK(json_valid(mining_values)),
                    army_values TEXT NOT NULL CHECK(json_valid(army_values)),
                    killed_values TEXT NOT NULL CHECK(json_valid(killed_values)),
                    PRIMARY KEY (replay_id, pid)
                );
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })
    }

    pub(super) fn create_current_indexes(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        connection
            .execute_batch(
                "
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_date
                    ON replay_cache_entries(date_seconds DESC, date_text DESC, file DESC, hash DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file
                    ON replay_cache_entries(file);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file_name
                    ON replay_cache_entries(file_name, date_seconds DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_detailed
                    ON replay_cache_entries(detailed_analysis, date_seconds DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_stats_filter
                    ON replay_cache_entries(detailed_analysis, extension, date_seconds DESC, brutal_plus, result);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_games_tab
                    ON replay_cache_entries(date_seconds DESC, result, difficulty_p1, difficulty_p2, map_name);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_handle
                    ON replay_cache_players(player_handle, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_name
                    ON replay_cache_players(player_handle, player_name, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_stats_name
                    ON replay_cache_players(player_name COLLATE NOCASE, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_commander
                    ON replay_cache_players(commander, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_last_played
                    ON replay_player_infos(latest_played_time DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_wins
                    ON replay_player_infos(wins DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_losses
                    ON replay_player_infos(losses DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_apm
                    ON replay_player_infos(average_apm DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_commander
                    ON replay_player_infos(latest_commander, latest_played_time DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_kill_ratio
                    ON replay_player_infos(kill_ratio DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_weeklies_mutation
                    ON replay_cache_weeklies(map_name, brutal_plus);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_player_units_unit
                    ON replay_cache_player_units(unit_name, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_amon_units_rollup
                    ON replay_cache_amon_units(
                        unit_name,
                        replay_id,
                        created_kind,
                        created_count,
                        lost_kind,
                        lost_count,
                        kills
                    );
                CREATE INDEX IF NOT EXISTS idx_replay_cache_unsaved_replay_checks_hash_time
                    ON replay_cache_unsaved_replay_checks(hash, file_modified_seconds);
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        Ok(())
    }
}
