use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ReplayUnitCountValue {
    #[default]
    Missing,
    Number(i64),
    Hidden,
}

impl ReplayUnitCountValue {
    fn is_explicit_zero(self) -> bool {
        matches!(self, Self::Number(0))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReplayUnitRow {
    created: ReplayUnitCountValue,
    lost: ReplayUnitCountValue,
    kills: i64,
}

impl ReplayAnalysisOps {
    fn replay_unit_count_value(value: Option<&Value>) -> ReplayUnitCountValue {
        value
            .and_then(Value::as_i64)
            .map(ReplayUnitCountValue::Number)
            .or_else(|| {
                value
                    .and_then(Value::as_f64)
                    .filter(|entry| entry.is_finite())
                    .map(|entry| ReplayUnitCountValue::Number(entry.round() as i64))
            })
            .or_else(|| {
                value
                    .filter(|entry| entry.is_string())
                    .map(|_| ReplayUnitCountValue::Hidden)
            })
            .unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn numeric_unit_stat_value(value: Option<&Value>) -> i64 {
        match ReplayAnalysisOps::replay_unit_count_value(value) {
            ReplayUnitCountValue::Number(number) => number,
            ReplayUnitCountValue::Missing | ReplayUnitCountValue::Hidden => 0,
        }
    }
}

impl ReplayAnalysisOps {
    fn replay_unit_row(row: &[Value]) -> ReplayUnitRow {
        ReplayUnitRow {
            created: ReplayAnalysisOps::replay_unit_count_value(row.first()),
            lost: ReplayAnalysisOps::replay_unit_count_value(row.get(1)),
            kills: ReplayAnalysisOps::numeric_unit_stat_value(row.get(2)),
        }
    }
}

impl ReplayAnalysisOps {
    fn apply_replay_unit_count(target: &mut i64, hidden: &mut bool, value: ReplayUnitCountValue) {
        match value {
            ReplayUnitCountValue::Number(number) if !*hidden => {
                *target = target.saturating_add(number);
            }
            ReplayUnitCountValue::Hidden => {
                *hidden = true;
            }
            ReplayUnitCountValue::Missing | ReplayUnitCountValue::Number(_) => {}
        }
    }
}

impl ReplayAnalysisOps {
    pub fn append_units_to_rollup_with_dictionary(
        side_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
        dictionary: &Sc2DictionaryData,
    ) {
        let commander = TauriOverlayOps::sanitize_replay_text(commander_name);
        if commander.trim().is_empty() {
            return;
        }
        let Some(units) = units_payload.as_object() else {
            return;
        };

        let commander_entry = side_rollup.entry(commander.clone()).or_default();
        commander_entry.count = commander_entry.count.saturating_add(1);

        let mut replay_units: Vec<(String, ReplayUnitRow)> = Vec::new();
        for (unit_name, row) in units {
            let Some(values) = row.as_array() else {
                continue;
            };
            replay_units.push((
                TauriOverlayOps::sanitize_replay_text(unit_name),
                ReplayAnalysisOps::replay_unit_row(values),
            ));
        }

        let mc_unit = dictionary.commander_mind_control_unit(&commander);
        let mut mc_unit_bonus_kills = 0_i64;
        if let Some(mc_unit_name) = mc_unit
            && replay_units.iter().any(|(unit, _)| unit == mc_unit_name)
        {
            for (unit, row) in &replay_units {
                if row.created.is_explicit_zero()
                    || (commander != "Fenix" && unit == "Disruptor")
                    || (commander != "Tychus" && unit == "Auto-Turret")
                {
                    mc_unit_bonus_kills = mc_unit_bonus_kills.saturating_add(row.kills);
                }
            }
        }

        for (unit, row) in replay_units {
            let is_mc_bonus_target = mc_unit == Some(unit.as_str());
            let entry = commander_entry.units.entry(unit.clone()).or_default();
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.created,
                &mut entry.created_hidden,
                row.created,
            );
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.lost,
                &mut entry.lost_hidden,
                row.lost,
            );
            entry.kills = entry.kills.saturating_add(row.kills);
            if !matches!(row.created, ReplayUnitCountValue::Hidden) || commander == "Tychus" {
                entry.made = entry.made.saturating_add(1);
            }

            if mc_unit_bonus_kills > 0 && is_mc_bonus_target {
                entry.kills = entry.kills.saturating_add(mc_unit_bonus_kills);
                let kills_in_game = row.kills.saturating_add(mc_unit_bonus_kills);
                if player_kills > 0 {
                    entry
                        .kill_percentages
                        .push(kills_in_game as f64 / player_kills as f64);
                } else {
                    entry.kill_percentages.push(1.0);
                }
                mc_unit_bonus_kills = 0;
            } else if player_kills > 0 {
                entry
                    .kill_percentages
                    .push(row.kills as f64 / player_kills as f64);
            }
        }
    }
}

impl ReplayAnalysisOps {
    pub fn append_units_to_rollup(
        side_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
    ) {
        let commander = TauriOverlayOps::sanitize_replay_text(commander_name);
        if commander.trim().is_empty() {
            return;
        }
        let Some(units) = units_payload.as_object() else {
            return;
        };

        let commander_entry = side_rollup.entry(commander.clone()).or_default();
        commander_entry.count = commander_entry.count.saturating_add(1);

        for (unit_name, row) in units {
            let Some(values) = row.as_array() else {
                continue;
            };
            let row = ReplayAnalysisOps::replay_unit_row(values);
            let entry = commander_entry
                .units
                .entry(TauriOverlayOps::sanitize_replay_text(unit_name))
                .or_default();
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.created,
                &mut entry.created_hidden,
                row.created,
            );
            ReplayAnalysisOps::apply_replay_unit_count(
                &mut entry.lost,
                &mut entry.lost_hidden,
                row.lost,
            );
            entry.kills = entry.kills.saturating_add(row.kills);
            if !matches!(row.created, ReplayUnitCountValue::Hidden) || commander == "Tychus" {
                entry.made = entry.made.saturating_add(1);
            }
            if player_kills > 0 {
                entry
                    .kill_percentages
                    .push(row.kills as f64 / player_kills as f64);
            }
        }
    }
}

impl ReplayAnalysisOps {
    fn append_player_units_to_rollups_with_dictionary(
        main_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        input: PlayerUnitRollupInput<'_>,
    ) {
        if ReplayAnalysis::is_main_player_by_handle(input.player_handle, input.main_handles) {
            ReplayAnalysisOps::append_units_to_rollup_with_dictionary(
                main_rollup,
                input.commander_name,
                input.units_payload,
                input.player_kills,
                input.dictionary,
            );
        } else {
            ReplayAnalysisOps::append_units_to_rollup_with_dictionary(
                ally_rollup,
                input.commander_name,
                input.units_payload,
                input.player_kills,
                input.dictionary,
            );
        }
    }
}

impl ReplayAnalysisOps {
    pub fn build_unit_data_from_replays_with_dictionary<R>(
        replays: &[R],
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> Value
    where
        R: Borrow<ReplayInfo>,
    {
        let mut main_rollup: std::collections::BTreeMap<String, CommanderUnitRollup> =
            std::collections::BTreeMap::new();
        let mut ally_rollup: std::collections::BTreeMap<String, CommanderUnitRollup> =
            std::collections::BTreeMap::new();
        let mut amon_rollup: std::collections::BTreeMap<String, UnitStatsRollup> =
            std::collections::BTreeMap::new();

        let mut append_amon_units = |units_payload: &Value| {
            let Some(units) = units_payload.as_object() else {
                return;
            };
            for (unit_name, row) in units {
                let Some(values) = row.as_array() else {
                    continue;
                };
                let created = ReplayAnalysisOps::numeric_unit_stat_value(values.first());
                let lost = ReplayAnalysisOps::numeric_unit_stat_value(values.get(1));
                let kills = ReplayAnalysisOps::numeric_unit_stat_value(values.get(2));
                if created == 0 && lost == 0 && kills == 0 {
                    continue;
                }
                let entry = amon_rollup
                    .entry(TauriOverlayOps::sanitize_replay_text(unit_name))
                    .or_default();
                entry.created = entry.created.saturating_add(created);
                entry.lost = entry.lost.saturating_add(lost);
                entry.kills = entry.kills.saturating_add(kills);
            }
        };

        for replay in replays.iter().map(Borrow::borrow) {
            if replay.result == "Unparsed" {
                continue;
            }
            if dictionary.canonicalize_coop_map_id(&replay.map).is_none() {
                continue;
            }

            ReplayAnalysisOps::append_player_units_to_rollups_with_dictionary(
                &mut main_rollup,
                &mut ally_rollup,
                PlayerUnitRollupInput {
                    commander_name: replay.main_commander(),
                    units_payload: replay.main_units(),
                    player_kills: replay.main_kills(),
                    player_handle: &replay.main().handle,
                    main_handles,
                    dictionary,
                },
            );
            ReplayAnalysisOps::append_player_units_to_rollups_with_dictionary(
                &mut main_rollup,
                &mut ally_rollup,
                PlayerUnitRollupInput {
                    commander_name: replay.ally_commander(),
                    units_payload: replay.ally_units(),
                    player_kills: replay.ally_kills(),
                    player_handle: &replay.ally().handle,
                    main_handles,
                    dictionary,
                },
            );
            append_amon_units(&replay.amon_units);
        }

        ReplayAnalysisOps::report_value(&StatsAggregateUnitDataPayload::new(
            StatsUnitDataOps::build_commander_unit_data_with_dictionary(main_rollup, dictionary),
            StatsUnitDataOps::build_commander_unit_data_with_dictionary(ally_rollup, dictionary),
            StatsUnitDataOps::build_amon_unit_data(amon_rollup),
        ))
    }
}

impl ReplayAnalysisOps {
    pub fn append_player_units_to_rollups(
        main_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        ally_rollup: &mut std::collections::BTreeMap<String, CommanderUnitRollup>,
        commander_name: &str,
        units_payload: &Value,
        player_kills: u64,
        player_handle: &str,
        main_handles: &HashSet<String>,
    ) {
        if ReplayAnalysis::is_main_player_by_handle(player_handle, main_handles) {
            ReplayAnalysisOps::append_units_to_rollup(
                main_rollup,
                commander_name,
                units_payload,
                player_kills,
            );
        } else {
            ReplayAnalysisOps::append_units_to_rollup(
                ally_rollup,
                commander_name,
                units_payload,
                player_kills,
            );
        }
    }
}
