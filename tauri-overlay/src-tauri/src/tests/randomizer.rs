use crate::randomizer::{catalog_payload, generate_with_rng, RandomizerRequest};
use fastrand::Rng;
use serde_json::json;

#[test]
fn randomizer_catalog_exposes_prestige_and_mastery_metadata() {
    let payload = catalog_payload();

    assert!(!payload.prestige_names.is_empty());
    assert!(!payload.commander_mastery.is_empty());
    assert_eq!(payload.prestige_names["Abathur"].en[0], "Evolution Master");
    assert_eq!(
        payload.commander_mastery["Fenix"].en[0],
        "Fenix Suit Attack Speed"
    );
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
    assert_eq!(result.mastery.len(), 6);
    assert_eq!(result.mastery.iter().map(|row| row.points).sum::<u64>(), 90);
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
    assert_eq!(result.prestige_name, "Network Administrator");
    assert!(result.mastery.iter().all(|row| row.points == 0));
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
    for pair_index in 0..3 {
        let left = result.mastery[pair_index * 2].points;
        let right = result.mastery[pair_index * 2 + 1].points;
        assert_eq!(left + right, 30);
        assert!(matches!(left, 0 | 30));
        assert!(matches!(right, 0 | 30));
    }
}
