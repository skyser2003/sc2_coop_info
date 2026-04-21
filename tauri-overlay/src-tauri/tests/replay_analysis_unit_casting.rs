use sco_tauri_overlay::replay_analysis::append_units_to_rollup;
use sco_tauri_overlay::test_helper::build_commander_unit_data;
use sco_tauri_overlay::CommanderUnitRollup;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn hidden_unit_rows_preserve_wx_made_and_mc_bonus_behavior() {
    let mut rollup = BTreeMap::<String, CommanderUnitRollup>::new();

    append_units_to_rollup(
        &mut rollup,
        "Karax",
        &json!({
            "Energizer": [1, 0, 10, 0.50],
            "Karax's Top Bar": ["-", "-", 4, 0.20]
        }),
        20,
    );

    let commander = &rollup["Karax"];
    assert_eq!(commander.units["Energizer"].kills, 10);
    assert_eq!(commander.units["Karax's Top Bar"].made, 0);
    assert!(commander.units["Karax's Top Bar"].created_hidden);
    assert!(commander.units["Karax's Top Bar"].lost_hidden);
}

#[test]
fn hidden_unit_rows_keep_created_and_lost_masked_in_unit_data() {
    let mut rollup = BTreeMap::<String, CommanderUnitRollup>::new();

    append_units_to_rollup(
        &mut rollup,
        "Karax",
        &json!({
            "Karax's Top Bar": ["-", "-", 4, 0.20]
        }),
        20,
    );

    let unit_data = build_commander_unit_data(rollup);

    assert_eq!(unit_data["Karax"]["Karax's Top Bar"]["created"], json!("-"));
    assert_eq!(unit_data["Karax"]["Karax's Top Bar"]["lost"], json!("-"));
    assert_eq!(unit_data["Karax"]["Karax's Top Bar"]["kills"], json!(4));
}
