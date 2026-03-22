use super::*;
use crate::dictionary_data;
use chrono::NaiveDate;
use serde_json::json;

fn weekly_replay(weekly_name: &str, result: &str) -> ReplayInfo {
    ReplayInfo {
        file: format!("fixtures/replays/{weekly_name}.SC2Replay"),
        result: result.to_string(),
        difficulty: "Brutal".to_string(),
        weekly: true,
        weekly_name: Some(weekly_name.to_string()),
        ..ReplayInfo::default()
    }
}

#[test]
fn rebuild_weeklies_rows_uses_dictionary_order_for_mutation_sort() {
    let replays = vec![
        weekly_replay("Time Lock", "Victory"),
        weekly_replay("Train of the Dead", "Defeat"),
        weekly_replay("First Strike", "Victory"),
    ];
    let seeded_current_name = dictionary_data::weekly_mutation_date().name.clone();
    let seeded_current_date =
        NaiveDate::parse_from_str(&dictionary_data::weekly_mutation_date().date, "%Y-%m-%d")
            .expect("seeded weekly mutation date should parse");

    let rows = ReplayAnalysis::rebuild_weeklies_rows_for_date(&replays, seeded_current_date);
    let train_of_the_dead = rows
        .iter()
        .find(|row| row.get("mutation") == Some(&json!("Train of the Dead")))
        .expect("Train of the Dead row should exist");
    let first_strike = rows
        .iter()
        .find(|row| row.get("mutation") == Some(&json!("First Strike")))
        .expect("First Strike row should exist");
    let time_lock = rows
        .iter()
        .find(|row| row.get("mutation") == Some(&json!("Time Lock")))
        .expect("Time Lock row should exist");

    assert!(rows.len() >= 3);
    assert_eq!(rows[0].get("mutation"), Some(&json!(seeded_current_name)));
    assert_eq!(rows[0].get("isCurrent"), Some(&json!(true)));
    assert_eq!(rows[0].get("nextDuration"), Some(&json!("Now")));

    assert_eq!(train_of_the_dead.get("mutationOrder"), Some(&json!(0)));
    assert_eq!(
        train_of_the_dead.get("map"),
        Some(&json!("Oblivion Express"))
    );
    assert_eq!(
        train_of_the_dead.get("nameEn"),
        Some(&json!("Train of the Dead"))
    );
    assert_eq!(train_of_the_dead.get("nameKo"), Some(&json!("망자의 열차")));
    assert_eq!(
        train_of_the_dead
            .get("mutators")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(3)
    );
    assert_eq!(
        train_of_the_dead
            .get("mutators")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|value| value.get("nameKo")),
        Some(&json!("암흑"))
    );
    assert_eq!(
        train_of_the_dead
            .get("mutators")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|value| value.get("descriptionEn")),
        Some(&json!(
            "Previously explored areas remain blacked out on the minimap while outside of player vision."
        ))
    );
    assert_eq!(first_strike.get("mutationOrder"), Some(&json!(1)));
    assert_eq!(time_lock.get("mutationOrder"), Some(&json!(2)));
}

#[test]
fn rebuild_weeklies_rows_without_record_uses_na_for_best_difficulty() {
    let seeded_current_date =
        NaiveDate::parse_from_str(&dictionary_data::weekly_mutation_date().date, "%Y-%m-%d")
            .expect("seeded weekly mutation date should parse");

    let rows = ReplayAnalysis::rebuild_weeklies_rows_for_date(&[], seeded_current_date);
    let row = rows
        .iter()
        .find(|entry| entry.get("mutation") == Some(&json!("Train of the Dead")))
        .expect("Train of the Dead row should exist");

    assert_eq!(row.get("difficulty"), Some(&json!("N/A")));
}
