use super::*;

pub(super) struct ReplayUnitDeathEventHandlers;

impl ReplayUnitDeathEventHandlers {
    pub(super) fn replay_handle_unit_died_kill_stats_event_fields(
        input: UnitDiedKillStatsHandlerInput<'_>,
    ) -> i64 {
        let UnitDiedKillStatsHandlerInput {
            killed_row,
            killing_player,
            gameloop,
            main_player,
            ally_player,
            amon_players,
            killcounts,
            ally_kills_transfer_to_main,
            last_aoe_unit_killed,
            ally_kills_counted_toward_main,
            do_not_count_kills,
            aoe_units,
        } = input;
        let mut ally_kills = ally_kills_counted_toward_main;
        let Some(unit_row) = killed_row else {
            return ally_kills;
        };
        let killed_unit_type = unit_row.unit_type.as_str();
        let losing_player = unit_row.control_pid;
        let losing_player_is_amon = amon_players.contains(losing_player);
        let losing_player_is_coop = losing_player == 1 || losing_player == 2;
        let killing_player_is_coop = matches!(killing_player, Some(1 | 2));

        if let Some(killer) = killing_player
            && !do_not_count_kills.contains(killed_unit_type)
        {
            let killer_is_amon = amon_players.contains(killer);
            if (killer == 1 || killer == 2) && !losing_player_is_amon {
                // ignore player-vs-player kills
            } else if killer_is_amon && !losing_player_is_coop {
                // ignore amon-vs-amon kills
            } else if killer == ally_player {
                if ally_kills_transfer_to_main {
                    ReplayEventHandlerHelpers::increment_i64_key(killcounts, main_player, 1);
                    ally_kills += 1;
                } else {
                    ReplayEventHandlerHelpers::increment_i64_key(killcounts, killer, 1);
                }
            } else {
                ReplayEventHandlerHelpers::increment_i64_key(killcounts, killer, 1);
            }
        }

        if aoe_units.contains(killed_unit_type)
            && killing_player_is_coop
            && losing_player_is_amon
            && let Ok(index) = usize::try_from(losing_player)
            && let Some(slot) = last_aoe_unit_killed.get_mut(index)
        {
            *slot = Some((killed_unit_type.to_owned(), gameloop as f64 / 16.0));
        }

        ally_kills
    }

    pub(super) fn replay_handle_unit_died_detail_event_fields<'a>(
        input: UnitDiedDetailHandlerInput<'_, 'a>,
    ) -> UnitDiedDetailUpdate<'a> {
        let UnitDiedDetailHandlerInput {
            event,
            killed_row,
            map_flags,
            main_player,
            ally_player,
            amon_players,
            unit_id,
            unit_type_dict_main,
            unit_type_dict_ally,
            unit_type_dict_amon,
            unit_dict,
            dt_ht_ignore,
            start_time,
            commander_by_player,
            killbot_feed,
            custom_kill_count,
            used_mutator_spider_mines,
            bonus_timings,
            abathur_kill_locusts,
            mutator_dehaka_drag_unit_ids,
            murvar_spawns,
            glevig_spawns,
            broodlord_broodlings,
            unit_killed_by,
            mind_controlled_units,
            zagaras_dummy_zerglings,
            last_aoe_unit_killed,
            commander_no_units,
            commander_no_units_values,
            hfts_units,
            tus_units,
            do_not_count_kills,
            self_killing_units,
            duplicating_units,
            salvage_units,
            string_sets,
        } = input;
        let mut update = UnitDiedDetailUpdate {
            current_unit_id: unit_id,
            salvaged_unit: None,
            mindcontrolled_unit_died: None,
        };
        let event_unit_id = event.event_unit_id;
        update.current_unit_id = event_unit_id;
        let killing_unit_id = event.killing_unit_id;
        let killing_player = event.killing_player;

        let killed_unit_type = killed_row.unit_type.as_str();
        let losing_player = killed_row.control_pid;
        let losing_player_is_coop = losing_player == 1 || losing_player == 2;
        let losing_player_is_amon = amon_players.contains(losing_player);
        let killing_player_is_coop = matches!(killing_player, Some(1 | 2));
        let killing_player_is_amon = killing_player
            .map(|value| amon_players.contains(value))
            .unwrap_or(false);
        let killing_player_is_main = killing_player == Some(main_player);
        let killing_player_is_ally = killing_player == Some(ally_player);
        let commander = killing_player
            .and_then(|pid| commander_by_player.get(&pid))
            .map(String::as_str);

        let mut killer_in_unit_dict = false;
        let mut killing_unit_type = if let Some(killer_id) = killing_unit_id {
            if let Some(row) = unit_dict.get(&killer_id) {
                killer_in_unit_dict = true;
                row.unit_type.as_str()
            } else {
                "NoUnit"
            }
        } else {
            "NoUnit"
        };

        if killing_unit_type == "NoUnit"
            && let Some(commander_name) = commander
            && let Some(backup_units) = commander_no_units.get(commander_name)
        {
            let source_dict: &UnitTypeCountMap = if killing_player == Some(main_player) {
                &*unit_type_dict_main
            } else {
                &*unit_type_dict_ally
            };
            for backup_unit in backup_units {
                if source_dict.contains_key(backup_unit.as_str()) {
                    killing_unit_type = backup_unit.as_str();
                    break;
                }
            }
        }

        if string_sets.contains_killbot_unit(killing_unit_type) && losing_player_is_coop {
            ReplayEventHandlerHelpers::increment_i64_key(killbot_feed, losing_player, 1);
        }

        if killing_unit_type == "Locust"
            && commander == Some("Abathur")
            && !killing_unit_id
                .map(|value| abathur_kill_locusts.contains(&value))
                .unwrap_or(false)
        {
            if killing_player_is_main && unit_type_dict_main.contains_key("SwarmHost") {
                killing_unit_type = "SwarmHost";
            }
            if killing_player_is_ally && unit_type_dict_ally.contains_key("SwarmHost") {
                killing_unit_type = "SwarmHost";
            }
        } else if string_sets.contains_glevig_killer_unit(killing_unit_type)
            && killing_unit_id.is_some_and(|value| glevig_spawns.contains(&value))
        {
            killing_unit_type = "Glevig";
        } else if string_sets.contains_murvar_spawn_unit(killing_unit_type)
            && killing_unit_id.is_some_and(|value| murvar_spawns.contains(&value))
        {
            killing_unit_type = "Murvar";
        } else if killing_unit_id.is_some_and(|value| broodlord_broodlings.contains(&value))
            && string_sets.contains_broodling_unit(killing_unit_type)
        {
            if killing_unit_type == "Broodling" {
                killing_unit_type = "BroodLord";
            } else if killing_unit_type == "BroodlingStetmann" {
                killing_unit_type = "BroodLordStetmann";
            }
        }

        if killing_player_is_coop && losing_player_is_amon {
            let killer = killing_player.unwrap_or_default();
            if hfts_units.contains(killed_unit_type) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "hfts",
                    killer,
                    1,
                );
            }
            if tus_units.contains(killed_unit_type) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "tus",
                    killer,
                    1,
                );
            }
            if let Some(category) = string_sets.custom_kill_count_category(killed_unit_type) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    category,
                    killer,
                    1,
                );
            }
        }

        if losing_player_is_coop
            && killing_player_is_amon
            && killing_unit_type == "MutatorSpiderMine"
            && killing_unit_id
                .map(|value| !used_mutator_spider_mines.contains(&value))
                .unwrap_or(false)
        {
            if let Some(killer_id) = killing_unit_id {
                used_mutator_spider_mines.insert(killer_id);
            }
            ReplayEventHandlerHelpers::increment_nested_player_count(
                custom_kill_count,
                "minesweeper",
                losing_player,
                1,
            );
        }

        if killing_unit_type == "NoUnit"
            && killing_unit_id.is_none()
            && killing_player_is_amon
            && killing_player != Some(losing_player)
            && let Some(killer) = killing_player
            && let Some((last_type, last_time)) = usize::try_from(killer)
                .ok()
                .and_then(|index| last_aoe_unit_killed.get(index))
                .and_then(|entry| entry.as_ref())
            && event.gameloop as f64 / 16.0 - *last_time < 9.0
        {
            ReplayEventHandlerHelpers::update_unit_count(
                unit_type_dict_amon,
                last_type.as_str(),
                0,
                0,
                1,
            );
        }

        if (killer_in_unit_dict || commander_no_units_values.contains(killing_unit_type))
            && killing_unit_id != Some(event_unit_id)
            && killing_player != Some(losing_player)
            && !do_not_count_kills.contains(killed_unit_type)
        {
            if killing_player_is_main && losing_player_is_amon {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    killing_unit_type,
                    0,
                    0,
                    1,
                );
            }
            if killing_player_is_ally && losing_player_is_amon {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    killing_unit_type,
                    0,
                    0,
                    1,
                );
            }
            if killing_player_is_amon && losing_player_is_coop {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_amon,
                    killing_unit_type,
                    0,
                    0,
                    1,
                );
            }
        }

        let game_time = event.gameloop as f64 / 16.0;
        if self_killing_units.contains(killed_unit_type) && killing_player.is_none() {
            if losing_player == main_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    killed_unit_type,
                    -1,
                    0,
                    0,
                );
            }
            if losing_player == ally_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    killed_unit_type,
                    -1,
                    0,
                    0,
                );
            }
            return update;
        }

        if game_time > 0.0
            && duplicating_units.contains(killed_unit_type)
            && killed_unit_type == killing_unit_type
            && killing_player == Some(losing_player)
        {
            if losing_player == main_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    killed_unit_type,
                    -1,
                    0,
                    0,
                );
                return update;
            }
            if losing_player == ally_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    killed_unit_type,
                    -1,
                    0,
                    0,
                );
                return update;
            }
            if killing_player_is_amon {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_amon,
                    killed_unit_type,
                    -1,
                    0,
                    0,
                );
                return update;
            }
        }

        let ignore_count = usize::try_from(losing_player)
            .ok()
            .and_then(|index| dt_ht_ignore.get(index))
            .copied()
            .unwrap_or_default();
        if (killed_unit_type == "HighTemplar" || killed_unit_type == "DarkTemplar")
            && ignore_count > 0
        {
            ReplayEventHandlerHelpers::increment_i64_key(dt_ht_ignore, losing_player, -1);
            return update;
        }

        let event_x = event.event_x;
        let event_y = event.event_y;
        let early_standard_bonus_timing = (map_flags.void_thrashing
            && (killed_unit_type == "ArchAngelCoopFighter"
                || killed_unit_type == "ArchAngelCoopAssault")
            && losing_player == 5)
            || (map_flags.dead_of_night
                && killed_unit_type == "ACVirophage"
                && losing_player == 7
                && killing_player_is_coop)
            || (map_flags.lock_and_load
                && killed_unit_type == "XelNagaConstruct"
                && losing_player == 3)
            || (map_flags.chain_of_ascension
                && killed_unit_type == "SlaynElemental"
                && losing_player == 10
                && killing_player_is_coop)
            || (map_flags.rifts_to_korhal
                && killed_unit_type == "ACPirateCapitalShip"
                && losing_player == 8
                && killing_player_is_coop);
        let late_standard_bonus_timing = (map_flags.oblivion_express
            && killed_unit_type == "TarsonisEngineFast"
            && losing_player == 7
            && event_x < 196)
            || (map_flags.mist_opportunities
                && killed_unit_type == "COOPTerrazineTank"
                && losing_player == 3
                && killing_player_is_coop)
            || (map_flags.vermillion_problem
                && (killed_unit_type == "RedstoneSalamander"
                    || killed_unit_type == "RedstoneSalamanderBurrowed")
                && losing_player == 9
                && killing_player_is_coop)
            || (map_flags.miner_evacuation
                && killed_unit_type == "Blightbringer"
                && losing_player == 5
                && killing_player_is_coop);

        let bonus_timing = if early_standard_bonus_timing {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.cradle_of_death
            && killed_unit_type == "LogisticsHeadquarters"
            && losing_player == 3
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time - 8.0,
                0,
            ))
        } else if map_flags.part_and_parcel
            && (killed_unit_type == "Caboose" || killed_unit_type == "TarsonisEngine")
            && losing_player == 8
            && !(event_x == 169 && event_y == 99)
            && !(event_x == 38 && event_y == 178)
        {
            let rounded_bonus =
                ReplayEventHandlerHelpers::round_to_digits_half_even(game_time - start_time, 0);
            if bonus_timings.len() < 2 && !bonus_timings.contains(&rounded_bonus) {
                Some(rounded_bonus)
            } else {
                None
            }
        } else if late_standard_bonus_timing {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.miner_evacuation
            && killed_unit_type == "NovaEradicator"
            && losing_player == 9
            && killing_player_is_coop
        {
            let nova_eradicator_lost = unit_type_dict_amon
                .get("NovaEradicator")
                .map(|row| row[1])
                .unwrap_or_default();
            if nova_eradicator_lost == 1 {
                Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                    game_time - start_time,
                    0,
                ))
            } else {
                None
            }
        } else if map_flags.temple_of_the_past
            && killed_unit_type == "ZenithStone"
            && losing_player == 8
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else {
            None
        };
        if let Some(value) = bonus_timing {
            bonus_timings.push(value);
        }

        let is_salvaged_death =
            salvage_units.contains(killed_unit_type) && killing_player == Some(losing_player);
        if is_salvaged_death {
            if losing_player == main_player {
                update.salvaged_unit = Some((StatsCounterTarget::Main, killed_unit_type));
            } else if losing_player == ally_player {
                update.salvaged_unit = Some((StatsCounterTarget::Ally, killed_unit_type));
            }
        }

        let killed_is_broodlord_broodling = string_sets.contains_broodling_unit(killed_unit_type)
            && broodlord_broodlings.contains(&event_unit_id);
        if is_salvaged_death
            || glevig_spawns.contains(&event_unit_id)
            || murvar_spawns.contains(&event_unit_id)
            || killed_is_broodlord_broodling
        {
            return update;
        }

        if zagaras_dummy_zerglings.contains(&event_unit_id) && killing_player.is_none() {
            return update;
        }

        let losing_commander = commander_by_player
            .get(&losing_player)
            .map(String::as_str)
            .unwrap_or_default();
        if string_sets.contains_abathur_free_death_unit(killed_unit_type)
            && losing_commander == "Abathur"
            && killing_player.is_none()
        {
            return update;
        }

        if killed_unit_type == "Drone" && killing_player.is_none() {
            return update;
        }

        if losing_player == main_player && game_time > 0.0 && game_time > start_time + 1.0 {
            ReplayEventHandlerHelpers::update_unit_count(
                unit_type_dict_main,
                killed_unit_type,
                0,
                1,
                0,
            );
            ReplayEventHandlerHelpers::append_to_text_list_mapping(
                unit_killed_by,
                killed_unit_type,
                killing_unit_type,
            );

            if mind_controlled_units.contains(&event_unit_id) {
                update.mindcontrolled_unit_died =
                    Some((StatsCounterTarget::Main, killed_unit_type));
            }
        }

        if losing_player == ally_player && game_time > 0.0 && game_time > start_time + 1.0 {
            ReplayEventHandlerHelpers::update_unit_count(
                unit_type_dict_ally,
                killed_unit_type,
                0,
                1,
                0,
            );
            if mind_controlled_units.contains(&event_unit_id) {
                update.mindcontrolled_unit_died =
                    Some((StatsCounterTarget::Ally, killed_unit_type));
            }
        }

        if losing_player_is_amon
            && game_time > 0.0
            && game_time > start_time + 1.0
            && !mutator_dehaka_drag_unit_ids.contains(&event_unit_id)
        {
            ReplayEventHandlerHelpers::update_unit_count(
                unit_type_dict_amon,
                killed_unit_type,
                0,
                1,
                0,
            );
        }

        update
    }
}
