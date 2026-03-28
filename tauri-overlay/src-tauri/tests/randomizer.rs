use fastrand::Rng;
use sco_tauri_overlay::randomizer::{catalog_payload, generate_with_rng, RandomizerRequest};
use serde_json::json;

#[test]
fn randomizer_catalog_exposes_prestige_metadata() {
    let payload = catalog_payload();

    assert!(!payload.prestige_names.is_empty());
    assert_eq!(payload.prestige_names["Abathur"].en[0], "Evolution Master");
}

#[test]
fn randomizer_defaults_to_p0_when_saved_choices_are_empty() {
    let request = RandomizerRequest {
        rng_choices: json!({}),
        mastery_mode: "all_in".to_string(),
        include_map: false,
        include_race: false,
    };
    let mut rng = Rng::with_seed(7);

    let result =
        generate_with_rng(&request, &mut rng).expect("randomizer should use default P0 selections");

    assert_eq!(result.prestige, 0);
    assert_eq!(result.map_race, "");
    assert_eq!(result.mastery_indices.len(), 3);
    assert!(result
        .mastery_indices
        .iter()
        .all(|value| matches!(value, Some(0 | 30))));
}

#[test]
fn randomizer_respects_selected_choices_and_none_mode() {
    let request = RandomizerRequest {
        rng_choices: json!({
            "Fenix_2": true,
        }),
        mastery_mode: "none".to_string(),
        include_map: true,
        include_race: false,
    };
    let mut rng = Rng::with_seed(11);

    let result = generate_with_rng(&request, &mut rng)
        .expect("randomizer should accept a single explicit selection");

    assert_eq!(result.commander, "Fenix");
    assert_eq!(result.prestige, 2);
    assert_eq!(result.mastery_indices, vec![None, None, None]);
    assert!(!result.map_race.is_empty());
    assert!(!result.map_race.contains('|'));
}

#[test]
fn randomizer_all_in_mode_assigns_one_side_of_each_mastery_pair() {
    let request = RandomizerRequest {
        rng_choices: json!({
            "Abathur_1": true,
        }),
        mastery_mode: "all_in".to_string(),
        include_map: false,
        include_race: true,
    };
    let mut rng = Rng::with_seed(19);

    let result =
        generate_with_rng(&request, &mut rng).expect("randomizer should produce mastery points");

    assert_eq!(result.commander, "Abathur");
    assert_eq!(result.prestige, 1);
    assert!(matches!(
        result.map_race.as_str(),
        "Terran" | "Protoss" | "Zerg"
    ));
    assert!(result
        .mastery_indices
        .iter()
        .all(|value| matches!(value, Some(0 | 30))));
}
