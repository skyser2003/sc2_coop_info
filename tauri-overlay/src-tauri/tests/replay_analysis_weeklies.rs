use chrono::NaiveDate;
use sco_tauri_overlay::replay_analysis::ReplayAnalysis;
use sco_tauri_overlay::test_helper::TestHelperOps;
use sco_tauri_overlay::ReplayInfo;

fn weekly_replay(weekly_name: &str, result: &str) -> ReplayInfo {
    let mut replay = ReplayInfo::default();
    replay.set_file(format!("fixtures/replays/{weekly_name}.SC2Replay"));
    replay.set_result(result);
    replay.set_difficulty("Brutal");
    replay.set_weekly(true);
    replay.set_weekly_name(Some(weekly_name.to_string()));
    replay
}

#[test]
fn rebuild_weeklies_rows_uses_dictionary_order_for_mutation_sort() {
    let dictionary = TestHelperOps::load_dictionary();
    let replays = vec![
        weekly_replay("Time Lock", "Victory"),
        weekly_replay("Train of the Dead", "Defeat"),
        weekly_replay("First Strike", "Victory"),
    ];
    let seeded_current_name = dictionary.weekly_mutation_date_json.name.clone();
    let seeded_current_date =
        NaiveDate::parse_from_str(&dictionary.weekly_mutation_date_json.date, "%Y-%m-%d")
            .expect("seeded weekly mutation date should parse");

    let rows = ReplayAnalysis::rebuild_weeklies_rows_with_dictionary(
        &replays,
        seeded_current_date,
        &dictionary,
    );
    let train_of_the_dead = rows
        .iter()
        .find(|row| row.mutation == "Train of the Dead")
        .expect("Train of the Dead row should exist");
    let first_strike = rows
        .iter()
        .find(|row| row.mutation == "First Strike")
        .expect("First Strike row should exist");
    let time_lock = rows
        .iter()
        .find(|row| row.mutation == "Time Lock")
        .expect("Time Lock row should exist");

    assert!(rows.len() >= 3);
    assert_eq!(rows[0].mutation, seeded_current_name);
    assert!(rows[0].is_current);
    assert_eq!(rows[0].next_duration, "Now");

    assert_eq!(train_of_the_dead.mutation_order, 0);
    assert_eq!(train_of_the_dead.map, "Oblivion Express");
    assert_eq!(train_of_the_dead.name_en, "Train of the Dead");
    assert_eq!(train_of_the_dead.name_ko, "망자의 열차");
    assert_eq!(train_of_the_dead.mutators.len(), 3);
    assert_eq!(
        train_of_the_dead
            .mutators
            .first()
            .map(|value| value.name.ko.as_str()),
        Some("암흑")
    );
    assert_eq!(
        train_of_the_dead
            .mutators
            .first()
            .map(|value| value.description.en.as_str()),
        Some(
            "Previously explored areas remain blacked out on the minimap while outside of player vision."
        )
    );
    assert_eq!(first_strike.mutation_order, 1);
    assert_eq!(time_lock.mutation_order, 2);
}

#[test]
fn rebuild_weeklies_rows_without_record_uses_na_for_best_difficulty() {
    let dictionary = TestHelperOps::load_dictionary();
    let seeded_current_date =
        NaiveDate::parse_from_str(&dictionary.weekly_mutation_date_json.date, "%Y-%m-%d")
            .expect("seeded weekly mutation date should parse");

    let rows = ReplayAnalysis::rebuild_weeklies_rows_with_dictionary(
        &[],
        seeded_current_date,
        &dictionary,
    );
    let row = rows
        .iter()
        .find(|entry| entry.mutation == "Train of the Dead")
        .expect("Train of the Dead row should exist");

    assert_eq!(row.difficulty, "N/A");
}
