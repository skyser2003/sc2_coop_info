use s2coop_analyzer::cache_overall_stats_generator::{
    cache_entry_from_report, serialize_cache_entries, CacheCountValue, CacheUnitStats,
    PlayerStatsSeries, ProtocolBuildValue, ReplayBuildInfo,
};
use s2coop_analyzer::tauri_replay_analysis_impl::{
    build_replay_report_detailed, ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer,
    PlayerPositions, ReplayReportDetailedInput,
};
use std::collections::{BTreeMap, HashSet};

fn player(
    pid: u8,
    name: &str,
    handle: &str,
    race: &str,
    result: &str,
    commander: &str,
    apm: u32,
) -> ParsedReplayPlayer {
    ParsedReplayPlayer {
        pid,
        name: name.to_string(),
        handle: handle.to_string(),
        race: race.to_string(),
        observer: false,
        result: result.to_string(),
        commander: commander.to_string(),
        commander_level: 15,
        commander_mastery_level: 90,
        prestige: 1,
        prestige_name: "P1".to_string(),
        apm,
        masteries: [30, 0, 30, 0, 30, 0],
    }
}

#[test]
fn cache_entry_matches_python_style_detailed_report_formatting() {
    let parser = ParsedReplayInput {
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
            player(1, "SlotOne", "1-S2-1-111", "Terran", "Loss", "Raynor", 120),
            player(2, "SlotTwo", "2-S2-1-222", "Zerg", "Win", "Abathur", 160),
        ],
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        accurate_length: 560.0,
        form_alength: "09:20".to_string(),
        length: 400,
        mutators: vec!["Barrier".to_string()],
        weekly: false,
        messages: vec![ParsedReplayMessage {
            text: "gg".to_string(),
            player: 2,
            time: 20.0,
        }],
        hash: Some("hash-abc".to_string()),
        build: ReplayBuildInfo {
            replay_build: 99999,
            protocol_build: ProtocolBuildValue::Int(99999),
        },
        date: "2026:02:25:12:00:00".to_string(),
        enemy_race: "Zerg".to_string(),
        ext_difficulty: "Brutal".to_string(),
        region: "NA".to_string(),
    };

    let mut detailed = ReplayReportDetailedInput::from_parser(parser);
    detailed.positions = Some(PlayerPositions { main: 2, ally: 1 });
    detailed.length = Some(400.0);
    detailed.bonus = Some(vec!["10:54".to_string()]);
    detailed.comp = Some("Masters and Machines".to_string());
    detailed.main_kills = Some(99);
    detailed.ally_kills = Some(12);
    detailed.main_icons = Some(BTreeMap::from([("shuttles".to_string(), 5)]));
    detailed.ally_icons = Some(BTreeMap::from([("shuttles".to_string(), 0)]));
    detailed.main_units = Some(BTreeMap::from([
        ("Abathur's Top Bar".to_string(), (10, 4, 0, 0.0)),
        ("Mutalisk".to_string(), (21, 2, 45, 0.38)),
    ]));
    detailed.ally_units = Some(BTreeMap::from([("Marine".to_string(), (5, 1, 12, 0.25))]));
    detailed.amon_units = Some(BTreeMap::from([(
        "Hybrid Nemesis".to_string(),
        (2, 2, 0, 0.0),
    )]));
    detailed.player_stats = Some(BTreeMap::from([
        (
            1,
            PlayerStatsSeries {
                name: "SlotTwo".to_string(),
                supply: vec![12.0, 20.5],
                mining: vec![0.0, 75.0],
                army: vec![300.0, 500.0],
                killed: vec![1.0, 4.0],
                army_force_float_indices: Default::default(),
            },
        ),
        (
            2,
            PlayerStatsSeries {
                name: "SlotOne".to_string(),
                supply: vec![10.0, 16.0],
                mining: vec![0.0, 50.0],
                army: vec![200.0, 300.0],
                killed: vec![0.0, 2.0],
                army_force_float_indices: Default::default(),
            },
        ),
    ]));

    let report = build_replay_report_detailed(&detailed.parser.file, &detailed, &HashSet::new());
    let hidden_units = HashSet::from(["Abathur's Top Bar".to_string()]);
    let entry = cache_entry_from_report(&report, &hidden_units);

    assert_eq!(entry.length, 400);
    assert_eq!(entry.form_alength, "09:20");
    assert_eq!(entry.ext_difficulty, "Brutal");
    assert_eq!(entry.players[0].pid, 0);
    assert!(entry.players[0].name.is_none());
    assert_eq!(entry.players[1].kills, Some(12));
    assert_eq!(entry.players[2].kills, Some(99));
    assert_eq!(entry.players[1].name.as_deref(), Some("SlotOne"));
    assert_eq!(entry.players[2].name.as_deref(), Some("SlotTwo"));
    assert_eq!(
        entry.players[2]
            .units
            .as_ref()
            .and_then(|units| units.get("Abathur's Top Bar")),
        Some(&CacheUnitStats(
            CacheCountValue::Hidden("-".to_string()),
            CacheCountValue::Hidden("-".to_string()),
            0,
            0.0,
        ))
    );

    let serialized =
        String::from_utf8(serialize_cache_entries(&[entry]).expect("serialization must succeed"))
            .expect("serialized json must be utf-8");

    assert!(serialized.starts_with("[{\"accurate_length\":560.0,\"amon_units\":"));
    assert!(serialized.contains("\"army\":[300,500]"));
    assert!(serialized.contains("\"supply\":[12.0,20.5]"));
    assert!(serialized.contains("\"Abathur's Top Bar\":[\"-\",\"-\",0,0.0]"));
}
