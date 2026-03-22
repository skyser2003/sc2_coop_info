use crate::sc2_dictionary_data::resolve_sc2_dictionary_data_dir;
use chrono::NaiveDate;
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

const REQUIRED_DATA_FILES: [&str; 28] = [
    "unit_names.json",
    "unit_add_kills_to.json",
    "commander_mastery.json",
    "co_mastery_upgrades.json",
    "prestige_names.json",
    "map_names.json",
    "bonus_objectives.json",
    "mc_units.json",
    "units_in_waves.json",
    "hfts_units.json",
    "tus_units.json",
    "mutators.json",
    "mutators_exclude_ids.json",
    "units_to_stats.json",
    "amon_player_ids.json",
    "prestige_upgrades.json",
    "unit_comp_dict.json",
    "unit_base_costs.json",
    "mutator_ids.json",
    "cached_mutators.json",
    "weekly_mutations.json",
    "weekly_mutation_date.json",
    "royal_guards.json",
    "horners_units.json",
    "tychus_base_upgrades.json",
    "tychus_ultimate_upgrades.json",
    "outlaws.json",
    "replay_analysis_data.json",
];

const MUTATOR_CUSTOM_FORBIDDEN: [&str; 15] = [
    "Nap Time",
    "Stone Zealots",
    "Chaos Studios",
    "Undying Evil",
    "Afraid of the Dark",
    "Trick or Treat",
    "Turkey Shoot",
    "Sharing Is Caring",
    "Gift Exchange",
    "Naughty List",
    "Extreme Caution",
    "Insubordination",
    "Fireworks",
    "Lucky Envelopes",
    "Sluggishness",
];

#[derive(Clone, Debug, Error)]
pub enum DictionaryDataError {
    #[error("SC2 dictionary data directory was not found from '{0}'")]
    DictionaryDirNotFound(PathBuf),
    #[error("failed to read '{path}': {message}")]
    IoRead { path: PathBuf, message: String },
    #[error("failed to parse JSON '{path}': {message}")]
    JsonParse { path: PathBuf, message: String },
    #[error("invalid dictionary file '{file}': {message}")]
    InvalidDictionaryData { file: &'static str, message: String },
}

macro_rules! transparent_json_wrapper {
    ($name:ident, $inner:ty) => {
        #[derive(Clone, Debug, Default, Deserialize, Serialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl Deref for $name {
            type Target = $inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

transparent_json_wrapper!(AmonPlayerIdsJson, IndexMap<String, Vec<i64>>);
transparent_json_wrapper!(BonusObjectivesJson, IndexMap<String, u64>);
transparent_json_wrapper!(CachedMutatorsJson, IndexMap<String, String>);
transparent_json_wrapper!(CoMasteryUpgradesJson, IndexMap<String, Vec<String>>);
transparent_json_wrapper!(HftsUnitsJson, Vec<String>);
transparent_json_wrapper!(HornersUnitsJson, Vec<String>);
transparent_json_wrapper!(MapNamesJson, IndexMap<String, IndexMap<String, String>>);
transparent_json_wrapper!(McUnitsJson, IndexMap<String, String>);
transparent_json_wrapper!(MutatorIdsJson, IndexMap<String, String>);
transparent_json_wrapper!(MutatorsExcludeIdsJson, Vec<String>);
transparent_json_wrapper!(OutlawsJson, Vec<String>);
transparent_json_wrapper!(PrestigeUpgradesJson, IndexMap<String, IndexMap<String, String>>);
transparent_json_wrapper!(RoyalGuardsJson, Vec<String>);
transparent_json_wrapper!(TusUnitsJson, Vec<String>);
transparent_json_wrapper!(TychusBaseUpgradesJson, Vec<String>);
transparent_json_wrapper!(TychusUltimateUpgradesJson, Vec<String>);
transparent_json_wrapper!(UnitAddKillsToJson, IndexMap<String, String>);
transparent_json_wrapper!(UnitBaseCostsJson, IndexMap<String, IndexMap<String, Vec<f64>>>);
transparent_json_wrapper!(UnitCompDictJson, IndexMap<String, Vec<Vec<String>>>);
transparent_json_wrapper!(UnitNamesJson, IndexMap<String, String>);
transparent_json_wrapper!(UnitsInWavesJson, Vec<String>);
transparent_json_wrapper!(UnitsToStatsJson, Vec<String>);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LocalizedPrestigeNames {
    pub en: Vec<String>,
    pub ko: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LocalizedMutatorDescription {
    #[serde(default)]
    #[serde(rename = "nameEn")]
    pub name_en: String,
    #[serde(default)]
    #[serde(rename = "nameKo")]
    pub name_ko: String,
    #[serde(default)]
    #[serde(rename = "descriptionEn")]
    pub description_en: String,
    #[serde(default)]
    #[serde(rename = "descriptionKo")]
    pub description_ko: String,
}

transparent_json_wrapper!(MutatorsJson, IndexMap<String, LocalizedMutatorDescription>);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LocalizedMasteryNames {
    pub en: Vec<String>,
    pub ko: Vec<String>,
}

transparent_json_wrapper!(CommanderMasteryJson, IndexMap<String, LocalizedMasteryNames>);
transparent_json_wrapper!(PrestigeNamesJson, IndexMap<String, LocalizedPrestigeNames>);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WeeklyMutationJson {
    #[serde(default)]
    #[serde(rename = "nameEn")]
    pub name_en: String,
    #[serde(default)]
    #[serde(rename = "nameKo")]
    pub name_ko: String,
    pub map: String,
    pub mutators: Vec<String>,
}

transparent_json_wrapper!(WeeklyMutationsJson, IndexMap<String, WeeklyMutationJson>);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WeeklyMutationDateJson {
    pub name: String,
    pub date: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ReplayAnalysisDataJson {
    pub commander_no_units: HashMap<String, Vec<String>>,
    pub commander_upgrades: HashMap<String, String>,
    pub do_not_count_kills: Vec<String>,
    pub dont_count_morphs: Vec<String>,
    pub dont_include_units: Vec<String>,
    pub dont_show_created_lost: Vec<String>,
    pub duplicating_units: Vec<String>,
    pub icon_units: Vec<String>,
    pub primal_combat_predecessors: HashMap<String, String>,
    pub revival_types: HashMap<String, String>,
    pub salvage_units: Vec<String>,
    pub self_killing_units: Vec<String>,
    pub tychus_outlaws: Vec<String>,
    pub units_killed_in_morph: Vec<String>,
    pub aoe_units: Vec<String>,
    pub skip_strings: Vec<String>,
    #[serde(rename = "UnitAddLossesTo")]
    pub unit_add_losses_to: HashMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct WeeklyMutation {
    pub map: String,
    pub mutators: HashSet<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct CacheGenerationData<'a> {
    pub map_names: &'a MapNamesJson,
    pub prestige_names: &'a HashMap<String, HashMap<i64, String>>,
    pub mutators_all: &'a Vec<String>,
    pub mutators_ui: &'a Vec<String>,
    pub mutator_ids: &'a MutatorIdsJson,
    pub cached_mutators: &'a CachedMutatorsJson,
    pub amon_player_ids: &'a AmonPlayerIdsJson,
    pub replay_analysis_data: &'a ReplayAnalysisDataJson,
    pub co_mastery_upgrades: &'a CoMasteryUpgradesJson,
    pub prestige_upgrades: &'a PrestigeUpgradesJson,
    pub unit_name_dict: &'a UnitNamesJson,
    pub unit_add_kills_to: &'a UnitAddKillsToJson,
    pub unit_comp_dict: &'a HashMap<String, Vec<HashSet<String>>>,
    pub unit_base_costs: &'a UnitBaseCostsJson,
    pub units_in_waves: &'a HashSet<String>,
    pub hfts_units: &'a HashSet<String>,
    pub tus_units: &'a HashSet<String>,
    pub royal_guards: &'a HashSet<String>,
    pub horners_units: &'a HashSet<String>,
    pub tychus_base_upgrades: &'a HashSet<String>,
    pub tychus_ultimate_upgrades: &'a HashSet<String>,
    pub outlaws: &'a HashSet<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct TauriUiData<'a> {
    pub commander_mastery: &'a CommanderMasteryJson,
    pub prestige_names_json: &'a PrestigeNamesJson,
    pub prestige_names: &'a HashMap<String, HashMap<i64, String>>,
    pub prestige_names_with_u64_levels: &'a HashMap<String, HashMap<u64, String>>,
    pub map_names: &'a MapNamesJson,
    pub bonus_objectives: &'a BonusObjectivesJson,
    pub commander_mind_control_units: &'a McUnitsJson,
    pub units_to_stats: &'a HashSet<String>,
    pub amon_player_ids: &'a AmonPlayerIdsJson,
    pub weekly_mutations_json: &'a WeeklyMutationsJson,
    pub weekly_mutations_as_sets: &'a HashMap<String, WeeklyMutation>,
    pub map_key_to_id_lookup: &'a HashMap<String, String>,
    pub map_id_to_english_lookup: &'a HashMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct Sc2DictionaryData {
    pub commander_mastery: CommanderMasteryJson,
    pub prestige_names_json: PrestigeNamesJson,
    pub prestige_names: HashMap<String, HashMap<i64, String>>,
    pub prestige_names_with_u64_levels: HashMap<String, HashMap<u64, String>>,
    pub map_names: MapNamesJson,
    pub bonus_objectives: BonusObjectivesJson,
    pub commander_mind_control_units: McUnitsJson,
    pub mutators_json: MutatorsJson,
    pub mutators_exclude_ids_json: MutatorsExcludeIdsJson,
    pub mutators_all: Vec<String>,
    pub mutators_ui: Vec<String>,
    pub units_to_stats_json: UnitsToStatsJson,
    pub units_to_stats: HashSet<String>,
    pub amon_player_ids: AmonPlayerIdsJson,
    pub amon_player_ids_as_sets: HashMap<String, HashSet<String>>,
    pub prestige_upgrades: PrestigeUpgradesJson,
    pub unit_name_dict: UnitNamesJson,
    pub unit_add_kills_to: UnitAddKillsToJson,
    pub unit_comp_dict_json: UnitCompDictJson,
    pub unit_comp_dict: HashMap<String, Vec<HashSet<String>>>,
    pub unit_base_costs: UnitBaseCostsJson,
    pub unit_base_costs_as_tuples: HashMap<String, HashMap<String, Vec<u64>>>,
    pub mutator_ids: MutatorIdsJson,
    pub cached_mutators: CachedMutatorsJson,
    pub weekly_mutations_json: WeeklyMutationsJson,
    pub weekly_mutation_date_json: WeeklyMutationDateJson,
    pub weekly_mutations_as_sets: HashMap<String, WeeklyMutation>,
    pub replay_analysis_data: ReplayAnalysisDataJson,
    pub co_mastery_upgrades: CoMasteryUpgradesJson,
    pub units_in_waves_json: UnitsInWavesJson,
    pub units_in_waves: HashSet<String>,
    pub hfts_units_json: HftsUnitsJson,
    pub hfts_units: HashSet<String>,
    pub tus_units_json: TusUnitsJson,
    pub tus_units: HashSet<String>,
    pub royal_guards_json: RoyalGuardsJson,
    pub royal_guards: HashSet<String>,
    pub horners_units_json: HornersUnitsJson,
    pub horners_units: HashSet<String>,
    pub tychus_base_upgrades_json: TychusBaseUpgradesJson,
    pub tychus_base_upgrades: HashSet<String>,
    pub tychus_ultimate_upgrades_json: TychusUltimateUpgradesJson,
    pub tychus_ultimate_upgrades: HashSet<String>,
    pub outlaws_json: OutlawsJson,
    pub outlaws: HashSet<String>,
    map_key_to_id_lookup: HashMap<String, String>,
    map_id_to_english_lookup: HashMap<String, String>,
}

impl Sc2DictionaryData {
    fn cache_generation_data(&self) -> CacheGenerationData<'_> {
        CacheGenerationData {
            map_names: &self.map_names,
            prestige_names: &self.prestige_names,
            mutators_all: &self.mutators_all,
            mutators_ui: &self.mutators_ui,
            mutator_ids: &self.mutator_ids,
            cached_mutators: &self.cached_mutators,
            amon_player_ids: &self.amon_player_ids,
            replay_analysis_data: &self.replay_analysis_data,
            co_mastery_upgrades: &self.co_mastery_upgrades,
            prestige_upgrades: &self.prestige_upgrades,
            unit_name_dict: &self.unit_name_dict,
            unit_add_kills_to: &self.unit_add_kills_to,
            unit_comp_dict: &self.unit_comp_dict,
            unit_base_costs: &self.unit_base_costs,
            units_in_waves: &self.units_in_waves,
            hfts_units: &self.hfts_units,
            tus_units: &self.tus_units,
            royal_guards: &self.royal_guards,
            horners_units: &self.horners_units,
            tychus_base_upgrades: &self.tychus_base_upgrades,
            tychus_ultimate_upgrades: &self.tychus_ultimate_upgrades,
            outlaws: &self.outlaws,
        }
    }

    fn tauri_ui_data(&self) -> TauriUiData<'_> {
        TauriUiData {
            commander_mastery: &self.commander_mastery,
            prestige_names_json: &self.prestige_names_json,
            prestige_names: &self.prestige_names,
            prestige_names_with_u64_levels: &self.prestige_names_with_u64_levels,
            map_names: &self.map_names,
            bonus_objectives: &self.bonus_objectives,
            commander_mind_control_units: &self.commander_mind_control_units,
            units_to_stats: &self.units_to_stats,
            amon_player_ids: &self.amon_player_ids,
            weekly_mutations_json: &self.weekly_mutations_json,
            weekly_mutations_as_sets: &self.weekly_mutations_as_sets,
            map_key_to_id_lookup: &self.map_key_to_id_lookup,
            map_id_to_english_lookup: &self.map_id_to_english_lookup,
        }
    }
}

fn resolve_data_dir() -> Result<PathBuf, DictionaryDataError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_sc2_dictionary_data_dir(&REQUIRED_DATA_FILES)
        .map_err(|_| DictionaryDataError::DictionaryDirNotFound(cwd))
}

fn read_dictionary_text(
    base_dir: &Path,
    file_name: &'static str,
) -> Result<(PathBuf, String), DictionaryDataError> {
    let path = base_dir.join(file_name);
    let content = fs::read_to_string(&path).map_err(|error| DictionaryDataError::IoRead {
        path: path.clone(),
        message: error.to_string(),
    })?;
    Ok((path, content))
}

fn load_dictionary_json<T>(
    base_dir: &Path,
    file_name: &'static str,
) -> Result<T, DictionaryDataError>
where
    T: DeserializeOwned,
{
    let (path, content) = read_dictionary_text(base_dir, file_name)?;
    serde_json::from_str::<T>(&content).map_err(|error| DictionaryDataError::JsonParse {
        path,
        message: error.to_string(),
    })
}

fn parse_string_set(values: &[String]) -> HashSet<String> {
    values.iter().cloned().collect()
}

fn parse_unit_comp_dict(rows: &UnitCompDictJson) -> HashMap<String, Vec<HashSet<String>>> {
    rows.iter()
        .map(|(key, waves)| {
            (
                key.clone(),
                waves
                    .iter()
                    .map(|wave| wave.iter().cloned().collect::<HashSet<String>>())
                    .collect::<Vec<HashSet<String>>>(),
            )
        })
        .collect()
}

fn parse_prestige_names_i64(
    raw: &PrestigeNamesJson,
) -> Result<HashMap<String, HashMap<i64, String>>, DictionaryDataError> {
    let mut prestige_names = HashMap::new();
    for (commander, entries) in raw.iter() {
        let mut parsed_entries = HashMap::new();
        for (index, value) in entries.en.iter().enumerate() {
            let parsed_key =
                i64::try_from(index).map_err(|_| DictionaryDataError::InvalidDictionaryData {
                    file: "prestige_names.json",
                    message: format!("prestige index '{index}' exceeds i64 range"),
                })?;
            parsed_entries.insert(parsed_key, value.clone());
        }
        prestige_names.insert(commander.clone(), parsed_entries);
    }
    Ok(prestige_names)
}

fn parse_prestige_names_with_u64_levels(
    prestige_names: &HashMap<String, HashMap<i64, String>>,
) -> HashMap<String, HashMap<u64, String>> {
    prestige_names
        .iter()
        .map(|(commander, levels)| {
            let parsed_levels = levels
                .iter()
                .filter_map(|(level, name)| {
                    u64::try_from(*level)
                        .ok()
                        .map(|level| (level, name.clone()))
                })
                .collect::<HashMap<u64, String>>();
            (commander.clone(), parsed_levels)
        })
        .collect()
}

fn build_mutator_list_all(mutators_json: &MutatorsJson) -> Vec<String> {
    let mut mutators_all = mutators_json.keys().cloned().collect::<Vec<String>>();
    if mutators_all.len() > 19 {
        mutators_all.truncate(mutators_all.len() - 19);
    } else {
        mutators_all.clear();
    }
    mutators_all
}

fn build_mutators_ui(
    mutators_all: &[String],
    mutators_exclude_ids_json: &MutatorsExcludeIdsJson,
) -> Vec<String> {
    let mutators_exclude_set = parse_string_set(mutators_exclude_ids_json);
    mutators_all
        .iter()
        .filter(|name| !mutators_exclude_set.contains(name.as_str()))
        .cloned()
        .collect()
}

fn parse_amon_player_ids_as_sets(
    amon_player_ids: &AmonPlayerIdsJson,
) -> HashMap<String, HashSet<String>> {
    amon_player_ids
        .iter()
        .map(|(key, values)| {
            (
                key.clone(),
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<String>>(),
            )
        })
        .collect()
}

fn parse_unit_base_costs_as_tuples(
    unit_base_costs: &UnitBaseCostsJson,
) -> HashMap<String, HashMap<String, Vec<u64>>> {
    unit_base_costs
        .iter()
        .map(|(unit, costs)| {
            (
                unit.clone(),
                costs
                    .iter()
                    .map(|(key, values)| {
                        (
                            key.clone(),
                            values
                                .iter()
                                .map(|value| value.round_ties_even().max(0.0) as u64)
                                .collect::<Vec<u64>>(),
                        )
                    })
                    .collect::<HashMap<String, Vec<u64>>>(),
            )
        })
        .collect()
}

fn normalize_map_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn normalize_lookup_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn build_map_key_to_id_lookup(map_names: &MapNamesJson) -> HashMap<String, String> {
    let mut out = HashMap::new();

    for (raw_name, info) in map_names.iter() {
        let map_id = info
            .get("ID")
            .filter(|value| !value.trim().is_empty())
            .cloned();
        let canonical = info
            .get("EN")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .or_else(|| Some(raw_name.clone()))
            .unwrap_or_default();

        let Some(map_id) = map_id else {
            continue;
        };
        if canonical.is_empty() {
            continue;
        }

        out.insert(normalize_map_key(raw_name), map_id.clone());
        out.insert(normalize_map_key(&canonical), map_id.clone());
        out.insert(normalize_map_key(&map_id), map_id.clone());
    }

    out.insert(
        "acbelshirescort".to_string(),
        "AC_BelshirEscort".to_string(),
    );

    out
}

fn build_map_id_to_english_lookup(map_names: &MapNamesJson) -> HashMap<String, String> {
    let mut out = HashMap::new();

    for info in map_names.values() {
        let map_id = info
            .get("ID")
            .filter(|value| !value.trim().is_empty())
            .cloned();
        let english = info
            .get("EN")
            .filter(|value| !value.trim().is_empty())
            .cloned();
        let (Some(map_id), Some(english)) = (map_id, english) else {
            continue;
        };
        out.insert(normalize_map_key(&map_id), english);
    }

    out
}

fn parse_weekly_mutations(data: &WeeklyMutationsJson) -> HashMap<String, WeeklyMutation> {
    data.iter()
        .map(|(name, entry)| {
            (
                name.clone(),
                WeeklyMutation {
                    map: entry.map.clone(),
                    mutators: entry
                        .mutators
                        .iter()
                        .map(|mutator| normalize_lookup_key(mutator))
                        .filter(|mutator| !mutator.is_empty())
                        .collect(),
                },
            )
        })
        .collect()
}

fn load_shared_dictionary_data_impl() -> Result<Sc2DictionaryData, DictionaryDataError> {
    let data_dir = resolve_data_dir()?;

    let commander_mastery =
        load_dictionary_json::<CommanderMasteryJson>(&data_dir, "commander_mastery.json")?;
    let prestige_names_json =
        load_dictionary_json::<PrestigeNamesJson>(&data_dir, "prestige_names.json")?;
    let prestige_names = parse_prestige_names_i64(&prestige_names_json)?;
    let prestige_names_with_u64_levels = parse_prestige_names_with_u64_levels(&prestige_names);
    let map_names = load_dictionary_json::<MapNamesJson>(&data_dir, "map_names.json")?;
    let bonus_objectives =
        load_dictionary_json::<BonusObjectivesJson>(&data_dir, "bonus_objectives.json")?;
    let commander_mind_control_units =
        load_dictionary_json::<McUnitsJson>(&data_dir, "mc_units.json")?;
    let mutators_json = load_dictionary_json::<MutatorsJson>(&data_dir, "mutators.json")?;
    let mutators_exclude_ids_json =
        load_dictionary_json::<MutatorsExcludeIdsJson>(&data_dir, "mutators_exclude_ids.json")?;
    let mutators_all = build_mutator_list_all(&mutators_json);
    let mutators_ui = build_mutators_ui(&mutators_all, &mutators_exclude_ids_json);
    let units_to_stats_json =
        load_dictionary_json::<UnitsToStatsJson>(&data_dir, "units_to_stats.json")?;
    let units_to_stats = parse_string_set(&units_to_stats_json);
    let amon_player_ids =
        load_dictionary_json::<AmonPlayerIdsJson>(&data_dir, "amon_player_ids.json")?;
    let amon_player_ids_as_sets = parse_amon_player_ids_as_sets(&amon_player_ids);
    let prestige_upgrades =
        load_dictionary_json::<PrestigeUpgradesJson>(&data_dir, "prestige_upgrades.json")?;
    let unit_name_dict = load_dictionary_json::<UnitNamesJson>(&data_dir, "unit_names.json")?;
    let unit_add_kills_to =
        load_dictionary_json::<UnitAddKillsToJson>(&data_dir, "unit_add_kills_to.json")?;
    let unit_comp_dict_json =
        load_dictionary_json::<UnitCompDictJson>(&data_dir, "unit_comp_dict.json")?;
    let unit_comp_dict = parse_unit_comp_dict(&unit_comp_dict_json);
    let unit_base_costs =
        load_dictionary_json::<UnitBaseCostsJson>(&data_dir, "unit_base_costs.json")?;
    let unit_base_costs_as_tuples = parse_unit_base_costs_as_tuples(&unit_base_costs);
    let mutator_ids = load_dictionary_json::<MutatorIdsJson>(&data_dir, "mutator_ids.json")?;
    let cached_mutators =
        load_dictionary_json::<CachedMutatorsJson>(&data_dir, "cached_mutators.json")?;
    let weekly_mutations_json =
        load_dictionary_json::<WeeklyMutationsJson>(&data_dir, "weekly_mutations.json")?;
    let weekly_mutation_date_json =
        load_dictionary_json::<WeeklyMutationDateJson>(&data_dir, "weekly_mutation_date.json")?;
    if !weekly_mutations_json.contains_key(&weekly_mutation_date_json.name) {
        return Err(DictionaryDataError::InvalidDictionaryData {
            file: "weekly_mutation_date.json",
            message: format!(
                "initial weekly mutation '{}' does not exist in weekly_mutations.json",
                weekly_mutation_date_json.name
            ),
        });
    }
    if NaiveDate::parse_from_str(&weekly_mutation_date_json.date, "%Y-%m-%d").is_err() {
        return Err(DictionaryDataError::InvalidDictionaryData {
            file: "weekly_mutation_date.json",
            message: format!(
                "date '{}' must use YYYY-MM-DD format",
                weekly_mutation_date_json.date
            ),
        });
    }
    let weekly_mutations_as_sets = parse_weekly_mutations(&weekly_mutations_json);
    let replay_analysis_data =
        load_dictionary_json::<ReplayAnalysisDataJson>(&data_dir, "replay_analysis_data.json")?;
    let co_mastery_upgrades =
        load_dictionary_json::<CoMasteryUpgradesJson>(&data_dir, "co_mastery_upgrades.json")?;
    let units_in_waves_json =
        load_dictionary_json::<UnitsInWavesJson>(&data_dir, "units_in_waves.json")?;
    let units_in_waves = parse_string_set(&units_in_waves_json);
    let hfts_units_json = load_dictionary_json::<HftsUnitsJson>(&data_dir, "hfts_units.json")?;
    let hfts_units = parse_string_set(&hfts_units_json);
    let tus_units_json = load_dictionary_json::<TusUnitsJson>(&data_dir, "tus_units.json")?;
    let tus_units = parse_string_set(&tus_units_json);
    let royal_guards_json =
        load_dictionary_json::<RoyalGuardsJson>(&data_dir, "royal_guards.json")?;
    let royal_guards = parse_string_set(&royal_guards_json);
    let horners_units_json =
        load_dictionary_json::<HornersUnitsJson>(&data_dir, "horners_units.json")?;
    let horners_units = parse_string_set(&horners_units_json);
    let tychus_base_upgrades_json =
        load_dictionary_json::<TychusBaseUpgradesJson>(&data_dir, "tychus_base_upgrades.json")?;
    let tychus_base_upgrades = parse_string_set(&tychus_base_upgrades_json);
    let tychus_ultimate_upgrades_json = load_dictionary_json::<TychusUltimateUpgradesJson>(
        &data_dir,
        "tychus_ultimate_upgrades.json",
    )?;
    let tychus_ultimate_upgrades = parse_string_set(&tychus_ultimate_upgrades_json);
    let outlaws_json = load_dictionary_json::<OutlawsJson>(&data_dir, "outlaws.json")?;
    let outlaws = parse_string_set(&outlaws_json);
    let map_key_to_id_lookup = build_map_key_to_id_lookup(&map_names);
    let map_id_to_english_lookup = build_map_id_to_english_lookup(&map_names);

    Ok(Sc2DictionaryData {
        commander_mastery,
        prestige_names_json,
        prestige_names,
        prestige_names_with_u64_levels,
        map_names,
        bonus_objectives,
        commander_mind_control_units,
        mutators_json,
        mutators_exclude_ids_json,
        mutators_all,
        mutators_ui,
        units_to_stats_json,
        units_to_stats,
        amon_player_ids,
        amon_player_ids_as_sets,
        prestige_upgrades,
        unit_name_dict,
        unit_add_kills_to,
        unit_comp_dict_json,
        unit_comp_dict,
        unit_base_costs,
        unit_base_costs_as_tuples,
        mutator_ids,
        cached_mutators,
        weekly_mutations_json,
        weekly_mutation_date_json,
        weekly_mutations_as_sets,
        replay_analysis_data,
        co_mastery_upgrades,
        units_in_waves_json,
        units_in_waves,
        hfts_units_json,
        hfts_units,
        tus_units_json,
        tus_units,
        royal_guards_json,
        royal_guards,
        horners_units_json,
        horners_units,
        tychus_base_upgrades_json,
        tychus_base_upgrades,
        tychus_ultimate_upgrades_json,
        tychus_ultimate_upgrades,
        outlaws_json,
        outlaws,
        map_key_to_id_lookup,
        map_id_to_english_lookup,
    })
}

pub fn shared_dictionary_data() -> Result<&'static Sc2DictionaryData, DictionaryDataError> {
    static DATA: OnceLock<Result<Sc2DictionaryData, DictionaryDataError>> = OnceLock::new();
    DATA.get_or_init(load_shared_dictionary_data_impl)
        .as_ref()
        .map_err(Clone::clone)
}

pub fn cache_generation_data() -> Result<CacheGenerationData<'static>, DictionaryDataError> {
    Ok(shared_dictionary_data()?.cache_generation_data())
}

pub fn tauri_ui_data() -> Result<TauriUiData<'static>, DictionaryDataError> {
    Ok(shared_dictionary_data()?.tauri_ui_data())
}

fn shared_dictionary_data_or_default() -> &'static Sc2DictionaryData {
    static FALLBACK: OnceLock<Sc2DictionaryData> = OnceLock::new();
    shared_dictionary_data().unwrap_or_else(|_| FALLBACK.get_or_init(Sc2DictionaryData::default))
}

fn cache_generation_data_or_default() -> CacheGenerationData<'static> {
    shared_dictionary_data_or_default().cache_generation_data()
}

fn tauri_ui_data_or_default() -> TauriUiData<'static> {
    shared_dictionary_data_or_default().tauri_ui_data()
}

pub fn unit_names() -> &'static UnitNamesJson {
    cache_generation_data_or_default().unit_name_dict
}

pub fn unit_add_kills_to() -> &'static UnitAddKillsToJson {
    cache_generation_data_or_default().unit_add_kills_to
}

pub fn commander_mastery() -> &'static CommanderMasteryJson {
    tauri_ui_data_or_default().commander_mastery
}

pub fn prestige_names() -> &'static PrestigeNamesJson {
    tauri_ui_data_or_default().prestige_names_json
}

pub fn prestige_names_with_u64_levels() -> &'static HashMap<String, HashMap<u64, String>> {
    tauri_ui_data_or_default().prestige_names_with_u64_levels
}

pub fn prestige_name(commander: &str, prestige: u64) -> Option<&'static str> {
    let prestige = i64::try_from(prestige).ok()?;
    tauri_ui_data_or_default()
        .prestige_names
        .get(commander)
        .and_then(|levels| levels.get(&prestige))
        .map(String::as_str)
}

pub fn prestige_upgrades() -> &'static PrestigeUpgradesJson {
    cache_generation_data_or_default().prestige_upgrades
}

pub fn map_names() -> &'static MapNamesJson {
    tauri_ui_data_or_default().map_names
}

pub fn bonus_objectives() -> &'static BonusObjectivesJson {
    tauri_ui_data_or_default().bonus_objectives
}

pub fn units_to_stats() -> &'static HashSet<String> {
    tauri_ui_data_or_default().units_to_stats
}

pub fn canonicalize_coop_map_id(raw: &str) -> Option<String> {
    let key = normalize_map_key(raw);
    tauri_ui_data_or_default()
        .map_key_to_id_lookup
        .get(&key)
        .cloned()
}

pub fn coop_map_id_to_english(raw_map_id: &str) -> Option<String> {
    let key = normalize_map_key(raw_map_id);
    tauri_ui_data_or_default()
        .map_id_to_english_lookup
        .get(&key)
        .cloned()
}

pub fn coop_map_english_name(raw: &str) -> Option<String> {
    canonicalize_coop_map_id(raw).and_then(|map_id| coop_map_id_to_english(&map_id))
}

pub fn canonicalize_coop_map_name(raw: &str) -> Option<String> {
    coop_map_english_name(raw)
}

pub fn commander_mind_control_unit(commander: &str) -> Option<&'static str> {
    tauri_ui_data_or_default()
        .commander_mind_control_units
        .get(commander)
        .map(String::as_str)
}

pub fn comastery_upgrades() -> &'static CoMasteryUpgradesJson {
    cache_generation_data_or_default().co_mastery_upgrades
}

pub fn units_in_waves() -> &'static HashSet<String> {
    cache_generation_data_or_default().units_in_waves
}

pub fn hfts_units() -> &'static HashSet<String> {
    cache_generation_data_or_default().hfts_units
}

pub fn tus_units() -> &'static HashSet<String> {
    cache_generation_data_or_default().tus_units
}

pub fn mutators() -> &'static MutatorsJson {
    &shared_dictionary_data_or_default().mutators_json
}

pub fn mutator_list_all() -> &'static Vec<String> {
    &shared_dictionary_data_or_default().mutators_all
}

pub fn mutator_list() -> &'static Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut mutators = mutator_list_all().clone();
        mutators.retain(|mutator| !MUTATOR_CUSTOM_FORBIDDEN.contains(&mutator.as_str()));
        mutators
    })
}

pub fn unit_comp_dict() -> &'static UnitCompDictJson {
    &shared_dictionary_data_or_default().unit_comp_dict_json
}

pub fn unit_comp_dict_as_sets() -> &'static HashMap<String, Vec<HashSet<String>>> {
    &shared_dictionary_data_or_default().unit_comp_dict
}

pub fn amon_player_ids() -> &'static AmonPlayerIdsJson {
    tauri_ui_data_or_default().amon_player_ids
}

pub fn amon_player_ids_as_sets() -> &'static HashMap<String, HashSet<String>> {
    &shared_dictionary_data_or_default().amon_player_ids_as_sets
}

pub fn unit_base_costs() -> &'static UnitBaseCostsJson {
    &shared_dictionary_data_or_default().unit_base_costs
}

pub fn unit_base_costs_as_tuples() -> &'static HashMap<String, HashMap<String, Vec<u64>>> {
    &shared_dictionary_data_or_default().unit_base_costs_as_tuples
}

pub fn mutator_ids() -> &'static MutatorIdsJson {
    &shared_dictionary_data_or_default().mutator_ids
}

pub fn cached_mutators() -> &'static CachedMutatorsJson {
    &shared_dictionary_data_or_default().cached_mutators
}

pub fn weekly_mutations() -> &'static WeeklyMutationsJson {
    tauri_ui_data_or_default().weekly_mutations_json
}

pub fn weekly_mutation_date() -> &'static WeeklyMutationDateJson {
    &shared_dictionary_data_or_default().weekly_mutation_date_json
}

pub fn weekly_mutations_as_sets() -> &'static HashMap<String, WeeklyMutation> {
    tauri_ui_data_or_default().weekly_mutations_as_sets
}

pub fn royal_guards() -> &'static HashSet<String> {
    cache_generation_data_or_default().royal_guards
}

pub fn horners_units() -> &'static HashSet<String> {
    cache_generation_data_or_default().horners_units
}

pub fn tychus_base_upgrades() -> &'static HashSet<String> {
    cache_generation_data_or_default().tychus_base_upgrades
}

pub fn tychus_ultimate_upgrades() -> &'static HashSet<String> {
    cache_generation_data_or_default().tychus_ultimate_upgrades
}

pub fn outlaws() -> &'static HashSet<String> {
    cache_generation_data_or_default().outlaws
}
