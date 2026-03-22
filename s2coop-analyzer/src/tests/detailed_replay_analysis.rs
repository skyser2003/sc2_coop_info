use super::*;

fn sample_parser_with_commanders(main_commander: &str, ally_commander: &str) -> ParsedReplayInput {
    ParsedReplayInput {
        file: "fixtures/replays/detailed.SC2Replay".to_string(),
        map_name: "Void Launch".to_string(),
        extension: false,
        brutal_plus: 0,
        result: "Victory".to_string(),
        players: vec![
            ParsedReplayPlayer {
                pid: 0,
                name: String::new(),
                handle: String::new(),
                race: String::new(),
                observer: false,
                result: String::new(),
                commander: String::new(),
                commander_level: 0,
                commander_mastery_level: 0,
                prestige: 0,
                prestige_name: String::new(),
                apm: 0,
                masteries: [0, 0, 0, 0, 0, 0],
            },
            ParsedReplayPlayer {
                pid: 1,
                name: "MainPlayer".to_string(),
                handle: "1-S2-1-111".to_string(),
                race: "Terran".to_string(),
                observer: false,
                result: "Win".to_string(),
                commander: main_commander.to_string(),
                commander_level: 15,
                commander_mastery_level: 90,
                prestige: 1,
                prestige_name: "P1".to_string(),
                apm: 145,
                masteries: [30, 0, 30, 0, 30, 0],
            },
            ParsedReplayPlayer {
                pid: 2,
                name: "AllyPlayer".to_string(),
                handle: "2-S2-1-222".to_string(),
                race: "Protoss".to_string(),
                observer: false,
                result: "Win".to_string(),
                commander: ally_commander.to_string(),
                commander_level: 15,
                commander_mastery_level: 90,
                prestige: 2,
                prestige_name: "P2".to_string(),
                apm: 130,
                masteries: [0, 30, 0, 30, 0, 30],
            },
        ],
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        accurate_length: 1260.0,
        form_alength: "21:00".to_string(),
        length: 1260,
        mutators: Vec::new(),
        weekly: false,
        messages: Vec::new(),
        hash: None,
        build: ReplayBuildInfo {
            replay_build: 99999,
            protocol_build: ProtocolBuildValue::Int(99999),
        },
        date: "2026:02:25:12:00:00".to_string(),
        enemy_race: "Zerg".to_string(),
        ext_difficulty: "Brutal".to_string(),
        region: "NA".to_string(),
    }
}

#[test]
fn parser_player_overrides_preserve_non_empty_slot_commanders() {
    let mut parser = sample_parser_with_commanders("Horner", "Vorazun");
    let commander_by_player = HashMap::from([
        (1_i64, "Han & Horner".to_string()),
        (2_i64, "Vorazun".to_string()),
    ]);
    let mastery_by_player = HashMap::from([(1_i64, [0_i64; 6]), (2_i64, [0_i64; 6])]);
    let prestige_by_player = HashMap::<i64, String>::new();

    apply_parser_player_overrides(
        &mut parser,
        &commander_by_player,
        &mastery_by_player,
        &prestige_by_player,
    );

    assert_eq!(
        find_replay_player(&parser.players, 1).unwrap().commander,
        "Horner"
    );
    assert_eq!(
        find_replay_player(&parser.players, 2).unwrap().commander,
        "Vorazun"
    );
}

#[test]
fn parser_player_overrides_fill_missing_commanders_from_events() {
    let mut parser = sample_parser_with_commanders("", "");
    let commander_by_player = HashMap::from([
        (1_i64, "Han & Horner".to_string()),
        (2_i64, "Vorazun".to_string()),
    ]);
    let mastery_by_player = HashMap::from([(1_i64, [0_i64; 6]), (2_i64, [0_i64; 6])]);
    let prestige_by_player = HashMap::<i64, String>::new();

    apply_parser_player_overrides(
        &mut parser,
        &commander_by_player,
        &mastery_by_player,
        &prestige_by_player,
    );

    assert_eq!(
        find_replay_player(&parser.players, 1).unwrap().commander,
        "Han & Horner"
    );
    assert_eq!(
        find_replay_player(&parser.players, 2).unwrap().commander,
        "Vorazun"
    );
}
