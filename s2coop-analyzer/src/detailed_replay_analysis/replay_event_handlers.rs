use crate::dictionary_data::{CoMasteryUpgradesJson, PrestigeUpgradesJson, UnitNamesJson};
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) type NestedPlayerCountMap = IndexMap<String, IndexMap<i64, i64>>;
pub(super) type TextListMapping = IndexMap<String, Vec<String>>;
pub(super) type UnitTypeCountMap = IndexMap<String, [i64; 4]>;
pub(super) type IdentifiedWavesMap = BTreeMap<i64, Vec<String>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UnitSnapshot {
    pub(super) unit_type: String,
    pub(super) control_pid: i64,
}

pub(super) type UnitStateMap = IndexMap<i64, UnitSnapshot>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WaveUnitsState {
    pub(super) second_gameloop: i64,
    pub(super) units: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StatsCounterTarget {
    Main,
    Ally,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlayerStatsUpdate {
    pub(super) target: StatsCounterTarget,
    pub(super) kills: i64,
    pub(super) supply_used: f64,
    pub(super) collection_rate: f64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UpgradeEventUpdate {
    pub(super) target: Option<StatsCounterTarget>,
    pub(super) commander_name: Option<String>,
    pub(super) mastery_index: Option<i64>,
    pub(super) upgrade_count: i64,
    pub(super) prestige_name: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitOwnerChangeUpdate {
    pub(super) mind_controlled_unit_id: Option<i64>,
    pub(super) icon_target: Option<StatsCounterTarget>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitBornOrInitEventFields {
    pub(super) unit_type: String,
    pub(super) ability_name: Option<String>,
    pub(super) unit_id: i64,
    pub(super) creator_unit_id: Option<i64>,
    pub(super) control_pid: i64,
    pub(super) gameloop: i64,
    pub(super) event_x: i64,
    pub(super) event_y: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitBornOrInitUpdate {
    pub(super) unit_id: i64,
    pub(super) last_biomass_position: [i64; 3],
    pub(super) created_event: Option<(StatsCounterTarget, String)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitTypeChangeEventFields {
    pub(super) event_unit_id: i64,
    pub(super) unit_type: String,
    pub(super) gameloop: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitTypeChangeUpdate {
    pub(super) landed_timing: Option<i64>,
    pub(super) unit_change_event: Option<(StatsCounterTarget, String, String)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitDiedEventFields {
    pub(super) event_unit_id: i64,
    pub(super) killing_unit_id: Option<i64>,
    pub(super) killing_player: Option<i64>,
    pub(super) gameloop: i64,
    pub(super) event_x: i64,
    pub(super) event_y: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitDiedDetailUpdate {
    pub(super) current_unit_id: i64,
    pub(super) salvaged_unit: Option<(StatsCounterTarget, String)>,
    pub(super) mindcontrolled_unit_died: Option<(StatsCounterTarget, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IndexedDelta {
    key: i64,
    delta: i64,
}

impl IndexedDelta {
    fn new(key: i64, delta: i64) -> Option<Self> {
        if delta == 0 {
            None
        } else {
            Some(Self { key, delta })
        }
    }
}

struct ReplayEventHandlerHelpers;

impl ReplayEventHandlerHelpers {
    fn update_unit_count(
        unit_dict: &mut UnitTypeCountMap,
        unit_name: &str,
        created_delta: i64,
        lost_delta: i64,
        kills_delta: i64,
    ) {
        let values = if let Some(values) = unit_dict.get_mut(unit_name) {
            values
        } else {
            unit_dict.entry(unit_name.to_owned()).or_insert([0_i64; 4])
        };
        values[0] += created_delta;
        values[1] += lost_delta;
        values[2] += kills_delta;
    }

    fn increment_nested_player_count(
        counts: &mut NestedPlayerCountMap,
        key: &str,
        player: i64,
        delta: i64,
    ) {
        if delta == 0 {
            return;
        }

        let player_row = if let Some(player_row) = counts.get_mut(key) {
            player_row
        } else {
            let mut defaults: IndexMap<i64, i64> = IndexMap::new();
            defaults.insert(1_i64, 0_i64);
            defaults.insert(2_i64, 0_i64);
            counts.entry(key.to_owned()).or_insert(defaults)
        };
        let current = player_row.get(&player).copied().unwrap_or_default();
        player_row.insert(player, current + delta);
    }

    fn append_to_text_list_mapping(mapping: &mut TextListMapping, key: &str, value: &str) {
        if let Some(values) = mapping.get_mut(key) {
            values.push(value.to_owned());
        } else {
            mapping.insert(key.to_owned(), vec![value.to_owned()]);
        }
    }

    fn apply_indexed_delta(container: &mut [i64], delta: IndexedDelta) {
        if let Ok(index) = usize::try_from(delta.key) {
            if let Some(slot) = container.get_mut(index) {
                *slot += delta.delta;
            }
        }
    }

    fn increment_i64_key(container: &mut [i64], key: i64, delta: i64) {
        if let Some(payload) = IndexedDelta::new(key, delta) {
            Self::apply_indexed_delta(container, payload);
        }
    }

    fn round_to_digits_half_even(value: f64, digits: i32) -> f64 {
        if !value.is_finite() {
            return value;
        }
        let factor = 10_f64.powi(digits);
        if !factor.is_finite() || factor == 0.0 {
            return value;
        }

        let scaled = value * factor;
        let floor = scaled.floor();
        let diff = scaled - floor;
        let eps = 1e-12;
        let rounded_scaled = if diff < 0.5 - eps {
            floor
        } else if diff > 0.5 + eps {
            floor + 1.0
        } else {
            let floor_is_even = ((floor / 2.0).fract()).abs() < eps;
            if floor_is_even {
                floor
            } else {
                floor + 1.0
            }
        };

        rounded_scaled / factor
    }
}

pub(super) struct ReplayEventHandlers;

impl ReplayEventHandlers {
    pub(super) fn replay_handle_game_user_leave_event_fields(
        user_id: i64,
        gameloop: f64,
        user_leave_times: &mut IndexMap<i64, f64>,
    ) {
        let user = user_id + 1;
        let leave_time = gameloop / 16.0;
        user_leave_times.insert(user, leave_time);
    }

    pub(super) fn replay_handle_player_stats_event_fields(
        player: i64,
        main_player: i64,
        ally_player: i64,
        supply_used: f64,
        collection_rate: f64,
        killcounts: &[i64],
    ) -> Option<PlayerStatsUpdate> {
        let kills = usize::try_from(player)
            .ok()
            .and_then(|index| killcounts.get(index))
            .copied()
            .unwrap_or_default();

        if player == main_player {
            return Some(PlayerStatsUpdate {
                target: StatsCounterTarget::Main,
                kills,
                supply_used,
                collection_rate,
            });
        }
        if player == ally_player {
            return Some(PlayerStatsUpdate {
                target: StatsCounterTarget::Ally,
                kills,
                supply_used,
                collection_rate,
            });
        }
        None
    }

    pub(super) fn replay_handle_upgrade_event_fields(
        upg_name: &str,
        upg_pid: i64,
        upgrade_count: i64,
        main_player: i64,
        ally_player: i64,
        commander_upgrades: &HashMap<String, String>,
        co_mastery_upgrades: &CoMasteryUpgradesJson,
        prestige_upgrades: &PrestigeUpgradesJson,
    ) -> UpgradeEventUpdate {
        let target = if upg_pid == main_player {
            Some(StatsCounterTarget::Main)
        } else if upg_pid == ally_player {
            Some(StatsCounterTarget::Ally)
        } else {
            None
        };

        let commander_name = commander_upgrades.get(upg_name).cloned();
        let mut mastery_index: Option<i64> = None;
        for upgrades in co_mastery_upgrades.values() {
            if let Some(index) = upgrades.iter().position(|name| name == upg_name) {
                mastery_index = Some(index as i64);
                break;
            }
        }

        let prestige_name = prestige_upgrades
            .values()
            .find_map(|prestige_by_upgrade| prestige_by_upgrade.get(upg_name).cloned());

        UpgradeEventUpdate {
            target,
            commander_name,
            mastery_index,
            upgrade_count,
            prestige_name,
        }
    }

    pub(super) fn replay_handle_unit_born_or_init_event_fields(
        event: &UnitBornOrInitEventFields,
        main_player: i64,
        ally_player: i64,
        amon_players: &HashSet<i64>,
        unit_dict: &mut UnitStateMap,
        start_time: f64,
        unit_type_dict_main: &mut UnitTypeCountMap,
        unit_type_dict_ally: &mut UnitTypeCountMap,
        unit_type_dict_amon: &mut UnitTypeCountMap,
        mutator_dehaka_drag_unit_ids: &mut HashSet<i64>,
        murvar_spawns: &mut HashSet<i64>,
        glevig_spawns: &mut HashSet<i64>,
        broodlord_broodlings: &mut HashSet<i64>,
        outlaw_order: &mut Vec<String>,
        wave_units: &mut WaveUnitsState,
        identified_waves: &mut IdentifiedWavesMap,
        abathur_kill_locusts: &mut HashSet<i64>,
        last_biomass_position: [i64; 3],
        revival_types: &HashMap<String, String>,
        primal_combat_predecessors: &HashMap<String, String>,
        tychus_outlaws: &HashSet<String>,
        units_in_waves: &HashSet<String>,
    ) -> UnitBornOrInitUpdate {
        let unit_type = event.unit_type.as_str();
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

        if matches!(
            unit_type,
            "DehakaLocust" | "DehakaCreeperFlying" | "DehakaLocustFlying" | "DehakaCreeper"
        ) && event.ability_name.as_deref() == Some("CoopMurvarSpawnCreepers")
        {
            murvar_spawns.insert(unit_id);
        }

        if matches!(
            unit_type,
            "CoopDehakaGlevigEggZergling"
                | "CoopDehakaGlevigEggRoach"
                | "CoopDehakaGlevigEggHydralisk"
        ) {
            glevig_spawns.insert(unit_id);
        }

        if (unit_type == "Broodling" || unit_type == "BroodlingStetmann")
            && event.creator_unit_id.is_some()
        {
            if let Some(creator_id) = event.creator_unit_id {
                if let Some(creator_row) = unit_dict.get(&creator_id) {
                    let creator_type = creator_row.unit_type.as_str();
                    if creator_type == "BroodlingEscort"
                        || creator_type == "BroodlingEscortStetmann"
                    {
                        broodlord_broodlings.insert(unit_id);
                    }
                }
            }
        }

        if let Some(revival_target) = revival_types.get(unit_type) {
            if (control_pid == 1 || control_pid == 2) && game_time > start_time + 1.0 {
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

        let is_broodlord_broodling = (unit_type == "Broodling" || unit_type == "BroodlingStetmann")
            && broodlord_broodlings.contains(&unit_id);
        let mut created_event: Option<(StatsCounterTarget, String)> = None;
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
                created_event = Some((StatsCounterTarget::Main, unit_type.to_owned()));
            } else if control_pid == ally_player {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    unit_type,
                    1,
                    0,
                    0,
                );
                created_event = Some((StatsCounterTarget::Ally, unit_type.to_owned()));
            } else if amon_players.contains(&control_pid) {
                if event.ability_name.as_deref() == Some("MutatorAmonDehakaDrag") {
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
            && !outlaw_order.iter().any(|name| name == unit_type)
        {
            outlaw_order.push(unit_type.to_owned());
        }

        if matches!(control_pid, 3 | 4 | 5 | 6)
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
        if let Ok(index) = usize::try_from(control_pid) {
            if let Some(value) = dt_ht_ignore.get_mut(index) {
                *value += 2;
            }
        }
    }

    pub(super) fn replay_handle_unit_type_change_event_fields(
        event: &UnitTypeChangeEventFields,
        map_name: &str,
        main_player: i64,
        ally_player: i64,
        amon_players: &HashSet<i64>,
        unit_dict: &mut UnitStateMap,
        unit_type_dict_main: &mut UnitTypeCountMap,
        unit_type_dict_ally: &mut UnitTypeCountMap,
        unit_type_dict_amon: &mut UnitTypeCountMap,
        start_time: f64,
        bonus_timings: &mut Vec<f64>,
        legacy_spawn_filter_unit_id: i64,
        glevig_spawns: &HashSet<i64>,
        murvar_spawns: &HashSet<i64>,
        zagaras_dummy_zerglings: &mut HashSet<i64>,
        broodlord_broodlings: &HashSet<i64>,
        research_vessel_landed_timing: Option<i64>,
        units_killed_in_morph: &HashSet<String>,
        unit_name_dict: &UnitNamesJson,
        unit_add_losses_to: &HashSet<String>,
        dont_count_morphs: &HashSet<String>,
    ) -> UnitTypeChangeUpdate {
        let mut update = UnitTypeChangeUpdate {
            landed_timing: research_vessel_landed_timing,
            unit_change_event: None,
        };
        let Some(unit_row) = unit_dict.get_mut(&event.event_unit_id) else {
            return update;
        };

        let control_pid = unit_row.control_pid;
        let unit_type = event.unit_type.as_str();
        let gameloop = event.gameloop;

        if control_pid == 7 && unit_type == "ResearchVesselLanded" {
            update.landed_timing = Some(gameloop);
        }
        if control_pid == 7 && unit_type == "ResearchVessel" {
            if let Some(timing) = update.landed_timing {
                if timing + 1500 > gameloop {
                    bonus_timings.push(gameloop as f64 / 16.0 - start_time);
                    update.landed_timing = None;
                }
            }
        }

        if map_name.contains("Scythe of Amon")
            && control_pid == 11
            && unit_type == "WarpPrismPhasing"
        {
            bonus_timings.push(gameloop as f64 / 16.0 - start_time);
        }

        if units_killed_in_morph.contains(unit_type) {
            return update;
        }

        let old_unit_type = std::mem::replace(&mut unit_row.unit_type, unit_type.to_owned());

        if control_pid == main_player {
            update.unit_change_event = Some((
                StatsCounterTarget::Main,
                unit_type.to_owned(),
                old_unit_type.clone(),
            ));
        } else if control_pid == ally_player {
            update.unit_change_event = Some((
                StatsCounterTarget::Ally,
                unit_type.to_owned(),
                old_unit_type.clone(),
            ));
        }

        if unit_name_dict.contains_key(unit_type)
            && unit_name_dict.contains_key(old_unit_type.as_str())
        {
            if old_unit_type == "BanelingCocoon" && unit_type == "HotSSwarmling" {
                zagaras_dummy_zerglings.insert(event.event_unit_id);
                return update;
            }

            let names_differ =
                unit_name_dict.get(unit_type) != unit_name_dict.get(old_unit_type.as_str());
            // Preserve the historical Python loop-variable quirk used by the original cache.
            let is_broodlord_broodling = (unit_type == "Broodling"
                || unit_type == "BroodlingStetmann")
                && broodlord_broodlings.contains(&legacy_spawn_filter_unit_id);
            let should_add_created = names_differ
                && !unit_add_losses_to.contains(old_unit_type.as_str())
                && !(glevig_spawns.contains(&legacy_spawn_filter_unit_id)
                    || murvar_spawns.contains(&legacy_spawn_filter_unit_id)
                    || is_broodlord_broodling)
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
                } else if amon_players.contains(&control_pid) {
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
                if amon_players.contains(&control_pid) {
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
        event_unit_id: i64,
        map_name: &str,
        control_pid: i64,
        main_player: i64,
        ally_player: i64,
        amon_players: &HashSet<i64>,
        unit_dict: &mut UnitStateMap,
        game_time: f64,
        bonus_timings: &mut Vec<f64>,
        mw_bonus_initial_timing: &mut [f64; 2],
    ) -> UnitOwnerChangeUpdate {
        let mut update = UnitOwnerChangeUpdate::default();
        let Some(unit_row) = unit_dict.get_mut(&event_unit_id) else {
            return update;
        };
        let losing_player = unit_row.control_pid;

        if control_pid == main_player && amon_players.contains(&losing_player) {
            update.mind_controlled_unit_id = Some(event_unit_id);
            update.icon_target = Some(StatsCounterTarget::Main);
        } else if control_pid == ally_player && amon_players.contains(&losing_player) {
            update.mind_controlled_unit_id = Some(event_unit_id);
            update.icon_target = Some(StatsCounterTarget::Ally);
        }

        unit_row.control_pid = control_pid;

        if map_name.contains("Malwarfare") {
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

    pub(super) fn replay_handle_unit_died_kill_stats_event_fields(
        event_unit_id: Option<i64>,
        killing_player: Option<i64>,
        gameloop: i64,
        main_player: i64,
        ally_player: i64,
        amon_players: &HashSet<i64>,
        unit_dict: &UnitStateMap,
        killcounts: &mut [i64],
        user_leave_times: &IndexMap<i64, f64>,
        end_time: f64,
        last_aoe_unit_killed: &mut [Option<(String, f64)>],
        ally_kills_counted_toward_main: i64,
        do_not_count_kills: &HashSet<String>,
        aoe_units: &HashSet<String>,
    ) -> i64 {
        let mut ally_kills = ally_kills_counted_toward_main;
        let Some(event_unit_id) = event_unit_id else {
            return ally_kills;
        };
        let Some(unit_row) = unit_dict.get(&event_unit_id) else {
            return ally_kills;
        };
        let killed_unit_type = unit_row.unit_type.as_str();
        let losing_player = unit_row.control_pid;

        if let Some(killer) = killing_player {
            if !do_not_count_kills.contains(killed_unit_type) {
                if (killer == 1 || killer == 2) && !amon_players.contains(&losing_player) {
                    // ignore player-vs-player kills
                } else if amon_players.contains(&killer)
                    && !(losing_player == 1 || losing_player == 2)
                {
                    // ignore amon-vs-amon kills
                } else if killer == ally_player {
                    let ally_leave_time = user_leave_times
                        .get(&ally_player)
                        .copied()
                        .unwrap_or(end_time);
                    if ally_leave_time < end_time * 0.5 {
                        ReplayEventHandlerHelpers::increment_i64_key(killcounts, main_player, 1);
                        ally_kills += 1;
                    } else {
                        ReplayEventHandlerHelpers::increment_i64_key(killcounts, killer, 1);
                    }
                } else {
                    ReplayEventHandlerHelpers::increment_i64_key(killcounts, killer, 1);
                }
            }
        }

        if aoe_units.contains(killed_unit_type)
            && killing_player
                .map(|value| value == 1 || value == 2)
                .unwrap_or(false)
            && amon_players.contains(&losing_player)
        {
            if let Ok(index) = usize::try_from(losing_player) {
                if let Some(slot) = last_aoe_unit_killed.get_mut(index) {
                    *slot = Some((killed_unit_type.to_owned(), gameloop as f64 / 16.0));
                }
            }
        }

        ally_kills
    }

    pub(super) fn replay_handle_unit_died_detail_event_fields(
        event: &UnitDiedEventFields,
        map_name: &str,
        main_player: i64,
        ally_player: i64,
        amon_players: &HashSet<i64>,
        unit_id: i64,
        unit_type_dict_main: &mut UnitTypeCountMap,
        unit_type_dict_ally: &mut UnitTypeCountMap,
        unit_type_dict_amon: &mut UnitTypeCountMap,
        unit_dict: &UnitStateMap,
        dt_ht_ignore: &mut [i64],
        start_time: f64,
        commander_by_player: &HashMap<i64, String>,
        killbot_feed: &mut [i64],
        custom_kill_count: &mut NestedPlayerCountMap,
        used_mutator_spider_mines: &mut HashSet<i64>,
        bonus_timings: &mut Vec<f64>,
        abathur_kill_locusts: &HashSet<i64>,
        mutator_dehaka_drag_unit_ids: &HashSet<i64>,
        murvar_spawns: &HashSet<i64>,
        glevig_spawns: &HashSet<i64>,
        broodlord_broodlings: &HashSet<i64>,
        unit_killed_by: &mut TextListMapping,
        mind_controlled_units: &HashSet<i64>,
        zagaras_dummy_zerglings: &HashSet<i64>,
        last_aoe_unit_killed: &[Option<(String, f64)>],
        commander_no_units: &HashMap<String, Vec<String>>,
        commander_no_units_values: &HashSet<String>,
        hfts_units: &HashSet<String>,
        tus_units: &HashSet<String>,
        do_not_count_kills: &HashSet<String>,
        self_killing_units: &HashSet<String>,
        duplicating_units: &HashSet<String>,
        salvage_units: &HashSet<String>,
    ) -> UnitDiedDetailUpdate {
        let mut update = UnitDiedDetailUpdate {
            current_unit_id: unit_id,
            salvaged_unit: None,
            mindcontrolled_unit_died: None,
        };
        let event_unit_id = event.event_unit_id;
        update.current_unit_id = event_unit_id;
        let killing_unit_id = event.killing_unit_id;
        let killing_player = event.killing_player;

        let Some(killed_row) = unit_dict.get(&event_unit_id) else {
            return update;
        };
        let killed_unit_type = killed_row.unit_type.as_str();
        let losing_player = killed_row.control_pid;
        let commander = killing_player
            .and_then(|pid| commander_by_player.get(&pid))
            .cloned();

        let mut killing_unit_type = if let Some(killer_id) = killing_unit_id {
            if let Some(row) = unit_dict.get(&killer_id) {
                row.unit_type.as_str()
            } else {
                "NoUnit"
            }
        } else {
            "NoUnit"
        };

        if killing_unit_type == "NoUnit" {
            if let Some(commander_name) = commander.as_deref() {
                if let Some(backup_units) = commander_no_units.get(commander_name) {
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
            }
        }

        if matches!(
            killing_unit_type.as_ref(),
            "MutatorKillBot" | "MutatorDeathBot" | "MutatorMurderBot"
        ) && (losing_player == 1 || losing_player == 2)
        {
            ReplayEventHandlerHelpers::increment_i64_key(killbot_feed, losing_player, 1);
        }

        if killing_unit_type == "Locust"
            && commander.as_deref() == Some("Abathur")
            && !killing_unit_id
                .map(|value| abathur_kill_locusts.contains(&value))
                .unwrap_or(false)
        {
            if killing_player == Some(main_player) && unit_type_dict_main.contains_key("SwarmHost")
            {
                killing_unit_type = "SwarmHost";
            }
            if killing_player == Some(ally_player) && unit_type_dict_ally.contains_key("SwarmHost")
            {
                killing_unit_type = "SwarmHost";
            }
        } else if matches!(
            killing_unit_type.as_ref(),
            "DehakaZerglingLevel2" | "DehakaRoachLevel2" | "DehakaHydraliskLevel2"
        ) && killing_unit_id
            .map(|value| glevig_spawns.contains(&value))
            .unwrap_or(false)
        {
            killing_unit_type = "Glevig";
        } else if matches!(
            killing_unit_type.as_ref(),
            "DehakaLocust" | "DehakaCreeperFlying" | "DehakaLocustFlying" | "DehakaCreeper"
        ) && killing_unit_id
            .map(|value| murvar_spawns.contains(&value))
            .unwrap_or(false)
        {
            killing_unit_type = "Murvar";
        } else if killing_unit_id
            .map(|value| broodlord_broodlings.contains(&value))
            .unwrap_or(false)
            && (killing_unit_type == "Broodling" || killing_unit_type == "BroodlingStetmann")
        {
            if killing_unit_type == "Broodling" {
                killing_unit_type = "BroodLord";
            } else if killing_unit_type == "BroodlingStetmann" {
                killing_unit_type = "BroodLordStetmann";
            }
        }

        if killing_player
            .map(|value| value == 1 || value == 2)
            .unwrap_or(false)
            && amon_players.contains(&losing_player)
        {
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
            if killed_unit_type == "ProtossFrigate" {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "shuttles",
                    killer,
                    1,
                );
            } else if killed_unit_type == "MutatorPropagator" {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "propagators",
                    killer,
                    1,
                );
            } else if matches!(
                killed_unit_type,
                "MutatorSpiderMine"
                    | "MutatorSpiderMineBurrowed"
                    | "WidowMineBurrowed"
                    | "WidowMine"
            ) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "minesweeper",
                    killer,
                    1,
                );
            } else if killed_unit_type == "MutatorVoidRift" {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "voidrifts",
                    killer,
                    1,
                );
            } else if matches!(
                killed_unit_type,
                "MutatorTurkey" | "MutatorTurking" | "MutatorInfestedTurkey"
            ) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "turkey",
                    killer,
                    1,
                );
            } else if killed_unit_type == "MutatorVoidReanimator" {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "voidreanimators",
                    killer,
                    1,
                );
            } else if matches!(
                killed_unit_type,
                "InfestableBiodome"
                    | "JarbanInfestibleColonistHut"
                    | "InfestedMercHaven"
                    | "InfestableHut"
            ) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "deadofnight",
                    killer,
                    1,
                );
            } else if matches!(
                killed_unit_type,
                "MutatorMissileSplitterChild"
                    | "MutatorMissileNuke"
                    | "MutatorMissileSplitter"
                    | "MutatorMissileStandard"
                    | "MutatorMissilePointDefense"
            ) {
                ReplayEventHandlerHelpers::increment_nested_player_count(
                    custom_kill_count,
                    "missilecommand",
                    killer,
                    1,
                );
            }
        }

        if (losing_player == 1 || losing_player == 2)
            && killing_player
                .map(|value| amon_players.contains(&value))
                .unwrap_or(false)
        {
            if killing_unit_type == "MutatorSpiderMine"
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
        }

        if killing_unit_type == "NoUnit"
            && killing_unit_id.is_none()
            && killing_player
                .map(|value| amon_players.contains(&value))
                .unwrap_or(false)
            && killing_player != Some(losing_player)
        {
            if let Some(killer) = killing_player {
                if let Some((last_type, last_time)) = usize::try_from(killer)
                    .ok()
                    .and_then(|index| last_aoe_unit_killed.get(index))
                    .and_then(|entry| entry.as_ref())
                {
                    if event.gameloop as f64 / 16.0 - *last_time < 9.0 {
                        ReplayEventHandlerHelpers::update_unit_count(
                            unit_type_dict_amon,
                            last_type.as_str(),
                            0,
                            0,
                            1,
                        );
                    }
                }
            }
        }

        let killer_in_unit_dict = killing_unit_id
            .map(|value| unit_dict.contains_key(&value))
            .unwrap_or(false);
        if (killer_in_unit_dict || commander_no_units_values.contains(killing_unit_type))
            && killing_unit_id != Some(event_unit_id)
            && killing_player != Some(losing_player)
            && !do_not_count_kills.contains(killed_unit_type)
        {
            if killing_player == Some(main_player) && amon_players.contains(&losing_player) {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    killing_unit_type.as_ref(),
                    0,
                    0,
                    1,
                );
            }
            if killing_player == Some(ally_player) && amon_players.contains(&losing_player) {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    killing_unit_type.as_ref(),
                    0,
                    0,
                    1,
                );
            }
            if killing_player
                .map(|value| amon_players.contains(&value))
                .unwrap_or(false)
                && (losing_player == 1 || losing_player == 2)
            {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_amon,
                    killing_unit_type.as_ref(),
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
            if killing_player
                .map(|value| amon_players.contains(&value))
                .unwrap_or(false)
            {
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
        let rounded_bonus =
            ReplayEventHandlerHelpers::round_to_digits_half_even(game_time - start_time, 0);
        let bonus_len = bonus_timings.len();
        let nova_eradicator_lost = unit_type_dict_amon
            .get("NovaEradicator")
            .map(|row| row[1])
            .unwrap_or_default();

        let bonus_triggered = (map_name.contains("Void Thrashing")
            && (killed_unit_type == "ArchAngelCoopFighter"
                || killed_unit_type == "ArchAngelCoopAssault")
            && losing_player == 5)
            || (map_name.contains("Dead of Night")
                && killed_unit_type == "ACVirophage"
                && losing_player == 7
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || ((map_name.contains("Lock & Load") || map_name.contains("[MM] LnL"))
                && killed_unit_type == "XelNagaConstruct"
                && losing_player == 3)
            || (map_name.contains("Chain of Ascension")
                && killed_unit_type == "SlaynElemental"
                && losing_player == 10
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || (map_name.contains("Rifts to Korhal")
                && killed_unit_type == "ACPirateCapitalShip"
                && losing_player == 8
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || (map_name.contains("Cradle of Death")
                && killed_unit_type == "LogisticsHeadquarters"
                && losing_player == 3)
            || (map_name.contains("Part and Parcel")
                && (killed_unit_type == "Caboose" || killed_unit_type == "TarsonisEngine")
                && !bonus_timings.contains(&rounded_bonus)
                && bonus_len < 2
                && losing_player == 8
                && !(event_x == 169 && event_y == 99)
                && !(event_x == 38 && event_y == 178))
            || (map_name.contains("Oblivion Express")
                && killed_unit_type == "TarsonisEngineFast"
                && losing_player == 7
                && event_x < 196)
            || (map_name.contains("Mist Opportunities")
                && killed_unit_type == "COOPTerrazineTank"
                && losing_player == 3
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || (map_name.contains("The Vermillion Problem")
                && (killed_unit_type == "RedstoneSalamander"
                    || killed_unit_type == "RedstoneSalamanderBurrowed")
                && losing_player == 9
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || (map_name.contains("Miner Evacuation")
                && killed_unit_type == "Blightbringer"
                && losing_player == 5
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || (map_name.contains("Miner Evacuation")
                && killed_unit_type == "NovaEradicator"
                && losing_player == 9
                && nova_eradicator_lost == 1
                && killing_player
                    .map(|value| value == 1 || value == 2)
                    .unwrap_or(false))
            || (map_name.contains("Temple of the Past")
                && killed_unit_type == "ZenithStone"
                && losing_player == 8);
        if bonus_triggered {
            if map_name.contains("Cradle of Death") {
                let value = ReplayEventHandlerHelpers::round_to_digits_half_even(
                    game_time - start_time - 8.0,
                    0,
                );
                bonus_timings.push(value);
            } else {
                let value =
                    ReplayEventHandlerHelpers::round_to_digits_half_even(game_time - start_time, 0);
                bonus_timings.push(value);
            }
        }

        if salvage_units.contains(killed_unit_type) && killing_player == Some(losing_player) {
            if losing_player == main_player {
                update.salvaged_unit =
                    Some((StatsCounterTarget::Main, killed_unit_type.to_owned()));
            } else if losing_player == ally_player {
                update.salvaged_unit =
                    Some((StatsCounterTarget::Ally, killed_unit_type.to_owned()));
            }
        }

        let killed_is_broodlord_broodling = (killed_unit_type == "Broodling"
            || killed_unit_type == "BroodlingStetmann")
            && broodlord_broodlings.contains(&event_unit_id);
        if (salvage_units.contains(killed_unit_type) && killing_player == Some(losing_player))
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
            .cloned()
            .unwrap_or_default();
        if matches!(
            killed_unit_type,
            "Roach"
                | "RavagerAbathur"
                | "RoachVileBurrowed"
                | "RoachBurrowed"
                | "SwarmHostBurrowed"
                | "QueenBurrowed"
        ) && losing_commander == "Abathur"
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
                killing_unit_type.as_ref(),
            );

            if mind_controlled_units.contains(&event_unit_id) {
                update.mindcontrolled_unit_died =
                    Some((StatsCounterTarget::Main, killed_unit_type.to_owned()));
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
                    Some((StatsCounterTarget::Ally, killed_unit_type.to_owned()));
            }
        }

        if amon_players.contains(&losing_player)
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
