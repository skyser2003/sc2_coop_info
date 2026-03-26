use sco_tauri_overlay::replay_analysis::*;
use sco_tauri_overlay::CommanderUnitRollup;
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

fn empty_rollup() -> BTreeMap<String, CommanderUnitRollup> {
    BTreeMap::new()
}

#[test]
fn append_player_units_to_rollups_routes_both_main_players_to_main_side() {
    let mut main_rollup = empty_rollup();
    let mut ally_rollup = empty_rollup();
    let main_handles = HashSet::from(["1-s2-1-111".to_string(), "1-s2-1-222".to_string()]);

    append_player_units_to_rollups(
        &mut main_rollup,
        &mut ally_rollup,
        "Dehaka",
        &json!({
            "Primal Hydralisk": [2, 0, 4, 0.25]
        }),
        8,
        "1-S2-1-111",
        &main_handles,
    );
    append_player_units_to_rollups(
        &mut main_rollup,
        &mut ally_rollup,
        "Abathur",
        &json!({
            "Roach": [1, 0, 2, 0.20]
        }),
        10,
        "1-S2-1-222",
        &main_handles,
    );

    assert_eq!(main_rollup["Dehaka"].units["Primal Hydralisk"].kills, 4);
    assert_eq!(main_rollup["Abathur"].units["Roach"].kills, 2);
    assert!(ally_rollup.is_empty());
}

#[test]
fn append_player_units_to_rollups_routes_unknown_handles_to_ally_side() {
    let mut main_rollup = empty_rollup();
    let mut ally_rollup = empty_rollup();
    let main_handles = HashSet::from(["1-s2-1-111".to_string()]);

    append_player_units_to_rollups(
        &mut main_rollup,
        &mut ally_rollup,
        "Dehaka",
        &json!({
            "Primal Roach": [1, 0, 3, 0.30]
        }),
        10,
        "9-S2-1-999",
        &main_handles,
    );

    assert!(main_rollup.is_empty());
    assert_eq!(ally_rollup["Dehaka"].units["Primal Roach"].kills, 3);
}
