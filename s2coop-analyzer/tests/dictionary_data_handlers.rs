use s2coop_analyzer::dictionary_data::Sc2DictionaryData;

#[test]
fn shared_dictionary_handlers_load_expected_dictionary_entries() {
    let dictionary =
        Sc2DictionaryData::load(None).expect("dictionary data should load from analyzer data");
    assert_eq!(
        dictionary
            .map_names
            .get("Void Launch")
            .and_then(|value| value.get("ID"))
            .map(String::as_str),
        Some("AC_KaldirShuttle")
    );

    assert_eq!(
        dictionary
            .canonicalize_coop_map_id("Void Launch")
            .as_deref(),
        Some("AC_KaldirShuttle")
    );
    assert_eq!(
        dictionary
            .coop_map_id_to_english("AC_KaldirShuttle")
            .as_deref(),
        Some("Void Launch")
    );

    let prestige_name = dictionary
        .prestige_name("Raynor", 0)
        .map(ToString::to_string);
    assert_eq!(
        prestige_name.as_deref(),
        dictionary
            .prestige_names_json
            .get("Raynor")
            .and_then(|value| value.en.first())
            .map(String::as_str)
    );
    assert!(prestige_name.is_some(), "Raynor prestige 0 should exist");

    let weekly_mutations = &dictionary.weekly_mutations_json;
    let weekly_mutations_sets = &dictionary.weekly_mutations_as_sets;

    assert_eq!(weekly_mutations_sets.len(), weekly_mutations.len());

    let cold_is_the_void = weekly_mutations_sets
        .get("Cold is the Void")
        .expect("Cold is the Void weekly mutation should exist");
    assert!(!cold_is_the_void.map.is_empty());
    assert!(!cold_is_the_void.mutators.is_empty());
    assert!(cold_is_the_void
        .mutators
        .iter()
        .all(|mutator| mutator.chars().all(|ch| ch.is_alphanumeric())));
}
