use super::{CommanderKind, PrestigeKind, ReplayStatsCommanderCache, StatsCounterDictionaries};
use crate::stats_counter_math::TotalUnitCost;
use std::collections::{HashMap, HashSet};

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
pub(super) struct UnitCostRules {
    before_non_zero_check: Vec<UnitCostRule>,
    after_non_zero_check: Vec<UnitCostRule>,
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
    pub(super) fn new(commander_cache: ReplayStatsCommanderCache) -> Self {
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

    pub(super) fn apply(
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
