use fastrand::Rng;
use sco_tauri_overlay::randomizer::{catalog_payload, generate_with_rng, RandomizerRequest};
use serde_json::json;

#[test]
fn randomizer_catalog_exposes_prestige_metadata() {
    let payload = catalog_payload();

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
    let request = RandomizerRequest {
        mode: "commander".to_string(),
        rng_choices: json!({}),
        mastery_mode: "all_in".to_string(),
        include_map: false,
        include_race: false,
        mutator_mode: "all_random".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(7);

    let result =
        generate_with_rng(&request, &mut rng).expect("randomizer should use default P0 selections");

    assert_eq!(result.kind, "commander");
    assert_eq!(result.prestige, Some(0));
    assert_eq!(result.map_race.as_deref(), Some(""));
    assert_eq!(result.mastery_indices.as_ref().map(Vec::len), Some(3));
    assert!(result
        .mastery_indices
        .as_ref()
        .is_some_and(|values| values.iter().all(|value| matches!(value, Some(0 | 30)))));
}

#[test]
fn randomizer_respects_selected_choices_and_none_mode() {
    let request = RandomizerRequest {
        mode: "commander".to_string(),
        rng_choices: json!({
            "Fenix_2": true,
        }),
        mastery_mode: "none".to_string(),
        include_map: true,
        include_race: false,
        mutator_mode: "all_random".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(11);

    let result = generate_with_rng(&request, &mut rng)
        .expect("randomizer should accept a single explicit selection");

    assert_eq!(result.kind, "commander");
    assert_eq!(result.commander.as_deref(), Some("Fenix"));
    assert_eq!(result.prestige, Some(2));
    assert_eq!(result.mastery_indices, Some(vec![None, None, None]));
    assert!(result
        .map_race
        .as_deref()
        .is_some_and(|value| !value.is_empty()));
    assert!(result
        .map_race
        .as_deref()
        .is_some_and(|value| !value.contains('|')));
}

#[test]
fn randomizer_all_in_mode_assigns_one_side_of_each_mastery_pair() {
    let request = RandomizerRequest {
        mode: "commander".to_string(),
        rng_choices: json!({
            "Abathur_1": true,
        }),
        mastery_mode: "all_in".to_string(),
        include_map: false,
        include_race: true,
        mutator_mode: "all_random".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(19);

    let result =
        generate_with_rng(&request, &mut rng).expect("randomizer should produce mastery points");

    assert_eq!(result.kind, "commander");
    assert_eq!(result.commander.as_deref(), Some("Abathur"));
    assert_eq!(result.prestige, Some(1));
    assert!(matches!(
        result.map_race.as_deref(),
        Some("Terran" | "Protoss" | "Zerg")
    ));
    assert!(result
        .mastery_indices
        .as_ref()
        .is_some_and(|values| values.iter().all(|value| matches!(value, Some(0 | 30)))));
}

#[test]
fn randomizer_generates_random_mutators_without_point_budget() {
    let request = RandomizerRequest {
        mode: "mutator".to_string(),
        rng_choices: json!({}),
        mastery_mode: "all_in".to_string(),
        include_map: true,
        include_race: true,
        mutator_mode: "all_random".to_string(),
        mutator_min: 3,
        mutator_max: 3,
        brutal_plus: 1,
    };
    let mut rng = Rng::with_seed(23);

    let result = generate_with_rng(&request, &mut rng).expect("randomizer should produce mutators");

    assert_eq!(result.kind, "mutator");
    assert_eq!(result.mutators.len(), 3);
    assert_eq!(result.mutator_count, Some(3));
    assert!(result.mutator_total_points.unwrap_or(0) > 0);
    assert_eq!(result.brutal_plus, None);
    assert!(!result.mutators.iter().any(|mutator| mutator.id == "Random"));
}

#[test]
fn randomizer_generates_brutal_plus_matched_mutators() {
    let request = RandomizerRequest {
        mode: "mutator".to_string(),
        rng_choices: json!({}),
        mastery_mode: "all_in".to_string(),
        include_map: true,
        include_race: true,
        mutator_mode: "brutal_plus".to_string(),
        mutator_min: 1,
        mutator_max: 10,
        brutal_plus: 3,
    };
    let mut rng = Rng::with_seed(31);

    let result = generate_with_rng(&request, &mut rng).expect("randomizer should produce mutators");

    assert_eq!(result.kind, "mutator");
    assert_eq!(result.brutal_plus, Some(3));
    assert!(matches!(result.mutator_count, Some(2 | 3)));
    assert!(matches!(result.mutator_total_points, Some(9 | 10)));
    assert!(!result.mutators.iter().any(|mutator| mutator.id == "Random"));
}
