use crate::dictionary_data::UnitNamesJson;
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) type NestedPlayerCountMap = IndexMap<String, IndexMap<i64, i64>>;
pub(super) type TextListMapping = IndexMap<String, Vec<String>>;
pub(super) type UnitTypeCountMap = IndexMap<String, [i64; 4]>;
pub(super) type IdentifiedWavesMap = BTreeMap<i64, Vec<String>>;

const PLAYER_ID_INDEXED_LIMIT: usize = 17;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReplayEventStringSets {
    custom_kill_count_categories: HashMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReplayPlayerIdSet {
    indexed: [bool; PLAYER_ID_INDEXED_LIMIT],
    values: HashSet<i64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ReplayMapAnalysisFlags {
    scythe_of_amon: bool,
    malwarfare: bool,
    void_thrashing: bool,
    dead_of_night: bool,
    lock_and_load: bool,
    chain_of_ascension: bool,
    rifts_to_korhal: bool,
    cradle_of_death: bool,
    part_and_parcel: bool,
    oblivion_express: bool,
    mist_opportunities: bool,
    vermillion_problem: bool,
    miner_evacuation: bool,
    temple_of_the_past: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UnitSnapshot {
    unit_type: String,
    control_pid: i64,
}

pub(super) type UnitStateMap = HashMap<i64, UnitSnapshot>;

impl ReplayEventStringSets {
    pub(super) fn new() -> Self {
        Self {
            custom_kill_count_categories: Self::custom_kill_count_categories(),
        }
    }

    fn contains_murvar_spawn_unit(&self, unit_type: &str) -> bool {
        matches!(
            unit_type,
            "DehakaLocust" | "DehakaCreeperFlying" | "DehakaLocustFlying" | "DehakaCreeper"
        )
    }

    fn contains_glevig_spawn_unit(&self, unit_type: &str) -> bool {
        matches!(
            unit_type,
            "CoopDehakaGlevigEggZergling"
                | "CoopDehakaGlevigEggRoach"
                | "CoopDehakaGlevigEggHydralisk"
        )
    }

    fn contains_glevig_killer_unit(&self, unit_type: &str) -> bool {
        matches!(
            unit_type,
            "DehakaZerglingLevel2" | "DehakaRoachLevel2" | "DehakaHydraliskLevel2"
        )
    }

    fn contains_broodling_unit(&self, unit_type: &str) -> bool {
        matches!(unit_type, "Broodling" | "BroodlingStetmann")
    }

    fn contains_broodling_escort_unit(&self, unit_type: &str) -> bool {
        matches!(unit_type, "BroodlingEscort" | "BroodlingEscortStetmann")
    }

    fn contains_killbot_unit(&self, unit_type: &str) -> bool {
        matches!(
            unit_type,
            "MutatorKillBot" | "MutatorDeathBot" | "MutatorMurderBot"
        )
    }

    fn contains_abathur_free_death_unit(&self, unit_type: &str) -> bool {
        matches!(
            unit_type,
            "Roach"
                | "RavagerAbathur"
                | "RoachVileBurrowed"
                | "RoachBurrowed"
                | "SwarmHostBurrowed"
                | "QueenBurrowed"
        )
    }

    fn custom_kill_count_category(&self, unit_type: &str) -> Option<&str> {
        self.custom_kill_count_categories
            .get(unit_type)
            .map(String::as_str)
    }

    fn custom_kill_count_categories() -> HashMap<String, String> {
        let mut categories = HashMap::new();
        Self::insert_custom_kill_count_category(&mut categories, "shuttles", &["ProtossFrigate"]);
        Self::insert_custom_kill_count_category(
            &mut categories,
            "propagators",
            &["MutatorPropagator"],
        );
        Self::insert_custom_kill_count_category(
            &mut categories,
            "minesweeper",
            &[
                "MutatorSpiderMine",
                "MutatorSpiderMineBurrowed",
                "WidowMineBurrowed",
                "WidowMine",
            ],
        );
        Self::insert_custom_kill_count_category(&mut categories, "voidrifts", &["MutatorVoidRift"]);
        Self::insert_custom_kill_count_category(
            &mut categories,
            "turkey",
            &["MutatorTurkey", "MutatorTurking", "MutatorInfestedTurkey"],
        );
        Self::insert_custom_kill_count_category(
            &mut categories,
            "voidreanimators",
            &["MutatorVoidReanimator"],
        );
        Self::insert_custom_kill_count_category(
            &mut categories,
            "deadofnight",
            &[
                "InfestableBiodome",
                "JarbanInfestibleColonistHut",
                "InfestedMercHaven",
                "InfestableHut",
            ],
        );
        Self::insert_custom_kill_count_category(
            &mut categories,
            "missilecommand",
            &[
                "MutatorMissileSplitterChild",
                "MutatorMissileNuke",
                "MutatorMissileSplitter",
                "MutatorMissileStandard",
                "MutatorMissilePointDefense",
            ],
        );
        categories
    }

    fn insert_custom_kill_count_category(
        categories: &mut HashMap<String, String>,
        category: &str,
        unit_types: &[&str],
    ) {
        for unit_type in unit_types {
            categories.insert((*unit_type).to_owned(), category.to_owned());
        }
    }
}

impl ReplayPlayerIdSet {
    pub(super) fn from_values(values: impl IntoIterator<Item = i64>) -> Self {
        let mut set = Self {
            indexed: [false; PLAYER_ID_INDEXED_LIMIT],
            values: HashSet::new(),
        };
        set.extend(values);
        set
    }

    pub(super) fn insert(&mut self, player_id: i64) {
        if let Ok(index) = usize::try_from(player_id) {
            if let Some(slot) = self.indexed.get_mut(index) {
                *slot = true;
            }
        }
        self.values.insert(player_id);
    }

    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = i64>) {
        for value in values {
            self.insert(value);
        }
    }

    pub(super) fn contains(&self, player_id: i64) -> bool {
        if let Ok(index) = usize::try_from(player_id) {
            if let Some(value) = self.indexed.get(index) {
                return *value;
            }
        }
        self.values.contains(&player_id)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = i64> + '_ {
        self.values.iter().copied()
    }
}

impl ReplayMapAnalysisFlags {
    pub(super) fn new(map_name: &str) -> Self {
        Self {
            scythe_of_amon: map_name.contains("Scythe of Amon"),
            malwarfare: map_name.contains("Malwarfare"),
            void_thrashing: map_name.contains("Void Thrashing"),
            dead_of_night: map_name.contains("Dead of Night"),
            lock_and_load: map_name.contains("Lock & Load")
                || map_name.contains("[MM] LnL")
                || map_name.contains("[MM] Lnl"),
            chain_of_ascension: map_name.contains("Chain of Ascension"),
            rifts_to_korhal: map_name.contains("Rifts to Korhal"),
            cradle_of_death: map_name.contains("Cradle of Death"),
            part_and_parcel: map_name.contains("Part and Parcel"),
            oblivion_express: map_name.contains("Oblivion Express"),
            mist_opportunities: map_name.contains("Mist Opportunities"),
            vermillion_problem: map_name.contains("The Vermillion Problem"),
            miner_evacuation: map_name.contains("Miner Evacuation"),
            temple_of_the_past: map_name.contains("Temple of the Past"),
        }
    }

    pub(super) fn is_dead_of_night(&self) -> bool {
        self.dead_of_night
    }

    fn is_scythe_of_amon(&self) -> bool {
        self.scythe_of_amon
    }

    fn is_malwarfare(&self) -> bool {
        self.malwarfare
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WaveUnitsState {
    second_gameloop: i64,
    units: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StatsCounterTarget {
    Main,
    Ally,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlayerStatsUpdate {
    target: StatsCounterTarget,
    kills: i64,
    supply_used: f64,
    collection_rate: f64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UpgradeEventUpdate {
    target: Option<StatsCounterTarget>,
    commander_name: Option<String>,
    mastery_index: Option<i64>,
    upgrade_count: i64,
    prestige_name: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitOwnerChangeUpdate {
    mind_controlled_unit_id: Option<i64>,
    icon_target: Option<StatsCounterTarget>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UnitBornOrInitEventFields<'a> {
    unit_type: &'a str,
    ability_name: Option<&'a str>,
    unit_id: i64,
    creator_unit_id: Option<i64>,
    control_pid: i64,
    gameloop: i64,
    event_x: i64,
    event_y: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitBornOrInitUpdate<'a> {
    unit_id: i64,
    last_biomass_position: [i64; 3],
    created_event: Option<(StatsCounterTarget, &'a str)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UnitTypeChangeEventFields<'a> {
    event_unit_id: i64,
    unit_type: &'a str,
    gameloop: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitTypeChangeUpdate<'a> {
    landed_timing: Option<i64>,
    unit_change_event: Option<(StatsCounterTarget, &'a str, String)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitDiedEventFields {
    event_unit_id: i64,
    killing_unit_id: Option<i64>,
    killing_player: Option<i64>,
    gameloop: i64,
    event_x: i64,
    event_y: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UnitDiedDetailUpdate<'a> {
    current_unit_id: i64,
    salvaged_unit: Option<(StatsCounterTarget, &'a str)>,
    mindcontrolled_unit_died: Option<(StatsCounterTarget, &'a str)>,
}

impl PlayerStatsUpdate {
    pub(super) fn target(&self) -> StatsCounterTarget {
        self.target
    }

    pub(super) fn kills(&self) -> i64 {
        self.kills
    }

    pub(super) fn supply_used(&self) -> f64 {
        self.supply_used
    }

    pub(super) fn collection_rate(&self) -> f64 {
        self.collection_rate
    }
}

impl UpgradeEventUpdate {
    pub(super) fn target(&self) -> Option<StatsCounterTarget> {
        self.target
    }

    pub(super) fn commander_name(&self) -> Option<&str> {
        self.commander_name.as_deref()
    }

    pub(super) fn mastery_index(&self) -> Option<i64> {
        self.mastery_index
    }

    pub(super) fn upgrade_count(&self) -> i64 {
        self.upgrade_count
    }

    pub(super) fn prestige_name(&self) -> Option<&str> {
        self.prestige_name.as_deref()
    }
}

impl UnitOwnerChangeUpdate {
    pub(super) fn mind_controlled_unit_id(&self) -> Option<i64> {
        self.mind_controlled_unit_id
    }

    pub(super) fn icon_target(&self) -> Option<StatsCounterTarget> {
        self.icon_target
    }
}

impl<'a> UnitBornOrInitEventFields<'a> {
    pub(super) fn new(
        unit_type: &'a str,
        ability_name: Option<&'a str>,
        unit_id: i64,
        creator_unit_id: Option<i64>,
        control_pid: i64,
        gameloop: i64,
        event_x: i64,
        event_y: i64,
    ) -> Self {
        Self {
            unit_type,
            ability_name,
            unit_id,
            creator_unit_id,
            control_pid,
            gameloop,
            event_x,
            event_y,
        }
    }
}

impl UnitBornOrInitUpdate<'_> {
    pub(super) fn unit_id(&self) -> i64 {
        self.unit_id
    }

    pub(super) fn last_biomass_position(&self) -> [i64; 3] {
        self.last_biomass_position
    }

    pub(super) fn created_event(&self) -> Option<(StatsCounterTarget, &str)> {
        self.created_event
            .map(|(target, unit_type)| (target, unit_type))
    }
}

impl<'a> UnitTypeChangeEventFields<'a> {
    pub(super) fn new(event_unit_id: i64, unit_type: &'a str, gameloop: i64) -> Self {
        Self {
            event_unit_id,
            unit_type,
            gameloop,
        }
    }
}

impl UnitTypeChangeUpdate<'_> {
    pub(super) fn landed_timing(&self) -> Option<i64> {
        self.landed_timing
    }

    pub(super) fn unit_change_event(&self) -> Option<(StatsCounterTarget, &str, &str)> {
        self.unit_change_event
            .as_ref()
            .map(|(target, new_unit, old_unit)| (*target, *new_unit, old_unit.as_str()))
    }
}

impl UnitDiedEventFields {
    pub(super) fn new(
        event_unit_id: i64,
        killing_unit_id: Option<i64>,
        killing_player: Option<i64>,
        gameloop: i64,
        event_x: i64,
        event_y: i64,
    ) -> Self {
        Self {
            event_unit_id,
            killing_unit_id,
            killing_player,
            gameloop,
            event_x,
            event_y,
        }
    }
}

impl UnitDiedDetailUpdate<'_> {
    pub(super) fn current_unit_id(&self) -> i64 {
        self.current_unit_id
    }

    pub(super) fn salvaged_unit(&self) -> Option<(StatsCounterTarget, &str)> {
        self.salvaged_unit
    }

    pub(super) fn mindcontrolled_unit_died(&self) -> Option<(StatsCounterTarget, &str)> {
        self.mindcontrolled_unit_died
    }
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
        mastery_upgrade_indices: &HashMap<String, i64>,
        prestige_upgrade_names: &HashMap<String, String>,
    ) -> UpgradeEventUpdate {
        let target = if upg_pid == main_player {
            Some(StatsCounterTarget::Main)
        } else if upg_pid == ally_player {
            Some(StatsCounterTarget::Ally)
        } else {
            None
        };

        let commander_name = commander_upgrades.get(upg_name).cloned();
        let mastery_index = mastery_upgrade_indices.get(upg_name).copied();
        let prestige_name = prestige_upgrade_names.get(upg_name).cloned();

        UpgradeEventUpdate {
            target,
            commander_name,
            mastery_index,
            upgrade_count,
            prestige_name,
        }
    }

    pub(super) fn replay_handle_unit_born_or_init_event_fields<'a>(
        event: &UnitBornOrInitEventFields<'a>,
        main_player: i64,
        ally_player: i64,
        amon_players: &ReplayPlayerIdSet,
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
        outlaw_order_seen: &mut HashSet<String>,
        wave_units: &mut WaveUnitsState,
        identified_waves: &mut IdentifiedWavesMap,
        abathur_kill_locusts: &mut HashSet<i64>,
        last_biomass_position: [i64; 3],
        revival_types: &HashMap<String, String>,
        primal_combat_predecessors: &HashMap<String, String>,
        tychus_outlaws: &HashSet<String>,
        units_in_waves: &HashSet<String>,
        string_sets: &ReplayEventStringSets,
    ) -> UnitBornOrInitUpdate<'a> {
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
        if is_broodling_unit && event.creator_unit_id.is_some() {
            if let Some(creator_id) = event.creator_unit_id {
                if let Some(creator_row) = unit_dict.get(&creator_id) {
                    let creator_type = creator_row.unit_type.as_str();
                    if string_sets.contains_broodling_escort_unit(creator_type) {
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

    pub(super) fn replay_handle_unit_type_change_event_fields<'a>(
        event: &UnitTypeChangeEventFields<'a>,
        map_flags: &ReplayMapAnalysisFlags,
        main_player: i64,
        ally_player: i64,
        amon_players: &ReplayPlayerIdSet,
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
        string_sets: &ReplayEventStringSets,
    ) -> UnitTypeChangeUpdate<'a> {
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
        if control_pid == 7 && unit_type == "ResearchVessel" {
            if let Some(timing) = update.landed_timing {
                if timing + 1500 > gameloop {
                    bonus_timings.push(gameloop as f64 / 16.0 - start_time);
                    update.landed_timing = None;
                }
            }
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
        event_unit_id: i64,
        map_flags: &ReplayMapAnalysisFlags,
        control_pid: i64,
        main_player: i64,
        ally_player: i64,
        amon_players: &ReplayPlayerIdSet,
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

    pub(super) fn replay_handle_unit_died_kill_stats_event_fields(
        killed_row: Option<&UnitSnapshot>,
        killing_player: Option<i64>,
        gameloop: i64,
        main_player: i64,
        ally_player: i64,
        amon_players: &ReplayPlayerIdSet,
        killcounts: &mut [i64],
        ally_kills_transfer_to_main: bool,
        last_aoe_unit_killed: &mut [Option<(String, f64)>],
        ally_kills_counted_toward_main: i64,
        do_not_count_kills: &HashSet<String>,
        aoe_units: &HashSet<String>,
    ) -> i64 {
        let mut ally_kills = ally_kills_counted_toward_main;
        let Some(unit_row) = killed_row else {
            return ally_kills;
        };
        let killed_unit_type = unit_row.unit_type.as_str();
        let losing_player = unit_row.control_pid;
        let losing_player_is_amon = amon_players.contains(losing_player);
        let losing_player_is_coop = losing_player == 1 || losing_player == 2;
        let killing_player_is_coop = matches!(killing_player, Some(1 | 2));

        if let Some(killer) = killing_player {
            if !do_not_count_kills.contains(killed_unit_type) {
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
        }

        if aoe_units.contains(killed_unit_type) && killing_player_is_coop && losing_player_is_amon {
            if let Ok(index) = usize::try_from(losing_player) {
                if let Some(slot) = last_aoe_unit_killed.get_mut(index) {
                    *slot = Some((killed_unit_type.to_owned(), gameloop as f64 / 16.0));
                }
            }
        }

        ally_kills
    }

    pub(super) fn replay_handle_unit_died_detail_event_fields<'a>(
        event: &UnitDiedEventFields,
        killed_row: &'a UnitSnapshot,
        map_flags: &ReplayMapAnalysisFlags,
        main_player: i64,
        ally_player: i64,
        amon_players: &ReplayPlayerIdSet,
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
        string_sets: &ReplayEventStringSets,
    ) -> UnitDiedDetailUpdate<'a> {
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

        if killing_unit_type == "NoUnit" {
            if let Some(commander_name) = commander {
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

        if losing_player_is_coop && killing_player_is_amon {
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
            && killing_player_is_amon
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

        if (killer_in_unit_dict || commander_no_units_values.contains(killing_unit_type))
            && killing_unit_id != Some(event_unit_id)
            && killing_player != Some(losing_player)
            && !do_not_count_kills.contains(killed_unit_type)
        {
            if killing_player_is_main && losing_player_is_amon {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_main,
                    killing_unit_type.as_ref(),
                    0,
                    0,
                    1,
                );
            }
            if killing_player_is_ally && losing_player_is_amon {
                ReplayEventHandlerHelpers::update_unit_count(
                    unit_type_dict_ally,
                    killing_unit_type.as_ref(),
                    0,
                    0,
                    1,
                );
            }
            if killing_player_is_amon && losing_player_is_coop {
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
        let bonus_timing = if map_flags.void_thrashing
            && (killed_unit_type == "ArchAngelCoopFighter"
                || killed_unit_type == "ArchAngelCoopAssault")
            && losing_player == 5
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.dead_of_night
            && killed_unit_type == "ACVirophage"
            && losing_player == 7
            && killing_player_is_coop
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.lock_and_load
            && killed_unit_type == "XelNagaConstruct"
            && losing_player == 3
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.chain_of_ascension
            && killed_unit_type == "SlaynElemental"
            && losing_player == 10
            && killing_player_is_coop
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.rifts_to_korhal
            && killed_unit_type == "ACPirateCapitalShip"
            && losing_player == 8
            && killing_player_is_coop
        {
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
        } else if map_flags.oblivion_express
            && killed_unit_type == "TarsonisEngineFast"
            && losing_player == 7
            && event_x < 196
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.mist_opportunities
            && killed_unit_type == "COOPTerrazineTank"
            && losing_player == 3
            && killing_player_is_coop
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.vermillion_problem
            && (killed_unit_type == "RedstoneSalamander"
                || killed_unit_type == "RedstoneSalamanderBurrowed")
            && losing_player == 9
            && killing_player_is_coop
        {
            Some(ReplayEventHandlerHelpers::round_to_digits_half_even(
                game_time - start_time,
                0,
            ))
        } else if map_flags.miner_evacuation
            && killed_unit_type == "Blightbringer"
            && losing_player == 5
            && killing_player_is_coop
        {
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
                killing_unit_type.as_ref(),
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
