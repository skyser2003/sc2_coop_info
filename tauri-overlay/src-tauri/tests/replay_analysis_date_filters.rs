use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CachePlayer, CacheReplayEntry, CacheUnitStats, ProtocolBuildValue,
    ReplayBuildInfo,
};
use sco_tauri_overlay::replay_analysis::{
    parse_replay_timestamp_seconds, replay_info_from_cache_entry,
};
use sco_tauri_overlay::test_helper::filter_replays_for_stats;
use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_path(label: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("sco_{label}_{timestamp}"))
}

fn sample_cache_player(pid: u8, name: &str, handle: &str, commander: &str) -> CachePlayer {
    let mut unit_map = BTreeMap::new();
    unit_map.insert(
        "Primal Hydralisk".to_string(),
        CacheUnitStats(
            CacheCountValue::Count(2),
            CacheCountValue::Count(1),
            4,
            0.25,
        ),
    );

    CachePlayer {
        pid,
        apm: Some(120),
        commander: Some(commander.to_string()),
        commander_level: Some(15),
        commander_mastery_level: Some(90),
        handle: Some(handle.to_string()),
        icons: Some(BTreeMap::new()),
        kills: Some(25),
        masteries: Some([30, 60, 30, 60, 30, 60]),
        name: Some(name.to_string()),
        observer: None,
        prestige: Some(0),
        prestige_name: Some("P0".to_string()),
        race: Some("Zerg".to_string()),
        result: Some("Victory".to_string()),
        units: Some(unit_map),
    }
}

fn sample_cache_entry(file: &Path, date: &str) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: s2coop_analyzer::cache_overall_stats_generator::CacheNumericValue::Integer(
            600,
        ),
        amon_units: None,
        bonus: Some(vec!["First".to_string()]),
        brutal_plus: 0,
        build: ReplayBuildInfo {
            replay_build: 1,
            protocol_build: ProtocolBuildValue::Int(1),
        },
        comp: Some("Terran".to_string()),
        date: date.to_string(),
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: "Brutal".to_string(),
        extension: false,
        file: file.display().to_string(),
        form_alength: "10:00".to_string(),
        detailed_analysis: true,
        hash: format!("hash_{}", file.display()),
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
            sample_cache_player(1, "Main Player", "1-S2-1-111", "Dehaka"),
            sample_cache_player(2, "Ally Player", "1-S2-1-222", "Karax"),
        ],
        region: "NA".to_string(),
        result: "Victory".to_string(),
        weekly: false,
    }
}

fn replay_for_date_filter(date: &str, file_suffix: &str) -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo::default()
            .with_name("Main")
            .with_handle("1-S2-1-111")
            .with_commander("Dehaka")
            .with_commander_level(15),
        ReplayPlayerInfo::default()
            .with_name("Ally")
            .with_handle("1-S2-1-222")
            .with_commander("Karax")
            .with_commander_level(15),
        0,
    );
    replay.set_file(format!("fixtures/replays/{file_suffix}.SC2Replay"));
    replay.set_date(parse_replay_timestamp_seconds(date).expect("date should parse"));
    replay.set_map("Void Launch");
    replay.set_result("Victory");
    replay.set_difficulty("Brutal");
    replay
}

#[test]
fn cache_entry_uses_recorded_replay_timestamp() {
    let replay_path = unique_temp_path("cache_replay_date").with_extension("SC2Replay");
    std::fs::write(&replay_path, []).expect("replay file should be created");

    let entry = sample_cache_entry(&replay_path, "2018:12:31:21:44:38");
    let replay = replay_info_from_cache_entry(&entry);

    assert_eq!(
        replay.date(),
        parse_replay_timestamp_seconds("2018:12:31:21:44:38")
            .expect("recorded replay timestamp should parse")
    );

    let _ = std::fs::remove_file(&replay_path);
}

#[test]
fn filter_replays_for_stats_uses_strict_maxdate_boundary() {
    let replays = vec![
        replay_for_date_filter("2020:12:30:13:00:00", "included"),
        replay_for_date_filter("2020:12:31:13:00:00", "excluded"),
    ];

    let filtered = filter_replays_for_stats(
        "/config/stats?mindate=2020-12-30&maxdate=2020-12-31",
        &replays,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].file(), "fixtures/replays/included.SC2Replay");
}
