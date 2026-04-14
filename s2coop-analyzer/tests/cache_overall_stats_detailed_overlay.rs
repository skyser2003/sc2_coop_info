use s2coop_analyzer::cache_overall_stats_generator::{
    cache_entry_from_report, CacheNumericValue, PlayerStatsSeries, ProtocolBuildValue,
    ReplayBuildInfo,
};
use s2coop_analyzer::tauri_replay_analysis_impl::{
    build_replay_report_detailed, ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer,
    PlayerPositions, ReplayReportDetailedInput,
};
use std::collections::{BTreeMap, HashSet};

fn sample_build_info() -> ReplayBuildInfo {
    ReplayBuildInfo {
        replay_build: 12345,
        protocol_build: ProtocolBuildValue::Int(12345),
    }
}

fn sample_player(
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

fn sample_parser(hash: &str, file: &str) -> ParsedReplayInput {
    ParsedReplayInput {
        file: file.to_string(),
        map_name: "Void Launch".to_string(),
        extension: false,
        brutal_plus: 0,
        result: "Victory".to_string(),
        players: vec![
            sample_player(0, "", "", "", "", "", 0),
            sample_player(1, "SlotOne", "1-S2-1-111", "Terran", "Loss", "Raynor", 120),
            sample_player(2, "SlotTwo", "2-S2-1-222", "Zerg", "Win", "Abathur", 160),
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
        hash: Some(hash.to_string()),
        build: sample_build_info(),
        date: "2026:02:25:12:00:00".to_string(),
        enemy_race: "Zerg".to_string(),
        ext_difficulty: "Brutal".to_string(),
        region: "NA".to_string(),
    }
}

#[test]
fn detailed_cache_entry_preserves_shared_base_fields() {
    let parser = sample_parser("hash-abc", "fixtures/replays/detailed.SC2Replay");
    let mut detailed = ReplayReportDetailedInput::from_parser(parser.clone());
    detailed.positions = Some(PlayerPositions { main: 2, ally: 1 });
    detailed.length = Some(400.0);
    detailed.bonus = Some(vec!["10:54".to_string()]);
    detailed.comp = Some("Masters and Machines".to_string());
    detailed.main_kills = Some(99);
    detailed.ally_kills = Some(12);
    detailed.main_icons = Some(BTreeMap::from([("shuttles".to_string(), 5)]));
    detailed.ally_icons = Some(BTreeMap::from([("shuttles".to_string(), 0)]));
    detailed.main_units = Some(BTreeMap::from([(
        "Mutalisk".to_string(),
        (21, 2, 45, 0.38),
    )]));
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

    let report = build_replay_report_detailed(&parser.file, &detailed, &HashSet::new());
    let entry = cache_entry_from_report(&report, &HashSet::new());

    assert_eq!(entry.brutal_plus, parser.brutal_plus);
    assert_eq!(entry.build, parser.build.clone());
    assert_eq!(entry.date, parser.date.clone());
    assert_eq!(entry.difficulty, parser.difficulty.clone());
    assert_eq!(
        entry.enemy_race.as_deref(),
        Some(parser.enemy_race.as_str())
    );
    assert_eq!(entry.ext_difficulty, parser.ext_difficulty.clone());
    assert_eq!(entry.extension, parser.extension);
    assert_eq!(
        entry.file.replace('\\', "/"),
        parser.file.replace('\\', "/")
    );
    assert_eq!(entry.hash, "hash-abc");
    assert_eq!(entry.length, 400);
    assert_eq!(entry.map_name, parser.map_name.clone());
    assert_eq!(entry.result, parser.result.clone());
    assert_eq!(entry.accurate_length, CacheNumericValue::Float(560.0));
    assert_eq!(entry.form_alength, "09:20");
    assert_eq!(entry.messages, parser.messages.clone());
    assert_eq!(entry.mutators, parser.mutators.clone());
    assert_eq!(entry.region, parser.region.clone());
    assert_eq!(entry.weekly, parser.weekly);
    assert!(entry.detailed_analysis);
    assert_eq!(entry.players.len(), 3);
    assert_eq!(entry.players[1].kills, Some(12));
    assert_eq!(entry.players[2].kills, Some(99));
    assert_eq!(entry.players[1].apm, Some(120));
    assert_eq!(entry.players[2].apm, Some(160));
    assert!(entry.player_stats.is_some());
    assert_eq!(entry.comp.as_deref(), Some("Masters and Machines"));
}
