use sco_tauri_overlay::{ReplayInfo, ReplayPlayerInfo, TestHelperOps};
use serde_json::Value;

fn test_map_id(raw: &str) -> String {
    TestHelperOps::canonicalize_map_id(raw).expect("map id should resolve")
}

fn player(name: &str, commander: &str, masteries: Vec<u64>) -> ReplayPlayerInfo {
    ReplayPlayerInfo::default()
        .with_name(name)
        .with_commander(commander)
        .with_masteries(masteries)
}

fn replay(main_masteries: Vec<u64>, main_prestige: u64) -> ReplayInfo {
    let mut replay = ReplayInfo::with_players(
        player("Main", "Abathur", main_masteries).with_prestige(main_prestige),
        player("Ally", "Raynor", vec![30, 0, 30, 0, 30, 0]),
        0,
    );
    replay.set_map(test_map_id("Void Launch"));
    replay.set_result("Victory");
    replay.set_difficulty("Brutal");
    replay
}

fn distribution_bucket(entry: &serde_json::Map<String, Value>, pair: &str, bucket: &str) -> f64 {
    entry
        .get("MasteryDistribution")
        .and_then(Value::as_object)
        .and_then(|distribution| distribution.get(pair))
        .and_then(Value::as_object)
        .and_then(|pair_distribution| pair_distribution.get(bucket))
        .and_then(Value::as_f64)
        .expect("distribution bucket should exist")
}

fn prestige_distribution_bucket(
    entry: &serde_json::Map<String, Value>,
    prestige: &str,
    pair: &str,
    bucket: &str,
) -> f64 {
    entry
        .get("MasteryDistributionByPrestige")
        .and_then(Value::as_object)
        .and_then(|distribution| distribution.get(prestige))
        .and_then(Value::as_object)
        .and_then(|prestige_distribution| prestige_distribution.get(pair))
        .and_then(Value::as_object)
        .and_then(|pair_distribution| pair_distribution.get(bucket))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

#[test]
fn commander_mastery_distribution_tracks_exact_pair_ratio_buckets() {
    let replays = vec![
        replay(vec![0, 30, 10, 20, 30, 0], 0),
        replay(vec![1, 2, 10, 20, 0, 30], 0),
        replay(vec![15, 15, 10, 20, 0, 30], 1),
        replay(vec![30, 0, 20, 10, 0, 30], 1),
    ];

    let snapshot = TestHelperOps::build_rebuild_snapshot(&replays, false);
    let abathur = snapshot
        .analysis()
        .get("CommanderData")
        .and_then(Value::as_object)
        .and_then(|commander_data| commander_data.get("Abathur"))
        .and_then(Value::as_object)
        .expect("Abathur commander stats should exist");

    assert!((distribution_bucket(abathur, "0", "0") - 0.25).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "0", "33.333") - 0.25).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "0", "50") - 0.25).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "0", "100") - 0.25).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "1", "33.333") - 0.75).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "1", "66.667") - 0.25).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "2", "0") - 0.75).abs() < 1e-9);
    assert!((distribution_bucket(abathur, "2", "100") - 0.25).abs() < 1e-9);
    assert!((prestige_distribution_bucket(abathur, "0", "0", "0") - 0.5).abs() < 1e-9);
    assert!((prestige_distribution_bucket(abathur, "0", "0", "33.333") - 0.5).abs() < 1e-9);
    assert!((prestige_distribution_bucket(abathur, "1", "0", "50") - 0.5).abs() < 1e-9);
    assert!((prestige_distribution_bucket(abathur, "1", "0", "100") - 0.5).abs() < 1e-9);
    assert_eq!(prestige_distribution_bucket(abathur, "2", "0", "0"), 0.0);
}
