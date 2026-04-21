mod common;

use fastrand::Rng;
use sco_tauri_overlay::randomizer::{
    catalog_payload_with_dictionary, generate_with_dictionary_with_rng, RandomizerRequest,
};
use std::collections::BTreeMap;

#[test]
fn randomizer_catalog_exposes_prestige_metadata() {
    let dictionary = common::load_dictionary();
    let payload = catalog_payload_with_dictionary(&dictionary);

    assert!(!payload.prestige_names.is_empty());
    assert!(!payload.mutators.is_empty());
    assert_eq!(payload.brutal_plus.len(), 6);
    assert_eq!(payload.prestige_names["Abathur"].en[0], "Evolution Master");
    assert!(!payload
        .mutators
        .iter()
        .any(|mutator| mutator.id == "Random"));
}

#[test]
fn randomizer_defaults_to_p0_when_saved_choices_are_empty() {
    let dictionary = common::load_dictionary();
    let request = RandomizerRequest {
        mode: "commander".to_string(),
        rng_choices: BTreeMap::new(),
        mastery_mode: "all_in".to_string(),
        include_map: false,
        include_race: false,
        mutator_mode: "all_random".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(7);

    let result = generate_with_dictionary_with_rng(&request, &mut rng, &dictionary)
        .expect("randomizer should use default P0 selections");

    match result {
        sco_tauri_overlay::randomizer::RandomizerResult::Commander {
            prestige,
            map_race,
            mastery_indices,
            ..
        } => {
            assert_eq!(prestige, 0);
            assert_eq!(map_race, "");
            assert_eq!(mastery_indices.len(), 3);
            assert!(mastery_indices
                .iter()
                .all(|value| matches!(value, Some(0 | 30))));
        }
        other => panic!("expected commander result, got {other:?}"),
    }
}

#[test]
fn randomizer_respects_selected_choices_and_none_mode() {
    let dictionary = common::load_dictionary();
    let request = RandomizerRequest {
        mode: "commander".to_string(),
        rng_choices: BTreeMap::from([(String::from("Fenix_2"), true)]),
        mastery_mode: "none".to_string(),
        include_map: true,
        include_race: false,
        mutator_mode: "all_random".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(11);

    let result = generate_with_dictionary_with_rng(&request, &mut rng, &dictionary)
        .expect("randomizer should accept a single explicit selection");

    match result {
        sco_tauri_overlay::randomizer::RandomizerResult::Commander {
            commander,
            prestige,
            mastery_indices,
            map_race,
        } => {
            assert_eq!(commander, "Fenix");
            assert_eq!(prestige, 2);
            assert_eq!(mastery_indices, vec![None, None, None]);
            assert!(!map_race.is_empty());
            assert!(!map_race.contains('|'));
        }
        other => panic!("expected commander result, got {other:?}"),
    }
}

#[test]
fn randomizer_all_in_mode_assigns_one_side_of_each_mastery_pair() {
    let dictionary = common::load_dictionary();
    let request = RandomizerRequest {
        mode: "commander".to_string(),
        rng_choices: BTreeMap::from([(String::from("Abathur_1"), true)]),
        mastery_mode: "all_in".to_string(),
        include_map: false,
        include_race: true,
        mutator_mode: "all_random".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(19);

    let result = generate_with_dictionary_with_rng(&request, &mut rng, &dictionary)
        .expect("randomizer should produce mastery points");

    match result {
        sco_tauri_overlay::randomizer::RandomizerResult::Commander {
            commander,
            prestige,
            map_race,
            mastery_indices,
        } => {
            assert_eq!(commander, "Abathur");
            assert_eq!(prestige, 1);
            assert!(matches!(map_race.as_str(), "Terran" | "Protoss" | "Zerg"));
            assert!(mastery_indices
                .iter()
                .all(|value| matches!(value, Some(0 | 30))));
        }
        other => panic!("expected commander result, got {other:?}"),
    }
}

#[test]
fn randomizer_generates_random_mutators_without_point_budget() {
    let dictionary = common::load_dictionary();
    let request = RandomizerRequest {
        mode: "mutator".to_string(),
        rng_choices: BTreeMap::new(),
        mastery_mode: "all_in".to_string(),
        include_map: true,
        include_race: true,
        mutator_mode: "all_random".to_string(),
        mutator_min: 3,
        mutator_max: 3,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(23);

    let result = generate_with_dictionary_with_rng(&request, &mut rng, &dictionary)
        .expect("randomizer should produce mutators");

    match result {
        sco_tauri_overlay::randomizer::RandomizerResult::Mutator {
            mutators,
            mutator_count,
            mutator_total_points,
            brutal_plus,
        } => {
            assert_eq!(mutators.len(), 3);
            assert_eq!(mutator_count, 3);
            assert!(mutator_total_points > 0);
            assert_eq!(brutal_plus, None);
            assert!(!mutators.iter().any(|mutator| mutator.id == "Random"));
        }
        other => panic!("expected mutator result, got {other:?}"),
    }
}

#[test]
fn randomizer_generates_brutal_plus_matched_mutators() {
    let dictionary = common::load_dictionary();
    let request = RandomizerRequest {
        mode: "mutator".to_string(),
        rng_choices: BTreeMap::new(),
        mastery_mode: "all_in".to_string(),
        include_map: true,
        include_race: true,
        mutator_mode: "brutal_plus".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 3,
    };
    let mut rng = Rng::with_seed(31);

    let result = generate_with_dictionary_with_rng(&request, &mut rng, &dictionary)
        .expect("randomizer should produce mutators");

    match result {
        sco_tauri_overlay::randomizer::RandomizerResult::Mutator {
            mutators,
            mutator_count,
            mutator_total_points,
            brutal_plus,
        } => {
            assert_eq!(brutal_plus, Some(3));
            assert!(matches!(mutator_count, 2 | 3));
            assert!(matches!(mutator_total_points, 9 | 10));
            assert!(!mutators.iter().any(|mutator| mutator.id == "Random"));
        }
        other => panic!("expected mutator result, got {other:?}"),
    }
}
