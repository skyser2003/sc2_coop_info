use super::*;

impl ReplayCacheDatabase {
    pub(super) fn load_statistics_unit_data_from_facts(
        &self,
        replay_ids: &[i64],
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<Value, ReplayCacheDbError> {
        let total_started_at = Instant::now();
        let temp_started_at = Instant::now();
        self.prepare_statistics_unit_temp_tables(replay_ids, main_handles)?;
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_temp_tables replay_ids={} main_handles={} elapsed_ms={:.3}",
            replay_ids.len(),
            main_handles.len(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(temp_started_at)
        );

        let mut main_rollup = BTreeMap::<String, CommanderUnitRollup>::new();
        let mut ally_rollup = BTreeMap::<String, CommanderUnitRollup>::new();
        let player_count_started_at = Instant::now();
        self.load_statistics_unit_player_counts(&mut main_rollup, &mut ally_rollup)?;
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_player_counts main_commanders={} ally_commanders={} elapsed_ms={:.3}",
            main_rollup.len(),
            ally_rollup.len(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(player_count_started_at)
        );

        let player_units_started_at = Instant::now();
        self.load_statistics_player_unit_rollup(&mut main_rollup, &mut ally_rollup, dictionary)?;
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_player_rollup main_commanders={} ally_commanders={} elapsed_ms={:.3}",
            main_rollup.len(),
            ally_rollup.len(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(player_units_started_at)
        );

        let amon_started_at = Instant::now();
        let amon_rollup = self.load_statistics_amon_unit_rollup()?;
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_amon_rollup rows={} elapsed_ms={:.3}",
            amon_rollup.len(),
            ReplayCacheStatisticsLoadOps::elapsed_ms(amon_started_at)
        );

        let build_started_at = Instant::now();
        let unit_payload =
            ReplayCacheStatisticsLoadOps::to_value(&StatsAggregateUnitDataPayload::new(
                StatsUnitDataOps::build_commander_unit_data_with_dictionary(
                    main_rollup,
                    dictionary,
                ),
                StatsUnitDataOps::build_commander_unit_data_with_dictionary(
                    ally_rollup,
                    dictionary,
                ),
                StatsUnitDataOps::build_amon_unit_data(amon_rollup),
            ));
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_payload_build elapsed_ms={:.3}",
            ReplayCacheStatisticsLoadOps::elapsed_ms(build_started_at)
        );
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_data_from_facts_total elapsed_ms={:.3}",
            ReplayCacheStatisticsLoadOps::elapsed_ms(total_started_at)
        );
        Ok(unit_payload)
    }

    fn prepare_statistics_unit_temp_tables(
        &self,
        replay_ids: &[i64],
        main_handles: &HashSet<String>,
    ) -> Result<(), ReplayCacheDbError> {
        self.connection
            .execute_batch(
                "
                DROP TABLE IF EXISTS temp_stats_unit_replays;
                DROP TABLE IF EXISTS temp_stats_main_handles;
                CREATE TEMP TABLE temp_stats_unit_replays (
                    replay_id INTEGER PRIMARY KEY
                );
                CREATE TEMP TABLE temp_stats_main_handles (
                    handle_key TEXT PRIMARY KEY
                );
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;

        self.insert_statistics_unit_replay_ids(replay_ids)?;
        self.insert_statistics_unit_main_handles(main_handles)?;
        Ok(())
    }

    fn insert_statistics_unit_replay_ids(
        &self,
        replay_ids: &[i64],
    ) -> Result<(), ReplayCacheDbError> {
        for replay_id_batch in ReplayCacheSqlBatch::chunks(replay_ids) {
            let placeholders = ReplayCacheSqlBatch::values_placeholders(replay_id_batch.len());
            let sql = format!(
                "INSERT OR IGNORE INTO temp_stats_unit_replays (replay_id) VALUES {placeholders}"
            );
            self.connection
                .execute(&sql, params_from_iter(replay_id_batch.iter().copied()))
                .map_err(|source| self.sqlite_error(source))?;
        }
        Ok(())
    }

    fn insert_statistics_unit_main_handles(
        &self,
        main_handles: &HashSet<String>,
    ) -> Result<(), ReplayCacheDbError> {
        let handle_keys = main_handles
            .iter()
            .map(|handle| ReplayCacheStatsFactOps::normalized_handle_key(handle))
            .filter(|handle_key| !handle_key.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        for handle_batch in ReplayCacheSqlBatch::chunks(&handle_keys) {
            let placeholders = ReplayCacheSqlBatch::values_placeholders(handle_batch.len());
            let sql = format!(
                "INSERT OR IGNORE INTO temp_stats_main_handles (handle_key) VALUES {placeholders}"
            );
            self.connection
                .execute(&sql, params_from_iter(handle_batch.iter()))
                .map_err(|source| self.sqlite_error(source))?;
        }
        Ok(())
    }

    fn load_statistics_unit_player_counts(
        &self,
        main_rollup: &mut BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut BTreeMap<String, CommanderUnitRollup>,
    ) -> Result<(), ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT
                    CASE
                        WHEN EXISTS (
                            SELECT 1
                            FROM temp_stats_main_handles main_handles
                            WHERE main_handles.handle_key = players.player_handle_key
                        )
                        THEN 1
                        ELSE 0
                    END AS is_main,
                    players.commander
                FROM replay_cache_stats_players players
                INNER JOIN temp_stats_unit_replays selected
                    ON selected.replay_id = players.replay_id
                ORDER BY players.replay_id ASC, players.pid ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let mut rows = statement
            .query([])
            .map_err(|source| self.sqlite_error(source))?;
        while let Some(row) = rows.next().map_err(|source| self.sqlite_error(source))? {
            let side = StatisticsUnitSide::from_is_main(self.sqlite_row(row.get::<_, i64>(0))?);
            let commander = self.sqlite_row(row.get::<_, String>(1))?;
            let rollup = Self::statistics_unit_rollup_for_side(main_rollup, ally_rollup, side);
            let entry = rollup.entry(commander).or_default();
            entry.count = entry.count.saturating_add(1);
        }
        Ok(())
    }

    fn load_statistics_player_unit_rollup(
        &self,
        main_rollup: &mut BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut BTreeMap<String, CommanderUnitRollup>,
        dictionary: &Sc2DictionaryData,
    ) -> Result<(), ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT
                    units.replay_id,
                    units.pid,
                    CASE
                        WHEN EXISTS (
                            SELECT 1
                            FROM temp_stats_main_handles main_handles
                            WHERE main_handles.handle_key = units.player_handle_key
                        )
                        THEN 1
                        ELSE 0
                    END AS is_main,
                    units.commander,
                    units.player_kills,
                    units.unit_name,
                    units.created_hidden,
                    units.created_count,
                    units.lost_hidden,
                    units.lost_count,
                    units.kills
                FROM replay_cache_stats_player_units units
                INNER JOIN temp_stats_unit_replays selected
                    ON selected.replay_id = units.replay_id
                ORDER BY units.replay_id ASC, units.pid ASC, units.unit_name ASC
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let mut rows = statement
            .query([])
            .map_err(|source| self.sqlite_error(source))?;
        let mut current_key = None::<(i64, i64)>;
        let mut current_side = StatisticsUnitSide::Ally;
        let mut current_commander = String::new();
        let mut current_player_kills = 0_i64;
        let mut current_rows = Vec::<StatisticsPlayerUnitFactRow>::new();
        let mut source_row_count = 0usize;
        let mut player_count = 0usize;

        while let Some(row) = rows.next().map_err(|source| self.sqlite_error(source))? {
            source_row_count = source_row_count.saturating_add(1);
            let replay_id = self.sqlite_row(row.get::<_, i64>(0))?;
            let pid = self.sqlite_row(row.get::<_, i64>(1))?;
            let key = (replay_id, pid);
            if current_key.is_some_and(|existing| existing != key) {
                Self::record_statistics_player_unit_facts(
                    main_rollup,
                    ally_rollup,
                    current_side,
                    &current_commander,
                    current_player_kills,
                    &current_rows,
                    dictionary,
                );
                current_rows.clear();
            }
            if current_key != Some(key) {
                player_count = player_count.saturating_add(1);
                current_key = Some(key);
                current_side =
                    StatisticsUnitSide::from_is_main(self.sqlite_row(row.get::<_, i64>(2))?);
                current_commander = self.sqlite_row(row.get::<_, String>(3))?;
                current_player_kills = self.sqlite_row(row.get::<_, i64>(4))?;
            }
            current_rows.push(StatisticsPlayerUnitFactRow::new(
                self.sqlite_row(row.get::<_, String>(5))?,
                self.sqlite_row(row.get::<_, i64>(6))? != 0,
                self.sqlite_row(row.get::<_, i64>(7))?,
                self.sqlite_row(row.get::<_, i64>(8))? != 0,
                self.sqlite_row(row.get::<_, i64>(9))?,
                self.sqlite_row(row.get::<_, i64>(10))?,
            ));
        }

        if current_key.is_some() {
            Self::record_statistics_player_unit_facts(
                main_rollup,
                ally_rollup,
                current_side,
                &current_commander,
                current_player_kills,
                &current_rows,
                dictionary,
            );
        }
        crate::sco_debug!(
            "[SCO/stats/e2e/backend] stage=unit_player_rollup_source rows={} players={}",
            source_row_count,
            player_count
        );
        Ok(())
    }

    fn load_statistics_amon_unit_rollup(
        &self,
    ) -> Result<BTreeMap<String, UnitStatsRollup>, ReplayCacheDbError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT
                    units.unit_name,
                    SUM(
                        CASE
                            WHEN units.created_kind = 'hidden' THEN 0
                            ELSE COALESCE(units.created_count, 0)
                        END
                    ) AS created_count,
                    SUM(
                        CASE
                            WHEN units.lost_kind = 'hidden' THEN 0
                            ELSE COALESCE(units.lost_count, 0)
                        END
                    ) AS lost_count,
                    SUM(units.kills) AS kills
                FROM replay_cache_amon_units units
                INNER JOIN temp_stats_unit_replays selected
                    ON selected.replay_id = units.replay_id
                GROUP BY units.unit_name
                HAVING created_count <> 0 OR lost_count <> 0 OR kills <> 0
                ",
            )
            .map_err(|source| self.sqlite_error(source))?;
        let mut rows = statement
            .query([])
            .map_err(|source| self.sqlite_error(source))?;
        let mut rollup = BTreeMap::<String, UnitStatsRollup>::new();
        while let Some(row) = rows.next().map_err(|source| self.sqlite_error(source))? {
            rollup.insert(
                self.sqlite_row(row.get::<_, String>(0))?,
                UnitStatsRollup {
                    created: self.sqlite_row(row.get::<_, i64>(1))?,
                    lost: self.sqlite_row(row.get::<_, i64>(2))?,
                    kills: self.sqlite_row(row.get::<_, i64>(3))?,
                    ..Default::default()
                },
            );
        }
        Ok(rollup)
    }

    fn record_statistics_player_unit_facts(
        main_rollup: &mut BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut BTreeMap<String, CommanderUnitRollup>,
        side: StatisticsUnitSide,
        commander: &str,
        player_kills: i64,
        rows: &[StatisticsPlayerUnitFactRow],
        dictionary: &Sc2DictionaryData,
    ) {
        if commander.trim().is_empty() {
            return;
        }
        let rollup = Self::statistics_unit_rollup_for_side(main_rollup, ally_rollup, side);
        let commander_entry = rollup.entry(commander.to_string()).or_default();
        let mc_unit = dictionary.commander_mind_control_unit(commander);
        let mut mc_unit_bonus_kills =
            Self::statistics_mind_control_bonus_kills(commander, mc_unit, rows);

        for row in rows {
            let is_mc_bonus_target = mc_unit == Some(row.unit_name());
            let unit_entry = commander_entry
                .units
                .entry(row.unit_name().to_string())
                .or_default();
            Self::apply_statistics_unit_count(
                &mut unit_entry.created,
                &mut unit_entry.created_hidden,
                row.created_count(),
                row.created_hidden(),
            );
            Self::apply_statistics_unit_count(
                &mut unit_entry.lost,
                &mut unit_entry.lost_hidden,
                row.lost_count(),
                row.lost_hidden(),
            );
            unit_entry.kills = unit_entry.kills.saturating_add(row.kills());
            if !row.created_hidden() || commander == "Tychus" {
                unit_entry.made = unit_entry.made.saturating_add(1);
            }

            if mc_unit_bonus_kills > 0 && is_mc_bonus_target {
                unit_entry.kills = unit_entry.kills.saturating_add(mc_unit_bonus_kills);
                let kills_in_game = row.kills().saturating_add(mc_unit_bonus_kills);
                if player_kills > 0 {
                    unit_entry
                        .kill_percentages
                        .push(kills_in_game as f64 / player_kills as f64);
                } else {
                    unit_entry.kill_percentages.push(1.0);
                }
                mc_unit_bonus_kills = 0;
            } else if player_kills > 0 {
                unit_entry
                    .kill_percentages
                    .push(row.kills() as f64 / player_kills as f64);
            }
        }
    }

    fn statistics_mind_control_bonus_kills(
        commander: &str,
        mc_unit: Option<&str>,
        rows: &[StatisticsPlayerUnitFactRow],
    ) -> i64 {
        let Some(mc_unit_name) = mc_unit else {
            return 0;
        };
        if !rows.iter().any(|row| row.unit_name() == mc_unit_name) {
            return 0;
        }
        rows.iter()
            .filter(|row| {
                (!row.created_hidden() && row.created_count() == 0)
                    || (commander != "Fenix" && row.unit_name() == "Disruptor")
                    || (commander != "Tychus" && row.unit_name() == "Auto-Turret")
            })
            .fold(0_i64, |total, row| total.saturating_add(row.kills()))
    }

    fn apply_statistics_unit_count(
        target: &mut i64,
        hidden_target: &mut bool,
        count: i64,
        hidden: bool,
    ) {
        if hidden {
            *hidden_target = true;
        } else if !*hidden_target {
            *target = target.saturating_add(count);
        }
    }

    fn statistics_unit_rollup_for_side<'a>(
        main_rollup: &'a mut BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &'a mut BTreeMap<String, CommanderUnitRollup>,
        side: StatisticsUnitSide,
    ) -> &'a mut BTreeMap<String, CommanderUnitRollup> {
        match side {
            StatisticsUnitSide::Main => main_rollup,
            StatisticsUnitSide::Ally => ally_rollup,
        }
    }
}
