use crate::cache_overall_stats_generator::AnalysisPlayerStatsSeries;
use crate::dictionary_data::UnitBaseCostsJson;
use s2protocol_port::{GameEvent, SnapshotPoint, SnapshotPointValue, TrackerEvent};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct StatsCounterDictionaries {
    pub unit_base_costs: UnitBaseCostsJson,
    pub royal_guards: HashSet<String>,
    pub horners_units: HashSet<String>,
    pub tychus_base_upgrades: HashSet<String>,
    pub tychus_ultimate_upgrades: HashSet<String>,
    pub outlaws: HashSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ReplayDroneIdentifierCore {
    commanders: [String; 2],
    recently_used: bool,
    drones: i64,
    refineries: HashSet<String>,
}

#[derive(Clone, Debug)]
pub struct ReplayStatsCounterCore {
    dictionaries: StatsCounterDictionaries,
    masteries: [i64; 6],
    unit_dict: HashMap<String, (f64, f64)>,
    commander: String,
    prestige: Option<String>,
    enable_updates: bool,
    salvaged_units: Vec<String>,
    unit_costs_cache: HashMap<String, TotalUnitCost>,
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

#[derive(Clone, Copy, Debug, Default)]
struct UnitCost {
    mineral: f64,
    gas: f64,
}

impl UnitCost {
    fn zero() -> Self {
        Self {
            mineral: 0.0,
            gas: 0.0,
        }
    }

    fn new(mineral: f64, gas: f64) -> Self {
        Self { mineral, gas }
    }

    fn sum(self) -> f64 {
        self.mineral + self.gas
    }

    fn scaled(self, min_mult: f64, gas_mult: f64) -> Self {
        Self {
            mineral: self.mineral * min_mult,
            gas: self.gas * gas_mult,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct TotalUnitCost {
    values: Vec<UnitCost>,
}

impl TotalUnitCost {
    fn zero() -> Self {
        Self {
            values: vec![UnitCost::zero()],
        }
    }

    fn from_slice(values: &[f64]) -> Self {
        let values = values
            .chunks_exact(2)
            .map(|chunk| UnitCost::new(chunk[0], chunk[1]))
            .collect::<Vec<UnitCost>>();
        if values.is_empty() {
            Self::zero()
        } else {
            Self { values }
        }
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn first(&self) -> UnitCost {
        self.values.first().copied().unwrap_or_else(UnitCost::zero)
    }

    fn get(&self, index: usize) -> UnitCost {
        self.values
            .get(index)
            .copied()
            .unwrap_or_else(UnitCost::zero)
    }

    fn sum(&self) -> f64 {
        self.values.iter().map(|value| value.sum()).sum()
    }

    fn scaled(&self, min_mult: f64, gas_mult: f64) -> Self {
        Self {
            values: self
                .values
                .iter()
                .copied()
                .map(|value| value.scaled(min_mult, gas_mult))
                .collect(),
        }
    }

    fn scaled_mineral(&self, min_mult: f64) -> Self {
        self.scaled(min_mult, 1.0)
    }

    fn scaled_gas(&self, gas_mult: f64) -> Self {
        self.scaled(1.0, gas_mult)
    }
}

fn normalize_commander_name(commander: &str) -> String {
    if commander == "Han & Horner" {
        "Horner".to_string()
    } else {
        commander.to_string()
    }
}

fn remove_upward_spikes(values: &mut [f64]) {
    if values.len() < 3 {
        return;
    }
    for idx in 1..(values.len() - 1) {
        if values[idx] > values[idx - 1] && values[idx] > values[idx + 1] {
            values[idx] = (values[idx - 1] + values[idx + 1]) / 2.0;
        }
    }
}

fn upward_spike_indices(values: &[f64]) -> HashSet<usize> {
    let mut indices = HashSet::new();
    if values.len() < 3 {
        return indices;
    }
    for idx in 1..(values.len() - 1) {
        if values[idx] > values[idx - 1] && values[idx] > values[idx + 1] {
            indices.insert(idx);
        }
    }
    indices
}

impl ReplayDroneIdentifierCore {
    pub fn new(com1: Option<String>, com2: Option<String>) -> Self {
        Self {
            commanders: [com1.unwrap_or_default(), com2.unwrap_or_default()],
            recently_used: false,
            drones: 0,
            refineries: HashSet::new(),
        }
    }

    pub fn update_commanders(&mut self, idx: i64, commander: &str) {
        if idx == 1 || idx == 2 {
            let Ok(position) = usize::try_from(idx - 1) else {
                return;
            };
            self.commanders[position] = commander.to_string();
        }
    }

    pub fn get_bonus_vespene(&self) -> f64 {
        self.drones as f64 * 19.055
    }

    pub fn event(&mut self, event: &GameEvent) {
        let event_name = event.event.as_str();
        if event_name != "NNet.Game.SCmdEvent"
            && event_name != "NNet.Game.SCmdUpdateTargetUnitEvent"
        {
            return;
        }

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

        if event_name == "NNet.Game.SCmdEvent" {
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
                {
                    if !self.refineries.contains(&snapshot_key) {
                        self.drones += 1;
                        self.refineries.insert(snapshot_key);
                    }
                }
            }
            return;
        }

        if self.recently_used && event_name == "NNet.Game.SCmdUpdateTargetUnitEvent" {
            if let Some(snapshot_key) = event
                .m_target
                .as_ref()
                .and_then(|value| value.m_snapshotPoint.as_ref())
                .and_then(parse_snapshot_point)
            {
                if !self.refineries.contains(&snapshot_key) {
                    self.drones += 1;
                    self.refineries.insert(snapshot_key);
                }
            }
        }
    }
}

impl ReplayStatsCounterCore {
    pub fn new(
        dictionaries: StatsCounterDictionaries,
        masteries: [u32; 6],
        commander: Option<String>,
    ) -> Self {
        let parsed_masteries = masteries.map(i64::from);
        Self {
            dictionaries,
            masteries: parsed_masteries,
            unit_dict: HashMap::new(),
            commander: normalize_commander_name(&commander.unwrap_or_default()),
            prestige: None,
            enable_updates: false,
            salvaged_units: Vec::new(),
            unit_costs_cache: HashMap::new(),
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

    fn get_base_cost(&self, unit: &str) -> Option<TotalUnitCost> {
        if self.commander.is_empty() {
            return None;
        }
        let Some(commander_costs) = self.dictionaries.unit_base_costs.get(&self.commander) else {
            return None;
        };
        if let Some(cost) = commander_costs.get(unit) {
            return Some(TotalUnitCost::from_slice(cost));
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
                if let Some(cost) = commander_costs.get(&candidate) {
                    return Some(TotalUnitCost::from_slice(cost));
                }
            }
        }

        None
    }

    fn unit_cost(&mut self, unit: &str) -> TotalUnitCost {
        if let Some(cost) = self.unit_costs_cache.get(unit) {
            return cost.clone();
        }

        let mut cost = self.get_base_cost(unit).unwrap_or_else(TotalUnitCost::zero);
        let prestige = self.prestige.as_deref().unwrap_or("");

        if self.commander == "Abathur" && prestige == "Essence Hoarder" {
            cost = cost.scaled_gas(1.2);
        } else if self.commander == "Alarak" && prestige == "Shadow of Death" {
            if unit == "SOAMothershipv4" {
                cost = TotalUnitCost::from_slice(&[400.0, 400.0]);
            } else if unit == "VoidRayTaldarim" {
                cost = TotalUnitCost::from_slice(&[125.0, 75.0]);
            }
        }

        if cost.sum() != 0.0 {
            if self.commander == "Artanis"
                && prestige == "Valorous Inspirator"
                && unit != "PhotonCannon"
                && unit != "Observer"
                && unit != "ObserverSiegeMode"
            {
                cost = cost.scaled(1.3, 1.3);
            } else if self.commander == "Fenix"
                && prestige == "Network Administrator"
                && unit != "PhotonCannon"
                && unit != "Observer"
                && unit != "ObserverSiegeMode"
            {
                cost = cost.scaled(0.5, 0.5);
            } else if self.commander == "Horner" {
                if prestige == "Chaotic Power Couple"
                    && self.dictionaries.horners_units.contains(unit)
                {
                    cost = cost.scaled(1.3, 1.3);
                } else if prestige == "Wing Commanders"
                    && self.dictionaries.horners_units.contains(unit)
                {
                    cost = cost.scaled_gas(0.8);
                } else if prestige == "Galactic Gunrunners" && unit == "HHBomberPlatform" {
                    cost = cost.scaled(2.0, 2.0);
                }
            } else if self.commander == "Karax"
                && prestige == "Templar Apparent"
                && !matches!(
                    unit,
                    "ShieldBattery"
                        | "KhaydarinMonolith"
                        | "PhotonCannon"
                        | "Observer"
                        | "ObserverSiegeMode"
                )
            {
                cost = cost.scaled(0.6, 0.6);
            } else if self.commander == "Kerrigan" && self.masteries[2] > 0 {
                cost = cost.scaled_gas(1.0 - self.masteries[2] as f64 / 100.0);
            } else if self.commander == "Mengsk" {
                if self.masteries[3] > 0 && self.dictionaries.royal_guards.contains(unit) {
                    let coef = 1.0 - 20.0 * self.masteries[3] as f64 / 3000.0;
                    cost = cost.scaled(coef, coef);
                }
                if prestige == "Principal Proletariat"
                    && self.dictionaries.royal_guards.contains(unit)
                {
                    cost = cost.scaled(2.0, 0.75);
                }
                if prestige == "Merchant of Death"
                    && matches!(
                        unit,
                        "TrooperMengskAA" | "TrooperMengskFlamethrower" | "TrooperMengskImproved"
                    )
                {
                    cost = TotalUnitCost::from_slice(&[40.0, 20.0, 80.0, 20.0]);
                }
            } else if self.commander == "Raynor" {
                if prestige == "Rough Rider"
                    && matches!(
                        unit,
                        "Banshee"
                            | "Battlecruiser"
                            | "VikingAssault"
                            | "VikingFighter"
                            | "SiegeTank"
                            | "SiegeTankSieged"
                    )
                {
                    cost = cost.scaled_gas(1.25);
                } else if prestige == "Rebel Raider" {
                    if matches!(
                        unit,
                        "Banshee" | "Battlecruiser" | "VikingAssault" | "VikingFighter"
                    ) {
                        cost = cost.scaled(1.5, 0.7);
                    } else if !matches!(unit, "Bunker" | "MissileTurret" | "SpiderMine") {
                        cost = cost.scaled_mineral(1.5);
                    }
                }
            } else if self.commander == "Stetmann"
                && prestige == "Oil Baron"
                && !matches!(
                    unit,
                    "SpineCrawlerStetmann"
                        | "SpineCrawlerUprootedStetmann"
                        | "SporeCrawlerStetmann"
                        | "SporeCrawlerUprootedStetmann"
                        | "OverseerStetmann"
                        | "OverseerStetmannSiegeMode"
                )
            {
                cost = cost.scaled_mineral(1.4);
            } else if self.commander == "Stukov"
                && prestige == "Frightful Fleshwelder"
                && matches!(
                    unit,
                    "SILiberator"
                        | "StukovInfestedBanshee"
                        | "StukovInfestedBansheeBurrowed"
                        | "StukovInfestedDiamondBack"
                        | "StukovInfestedSiegeTank"
                        | "StukovInfestedSiegeTankUprooted"
                )
            {
                cost = cost.scaled(0.7, 0.7);
            } else if self.commander == "Swann" {
                if prestige == "Grease Monkey"
                    && !matches!(
                        unit,
                        "KelMorianGrenadeTurret"
                            | "KelMorianMissileTurret"
                            | "PerditionTurret"
                            | "PerditionTurretUnderground"
                    )
                {
                    cost = cost.scaled_gas(1.5);
                }
            } else if self.commander == "Tychus" {
                if prestige == "Technical Recruiter" && unit != "TychusSCVAutoTurret" {
                    cost = cost.scaled(1.5, 1.5);
                }
            } else if self.commander == "Zagara" {
                if prestige == "Mother of Constructs"
                    && matches!(unit, "ZagaraCorruptor" | "InfestedAbomination")
                {
                    cost = cost.scaled(0.75, 0.75);
                } else if prestige == "Apex Predator"
                    && !matches!(
                        unit,
                        "BileLauncherZagara"
                            | "QueenCoop"
                            | "QueenCoopBurrowed"
                            | "Overseer"
                            | "OverseerSiegeMode"
                            | "SpineCrawler"
                            | "SpineCrawlerUprooted"
                            | "SporeCrawler"
                            | "SporeCrawlerUprooted"
                    )
                {
                    cost = cost.scaled(1.25, 1.25);
                }
            } else if self.commander == "Zeratul"
                && prestige == "Knowledge Seeker"
                && !matches!(
                    unit,
                    "ZeratulObserver"
                        | "ZeratulObserverSiegeMode"
                        | "ZeratulPhotonCannon"
                        | "ZeratulWarpPrism"
                        | "ZeratulWarpPrismPhasing"
                )
            {
                cost = cost.scaled(1.25, 1.25);
            }
        }
        self.unit_costs_cache.insert(unit.to_owned(), cost);
        self.unit_costs_cache
            .get(unit)
            .cloned()
            .unwrap_or_else(TotalUnitCost::zero)
    }

    fn calculate_total_unit_value(&self, unit: &str, cost: &TotalUnitCost) -> f64 {
        if cost.sum() == 0.0 {
            return 0.0;
        }

        let Some((unit_alive_raw, unit_dead_raw)) = self.unit_dict.get(unit).copied() else {
            return 0.0;
        };
        let unit_dead = if self.dictionaries.outlaws.contains(unit) {
            0.0
        } else {
            unit_dead_raw
        };
        let salvaged_count = self
            .salvaged_units
            .iter()
            .filter(|saved| saved.as_str() == unit)
            .count();
        let unit_alive = unit_alive_raw - salvaged_count as f64;

        if self.commander == "Zagara" && (unit == "Baneling" || unit == "HotSSplitterlingBig") {
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
        self.unit_cost(unit).first().gas
    }

    fn calculate_army_value(&mut self) -> i64 {
        let mut total = 0.0_f64;
        let keys = self.unit_dict.keys().cloned().collect::<Vec<String>>();
        for unit in keys {
            let cost = self.unit_cost(&unit);
            total += self.calculate_total_unit_value(&unit, &cost);
        }

        total += self.army_value_offset;

        if self.commander == "Tychus" && !self.tychus_has_first_outlaw {
            if self
                .unit_dict
                .keys()
                .any(|unit| self.dictionaries.outlaws.contains(unit))
            {
                self.tychus_has_first_outlaw = true;
            }
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

    pub fn set_unit_dict(&mut self, unit_dict: &indexmap::IndexMap<String, [i64; 4]>) {
        let mut out: HashMap<String, (f64, f64)> = HashMap::new();
        for (unit, values) in unit_dict {
            out.insert(unit.clone(), (values[0] as f64, values[1] as f64));
        }
        self.unit_dict = out;
    }

    pub fn set_enable_updates(&mut self, enabled: bool) {
        self.enable_updates = enabled;
    }

    pub fn append_salvaged_unit(&mut self, unit: &str) {
        self.salvaged_units.push(unit.to_string());
    }

    pub fn update_mastery(&mut self, idx: i64, count: i64) {
        let Ok(index) = usize::try_from(idx) else {
            return;
        };
        if self.enable_updates && index < self.masteries.len() && self.masteries[index] != count {
            self.masteries[index] = count;
        }
    }

    pub fn update_prestige(&mut self, prestige: &str) {
        if self.prestige.as_deref() == Some(prestige) {
            return;
        }
        self.prestige = Some(prestige.to_owned());

        if prestige == "Merchant of Death" {
            self.trooper_weapon_cost = (40.0, 20.0);
        }
        if prestige == "Lone Wolf" {
            self.tychus_gear_cost = (self.tychus_gear_cost.0 * 1.25, 0.0);
        }
        self.unit_costs_cache.clear();
    }

    pub fn update_commander(&mut self, commander: &str) {
        let commander_name = normalize_commander_name(commander);
        if self.enable_updates && self.commander != commander_name {
            self.commander = commander_name;
            self.unit_costs_cache.clear();
        }
    }

    pub fn unit_change_event(&mut self, unit: &str, old_unit: &str) {
        if old_unit == "TrooperMengsk"
            && matches!(
                unit,
                "TrooperMengskAA" | "TrooperMengskFlamethrower" | "TrooperMengskImproved"
            )
        {
            self.army_value_offset += self.trooper_weapon_cost.0 + self.trooper_weapon_cost.1;
        } else if old_unit == "GaryStetmann" && unit == "SuperGaryStetmann" {
            self.army_value_offset += 750.0;
        } else if old_unit == "TrooperMengsk" && unit == "SCVMengsk" {
            self.army_value_offset -= 40.0;
        } else if old_unit == "SCVMengsk" && unit == "TrooperMengsk" {
            self.army_value_offset += 40.0;
        } else if matches!(
            old_unit,
            "TrooperMengskAA" | "TrooperMengskFlamethrower" | "TrooperMengskImproved"
        ) && unit == "SCVMengsk"
        {
            self.army_value_offset -=
                40.0 + self.trooper_weapon_cost.0 + self.trooper_weapon_cost.1;
        } else if old_unit == "Thor" && unit == "ThorWreckageSwann" {
            self.army_value_offset -= self.cost_gas("Thor");
        } else if old_unit == "ThorWreckageSwann" && unit == "Thor" {
            self.army_value_offset += self.cost_gas("Thor");
        } else if old_unit == "SiegeTank" && unit == "SiegeTankWreckage" {
            self.army_value_offset -= self.cost_gas("SiegeTank");
        } else if old_unit == "SiegeTankWreckage" && unit == "SiegeTank" {
            self.army_value_offset += self.cost_gas("SiegeTank");
        } else if old_unit == "GuardianMP" && unit == "LeviathanCocoon" {
            let guardian_additive = self.cost_additive("GuardianMP");
            let mutalisk_total = self.cost_sum("Mutalisk");
            self.army_value_offset -= guardian_additive - mutalisk_total;
        } else if old_unit == "LeviathanCocoon" && unit == "GuardianMP" {
            let guardian_additive = self.cost_additive("GuardianMP");
            let mutalisk_total = self.cost_sum("Mutalisk");
            self.army_value_offset += guardian_additive - mutalisk_total;
        } else if old_unit == "Devourer" && unit == "LeviathanCocoon" {
            let devourer_additive = self.cost_additive("Devourer");
            let mutalisk_total = self.cost_sum("Mutalisk");
            self.army_value_offset -= devourer_additive - mutalisk_total;
        } else if old_unit == "LeviathanCocoon" && unit == "Devourer" {
            let devourer_additive = self.cost_additive("Devourer");
            let mutalisk_total = self.cost_sum("Mutalisk");
            self.army_value_offset += devourer_additive - mutalisk_total;
        } else if old_unit == "Viper" && unit == "LeviathanCocoon" {
            let viper_total = self.cost_sum("Viper");
            let mutalisk_total = self.cost_sum("Mutalisk");
            self.army_value_offset -= viper_total - mutalisk_total;
        } else if old_unit == "LeviathanCocoon" && unit == "Viper" {
            let viper_total = self.cost_sum("Viper");
            let mutalisk_total = self.cost_sum("Mutalisk");
            self.army_value_offset += viper_total - mutalisk_total;
        } else if matches!(old_unit, "SwarmHost" | "SwarmHostBurrowed")
            && unit == "BrutaliskCocoonSwarmhost"
        {
            let swarm_host_total = self.cost_sum("SwarmHost");
            let roach_total = self.cost_sum("RoachVile");
            self.army_value_offset -= swarm_host_total - roach_total;
        } else if old_unit == "BrutaliskCocoonSwarmhost"
            && matches!(unit, "SwarmHost" | "SwarmHostBurrowed")
        {
            let swarm_host_total = self.cost_sum("SwarmHost");
            let roach_total = self.cost_sum("RoachVile");
            self.army_value_offset += swarm_host_total - roach_total;
        } else if matches!(old_unit, "RavagerAbathur" | "RavagerAbathurBurrowed")
            && unit == "BrutaliskCocoonRavager"
        {
            let ravager_additive = self.cost_additive("RavagerAbathur");
            let roach_total = self.cost_sum("RoachVile");
            self.army_value_offset -= ravager_additive - roach_total;
        } else if old_unit == "BrutaliskCocoonRavager"
            && matches!(unit, "RavagerAbathur" | "RavagerAbathurBurrowed")
        {
            let ravager_additive = self.cost_additive("RavagerAbathur");
            let roach_total = self.cost_sum("RoachVile");
            self.army_value_offset += ravager_additive - roach_total;
        } else if matches!(old_unit, "Queen" | "QueenBurrowed") && unit == "BrutaliskCocoonQueen" {
            let queen_total = self.cost_sum("Queen");
            let roach_total = self.cost_sum("RoachVile");
            self.army_value_offset -= queen_total - roach_total;
        } else if old_unit == "BrutaliskCocoonQueen" && matches!(unit, "Queen" | "QueenBurrowed") {
            let queen_total = self.cost_sum("Queen");
            let roach_total = self.cost_sum("RoachVile");
            self.army_value_offset += queen_total - roach_total;
        }
    }

    pub fn mindcontrolled_unit_dies(&mut self, unit: &str) {
        let cost = self.cost_sum(unit);
        if cost > 0.0 {
            self.army_value_offset += cost;
        }
    }

    pub fn upgrade_event(&mut self, upgrade: &str) {
        if self.dictionaries.tychus_base_upgrades.contains(upgrade) {
            self.army_value_offset += self.tychus_gear_cost.0;
        } else if self.dictionaries.tychus_ultimate_upgrades.contains(upgrade) {
            self.army_value_offset += self.tychus_gear_cost.1;
        }
    }

    pub fn unit_created_event(&mut self, unit_type: &str, event: &TrackerEvent) {
        if self.commander != "Zagara"
            || (unit_type != "Baneling" && unit_type != "HotSSplitterlingBig")
        {
            return;
        }

        if event.m_creator_ability_name.as_deref() != Some("MorphZerglingToSplitterling") {
            self.zagara_free_banelings += 1;
        }
    }

    pub fn add_stats(
        &mut self,
        drone_counter: &ReplayDroneIdentifierCore,
        kills: i64,
        supply_used: f64,
        collection_rate: f64,
    ) {
        self.kills.push(kills);
        let current_army = self.calculate_army_value() as f64;
        self.army_value.push(current_army);
        self.supply.push(supply_used);
        self.collection_rate
            .push(collection_rate + drone_counter.get_bonus_vespene());
    }

    pub fn get_stats(&mut self, player_name: &str) -> AnalysisPlayerStatsSeries {
        let mut dehaka_changed_indices = BTreeSet::new();
        if self.commander == "Dehaka" {
            dehaka_changed_indices = upward_spike_indices(&self.army_value).into_iter().collect();
            remove_upward_spikes(&mut self.army_value);
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
            mining: rolling_average(&self.collection_rate),
            army_force_float_indices: dehaka_changed_indices,
        }
    }
}

fn rolling_average(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(values.len());
    for (idx, value) in values.iter().enumerate() {
        if idx == 0 {
            out.push(*value);
        } else {
            out.push(0.5 * *value + 0.5 * values[idx - 1]);
        }
    }
    out
}
