mod unit_death;
mod unit_lifecycle;

use super::{
    UnitBornOrInitHandlerInput, UnitDiedDetailHandlerInput, UnitDiedKillStatsHandlerInput,
    UnitOwnerChangeHandlerInput, UnitTypeChangeHandlerInput, UpgradeEventHandlerInput,
};
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet};
use unit_death::ReplayUnitDeathEventHandlers;
use unit_lifecycle::ReplayUnitLifecycleEventHandlers;

pub(super) type NestedPlayerCountMap = IndexMap<String, IndexMap<i64, i64>>;
pub(super) type TextListMapping = IndexMap<String, Vec<String>>;
pub(super) type UnitTypeCountMap = IndexMap<String, [i64; 4]>;
pub(super) type IdentifiedWavesMap = BTreeMap<i64, Vec<String>>;

const PLAYER_ID_INDEXED_LIMIT: usize = 17;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ReplayEventStringSets;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UnitBornOrInitUnitIds {
    unit_id: i64,
    creator_unit_id: Option<i64>,
}

impl UnitBornOrInitUnitIds {
    pub(super) fn new(unit_id: i64, creator_unit_id: Option<i64>) -> Self {
        Self {
            unit_id,
            creator_unit_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UnitEventPosition {
    x: i64,
    y: i64,
}

impl UnitEventPosition {
    pub(super) fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

pub(super) type UnitStateMap = HashMap<i64, UnitSnapshot>;

impl ReplayEventStringSets {
    pub(super) fn new() -> Self {
        Self
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
        match unit_type {
            "ProtossFrigate" => Some("shuttles"),
            "MutatorPropagator" => Some("propagators"),
            "MutatorSpiderMine"
            | "MutatorSpiderMineBurrowed"
            | "WidowMineBurrowed"
            | "WidowMine" => Some("minesweeper"),
            "MutatorVoidRift" => Some("voidrifts"),
            "MutatorTurkey" | "MutatorTurking" | "MutatorInfestedTurkey" => Some("turkey"),
            "MutatorVoidReanimator" => Some("voidreanimators"),
            "InfestableBiodome"
            | "JarbanInfestibleColonistHut"
            | "InfestedMercHaven"
            | "InfestableHut" => Some("deadofnight"),
            "MutatorMissileSplitterChild"
            | "MutatorMissileNuke"
            | "MutatorMissileSplitter"
            | "MutatorMissileStandard"
            | "MutatorMissilePointDefense" => Some("missilecommand"),
            _ => None,
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
        if let Ok(index) = usize::try_from(player_id)
            && let Some(slot) = self.indexed.get_mut(index)
        {
            *slot = true;
        }
        self.values.insert(player_id);
    }

    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = i64>) {
        for value in values {
            self.insert(value);
        }
    }

    pub(super) fn contains(&self, player_id: i64) -> bool {
        if let Ok(index) = usize::try_from(player_id)
            && let Some(value) = self.indexed.get(index)
        {
            return *value;
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

    fn is_cradle_of_death(&self) -> bool {
        self.cradle_of_death
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
        unit_ids: UnitBornOrInitUnitIds,
        control_pid: i64,
        gameloop: i64,
        position: UnitEventPosition,
    ) -> Self {
        Self {
            unit_type,
            ability_name,
            unit_id: unit_ids.unit_id,
            creator_unit_id: unit_ids.creator_unit_id,
            control_pid,
            gameloop,
            event_x: position.x,
            event_y: position.y,
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
        if let Ok(index) = usize::try_from(delta.key)
            && let Some(slot) = container.get_mut(index)
        {
            *slot += delta.delta;
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
            if floor_is_even { floor } else { floor + 1.0 }
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
        input: UpgradeEventHandlerInput<'_>,
    ) -> UpgradeEventUpdate {
        let UpgradeEventHandlerInput {
            upg_name,
            upg_pid,
            upgrade_count,
            main_player,
            ally_player,
            commander_upgrades,
            mastery_upgrade_indices,
            prestige_upgrade_names,
        } = input;
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
        input: UnitBornOrInitHandlerInput<'_, 'a>,
    ) -> UnitBornOrInitUpdate<'a> {
        ReplayUnitLifecycleEventHandlers::replay_handle_unit_born_or_init_event_fields(input)
    }

    pub(super) fn replay_handle_archon_init_event_control_pid(
        control_pid: i64,
        dt_ht_ignore: &mut [i64],
    ) {
        ReplayUnitLifecycleEventHandlers::replay_handle_archon_init_event_control_pid(
            control_pid,
            dt_ht_ignore,
        );
    }

    pub(super) fn replay_handle_unit_type_change_event_fields<'a>(
        input: UnitTypeChangeHandlerInput<'_, 'a>,
    ) -> UnitTypeChangeUpdate<'a> {
        ReplayUnitLifecycleEventHandlers::replay_handle_unit_type_change_event_fields(input)
    }

    pub(super) fn replay_handle_unit_owner_change_event_fields(
        input: UnitOwnerChangeHandlerInput<'_>,
    ) -> UnitOwnerChangeUpdate {
        ReplayUnitLifecycleEventHandlers::replay_handle_unit_owner_change_event_fields(input)
    }

    pub(super) fn replay_handle_unit_died_kill_stats_event_fields(
        input: UnitDiedKillStatsHandlerInput<'_>,
    ) -> i64 {
        ReplayUnitDeathEventHandlers::replay_handle_unit_died_kill_stats_event_fields(input)
    }

    pub(super) fn replay_handle_unit_died_detail_event_fields<'a>(
        input: UnitDiedDetailHandlerInput<'_, 'a>,
    ) -> UnitDiedDetailUpdate<'a> {
        ReplayUnitDeathEventHandlers::replay_handle_unit_died_detail_event_fields(input)
    }
}
