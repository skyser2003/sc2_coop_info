use s2coop_analyzer::cache_overall_stats_generator::{
    CacheNumericValue, CachePlayer, CacheReplayEntry, ProtocolBuildValue, ReplayBuildInfo,
    ReplayMessage,
};

fn sample_build_info() -> ReplayBuildInfo {
    ReplayBuildInfo::new(12345, ProtocolBuildValue::Int(12345))
}

fn sample_player(pid: u8) -> CachePlayer {
    CachePlayer {
        pid,
        apm: None,
        commander: Some(format!("Commander {pid}")),
        commander_level: None,
        commander_mastery_level: None,
        handle: Some(format!("{pid}-S2-1-2-3")),
        icons: None,
        kills: None,
        masteries: None,
        name: Some(format!("Player {pid}")),
        observer: None,
        prestige: None,
        prestige_name: None,
        race: Some("Terran".to_string()),
        result: Some("Victory".to_string()),
        units: None,
    }
}

fn sample_cached_entry(hash: &str, file: &str, detailed_analysis: bool) -> CacheReplayEntry {
    CacheReplayEntry {
        accurate_length: CacheNumericValue::Float(1750.0),
        amon_units: None,
        bonus: None,
        brutal_plus: 0,
        build: sample_build_info(),
        comp: if detailed_analysis {
            Some("Commander 1, Commander 2".to_string())
        } else {
            None
        },
        date: "2024-01-01 00:00:00".to_string(),
        difficulty: ("Brutal".to_string(), "Brutal".to_string()),
        enemy_race: Some("Zerg".to_string()),
        ext_difficulty: "Brutal".to_string(),
        extension: false,
        file: file.to_string(),
        form_alength: "20:50".to_string(),
        detailed_analysis,
        hash: hash.to_string(),
        length: 1250,
        map_name: "Dead of Night".to_string(),
        messages: vec![ReplayMessage {
            text: "gl hf".to_string(),
            player: 1,
            time: 1.0,
        }],
        mutators: vec!["Alien Incubation".to_string()],
        player_stats: None,
        players: vec![sample_player(1), sample_player(2)],
        region: "NA".to_string(),
        result: "Victory".to_string(),
        weekly: false,
    }
}

#[test]
fn serialize_entries_preserves_input_order_after_parallel_canonicalization() {
    let entries = vec![
        sample_cached_entry("hash-03", "third.SC2Replay", true),
        sample_cached_entry("hash-01", "first.SC2Replay", true),
        sample_cached_entry("hash-02", "second.SC2Replay", false),
    ];

    let payload = CacheReplayEntry::serialize_entries(&entries).expect("entries should serialize");
    let serialized =
        serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).expect("entries should parse");
    let hashes = serialized
        .iter()
        .map(|entry| entry.hash.as_str())
        .collect::<Vec<&str>>();

    assert_eq!(hashes, vec!["hash-03", "hash-01", "hash-02"]);
}
