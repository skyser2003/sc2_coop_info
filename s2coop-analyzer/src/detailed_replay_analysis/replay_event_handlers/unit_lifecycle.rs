use super::*;

pub(super) struct ReplayUnitLifecycleEventHandlers;

impl ReplayUnitLifecycleEventHandlers {
    pub(super) fn replay_handle_unit_born_or_init_event_fields<'a>(
        input: UnitBornOrInitHandlerInput<'_, 'a>,
    ) -> UnitBornOrInitUpdate<'a> {
        let UnitBornOrInitHandlerInput {
            event,
            main_player,
            ally_player,
            amon_players,
            unit_dict,
            start_time,
            unit_type_dict_main,
            unit_type_dict_ally,
            unit_type_dict_amon,
            mutator_dehaka_drag_unit_ids,
            murvar_spawns,
            glevig_spawns,
            broodlord_broodlings,
            outlaw_order,
            outlaw_order_seen,
            wave_units,
            identified_waves,
            abathur_kill_locusts,
            last_biomass_position,
            revival_types,
            primal_combat_predecessors,
            tychus_outlaws,
            units_in_waves,
            string_sets,
        } = input;
        let unit_type = event.unit_type;
        let unit_id = event.unit_id;
        let control_pid = event.control_pid;
        let gameloop = event.gameloop;
        let game_time = gameloop as f64 / 16.0;

        unit_dict.insert(
            unit_id,
            UnitSnapshot {
                unit_type: unit_type.to_owned(),
                control_pid,
            },
        );

        if string_sets.contains_murvar_spawn_unit(unit_type)
            && event.ability_name == Some("CoopMurvarSpawnCreepers")
        {
            murvar_spawns.insert(unit_id);
        }

        if string_sets.contains_glevig_spawn_unit(unit_type) {
            glevig_spawns.insert(unit_id);
        }

        let is_broodling_unit = string_sets.contains_broodling_unit(unit_type);
        if is_broodling_unit
            && event.creator_unit_id.is_some()
            && let Some(creator_id) = event.creator_unit_id
            && let Some(creator_row) = unit_dict.get(&creator_id)
        {
            let creator_type = creator_row.unit_type.as_str();
            if string_sets.contains_broodling_escort_unit(creator_type) {
                broodlord_broodlings.insert(unit_id);
            }
        }

        if let Some(revival_target) = revival_types.get(unit_type)
            && (control_pid == 1 || control_pid == 2)
            && game_time > start_time + 1.0
        {
            if control_pid == main_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    revival_target.as_str(),
                    1,
                    1,
                    0,
                );
            }
            if control_pid == ally_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    revival_target.as_str(),
                    1,
                    1,
                    0,
                );
            }
        }

        if let Some(predecessor) = primal_combat_predecessors.get(unit_type) {
            if control_pid == main_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    predecessor.as_str(),
                    0,
                    -2,
                    0,
                );
            }
            if control_pid == ally_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    predecessor.as_str(),
                    0,
                    -2,
                    0,
                );
            }
        }

        let is_broodlord_broodling = is_broodling_unit && broodlord_broodlings.contains(&unit_id);
        let mut created_event: Option<(StatsCounterTarget, &str)> = None;
        if !glevig_spawns.contains(&unit_id)
            && !murvar_spawns.contains(&unit_id)
            && !is_broodlord_broodling
        {
            if control_pid == main_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    unit_type,
                    1,
                    0,
                    0,
                );
                created_event = Some((StatsCounterTarget::Main, unit_type));
            } else if control_pid == ally_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    unit_type,
                    1,
                    0,
                    0,
                );
                created_event = Some((StatsCounterTarget::Ally, unit_type));
            } else if amon_players.contains(control_pid) {
                if event.ability_name == Some("MutatorAmonDehakaDrag") {
                    mutator_dehaka_drag_unit_ids.insert(unit_id);
                } else {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_amon,
                        unit_type,
                        1,
                        0,
                        0,
                    );
                }
            }
        }

        if tychus_outlaws.contains(unit_type)
            && (control_pid == 1 || control_pid == 2)
            && !outlaw_order_seen.contains(unit_type)
        {
            outlaw_order_seen.insert(unit_type.to_owned());
            outlaw_order.push(unit_type.to_owned());
        }

        if matches!(control_pid, 3..=6)
            && game_time > start_time + 60.0
            && units_in_waves.contains(unit_type)
        {
            if wave_units.second_gameloop == gameloop {
                wave_units.units.push(unit_type.to_owned());
            } else {
                wave_units.second_gameloop = gameloop;
                wave_units.units.clear();
                wave_units.units.push(unit_type.to_owned());
            }

            if wave_units.units.len() > 5 {
                identified_waves.insert(gameloop, wave_units.units.clone());
            }
        }

        let mut last_biomass = last_biomass_position;
        let event_x = event.event_x;
        let event_y = event.event_y;

        if unit_type == "BiomassPickup" {
            last_biomass = [event_x, event_y, gameloop];
        }

        if unit_type == "Locust" && [event_x, event_y, gameloop] == last_biomass {
            abathur_kill_locusts.insert(unit_id);
        }

        UnitBornOrInitUpdate {
            unit_id,
            last_biomass_position: last_biomass,
            created_event,
        }
    }

    pub(super) fn replay_handle_archon_init_event_control_pid(
        control_pid: i64,
        dt_ht_ignore: &mut [i64],
    ) {
        if let Ok(index) = usize::try_from(control_pid)
            && let Some(value) = dt_ht_ignore.get_mut(index)
        {
            *value += 2;
        }
    }

    pub(super) fn replay_handle_unit_type_change_event_fields<'a>(
        input: UnitTypeChangeHandlerInput<'_, 'a>,
    ) -> UnitTypeChangeUpdate<'a> {
        let UnitTypeChangeHandlerInput {
            event,
            map_flags,
            main_player,
            ally_player,
            amon_players,
            unit_dict,
            unit_type_dict_main,
            unit_type_dict_ally,
            unit_type_dict_amon,
            start_time,
            bonus_timings,
            legacy_spawn_filter_unit_id,
            glevig_spawns,
            murvar_spawns,
            zagaras_dummy_zerglings,
            broodlord_broodlings,
            research_vessel_landed_timing,
            units_killed_in_morph,
            unit_name_dict,
            unit_add_losses_to,
            dont_count_morphs,
            string_sets,
        } = input;
        let mut update = UnitTypeChangeUpdate {
            landed_timing: research_vessel_landed_timing,
            unit_change_event: None,
        };
        let Some(unit_row) = unit_dict.get_mut(&event.event_unit_id) else {
            return update;
        };

        let control_pid = unit_row.control_pid;
        let unit_type = event.unit_type;
        let gameloop = event.gameloop;

        if control_pid == 7 && unit_type == "ResearchVesselLanded" {
            update.landed_timing = Some(gameloop);
        }
        if control_pid == 7
            && unit_type == "ResearchVessel"
            && let Some(timing) = update.landed_timing
            && timing + 1500 > gameloop
        {
            bonus_timings.push(gameloop as f64 / 16.0 - start_time);
            update.landed_timing = None;
        }

        if map_flags.is_scythe_of_amon() && control_pid == 11 && unit_type == "WarpPrismPhasing" {
            bonus_timings.push(gameloop as f64 / 16.0 - start_time);
        }

        if units_killed_in_morph.contains(unit_type) {
            return update;
        }

        let old_unit_type = std::mem::replace(&mut unit_row.unit_type, unit_type.to_owned());

        if control_pid == main_player {
            update.unit_change_event =
                Some((StatsCounterTarget::Main, unit_type, old_unit_type.clone()));
        } else if control_pid == ally_player {
            update.unit_change_event =
                Some((StatsCounterTarget::Ally, unit_type, old_unit_type.clone()));
        }

        let new_display_name = unit_name_dict.get(unit_type);
        let old_display_name = unit_name_dict.get(old_unit_type.as_str());
        if let (Some(new_display_name), Some(old_display_name)) =
            (new_display_name, old_display_name)
        {
            if old_unit_type == "BanelingCocoon" && unit_type == "HotSSwarmling" {
                zagaras_dummy_zerglings.insert(event.event_unit_id);
                return update;
            }

            let names_differ = new_display_name != old_display_name;
            // Preserve the historical Python loop-variable quirk used by the original cache.
            let is_broodlord_broodling = string_sets.contains_broodling_unit(unit_type)
                && broodlord_broodlings.contains(&legacy_spawn_filter_unit_id);
            let is_suppressed_spawn = glevig_spawns.contains(&legacy_spawn_filter_unit_id)
                || murvar_spawns.contains(&legacy_spawn_filter_unit_id)
                || is_broodlord_broodling;
            let should_add_created = names_differ
                && !unit_add_losses_to.contains(old_unit_type.as_str())
                && !is_suppressed_spawn
                && !dont_count_morphs.contains(unit_type);

            if should_add_created {
                if control_pid == main_player {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_main,
                        unit_type,
                        1,
                        0,
                        0,
                    );
                } else if control_pid == ally_player {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_ally,
                        unit_type,
                        1,
                        0,
                        0,
                    );
                } else if amon_players.contains(control_pid) {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_amon,
                        unit_type,
                        1,
                        0,
                        0,
                    );
                }
            } else {
                if control_pid == main_player {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_main,
                        unit_type,
                        0,
                        0,
                        0,
                    );
                }
                if control_pid == ally_player {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_ally,
                        unit_type,
                        0,
                        0,
                        0,
                    );
                }
                if amon_players.contains(control_pid) {
                    ReplayEventHandlerHelpers::update_unit_count(
                        unit_type_dict_amon,
                        unit_type,
                        0,
                        0,
                        0,
                    );
                }
            }
        }

        update
    }

    pub(super) fn replay_handle_unit_owner_change_event_fields(
        input: UnitOwnerChangeHandlerInput<'_>,
    ) -> UnitOwnerChangeUpdate {
        let UnitOwnerChangeHandlerInput {
            event_unit_id,
            map_flags,
            control_pid,
            main_player,
            ally_player,
            amon_players,
            unit_dict,
            game_time,
            bonus_timings,
            mw_bonus_initial_timing,
        } = input;
        let mut update = UnitOwnerChangeUpdate::default();
        let Some(unit_row) = unit_dict.get_mut(&event_unit_id) else {
            return update;
        };
        let losing_player = unit_row.control_pid;

        if control_pid == main_player && amon_players.contains(losing_player) {
            update.mind_controlled_unit_id = Some(event_unit_id);
            update.icon_target = Some(StatsCounterTarget::Main);
        } else if control_pid == ally_player && amon_players.contains(losing_player) {
            update.mind_controlled_unit_id = Some(event_unit_id);
            update.icon_target = Some(StatsCounterTarget::Ally);
        }

        unit_row.control_pid = control_pid;

        if map_flags.is_malwarfare() {
            let first_time = mw_bonus_initial_timing[0];
            let second_time = mw_bonus_initial_timing[1];

            if control_pid == 9 {
                mw_bonus_initial_timing[0] = game_time;
            } else if control_pid == 10 {
                mw_bonus_initial_timing[1] = game_time;
            } else if control_pid == 6 {
                if second_time != 0.0 && game_time - second_time != 245.9375 {
                    bonus_timings.push(game_time);
                }
                if first_time != 0.0 && second_time == 0.0 && game_time - first_time != 245.9375 {
                    bonus_timings.push(game_time);
                }
            }
        }

        update
    }
}
