use crate::cache_overall_stats_generator::AnalysisPlayerStatsSeries;
use crate::dictionary_data::UnitBaseCostsJson;
use crate::stats_counter_math::{StatsCounterMath, TotalUnitCost};
use s2protocol_port::{GameEvent, SnapshotPoint, SnapshotPointValue, TrackerEvent};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct StatsCounterDictionaries {
    unit_base_costs: UnitBaseCostsJson,
    royal_guards: HashSet<String>,
    horners_units: HashSet<String>,
    tychus_base_upgrades: HashSet<String>,
    tychus_ultimate_upgrades: HashSet<String>,
    outlaws: HashSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommanderKind {
    Abathur,
    Alarak,
    Artanis,
    Dehaka,
    Fenix,
    Horner,
    Karax,
    Kerrigan,
    Mengsk,
    Raynor,
    Stetmann,
    Stukov,
    Swann,
    Tychus,
    Zagara,
    Zeratul,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrestigeKind {
    ApexPredator,
    ChaoticPowerCouple,
    EssenceHoarder,
    FrightfulFleshwelder,
    GalacticGunrunners,
    GreaseMonkey,
    KnowledgeSeeker,
    LoneWolf,
    MerchantOfDeath,
    MotherOfConstructs,
    NetworkAdministrator,
    OilBaron,
    PrincipalProletariat,
    RebelRaider,
    RoughRider,
    ShadowOfDeath,
    TechnicalRecruiter,
    TemplarApparent,
    ValorousInspirator,
    WingCommanders,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ReplayStatsCommanderCache {
    commander: CommanderKind,
    prestige: PrestigeKind,
    kerrigan_gas_factor: Option<f64>,
    mengsk_royal_guard_factor: Option<f64>,
    uses_zagara_baneling_value_adjustment: bool,
    uses_tychus_first_outlaw_discount: bool,
    uses_dehaka_army_value_spike_filter: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct UnitChangeCostCache {
    thor_gas: f64,
    siege_tank_gas: f64,
    guardian_cocoon_delta: f64,
    devourer_cocoon_delta: f64,
    viper_cocoon_delta: f64,
    swarm_host_cocoon_delta: f64,
    ravager_cocoon_delta: f64,
    queen_cocoon_delta: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnitChangeCachedDelta {
    TrooperWeaponTotal,
    ThorGas,
    SiegeTankGas,
    GuardianCocoonDelta,
    DevourerCocoonDelta,
    ViperCocoonDelta,
    SwarmHostCocoonDelta,
    RavagerCocoonDelta,
    QueenCocoonDelta,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct UnitChangeDelta {
    constant: f64,
    cached_delta: Option<UnitChangeCachedDelta>,
    cached_multiplier: f64,
}

#[derive(Clone, Debug, Default)]
struct UnitChangeTransitionMap {
    by_old_unit: HashMap<String, HashMap<String, UnitChangeDelta>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DictionaryUnitSet {
    HornerUnits,
    RoyalGuards,
}

#[derive(Clone, Debug)]
enum UnitCostRule {
    ReplaceExact {
        costs: HashMap<String, TotalUnitCost>,
    },
    ScaleAll {
        mineral_factor: f64,
        gas_factor: f64,
    },
    ScaleExact {
        units: HashSet<String>,
        mineral_factor: f64,
        gas_factor: f64,
    },
    ScaleExcept {
        excluded_units: HashSet<String>,
        mineral_factor: f64,
        gas_factor: f64,
    },
    ScaleDictionary {
        unit_set: DictionaryUnitSet,
        mineral_factor: f64,
        gas_factor: f64,
    },
}

#[derive(Clone, Debug, Default)]
struct UnitCostRules {
    before_non_zero_check: Vec<UnitCostRule>,
    after_non_zero_check: Vec<UnitCostRule>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ReplayDroneIdentifierCore {
    commanders: [String; 2],
    recently_used: bool,
    drones: i64,
    refineries: HashSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplayDroneCommandEventKind {
    Command,
    CommandUpdateTargetUnit,
}

#[derive(Clone, Debug)]
pub(crate) struct ReplayStatsCounterCore {
    dictionaries: Arc<StatsCounterDictionaries>,
    masteries: [i64; 6],
    commander: String,
    commander_cache: ReplayStatsCommanderCache,
    prestige: Option<String>,
    enable_updates: bool,
    salvaged_unit_counts: HashMap<String, i64>,
    unit_costs_cache: HashMap<String, TotalUnitCost>,
    unit_change_costs: Option<UnitChangeCostCache>,
    unit_change_transitions: UnitChangeTransitionMap,
    unit_cost_rules: UnitCostRules,
    army_value_offset: f64,
    trooper_weapon_cost: (f64, f64),
    tychus_gear_cost: (f64, f64),
    tychus_has_first_outlaw: bool,
    zagara_free_banelings: i64,
    kills: Vec<i64>,
    army_value: Vec<f64>,
    supply: Vec<f64>,
    collection_rate: Vec<f64>,
}

impl StatsCounterDictionaries {
    pub(crate) fn new(
        unit_base_costs: UnitBaseCostsJson,
        royal_guards: HashSet<String>,
        horners_units: HashSet<String>,
        tychus_base_upgrades: HashSet<String>,
        tychus_ultimate_upgrades: HashSet<String>,
        outlaws: HashSet<String>,
    ) -> Self {
        Self {
            unit_base_costs,
            royal_guards,
            horners_units,
            tychus_base_upgrades,
            tychus_ultimate_upgrades,
            outlaws,
        }
    }

    fn base_cost(&self, commander: &str, unit: &str) -> Option<TotalUnitCost> {
        self.unit_base_costs
            .get(commander)
            .and_then(|commander_costs| commander_costs.get(unit))
            .map(|cost| TotalUnitCost::from_slice(cost))
    }

    fn contains_horner_unit(&self, unit: &str) -> bool {
        self.horners_units.contains(unit)
    }

    fn contains_royal_guard(&self, unit: &str) -> bool {
        self.royal_guards.contains(unit)
    }

    fn contains_outlaw(&self, unit: &str) -> bool {
        self.outlaws.contains(unit)
    }

    fn contains_tychus_base_upgrade(&self, upgrade: &str) -> bool {
        self.tychus_base_upgrades.contains(upgrade)
    }

    fn contains_tychus_ultimate_upgrade(&self, upgrade: &str) -> bool {
        self.tychus_ultimate_upgrades.contains(upgrade)
    }
}

impl CommanderKind {
    fn from_name(name: &str) -> Self {
        match name {
            "Abathur" => Self::Abathur,
            "Alarak" => Self::Alarak,
            "Artanis" => Self::Artanis,
            "Dehaka" => Self::Dehaka,
            "Fenix" => Self::Fenix,
            "Horner" => Self::Horner,
            "Karax" => Self::Karax,
            "Kerrigan" => Self::Kerrigan,
            "Mengsk" => Self::Mengsk,
            "Raynor" => Self::Raynor,
            "Stetmann" => Self::Stetmann,
            "Stukov" => Self::Stukov,
            "Swann" => Self::Swann,
            "Tychus" => Self::Tychus,
            "Zagara" => Self::Zagara,
            "Zeratul" => Self::Zeratul,
            _ => Self::Other,
        }
    }
}

impl PrestigeKind {
    fn from_name(name: Option<&str>) -> Self {
        match name.unwrap_or_default() {
            "Apex Predator" => Self::ApexPredator,
            "Chaotic Power Couple" => Self::ChaoticPowerCouple,
            "Essence Hoarder" => Self::EssenceHoarder,
            "Frightful Fleshwelder" => Self::FrightfulFleshwelder,
            "Galactic Gunrunners" => Self::GalacticGunrunners,
            "Grease Monkey" => Self::GreaseMonkey,
            "Knowledge Seeker" => Self::KnowledgeSeeker,
            "Lone Wolf" => Self::LoneWolf,
            "Merchant of Death" => Self::MerchantOfDeath,
            "Mother of Constructs" => Self::MotherOfConstructs,
            "Network Administrator" => Self::NetworkAdministrator,
            "Oil Baron" => Self::OilBaron,
            "Principal Proletariat" => Self::PrincipalProletariat,
            "Rebel Raider" => Self::RebelRaider,
            "Rough Rider" => Self::RoughRider,
            "Shadow of Death" => Self::ShadowOfDeath,
            "Technical Recruiter" => Self::TechnicalRecruiter,
            "Templar Apparent" => Self::TemplarApparent,
            "Valorous Inspirator" => Self::ValorousInspirator,
            "Wing Commanders" => Self::WingCommanders,
            _ => Self::Other,
        }
    }
}

impl ReplayStatsCommanderCache {
    fn new(commander: &str, prestige: Option<&str>, masteries: &[i64; 6]) -> Self {
        let commander = CommanderKind::from_name(commander);
        let prestige = PrestigeKind::from_name(prestige);
        Self {
            commander,
            prestige,
            kerrigan_gas_factor: Self::build_kerrigan_gas_factor(commander, masteries),
            mengsk_royal_guard_factor: Self::build_mengsk_royal_guard_factor(commander, masteries),
            uses_zagara_baneling_value_adjustment:
                Self::build_uses_zagara_baneling_value_adjustment(commander),
            uses_tychus_first_outlaw_discount: Self::build_uses_tychus_first_outlaw_discount(
                commander,
            ),
            uses_dehaka_army_value_spike_filter: Self::build_uses_dehaka_army_value_spike_filter(
                commander,
            ),
        }
    }

    fn commander(&self) -> CommanderKind {
        self.commander
    }

    fn prestige(&self) -> PrestigeKind {
        self.prestige
    }

    fn kerrigan_gas_factor(&self) -> Option<f64> {
        self.kerrigan_gas_factor
    }

    fn mengsk_royal_guard_factor(&self) -> Option<f64> {
        self.mengsk_royal_guard_factor
    }

    fn uses_zagara_baneling_value_adjustment(&self) -> bool {
        self.uses_zagara_baneling_value_adjustment
    }

    fn uses_tychus_first_outlaw_discount(&self) -> bool {
        self.uses_tychus_first_outlaw_discount
    }

    fn uses_dehaka_army_value_spike_filter(&self) -> bool {
        self.uses_dehaka_army_value_spike_filter
    }

    fn build_kerrigan_gas_factor(commander: CommanderKind, masteries: &[i64; 6]) -> Option<f64> {
        match commander {
            CommanderKind::Kerrigan if masteries[2] > 0 => Some(1.0 - masteries[2] as f64 / 100.0),
            _ => None,
        }
    }

    fn build_mengsk_royal_guard_factor(
        commander: CommanderKind,
        masteries: &[i64; 6],
    ) -> Option<f64> {
        match commander {
            CommanderKind::Mengsk if masteries[3] > 0 => {
                Some(1.0 - 20.0 * masteries[3] as f64 / 3000.0)
            }
            _ => None,
        }
    }

    fn build_uses_zagara_baneling_value_adjustment(commander: CommanderKind) -> bool {
        matches!(commander, CommanderKind::Zagara)
    }

    fn build_uses_tychus_first_outlaw_discount(commander: CommanderKind) -> bool {
        matches!(commander, CommanderKind::Tychus)
    }

    fn build_uses_dehaka_army_value_spike_filter(commander: CommanderKind) -> bool {
        matches!(commander, CommanderKind::Dehaka)
    }
}

impl UnitChangeDelta {
    fn constant(constant: f64) -> Self {
        Self {
            constant,
            cached_delta: None,
            cached_multiplier: 0.0,
        }
    }

    fn cached(cached_delta: UnitChangeCachedDelta, cached_multiplier: f64) -> Self {
        Self {
            constant: 0.0,
            cached_delta: Some(cached_delta),
            cached_multiplier,
        }
    }

    fn combined(
        constant: f64,
        cached_delta: UnitChangeCachedDelta,
        cached_multiplier: f64,
    ) -> Self {
        Self {
            constant,
            cached_delta: Some(cached_delta),
            cached_multiplier,
        }
    }
}

impl UnitChangeTransitionMap {
    fn new() -> Self {
        let mut transitions = Self::default();

        for equipped_trooper in [
            "TrooperMengskAA",
            "TrooperMengskFlamethrower",
            "TrooperMengskImproved",
        ] {
            transitions.insert(
                "TrooperMengsk",
                equipped_trooper,
                UnitChangeDelta::cached(UnitChangeCachedDelta::TrooperWeaponTotal, 1.0),
            );
            transitions.insert(
                equipped_trooper,
                "SCVMengsk",
                UnitChangeDelta::combined(-40.0, UnitChangeCachedDelta::TrooperWeaponTotal, -1.0),
            );
        }

        transitions.insert(
            "TrooperMengsk",
            "SCVMengsk",
            UnitChangeDelta::constant(-40.0),
        );
        transitions.insert(
            "SCVMengsk",
            "TrooperMengsk",
            UnitChangeDelta::constant(40.0),
        );
        transitions.insert(
            "GaryStetmann",
            "SuperGaryStetmann",
            UnitChangeDelta::constant(750.0),
        );
        transitions.insert_pair("Thor", "ThorWreckageSwann", UnitChangeCachedDelta::ThorGas);
        transitions.insert_pair(
            "SiegeTank",
            "SiegeTankWreckage",
            UnitChangeCachedDelta::SiegeTankGas,
        );
        transitions.insert_pair(
            "GuardianMP",
            "LeviathanCocoon",
            UnitChangeCachedDelta::GuardianCocoonDelta,
        );
        transitions.insert_pair(
            "Devourer",
            "LeviathanCocoon",
            UnitChangeCachedDelta::DevourerCocoonDelta,
        );
        transitions.insert_pair(
            "Viper",
            "LeviathanCocoon",
            UnitChangeCachedDelta::ViperCocoonDelta,
        );

        for swarm_host in ["SwarmHost", "SwarmHostBurrowed"] {
            transitions.insert_pair(
                swarm_host,
                "BrutaliskCocoonSwarmhost",
                UnitChangeCachedDelta::SwarmHostCocoonDelta,
            );
        }
        for ravager in ["RavagerAbathur", "RavagerAbathurBurrowed"] {
            transitions.insert_pair(
                ravager,
                "BrutaliskCocoonRavager",
                UnitChangeCachedDelta::RavagerCocoonDelta,
            );
        }
        for queen in ["Queen", "QueenBurrowed"] {
            transitions.insert_pair(
                queen,
                "BrutaliskCocoonQueen",
                UnitChangeCachedDelta::QueenCocoonDelta,
            );
        }

        transitions
    }

    fn insert(&mut self, old_unit: &str, new_unit: &str, delta: UnitChangeDelta) {
        self.by_old_unit
            .entry(old_unit.to_owned())
            .or_default()
            .insert(new_unit.to_owned(), delta);
    }

    fn insert_pair(&mut self, source_unit: &str, cocoon_unit: &str, delta: UnitChangeCachedDelta) {
        self.insert(
            source_unit,
            cocoon_unit,
            UnitChangeDelta::cached(delta, -1.0),
        );
        self.insert(
            cocoon_unit,
            source_unit,
            UnitChangeDelta::cached(delta, 1.0),
        );
    }

    fn delta(&self, old_unit: &str, new_unit: &str) -> Option<UnitChangeDelta> {
        self.by_old_unit
            .get(old_unit)
            .and_then(|target_units| target_units.get(new_unit))
            .copied()
    }
}

impl DictionaryUnitSet {
    fn contains(self, dictionaries: &StatsCounterDictionaries, unit: &str) -> bool {
        match self {
            Self::HornerUnits => dictionaries.contains_horner_unit(unit),
            Self::RoyalGuards => dictionaries.contains_royal_guard(unit),
        }
    }
}

impl UnitCostRule {
    fn replace_exact<const N: usize>(costs: [(&str, TotalUnitCost); N]) -> Self {
        Self::ReplaceExact {
            costs: costs
                .into_iter()
                .map(|(unit, cost)| (unit.to_owned(), cost))
                .collect(),
        }
    }

    fn scale_all(mineral_factor: f64, gas_factor: f64) -> Self {
        Self::ScaleAll {
            mineral_factor,
            gas_factor,
        }
    }

    fn scale_gas_all(gas_factor: f64) -> Self {
        Self::scale_all(1.0, gas_factor)
    }

    fn scale_exact<const N: usize>(units: [&str; N], mineral_factor: f64, gas_factor: f64) -> Self {
        Self::ScaleExact {
            units: Self::unit_set(units),
            mineral_factor,
            gas_factor,
        }
    }

    fn scale_except<const N: usize>(
        excluded_units: [&str; N],
        mineral_factor: f64,
        gas_factor: f64,
    ) -> Self {
        Self::ScaleExcept {
            excluded_units: Self::unit_set(excluded_units),
            mineral_factor,
            gas_factor,
        }
    }

    fn scale_mineral_except<const N: usize>(excluded_units: [&str; N], factor: f64) -> Self {
        Self::scale_except(excluded_units, factor, 1.0)
    }

    fn scale_gas_except<const N: usize>(excluded_units: [&str; N], factor: f64) -> Self {
        Self::scale_except(excluded_units, 1.0, factor)
    }

    fn scale_dictionary(unit_set: DictionaryUnitSet, mineral_factor: f64, gas_factor: f64) -> Self {
        Self::ScaleDictionary {
            unit_set,
            mineral_factor,
            gas_factor,
        }
    }

    fn scale_gas_dictionary(unit_set: DictionaryUnitSet, gas_factor: f64) -> Self {
        Self::scale_dictionary(unit_set, 1.0, gas_factor)
    }

    fn unit_set<const N: usize>(units: [&str; N]) -> HashSet<String> {
        units.into_iter().map(str::to_owned).collect()
    }

    fn scaled_cost(cost: TotalUnitCost, mineral_factor: f64, gas_factor: f64) -> TotalUnitCost {
        if mineral_factor == 1.0 {
            cost.scaled_gas(gas_factor)
        } else if gas_factor == 1.0 {
            cost.scaled_mineral(mineral_factor)
        } else {
            cost.scaled(mineral_factor, gas_factor)
        }
    }

    fn apply(
        &self,
        dictionaries: &StatsCounterDictionaries,
        unit: &str,
        cost: TotalUnitCost,
    ) -> TotalUnitCost {
        match self {
            Self::ReplaceExact { costs } => costs.get(unit).cloned().unwrap_or(cost),
            Self::ScaleAll {
                mineral_factor,
                gas_factor,
            } => Self::scaled_cost(cost, *mineral_factor, *gas_factor),
            Self::ScaleExact {
                units,
                mineral_factor,
                gas_factor,
            } => {
                if units.contains(unit) {
                    Self::scaled_cost(cost, *mineral_factor, *gas_factor)
                } else {
                    cost
                }
            }
            Self::ScaleExcept {
                excluded_units,
                mineral_factor,
                gas_factor,
            } => {
                if excluded_units.contains(unit) {
                    cost
                } else {
                    Self::scaled_cost(cost, *mineral_factor, *gas_factor)
                }
            }
            Self::ScaleDictionary {
                unit_set,
                mineral_factor,
                gas_factor,
            } => {
                if unit_set.contains(dictionaries, unit) {
                    Self::scaled_cost(cost, *mineral_factor, *gas_factor)
                } else {
                    cost
                }
            }
        }
    }
}

impl UnitCostRules {
    fn new(commander_cache: ReplayStatsCommanderCache) -> Self {
        let mut rules = Self::default();

        match commander_cache.commander() {
            CommanderKind::Abathur => {
                if commander_cache.prestige() == PrestigeKind::EssenceHoarder {
                    rules
                        .after_non_zero_check
                        .push(UnitCostRule::scale_gas_all(1.2));
                }
            }
            CommanderKind::Alarak => {
                if commander_cache.prestige() == PrestigeKind::ShadowOfDeath {
                    rules
                        .before_non_zero_check
                        .push(UnitCostRule::replace_exact([
                            (
                                "SOAMothershipv4",
                                TotalUnitCost::from_slice(&[400.0, 400.0]),
                            ),
                            ("VoidRayTaldarim", TotalUnitCost::from_slice(&[125.0, 75.0])),
                        ]));
                }
            }
            CommanderKind::Artanis => {
                if commander_cache.prestige() == PrestigeKind::ValorousInspirator {
                    rules.after_non_zero_check.push(UnitCostRule::scale_except(
                        ["PhotonCannon", "Observer", "ObserverSiegeMode"],
                        1.3,
                        1.3,
                    ));
                }
            }
            CommanderKind::Fenix => {
                if commander_cache.prestige() == PrestigeKind::NetworkAdministrator {
                    rules.after_non_zero_check.push(UnitCostRule::scale_except(
                        ["PhotonCannon", "Observer", "ObserverSiegeMode"],
                        0.5,
                        0.5,
                    ));
                }
            }
            CommanderKind::Horner => {
                Self::add_horner_rules(&mut rules, commander_cache.prestige());
            }
            CommanderKind::Karax => {
                if commander_cache.prestige() == PrestigeKind::TemplarApparent {
                    rules.after_non_zero_check.push(UnitCostRule::scale_except(
                        [
                            "ShieldBattery",
                            "KhaydarinMonolith",
                            "PhotonCannon",
                            "Observer",
                            "ObserverSiegeMode",
                        ],
                        0.6,
                        0.6,
                    ));
                }
            }
            CommanderKind::Kerrigan => {
                if let Some(coef) = commander_cache.kerrigan_gas_factor() {
                    rules
                        .after_non_zero_check
                        .push(UnitCostRule::scale_gas_all(coef));
                }
            }
            CommanderKind::Mengsk => {
                Self::add_mengsk_rules(&mut rules, commander_cache);
            }
            CommanderKind::Raynor => {
                Self::add_raynor_rules(&mut rules, commander_cache.prestige());
            }
            CommanderKind::Stetmann => {
                if commander_cache.prestige() == PrestigeKind::OilBaron {
                    rules
                        .after_non_zero_check
                        .push(UnitCostRule::scale_mineral_except(
                            [
                                "SpineCrawlerStetmann",
                                "SpineCrawlerUprootedStetmann",
                                "SporeCrawlerStetmann",
                                "SporeCrawlerUprootedStetmann",
                                "OverseerStetmann",
                                "OverseerStetmannSiegeMode",
                            ],
                            1.4,
                        ));
                }
            }
            CommanderKind::Stukov => {
                if commander_cache.prestige() == PrestigeKind::FrightfulFleshwelder {
                    rules.after_non_zero_check.push(UnitCostRule::scale_exact(
                        [
                            "SILiberator",
                            "StukovInfestedBanshee",
                            "StukovInfestedBansheeBurrowed",
                            "StukovInfestedDiamondBack",
                            "StukovInfestedSiegeTank",
                            "StukovInfestedSiegeTankUprooted",
                        ],
                        0.7,
                        0.7,
                    ));
                }
            }
            CommanderKind::Swann => {
                if commander_cache.prestige() == PrestigeKind::GreaseMonkey {
                    rules
                        .after_non_zero_check
                        .push(UnitCostRule::scale_gas_except(
                            [
                                "KelMorianGrenadeTurret",
                                "KelMorianMissileTurret",
                                "PerditionTurret",
                                "PerditionTurretUnderground",
                            ],
                            1.5,
                        ));
                }
            }
            CommanderKind::Tychus => {
                if commander_cache.prestige() == PrestigeKind::TechnicalRecruiter {
                    rules.after_non_zero_check.push(UnitCostRule::scale_except(
                        ["TychusSCVAutoTurret"],
                        1.5,
                        1.5,
                    ));
                }
            }
            CommanderKind::Zagara => {
                Self::add_zagara_rules(&mut rules, commander_cache.prestige());
            }
            CommanderKind::Zeratul => {
                if commander_cache.prestige() == PrestigeKind::KnowledgeSeeker {
                    rules.after_non_zero_check.push(UnitCostRule::scale_except(
                        [
                            "ZeratulObserver",
                            "ZeratulObserverSiegeMode",
                            "ZeratulPhotonCannon",
                            "ZeratulWarpPrism",
                            "ZeratulWarpPrismPhasing",
                        ],
                        1.25,
                        1.25,
                    ));
                }
            }
            CommanderKind::Dehaka | CommanderKind::Other => {}
        }

        rules
    }

    fn add_horner_rules(rules: &mut Self, prestige: PrestigeKind) {
        match prestige {
            PrestigeKind::ChaoticPowerCouple => {
                rules
                    .after_non_zero_check
                    .push(UnitCostRule::scale_dictionary(
                        DictionaryUnitSet::HornerUnits,
                        1.3,
                        1.3,
                    ));
            }
            PrestigeKind::WingCommanders => {
                rules
                    .after_non_zero_check
                    .push(UnitCostRule::scale_gas_dictionary(
                        DictionaryUnitSet::HornerUnits,
                        0.8,
                    ));
            }
            PrestigeKind::GalacticGunrunners => {
                rules.after_non_zero_check.push(UnitCostRule::scale_exact(
                    ["HHBomberPlatform"],
                    2.0,
                    2.0,
                ));
            }
            _ => {}
        }
    }

    fn add_mengsk_rules(rules: &mut Self, commander_cache: ReplayStatsCommanderCache) {
        if let Some(coef) = commander_cache.mengsk_royal_guard_factor() {
            rules
                .after_non_zero_check
                .push(UnitCostRule::scale_dictionary(
                    DictionaryUnitSet::RoyalGuards,
                    coef,
                    coef,
                ));
        }

        match commander_cache.prestige() {
            PrestigeKind::PrincipalProletariat => {
                rules
                    .after_non_zero_check
                    .push(UnitCostRule::scale_dictionary(
                        DictionaryUnitSet::RoyalGuards,
                        2.0,
                        0.75,
                    ));
            }
            PrestigeKind::MerchantOfDeath => {
                rules
                    .after_non_zero_check
                    .push(UnitCostRule::replace_exact([
                        (
                            "TrooperMengskAA",
                            TotalUnitCost::from_slice(&[40.0, 20.0, 80.0, 20.0]),
                        ),
                        (
                            "TrooperMengskFlamethrower",
                            TotalUnitCost::from_slice(&[40.0, 20.0, 80.0, 20.0]),
                        ),
                        (
                            "TrooperMengskImproved",
                            TotalUnitCost::from_slice(&[40.0, 20.0, 80.0, 20.0]),
                        ),
                    ]));
            }
            _ => {}
        }
    }

    fn add_raynor_rules(rules: &mut Self, prestige: PrestigeKind) {
        match prestige {
            PrestigeKind::RoughRider => {
                rules.after_non_zero_check.push(UnitCostRule::scale_exact(
                    [
                        "Banshee",
                        "Battlecruiser",
                        "VikingAssault",
                        "VikingFighter",
                        "SiegeTank",
                        "SiegeTankSieged",
                    ],
                    1.0,
                    1.25,
                ));
            }
            PrestigeKind::RebelRaider => {
                rules.after_non_zero_check.push(UnitCostRule::scale_exact(
                    ["Banshee", "Battlecruiser", "VikingAssault", "VikingFighter"],
                    1.5,
                    0.7,
                ));
                rules
                    .after_non_zero_check
                    .push(UnitCostRule::scale_mineral_except(
                        [
                            "Banshee",
                            "Battlecruiser",
                            "VikingAssault",
                            "VikingFighter",
                            "Bunker",
                            "MissileTurret",
                            "SpiderMine",
                        ],
                        1.5,
                    ));
            }
            _ => {}
        }
    }

    fn add_zagara_rules(rules: &mut Self, prestige: PrestigeKind) {
        match prestige {
            PrestigeKind::MotherOfConstructs => {
                rules.after_non_zero_check.push(UnitCostRule::scale_exact(
                    ["ZagaraCorruptor", "InfestedAbomination"],
                    0.75,
                    0.75,
                ));
            }
            PrestigeKind::ApexPredator => {
                rules.after_non_zero_check.push(UnitCostRule::scale_except(
                    [
                        "BileLauncherZagara",
                        "QueenCoop",
                        "QueenCoopBurrowed",
                        "Overseer",
                        "OverseerSiegeMode",
                        "SpineCrawler",
                        "SpineCrawlerUprooted",
                        "SporeCrawler",
                        "SporeCrawlerUprooted",
                    ],
                    1.25,
                    1.25,
                ));
            }
            _ => {}
        }
    }

    fn apply(
        &self,
        dictionaries: &StatsCounterDictionaries,
        unit: &str,
        mut cost: TotalUnitCost,
    ) -> TotalUnitCost {
        for rule in &self.before_non_zero_check {
            cost = rule.apply(dictionaries, unit, cost);
        }

        if cost.sum() != 0.0 {
            for rule in &self.after_non_zero_check {
                cost = rule.apply(dictionaries, unit, cost);
            }
        }

        cost
    }
}

impl ReplayDroneIdentifierCore {
    pub(crate) fn new(com1: Option<String>, com2: Option<String>) -> Self {
        Self {
            commanders: [com1.unwrap_or_default(), com2.unwrap_or_default()],
            recently_used: false,
            drones: 0,
            refineries: HashSet::new(),
        }
    }

    pub(crate) fn update_commanders(&mut self, idx: i64, commander: &str) {
        if idx == 1 || idx == 2 {
            let Ok(position) = usize::try_from(idx - 1) else {
                return;
            };
            self.commanders[position] = commander.to_string();
        }
    }

    pub(crate) fn get_bonus_vespene(&self) -> f64 {
        self.drones as f64 * 19.055
    }

    pub(crate) fn event(&mut self, event_kind: ReplayDroneCommandEventKind, event: &GameEvent) {
        let Some(user_id) = event.user_id else {
            return;
        };
        if !(0..=1).contains(&user_id) {
            return;
        }
        let Ok(user_idx) = usize::try_from(user_id) else {
            return;
        };
        if self.commanders[user_idx] != "Swann" {
            return;
        }

        let parse_snapshot_point = |snapshot: &SnapshotPoint| -> Option<String> {
            let mut values: Vec<String> = Vec::new();
            for value in &snapshot.values {
                match value {
                    SnapshotPointValue::Int(value) => values.push(value.to_string()),
                    SnapshotPointValue::Float(value) => values.push(format!("{value}")),
                }
            }
            if values.is_empty() {
                None
            } else {
                Some(values.join(":"))
            }
        };

        match event_kind {
            ReplayDroneCommandEventKind::Command => {
                self.recently_used = false;
                let ability_link = event
                    .m_abil
                    .as_ref()
                    .map(|value| value.m_abilLink)
                    .unwrap_or_default();
                if ability_link == 2536 {
                    self.recently_used = true;
                    if let Some(snapshot_key) = event
                        .m_data
                        .as_ref()
                        .and_then(|value| value.TargetUnit.as_ref())
                        .and_then(|value| value.m_snapshotPoint.as_ref())
                        .and_then(parse_snapshot_point)
                        && !self.refineries.contains(&snapshot_key)
                    {
                        self.drones += 1;
                        self.refineries.insert(snapshot_key);
                    }
                }
            }
            ReplayDroneCommandEventKind::CommandUpdateTargetUnit => {
                if self.recently_used
                    && let Some(snapshot_key) = event
                        .m_target
                        .as_ref()
                        .and_then(|value| value.m_snapshotPoint.as_ref())
                        .and_then(parse_snapshot_point)
                    && !self.refineries.contains(&snapshot_key)
                {
                    self.drones += 1;
                    self.refineries.insert(snapshot_key);
                }
            }
        }
    }
}

impl ReplayStatsCounterCore {
    pub(crate) fn new(
        dictionaries: Arc<StatsCounterDictionaries>,
        masteries: [u32; 6],
        commander: Option<String>,
    ) -> Self {
        let parsed_masteries = masteries.map(i64::from);
        let commander = StatsCounterMath::normalize_commander_name(&commander.unwrap_or_default());
        let commander_cache = ReplayStatsCommanderCache::new(&commander, None, &parsed_masteries);
        let unit_cost_rules = UnitCostRules::new(commander_cache);
        Self {
            dictionaries,
            masteries: parsed_masteries,
            commander,
            commander_cache,
            prestige: None,
            enable_updates: false,
            salvaged_unit_counts: HashMap::new(),
            unit_costs_cache: HashMap::new(),
            unit_change_costs: None,
            unit_change_transitions: UnitChangeTransitionMap::new(),
            unit_cost_rules,
            army_value_offset: 0.0,
            trooper_weapon_cost: (160.0, 0.0),
            tychus_gear_cost: (750.0, 1600.0),
            tychus_has_first_outlaw: false,
            zagara_free_banelings: 0,
            kills: Vec::new(),
            army_value: Vec::new(),
            supply: Vec::new(),
            collection_rate: Vec::new(),
        }
    }

    fn refresh_commander_cache(&mut self) {
        self.commander_cache = ReplayStatsCommanderCache::new(
            &self.commander,
            self.prestige.as_deref(),
            &self.masteries,
        );
        self.unit_cost_rules = UnitCostRules::new(self.commander_cache);
    }

    fn clear_cost_caches(&mut self) {
        self.unit_costs_cache.clear();
        self.unit_change_costs = None;
    }

    fn get_base_cost(&self, unit: &str) -> Option<TotalUnitCost> {
        if self.commander.is_empty() {
            return None;
        }
        if let Some(cost) = self.dictionaries.base_cost(&self.commander, unit) {
            return Some(cost);
        }

        let replacements: [(&str, &str); 6] = [
            ("Burrowed", ""),
            ("Phasing", ""),
            ("Uprooted", ""),
            ("Sieged", ""),
            ("SiegeMode", ""),
            ("Fighter", "Assault"),
        ];
        for (suffix, replace_with) in replacements {
            if unit.ends_with(suffix) {
                let candidate = if replace_with.is_empty() {
                    unit.replace(suffix, "")
                } else {
                    unit.replace(suffix, replace_with)
                };
                if let Some(cost) = self.dictionaries.base_cost(&self.commander, &candidate) {
                    return Some(cost);
                }
            }
        }

        None
    }

    fn unit_cost(&mut self, unit: &str) -> TotalUnitCost {
        if let Some(cost) = self.unit_costs_cache.get(unit) {
            return cost.clone();
        }

        let cost = self.get_base_cost(unit).unwrap_or_else(TotalUnitCost::zero);
        let cost = self
            .unit_cost_rules
            .apply(self.dictionaries.as_ref(), unit, cost);
        self.unit_costs_cache.insert(unit.to_owned(), cost);
        self.unit_costs_cache
            .get(unit)
            .cloned()
            .unwrap_or_else(TotalUnitCost::zero)
    }

    fn calculate_total_unit_value(
        &self,
        unit: &str,
        unit_alive_raw: f64,
        unit_dead_raw: f64,
        cost: &TotalUnitCost,
    ) -> f64 {
        if cost.sum() == 0.0 {
            return 0.0;
        }

        let unit_dead = if self.dictionaries.contains_outlaw(unit) {
            0.0
        } else {
            unit_dead_raw
        };
        let salvaged_count = self
            .salvaged_unit_counts
            .get(unit)
            .copied()
            .unwrap_or_default();
        let unit_alive = unit_alive_raw - salvaged_count as f64;

        if self.commander_cache.uses_zagara_baneling_value_adjustment()
            && (unit == "Baneling" || unit == "HotSSplitterlingBig")
        {
            let primary_cost = cost.first().sum();
            let full_cost = cost.get(1).sum();
            let free_banelings = self.zagara_free_banelings as f64;
            let mut result = (unit_alive - free_banelings) * primary_cost;
            result += free_banelings * full_cost;
            result -= unit_dead * full_cost;
            return result;
        }

        if cost.len() == 1 {
            return (unit_alive - unit_dead) * cost.first().sum();
        }
        if cost.len() >= 2 {
            return unit_alive * cost.first().sum() - unit_dead * cost.get(1).sum();
        }
        0.0
    }

    fn cost_sum(&mut self, unit: &str) -> f64 {
        self.unit_cost(unit).sum()
    }

    fn cost_additive(&mut self, unit: &str) -> f64 {
        let cost = self.unit_cost(unit);
        cost.get(1).sum()
    }

    fn cost_gas(&mut self, unit: &str) -> f64 {
        self.unit_cost(unit).first().gas()
    }

    fn unit_change_costs(&mut self) -> UnitChangeCostCache {
        if let Some(cache) = self.unit_change_costs {
            return cache;
        }

        let mutalisk_total = self.cost_sum("Mutalisk");
        let roach_total = self.cost_sum("RoachVile");
        let cache = UnitChangeCostCache {
            thor_gas: self.cost_gas("Thor"),
            siege_tank_gas: self.cost_gas("SiegeTank"),
            guardian_cocoon_delta: self.cost_additive("GuardianMP") - mutalisk_total,
            devourer_cocoon_delta: self.cost_additive("Devourer") - mutalisk_total,
            viper_cocoon_delta: self.cost_sum("Viper") - mutalisk_total,
            swarm_host_cocoon_delta: self.cost_sum("SwarmHost") - roach_total,
            ravager_cocoon_delta: self.cost_additive("RavagerAbathur") - roach_total,
            queen_cocoon_delta: self.cost_sum("Queen") - roach_total,
        };
        self.unit_change_costs = Some(cache);
        cache
    }

    fn calculate_army_value(&mut self, unit_counts: &indexmap::IndexMap<String, [i64; 4]>) -> i64 {
        let mut total = 0.0_f64;
        for (unit, values) in unit_counts {
            let cost = self.unit_cost(unit);
            total +=
                self.calculate_total_unit_value(unit, values[0] as f64, values[1] as f64, &cost);
        }

        total += self.army_value_offset;

        if self.commander_cache.uses_tychus_first_outlaw_discount()
            && !self.tychus_has_first_outlaw
            && unit_counts
                .keys()
                .any(|unit| self.dictionaries.contains_outlaw(unit))
        {
            self.tychus_has_first_outlaw = true;
        }
        if self.tychus_has_first_outlaw && total > 600.0 {
            total -= 600.0;
        }

        total = total.round_ties_even();
        if total < 0.0 {
            total = 0.0;
        }
        total as i64
    }

    pub(crate) fn set_enable_updates(&mut self, enabled: bool) {
        self.enable_updates = enabled;
    }

    pub(crate) fn append_salvaged_unit(&mut self, unit: &str) {
        let count = self
            .salvaged_unit_counts
            .entry(unit.to_owned())
            .or_default();
        *count += 1;
    }

    pub(crate) fn update_mastery(&mut self, idx: i64, count: i64) {
        let Ok(index) = usize::try_from(idx) else {
            return;
        };
        if self.enable_updates && index < self.masteries.len() && self.masteries[index] != count {
            self.masteries[index] = count;
            self.refresh_commander_cache();
            self.clear_cost_caches();
        }
    }

    pub(crate) fn update_prestige(&mut self, prestige: &str) {
        if self.prestige.as_deref() == Some(prestige) {
            return;
        }
        self.prestige = Some(prestige.to_owned());
        self.refresh_commander_cache();

        match self.commander_cache.prestige() {
            PrestigeKind::MerchantOfDeath => {
                self.trooper_weapon_cost = (40.0, 20.0);
            }
            PrestigeKind::LoneWolf => {
                self.tychus_gear_cost = (self.tychus_gear_cost.0 * 1.25, 0.0);
            }
            _ => {}
        }
        self.clear_cost_caches();
    }

    pub(crate) fn update_commander(&mut self, commander: &str) {
        let commander_name = StatsCounterMath::normalize_commander_name(commander);
        if self.enable_updates && self.commander != commander_name {
            self.commander = commander_name;
            self.refresh_commander_cache();
            self.clear_cost_caches();
        }
    }

    fn cached_unit_change_delta(&mut self, delta: UnitChangeCachedDelta) -> f64 {
        match delta {
            UnitChangeCachedDelta::TrooperWeaponTotal => {
                self.trooper_weapon_cost.0 + self.trooper_weapon_cost.1
            }
            UnitChangeCachedDelta::ThorGas => self.unit_change_costs().thor_gas,
            UnitChangeCachedDelta::SiegeTankGas => self.unit_change_costs().siege_tank_gas,
            UnitChangeCachedDelta::GuardianCocoonDelta => {
                self.unit_change_costs().guardian_cocoon_delta
            }
            UnitChangeCachedDelta::DevourerCocoonDelta => {
                self.unit_change_costs().devourer_cocoon_delta
            }
            UnitChangeCachedDelta::ViperCocoonDelta => self.unit_change_costs().viper_cocoon_delta,
            UnitChangeCachedDelta::SwarmHostCocoonDelta => {
                self.unit_change_costs().swarm_host_cocoon_delta
            }
            UnitChangeCachedDelta::RavagerCocoonDelta => {
                self.unit_change_costs().ravager_cocoon_delta
            }
            UnitChangeCachedDelta::QueenCocoonDelta => self.unit_change_costs().queen_cocoon_delta,
        }
    }

    fn unit_change_delta_value(&mut self, delta: UnitChangeDelta) -> f64 {
        let cached_delta = delta
            .cached_delta
            .map(|cached_delta| self.cached_unit_change_delta(cached_delta))
            .unwrap_or_default();
        delta.constant + delta.cached_multiplier * cached_delta
    }

    pub(crate) fn unit_change_event(&mut self, unit: &str, old_unit: &str) {
        let Some(delta) = self.unit_change_transitions.delta(old_unit, unit) else {
            return;
        };
        let delta = self.unit_change_delta_value(delta);

        if delta != 0.0 {
            self.army_value_offset += delta;
        }
    }

    pub(crate) fn mindcontrolled_unit_dies(&mut self, unit: &str) {
        let cost = self.cost_sum(unit);
        if cost > 0.0 {
            self.army_value_offset += cost;
        }
    }

    pub(crate) fn upgrade_event(&mut self, upgrade: &str) {
        if self.dictionaries.contains_tychus_base_upgrade(upgrade) {
            self.army_value_offset += self.tychus_gear_cost.0;
        } else if self.dictionaries.contains_tychus_ultimate_upgrade(upgrade) {
            self.army_value_offset += self.tychus_gear_cost.1;
        }
    }

    pub(crate) fn unit_created_event(&mut self, unit_type: &str, event: &TrackerEvent) {
        if !self.commander_cache.uses_zagara_baneling_value_adjustment()
            || (unit_type != "Baneling" && unit_type != "HotSSplitterlingBig")
        {
            return;
        }

        if event.m_creator_ability_name.as_deref() != Some("MorphZerglingToSplitterling") {
            self.zagara_free_banelings += 1;
        }
    }

    pub(crate) fn add_stats(
        &mut self,
        unit_counts: &indexmap::IndexMap<String, [i64; 4]>,
        drone_counter: &ReplayDroneIdentifierCore,
        kills: i64,
        supply_used: f64,
        collection_rate: f64,
    ) {
        self.kills.push(kills);
        let current_army = self.calculate_army_value(unit_counts) as f64;
        self.army_value.push(current_army);
        self.supply.push(supply_used);
        self.collection_rate
            .push(collection_rate + drone_counter.get_bonus_vespene());
    }

    pub(crate) fn get_stats(&mut self, player_name: &str) -> AnalysisPlayerStatsSeries {
        let mut dehaka_changed_indices = BTreeSet::new();
        if self.commander_cache.uses_dehaka_army_value_spike_filter() {
            dehaka_changed_indices = StatsCounterMath::upward_spike_indices(&self.army_value)
                .into_iter()
                .collect();
            StatsCounterMath::remove_upward_spikes(&mut self.army_value);
        }

        let army = self
            .army_value
            .iter()
            .enumerate()
            .map(|(idx, value)| {
                if dehaka_changed_indices.contains(&idx) {
                    *value
                } else if value.is_finite() && value.fract().abs() < 1e-9 {
                    value.round()
                } else {
                    *value
                }
            })
            .collect::<Vec<f64>>();

        AnalysisPlayerStatsSeries {
            name: player_name.to_string(),
            killed: self.kills.iter().map(|value| *value as f64).collect(),
            army,
            supply: self.supply.clone(),
            mining: StatsCounterMath::rolling_average(&self.collection_rate),
            army_force_float_indices: dehaka_changed_indices,
        }
    }
}
