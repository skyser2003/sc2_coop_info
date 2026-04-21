use s2coop_analyzer::cache_overall_stats_generator::{
    CacheNumericValue, CachePlayer, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
};
use sco_tauri_overlay::replay_analysis::replay_info_from_cache_entry;
use sco_tauri_overlay::test_helper::build_rebuild_snapshot;
use serde_json::Value;
use std::collections::BTreeMap;

fn sample_cache_player(pid: u8, name: &str, handle: &str, commander: &str) -> CachePlayer {
    CachePlayer {
        pid,
        apm: Some(150),
        commander: Some(commander.to_string()),
        commander_level: Some(15),
        commander_mastery_level: Some(90),
        handle: Some(handle.to_string()),
        icons: Some(BTreeMap::new()),
        kills: Some(25),
        masteries: Some([30, 60, 30, 60, 30, 60]),
        name: Some(name.to_string()),
        observer: None,
        prestige: Some(1),
        prestige_name: Some("P1".to_string()),
        race: Some("Terran".to_string()),
        result: Some("Victory".to_string()),
        units: Some(BTreeMap::new()),
    }
}

fn sample_cache_entry(ext_difficulty: &str, difficulty_pair: (&str, &str)) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Integer(600),
        amon_units: None,
        bonus: Some(vec!["First".to_string()]),
        brutal_plus: 0,
        build: ReplayBuildInfo {
            replay_build: 1,
            protocol_build: ProtocolBuildValue::Int(1),
        },
        comp: Some("Terran".to_string()),
        date: "2026:03:10:12:00:00".to_string(),
        difficulty: (difficulty_pair.0.to_string(), difficulty_pair.1.to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: ext_difficulty.to_string(),
        extension: false,
        file: format!(
            "fixtures/replays/{}.SC2Replay",
            ext_difficulty.replace('/', "_")
        ),
        form_alength: "10:00".to_string(),
        detailed_analysis: true,
        hash: format!("hash_{}", ext_difficulty.replace('/', "_")),
        length: 600,
        map_name: "Void Launch".to_string(),
        messages: Vec::new(),
        mutators: Vec::new(),
        player_stats: None,
        players: vec![
            CachePlayer {
                pid: 0,
                apm: None,
                commander: None,
                commander_level: None,
                commander_mastery_level: None,
                handle: None,
                icons: None,
                kills: None,
                masteries: None,
                name: None,
                observer: None,
                prestige: None,
                prestige_name: None,
                race: None,
                result: None,
                units: None,
            },
            sample_cache_player(1, "Main Player", "1-S2-1-111", "Raynor"),
            sample_cache_player(2, "Ally Player", "1-S2-1-222", "Karax"),
        ],
        region: "NA".to_string(),
        result: "Victory".to_string(),
        weekly: false,
    }
}

#[test]
fn replay_info_from_cache_entry_preserves_mixed_difficulty_label() {
    let mixed =
        replay_info_from_cache_entry(&sample_cache_entry("Hard/Brutal", ("Hard", "Brutal")));
    let brutal = replay_info_from_cache_entry(&sample_cache_entry("Brutal", ("Brutal", "Brutal")));

    assert_eq!(mixed.difficulty, "Hard/Brutal");
    assert_eq!(brutal.difficulty, "Brutal");

    let snapshot = build_rebuild_snapshot(&[mixed, brutal], false);
    let difficulty_data = snapshot
        .analysis
        .get("DifficultyData")
        .and_then(Value::as_object)
        .expect("difficulty data should exist");

    assert_eq!(difficulty_data["Brutal"]["Victory"], serde_json::json!(1));
    assert!(difficulty_data.get("Hard/Brutal").is_none());
}
