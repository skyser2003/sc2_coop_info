use sco_tauri_overlay::replay_analysis::*;
use sco_tauri_overlay::{canonicalize_coop_map_id, ReplayInfo};

fn replay_for_checkbox_filter(
    file_name: &str,
    difficulty: &str,
    brutal_plus: u64,
    p1_handle: &str,
) -> ReplayInfo {
    ReplayInfo {
        file: format!("fixtures/replays/{file_name}.SC2Replay"),
        map: canonicalize_coop_map_id("Void Launch").expect("map id should resolve"),
        result: "Victory".to_string(),
        difficulty: difficulty.to_string(),
        p1: "Main".to_string(),
        p2: "Ally".to_string(),
        p1_handle: p1_handle.to_string(),
        p2_handle: "1-S2-1-999".to_string(),
        main_commander: "Raynor".to_string(),
        ally_commander: "Karax".to_string(),
        main_commander_level: 15,
        ally_commander_level: 15,
        brutal_plus,
        ..ReplayInfo::default()
    }
}

#[test]
fn filter_replays_for_stats_decodes_checkbox_filter_lists_from_query_string() {
    let replays = vec![
        replay_for_checkbox_filter("na_brutal", "Brutal", 0, "1-S2-1-111"),
        replay_for_checkbox_filter("eu_normal", "Normal", 0, "2-S2-1-222"),
        replay_for_checkbox_filter("kr_bplus3", "Brutal", 3, "3-S2-1-333"),
    ];

    let filtered = ReplayAnalysis::filter_replays_for_stats(
        "/config/stats?difficulty_filter=Normal%2C3&region_filter=EU%2CKR",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file, "fixtures/replays/na_brutal.SC2Replay");
}

#[test]
fn filter_replays_for_stats_decodes_brutal_plus_checkbox_values() {
    let replays = vec![
        replay_for_checkbox_filter("plain_brutal", "Brutal", 0, "1-S2-1-111"),
        replay_for_checkbox_filter("bplus1", "Brutal", 1, "1-S2-1-222"),
    ];

    let filtered = ReplayAnalysis::filter_replays_for_stats(
        "/config/stats?difficulty_filter=1%2C2%2C3%2C4%2C5%2C6",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file, "fixtures/replays/plain_brutal.SC2Replay");
}
