use fastrand::Rng;
use s2coop_analyzer::dictionary_data;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use ts_rs::TS;

use crate::app_settings::RandomizerChoices;
use crate::shared_types::{
    LocalizedLabels, LocalizedText, OverlayRandomizerBrutalPlus, OverlayRandomizerCatalog,
    OverlayRandomizerMutator, OverlayRandomizerRange,
};

const RANDOMIZER_RACES: [&str; 3] = ["Terran", "Protoss", "Zerg"];

fn default_randomizer_mode() -> String {
    "commander".to_string()
}

fn default_mastery_mode() -> String {
    "all_in".to_string()
}

fn default_mutator_mode() -> String {
    "all_random".to_string()
}

fn default_true() -> bool {
    true
}

fn default_mutator_min() -> u64 {
    1
}

fn default_mutator_max() -> u64 {
    10
}

fn default_brutal_plus() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize)]
pub struct RandomizerRequest {
    #[serde(default = "default_randomizer_mode")]
    pub mode: String,
    #[serde(default)]
    pub rng_choices: RandomizerChoices,
    #[serde(default = "default_mastery_mode")]
    pub mastery_mode: String,
    #[serde(default = "default_true")]
    pub include_map: bool,
    #[serde(default = "default_true")]
    pub include_race: bool,
    #[serde(default = "default_mutator_mode")]
    pub mutator_mode: String,
    #[serde(default = "default_mutator_min")]
    pub mutator_min: u64,
    #[serde(default = "default_mutator_max")]
    pub mutator_max: u64,
    #[serde(default = "default_brutal_plus")]
    pub brutal_plus: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct RandomizerMutatorResult {
    pub id: String,
    pub name: LocalizedText,
    #[serde(rename = "iconName")]
    pub icon_name: String,
    pub description: LocalizedText,
    pub points: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
#[ts(tag = "kind", rename_all = "snake_case")]
pub enum RandomizerResult {
    Commander {
        commander: String,
        prestige: u32,
        mastery_indices: Vec<Option<u32>>,
        map_race: String,
    },
    Mutator {
        mutators: Vec<RandomizerMutatorResult>,
        mutator_total_points: u32,
        mutator_count: u32,
        #[ts(optional)]
        brutal_plus: Option<u8>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RandomizerMutatorEntry {
    id: String,
    name: LocalizedText,
    icon_name: String,
    description: LocalizedText,
    points: u32,
}

pub fn catalog_payload() -> OverlayRandomizerCatalog {
    let prestige_names = dictionary_data::tauri_ui_data()
        .map(|data| {
            data.prestige_names_json
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        LocalizedLabels {
                            en: value.en.clone(),
                            ko: value.ko.clone(),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let mutators = mutator_pool()
        .into_iter()
        .map(|entry| OverlayRandomizerMutator {
            id: entry.id,
            name: entry.name,
            icon_name: entry.icon_name,
            description: entry.description,
            points: entry.points,
        })
        .collect();

    let brutal_plus = dictionary_data::mutator_brutal_plus()
        .iter()
        .map(|entry| OverlayRandomizerBrutalPlus {
            brutal_plus: entry.brutal_plus,
            mutator_points: OverlayRandomizerRange {
                min: u32::try_from(entry.mutator_points.min).unwrap_or(u32::MAX),
                max: u32::try_from(entry.mutator_points.max).unwrap_or(u32::MAX),
            },
            mutator_count: OverlayRandomizerRange {
                min: u32::try_from(entry.mutator_count.min).unwrap_or(u32::MAX),
                max: u32::try_from(entry.mutator_count.max).unwrap_or(u32::MAX),
            },
        })
        .collect();

    OverlayRandomizerCatalog {
        prestige_names,
        mutators,
        brutal_plus,
    }
}

pub(crate) fn generate_from_body(body: Option<&Value>) -> Result<RandomizerResult, String> {
    let request = body
        .cloned()
        .map(serde_json::from_value::<RandomizerRequest>)
        .transpose()
        .map_err(|error| format!("Invalid randomizer payload: {error}"))?
        .unwrap_or(RandomizerRequest {
            mode: default_randomizer_mode(),
            rng_choices: RandomizerChoices::default(),
            mastery_mode: default_mastery_mode(),
            include_map: true,
            include_race: true,
            mutator_mode: default_mutator_mode(),
            mutator_min: default_mutator_min(),
            mutator_max: default_mutator_max(),
            brutal_plus: default_brutal_plus(),
        });

    let mut rng = Rng::new();
    generate_with_rng(&request, &mut rng)
}

pub fn generate_with_rng(
    request: &RandomizerRequest,
    rng: &mut Rng,
) -> Result<RandomizerResult, String> {
    match request.mode.as_str() {
        "commander" => generate_commander_with_rng(request, rng),
        "mutator" => generate_mutators_with_rng(request, rng),
        other => Err(format!("Unsupported randomizer mode: {other}")),
    }
}

fn generate_commander_with_rng(
    request: &RandomizerRequest,
    rng: &mut Rng,
) -> Result<RandomizerResult, String> {
    let commander_choices = effective_commander_choices(&request.rng_choices);
    let available_commanders = commander_choices
        .iter()
        .filter(|(_, prestiges)| !prestiges.is_empty())
        .collect::<Vec<_>>();
    if available_commanders.is_empty() {
        return Err("Select at least one commander/prestige".to_string());
    }

    let commander_index = rng.usize(0..available_commanders.len());
    let (commander, prestiges) = available_commanders[commander_index];
    let prestige = prestiges[rng.usize(0..prestiges.len())];
    let mastery_indices = generate_mastery_indices(&request.mastery_mode, rng)?;

    let map_name = random_map_name(rng);
    let race_name = RANDOMIZER_RACES[rng.usize(0..RANDOMIZER_RACES.len())].to_string();
    let mut map_race_parts = Vec::<String>::new();
    if request.include_map && !map_name.is_empty() {
        map_race_parts.push(map_name);
    }
    if request.include_race {
        map_race_parts.push(race_name);
    }

    Ok(RandomizerResult::Commander {
        commander: commander.clone(),
        prestige,
        mastery_indices: mastery_indices.into_iter().collect(),
        map_race: map_race_parts.join(" | "),
    })
}

fn generate_mutators_with_rng(
    request: &RandomizerRequest,
    rng: &mut Rng,
) -> Result<RandomizerResult, String> {
    let pool = mutator_pool();
    if pool.is_empty() {
        return Err("Mutator data is not available".to_string());
    }

    match request.mutator_mode.as_str() {
        "all_random" => {
            let count_min = usize::try_from(request.mutator_min)
                .map_err(|_| "Mutator minimum is too large".to_string())?;
            let count_max = usize::try_from(request.mutator_max)
                .map_err(|_| "Mutator maximum is too large".to_string())?;
            if count_min == 0 {
                return Err("Mutator minimum must be at least 1".to_string());
            }
            if count_min > count_max {
                return Err("Mutator minimum cannot exceed maximum".to_string());
            }
            let effective_max = count_max.min(pool.len());
            if count_min > effective_max {
                return Err(format!(
                    "Mutator maximum exceeds available mutators ({})",
                    pool.len()
                ));
            }

            let count = if count_min == effective_max {
                count_min
            } else {
                rng.usize(count_min..(effective_max + 1))
            };
            let chosen = choose_random_unique_mutators(&pool, count, rng);
            build_mutator_result(chosen, None)
        }
        "brutal_plus" => {
            let Some(bplus_entry) = dictionary_data::mutator_brutal_plus()
                .iter()
                .find(|entry| entry.brutal_plus == request.brutal_plus)
            else {
                return Err(format!(
                    "Unsupported Brutal+ mutator level: {}",
                    request.brutal_plus
                ));
            };

            let combinations = build_brutal_plus_combinations(
                &pool,
                bplus_entry.mutator_count.min,
                bplus_entry.mutator_count.max,
                bplus_entry.mutator_points.min,
                bplus_entry.mutator_points.max,
            )?;
            let selected = &combinations[rng.usize(0..combinations.len())];
            let chosen = selected
                .iter()
                .map(|index| pool[*index].clone())
                .collect::<Vec<_>>();
            build_mutator_result(chosen, Some(request.brutal_plus))
        }
        other => Err(format!("Unsupported mutator mode: {other}")),
    }
}

fn build_mutator_result(
    mutators: Vec<RandomizerMutatorEntry>,
    brutal_plus: Option<u8>,
) -> Result<RandomizerResult, String> {
    if mutators.is_empty() {
        return Err("No mutators were generated".to_string());
    }

    let total_points = mutators.iter().map(|mutator| mutator.points).sum::<u32>();
    let mutator_count = u32::try_from(mutators.len()).unwrap_or(u32::MAX);

    Ok(RandomizerResult::Mutator {
        mutators: mutators
            .into_iter()
            .map(|entry| RandomizerMutatorResult {
                id: entry.id,
                name: entry.name,
                icon_name: entry.icon_name,
                description: entry.description,
                points: entry.points,
            })
            .collect(),
        mutator_total_points: total_points,
        mutator_count,
        brutal_plus,
    })
}

fn effective_commander_choices(saved: &RandomizerChoices) -> BTreeMap<String, Vec<u32>> {
    let has_saved = !saved.is_empty();
    let mut out = BTreeMap::<String, Vec<u32>>::new();

    for commander in commander_names() {
        let mut prestiges = Vec::<u32>::new();
        for prestige in 0..=3 {
            let is_selected = if has_saved {
                saved
                    .get(&format!("{commander}_{prestige}"))
                    .copied()
                    .unwrap_or(false)
            } else {
                prestige == 0
            };
            if is_selected {
                prestiges.push(prestige);
            }
        }
        out.insert(commander, prestiges);
    }

    out
}

fn commander_names() -> Vec<String> {
    dictionary_data::tauri_ui_data()
        .map(|data| data.prestige_names_json.keys().cloned().collect())
        .unwrap_or_default()
}

fn random_map_name(rng: &mut Rng) -> String {
    let maps = dictionary_data::tauri_ui_data()
        .map(|data| data.amon_player_ids.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if maps.is_empty() {
        String::new()
    } else {
        maps[rng.usize(0..maps.len())].clone()
    }
}

fn generate_mastery_indices(mastery_mode: &str, rng: &mut Rng) -> Result<[Option<u32>; 3], String> {
    let mut mastery = [None; 3];
    match mastery_mode {
        "all_in" => {
            for pair_index in 0..3 {
                let chosen = rng.usize(0..2);
                mastery[pair_index] = Some(if chosen == 0 { 30 } else { 0 });
            }
            Ok(mastery)
        }
        "random" => {
            for pair_index in 0..3 {
                mastery[pair_index] = Some(rng.usize(0..31) as u32);
            }
            Ok(mastery)
        }
        "none" => Ok(mastery),
        _ => Err(format!("Unsupported mastery mode: {mastery_mode}")),
    }
}

fn mutator_points_lookup() -> HashMap<String, u32> {
    let mut out = HashMap::<String, u32>::new();
    for entry in dictionary_data::mutator_points().iter() {
        for id in &entry.ids {
            out.insert(id.clone(), entry.value);
        }
    }
    out
}

fn is_randomizer_excluded_mutator(mutator_id: &str) -> bool {
    mutator_id == "Random"
}

fn mutator_pool() -> Vec<RandomizerMutatorEntry> {
    let point_lookup = mutator_points_lookup();
    dictionary_data::mutator_list()
        .iter()
        .filter(|mutator_id| !is_randomizer_excluded_mutator(mutator_id))
        .map(|mutator_id| {
            let data = dictionary_data::mutator_data(mutator_id);
            let name = data
                .map(|value| LocalizedText {
                    en: decode_html_entities(&value.name.en),
                    ko: decode_html_entities(&value.name.ko),
                })
                .unwrap_or_default();
            let description = data
                .map(|value| LocalizedText {
                    en: decode_html_entities(&value.description.en),
                    ko: decode_html_entities(&value.description.ko),
                })
                .unwrap_or_default();
            let icon_name_source = if name.en.is_empty() {
                dictionary_data::mutator_ids()
                    .get(mutator_id)
                    .cloned()
                    .unwrap_or_else(|| mutator_id.clone())
            } else {
                name.en.clone()
            };
            let icon_name = mutator_icon_name(&icon_name_source);

            RandomizerMutatorEntry {
                id: mutator_id.clone(),
                name,
                icon_name: icon_name.to_string(),
                description,
                points: point_lookup.get(mutator_id).copied().unwrap_or(0),
            }
        })
        .collect()
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn mutator_icon_name(name_en: &str) -> &str {
    match name_en {
        "Moment Of Silence" => "Moment of Silence",
        _ => name_en,
    }
}

fn choose_random_unique_mutators(
    pool: &[RandomizerMutatorEntry],
    count: usize,
    rng: &mut Rng,
) -> Vec<RandomizerMutatorEntry> {
    let mut indices = (0..pool.len()).collect::<Vec<_>>();
    rng.shuffle(&mut indices);
    let mut chosen = indices.into_iter().take(count).collect::<Vec<_>>();
    chosen.sort_unstable();
    chosen
        .into_iter()
        .map(|index| pool[index].clone())
        .collect()
}

fn build_brutal_plus_combinations(
    pool: &[RandomizerMutatorEntry],
    count_min: u32,
    count_max: u32,
    points_min: u32,
    points_max: u32,
) -> Result<Vec<Vec<usize>>, String> {
    let count_min = usize::try_from(count_min)
        .map_err(|_| "B+ mutator count minimum is too large".to_string())?;
    let count_max = usize::try_from(count_max)
        .map_err(|_| "B+ mutator count maximum is too large".to_string())?;
    if count_min == 0 || count_min > count_max {
        return Err("Invalid Brutal+ mutator count range".to_string());
    }

    let max_count = count_max.min(pool.len());
    if count_min > max_count {
        return Err("Brutal+ mutator count range exceeds available mutators".to_string());
    }

    let mut combinations = Vec::<Vec<usize>>::new();
    for count in count_min..=max_count {
        let mut current = Vec::<usize>::new();
        collect_point_matched_combinations(
            pool,
            count,
            points_min,
            points_max,
            0,
            &mut current,
            0,
            &mut combinations,
        );
    }

    if combinations.is_empty() {
        return Err("No mutator combinations match the selected Brutal+ level".to_string());
    }

    Ok(combinations)
}

fn collect_point_matched_combinations(
    pool: &[RandomizerMutatorEntry],
    target_count: usize,
    points_min: u32,
    points_max: u32,
    start_index: usize,
    current: &mut Vec<usize>,
    current_points: u32,
    combinations: &mut Vec<Vec<usize>>,
) {
    if current.len() == target_count {
        if (points_min..=points_max).contains(&current_points) {
            combinations.push(current.clone());
        }
        return;
    }

    for index in start_index..pool.len() {
        let next_points = current_points.saturating_add(pool[index].points);
        if next_points > points_max {
            continue;
        }

        current.push(index);
        collect_point_matched_combinations(
            pool,
            target_count,
            points_min,
            points_max,
            index + 1,
            current,
            next_points,
            combinations,
        );
        current.pop();
    }
}
