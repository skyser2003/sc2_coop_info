use chrono::{Duration, NaiveDate};
use s2coop_analyzer::weekly_mutation_manager::WeeklyMutationManager;

mod common;

#[test]
fn weekly_mutation_manager_cycles_from_initial_date() {
    let dictionary = common::load_dictionary();
    let manager = WeeklyMutationManager::from_dictionary_data(&dictionary)
        .expect("weekly mutation manager should load from dictionary data");
    let initial = &dictionary.weekly_mutation_date_json;
    let weekly_names = dictionary
        .weekly_mutations_json
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    let start_date =
        NaiveDate::parse_from_str(&initial.date, "%Y-%m-%d").expect("initial date should parse");
    let next_week = start_date + Duration::days(7);
    let full_cycle = start_date + Duration::days(dictionary.weekly_mutations_json.len() as i64 * 7);
    let initial_index = weekly_names
        .iter()
        .position(|name| name == &initial.name)
        .expect("initial weekly mutation should exist in the dictionary");
    let following_name = weekly_names[(initial_index + 1) % weekly_names.len()].clone();

    let current = manager
        .current_for_date(start_date)
        .expect("initial weekly mutation should resolve");
    assert_eq!(current.name, initial.name);

    let following = manager
        .current_for_date(next_week)
        .expect("next weekly mutation should resolve");
    assert_eq!(following.name, following_name);

    let wrapped = manager
        .current_for_date(full_cycle)
        .expect("weekly mutation cycle should wrap");
    assert_eq!(wrapped.name, initial.name);
}
