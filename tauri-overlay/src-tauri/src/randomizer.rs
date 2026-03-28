use fastrand::Rng;
use s2coop_analyzer::dictionary_data;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

use crate::shared_types::{LocalizedLabels, OverlayRandomizerCatalog};

const RANDOMIZER_RACES: [&str; 3] = ["Terran", "Protoss", "Zerg"];

fn default_mastery_mode() -> String {
    "all_in".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct RandomizerRequest {
    #[serde(default)]
    pub rng_choices: Value,
    #[serde(default = "default_mastery_mode")]
    pub mastery_mode: String,
    #[serde(default = "default_true")]
    pub include_map: bool,
    #[serde(default = "default_true")]
    pub include_race: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct RandomizerMasteryRow {
    pub points: u64,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../src/bindings/overlay.ts")]
pub struct RandomizerResult {
    pub commander: String,
    pub prestige: u64,
    pub prestige_name: String,
    pub mastery: Vec<RandomizerMasteryRow>,
    pub map_race: String,
}

pub fn catalog_payload() -> OverlayRandomizerCatalog {
    match dictionary_data::tauri_ui_data() {
        Ok(data) => OverlayRandomizerCatalog {
            commander_mastery: data
                .commander_mastery
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
                .collect(),
            prestige_names: data
                .prestige_names_json
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
                .collect(),
        },
        Err(_) => OverlayRandomizerCatalog::default(),
    }
}

pub(crate) fn generate_from_body(body: Option<&Value>) -> Result<RandomizerResult, String> {
    let request = body
        .cloned()
        .map(serde_json::from_value::<RandomizerRequest>)
        .transpose()
        .map_err(|error| format!("Invalid randomizer payload: {error}"))?
        .unwrap_or(RandomizerRequest {
            rng_choices: Value::Object(Default::default()),
            mastery_mode: default_mastery_mode(),
            include_map: true,
            include_race: true,
        });

    let mut rng = Rng::new();
    generate_with_rng(&request, &mut rng)
}

pub fn generate_with_rng(
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
    let mastery_points = generate_masteries(&request.mastery_mode, rng)?;
    let mastery = mastery_rows(commander, &mastery_points);

    let map_name = random_map_name(rng);
    let race_name = RANDOMIZER_RACES[rng.usize(0..RANDOMIZER_RACES.len())].to_string();
    let mut map_race_parts = Vec::<String>::new();
    if request.include_map && !map_name.is_empty() {
        map_race_parts.push(map_name);
    }
    if request.include_race {
        map_race_parts.push(race_name);
    }

    Ok(RandomizerResult {
        commander: commander.clone(),
        prestige,
        prestige_name: dictionary_data::prestige_name(commander, prestige)
            .map(str::to_string)
            .unwrap_or_else(|| format!("P{prestige}")),
        mastery,
        map_race: map_race_parts.join(" | "),
    })
}

fn effective_commander_choices(saved: &Value) -> BTreeMap<String, Vec<u64>> {
    let saved_object = saved.as_object();
    let has_saved = saved_object.is_some_and(|value| !value.is_empty());
    let mut out = BTreeMap::<String, Vec<u64>>::new();

    for commander in commander_names() {
        let mut prestiges = Vec::<u64>::new();
        for prestige in 0..=3 {
            let is_selected = if has_saved {
                saved_object
                    .and_then(|value| value.get(&format!("{commander}_{prestige}")))
                    .and_then(Value::as_bool)
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

fn generate_masteries(mastery_mode: &str, rng: &mut Rng) -> Result<[u64; 6], String> {
    let mut mastery = [0u64; 6];
    match mastery_mode {
        "all_in" => {
            for pair_index in 0..3 {
                let chosen = rng.usize(0..2);
                mastery[pair_index * 2 + chosen] = 30;
            }
            Ok(mastery)
        }
        "random" => {
            for pair_index in 0..3 {
                let chosen = rng.usize(0..31) as u64;
                mastery[pair_index * 2] = chosen;
                mastery[pair_index * 2 + 1] = 30 - chosen;
            }
            Ok(mastery)
        }
        "none" => Ok(mastery),
        _ => Err(format!("Unsupported mastery mode: {mastery_mode}")),
    }
}

fn mastery_rows(commander: &str, points: &[u64; 6]) -> Vec<RandomizerMasteryRow> {
    let labels = commander_mastery_labels(commander);
    (0..6)
        .map(|index| RandomizerMasteryRow {
            points: points[index],
            label: labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("Mastery {}", index + 1)),
        })
        .collect()
}

fn commander_mastery_labels(commander: &str) -> Vec<String> {
    dictionary_data::tauri_ui_data()
        .ok()
        .and_then(|data| data.commander_mastery.get(commander))
        .map(|labels| labels.en.clone())
        .unwrap_or_default()
}
