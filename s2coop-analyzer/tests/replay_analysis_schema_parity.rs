use s2coop_analyzer::cache_overall_stats_generator::{ProtocolBuildValue, ReplayBuildInfo};
use s2coop_analyzer::tauri_replay_analysis_impl::{
    build_replay_report, ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer,
};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};

fn player(pid: u8, name: &str, handle: &str, commander: &str, apm: u32) -> ParsedReplayPlayer {
    ParsedReplayPlayer {
        pid,
        name: name.to_string(),
        handle: handle.to_string(),
        race: "Terran".to_string(),
        observer: false,
        result: "Win".to_string(),
        commander: commander.to_string(),
        commander_level: 15,
        commander_mastery_level: 90,
        prestige: 1,
        prestige_name: "P1".to_string(),
        apm,
        masteries: [30, 0, 30, 0, 30, 0],
    }
}

fn object_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("value should be object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn replay_report_has_expected_top_level_schema() {
    let replay = ParsedReplayInput {
        file: "fixtures/replays/test.SC2Replay".to_string(),
        map_name: "Void Launch".to_string(),
        extension: false,
        brutal_plus: 0,
        result: "Victory".to_string(),
        players: vec![
            player(0, "", "", "", 0),
            player(1, "MainPlayer", "1-S2-1-111", "Raynor", 145),
            player(2, "AllyPlayer", "2-S2-1-222", "Artanis", 130),
        ],
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        accurate_length: 1260.0,
        form_alength: "21:00".to_string(),
        length: 1260,
        mutators: vec!["Barrier".to_string()],
        weekly: false,
        messages: vec![ParsedReplayMessage {
            text: "gg".to_string(),
            player: 1,
            time: 120.0,
        }],
        hash: Some("abc123".to_string()),
        build: ReplayBuildInfo {
            replay_build: 99999,
            protocol_build: ProtocolBuildValue::Int(99999),
        },
        date: "2026:02:25:12:00:00".to_string(),
        enemy_race: "Zerg".to_string(),
        ext_difficulty: "Brutal".to_string(),
        region: "NA".to_string(),
    };

    let main_handles = HashSet::from(["1-S2-1-111".to_string()]);
    let report = build_replay_report(&replay.file, &replay, &main_handles);

    let report_json = serde_json::to_value(&report).expect("report should serialize");
    let keys = object_keys(&report_json);
    let expected = BTreeSet::from([
        "B+".to_string(),
        "ally".to_string(),
        "allyAPM".to_string(),
        "allyCommander".to_string(),
        "allyCommanderLevel".to_string(),
        "allyIcons".to_string(),
        "allyMasteries".to_string(),
        "allyPrestige".to_string(),
        "allyUnits".to_string(),
        "allykills".to_string(),
        "amon_units".to_string(),
        "bonus".to_string(),
        "comp".to_string(),
        "difficulty".to_string(),
        "extension".to_string(),
        "file".to_string(),
        "length".to_string(),
        "main".to_string(),
        "mainAPM".to_string(),
        "mainCommander".to_string(),
        "mainCommanderLevel".to_string(),
        "mainIcons".to_string(),
        "mainMasteries".to_string(),
        "mainPrestige".to_string(),
        "mainUnits".to_string(),
        "mainkills".to_string(),
        "map_name".to_string(),
        "mutators".to_string(),
        "parser".to_string(),
        "player_stats".to_string(),
        "positions".to_string(),
        "replaydata".to_string(),
        "result".to_string(),
        "weekly".to_string(),
    ]);

    assert_eq!(keys, expected);
}

#[test]
fn replay_report_uses_handle_to_choose_main_player_position() {
    let replay = ParsedReplayInput {
        file: "fixtures/replays/test.SC2Replay".to_string(),
        map_name: "Void Launch".to_string(),
        extension: false,
        brutal_plus: 0,
        result: "Victory".to_string(),
        players: vec![
            player(0, "", "", "", 0),
            player(1, "PlayerA", "1-S2-1-111", "Raynor", 110),
            player(2, "PlayerB", "2-S2-1-222", "Artanis", 200),
        ],
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        accurate_length: 140.0,
        form_alength: "02:20".to_string(),
        length: 140,
        mutators: Vec::new(),
        weekly: false,
        messages: Vec::new(),
        hash: None,
        build: ReplayBuildInfo {
            replay_build: 1,
            protocol_build: ProtocolBuildValue::Int(1),
        },
        date: "2026:02:25:12:00:00".to_string(),
        enemy_race: "Zerg".to_string(),
        ext_difficulty: "Brutal".to_string(),
        region: "NA".to_string(),
    };

    let main_handles = HashSet::from(["2-S2-1-222".to_string()]);
    let report = build_replay_report(&replay.file, &replay, &main_handles);
    let json = serde_json::to_value(report).expect("report should serialize");

    let positions = json
        .get("positions")
        .and_then(Value::as_object)
        .expect("positions should be object");
    assert_eq!(
        positions.get("main").and_then(Value::as_u64),
        Some(2),
        "player 2 should become main when handle is in main set"
    );
    assert_eq!(
        positions.get("ally").and_then(Value::as_u64),
        Some(1),
        "player 1 should become ally when player 2 is main"
    );
}
