use s2coop_analyzer::cache_overall_stats_generator::{ProtocolBuildValue, ReplayBuildInfo};
use s2coop_analyzer::tauri_replay_analysis_impl::{
    ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer, PlayerPositions, ReplayReport,
    ReplayReportDetailData, ReplayReportDetailedInput,
};
use std::collections::{BTreeMap, HashSet};

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

fn sample_replay() -> ParsedReplayInput {
    ParsedReplayInput {
        file: "fixtures/replays/detailed.SC2Replay".to_string(),
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
fn detailed_builder_applies_positions_hash_length_and_payload_fields() {
    let mut detailed = ReplayReportDetailedInput::from_parser(sample_replay());
    detailed.positions = Some(PlayerPositions { main: 2, ally: 1 });
    detailed.detail = Some(ReplayReportDetailData {
        length: 100.0,
        bonus: vec!["Bonus A".to_string()],
        comp: "Zerg".to_string(),
        replay_hash: Some("hash-abc".to_string()),
        main_kills: 77,
        ally_kills: 12,
        main_icons: BTreeMap::from([("icon_main".to_string(), 5)]),
        ally_icons: BTreeMap::from([("icon_ally".to_string(), 9)]),
        main_units: BTreeMap::from([("Marine".to_string(), (1, 2, 3, 4.0))]),
        ally_units: BTreeMap::from([("Dragoon".to_string(), (5, 6, 7, 8.0))]),
        amon_units: BTreeMap::from([("Zergling".to_string(), (9, 10, 11, 12.0))]),
        player_stats: BTreeMap::new(),
        outlaw_order: Vec::new(),
    });

    let report =
        ReplayReport::from_detailed_input(&detailed.parser.file, &detailed, &HashSet::new());

    assert_eq!(report.positions.main, 2);
    assert_eq!(report.positions.ally, 1);
    assert_eq!(report.main, "AllyPlayer");
    assert_eq!(report.ally, "MainPlayer");
    assert_eq!(report.length, 100.0);
    assert_eq!(report.parser.accurate_length, 1260.0);
    assert_eq!(report.parser.form_alength, "21:00");
    assert_eq!(report.parser.length, 1260);
    assert_eq!(report.parser.hash.as_deref(), Some("hash-abc"));
    assert_eq!(report.bonus, vec!["Bonus A".to_string()]);
    assert_eq!(report.comp, "Zerg".to_string());
    assert_eq!(report.main_kills, 77);
    assert_eq!(report.ally_kills, 12);
    assert_eq!(
        report.main_icons,
        BTreeMap::from([("icon_main".to_string(), 5)])
    );
    assert_eq!(
        report.ally_icons,
        BTreeMap::from([("icon_ally".to_string(), 9)])
    );
    assert_eq!(
        report.main_units,
        BTreeMap::from([("Marine".to_string(), (1, 2, 3, 4.0))])
    );
    assert_eq!(
        report.ally_units,
        BTreeMap::from([("Dragoon".to_string(), (5, 6, 7, 8.0))])
    );
    assert_eq!(
        report.amon_units,
        BTreeMap::from([("Zergling".to_string(), (9, 10, 11, 12.0))])
    );
}

#[test]
fn detailed_builder_defaults_match_default_builder() {
    let replay = sample_replay();
    let main_handles = HashSet::from(["1-S2-1-111".to_string()]);
    let detailed = ReplayReportDetailedInput::from_parser(replay.clone());

    let report = ReplayReport::from_parser(&replay.file, &replay, &main_handles);
    let detailed_built = ReplayReport::from_detailed_input(&replay.file, &detailed, &main_handles);

    assert_eq!(detailed_built, report);
}
