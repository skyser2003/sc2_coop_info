use super::*;

#[test]
fn update_analysis_replay_cache_slots_populates_shared_and_stats_caches() {
    let replays = Arc::new(Mutex::new(Vec::<ReplayInfo>::new()));
    let stats_replays = Arc::new(Mutex::new(Vec::<ReplayInfo>::new()));
    let replay = ReplayInfo {
        file: test_replay_path("cached.SC2Replay"),
        result: "Victory".to_string(),
        ..ReplayInfo::default()
    };

    update_analysis_replay_cache_slots(&[replay.clone()], &replays, &stats_replays);

    let shared_cache = replays
        .lock()
        .expect("replays mutex should not be poisoned")
        .clone();
    let stats_cache = stats_replays
        .lock()
        .expect("stats replays mutex should not be poisoned")
        .clone();

    assert_eq!(shared_cache.len(), 1);
    assert_eq!(stats_cache.len(), 1);
    assert_eq!(shared_cache[0].file, replay.file);
    assert_eq!(stats_cache[0].file, replay.file);
    assert_eq!(shared_cache[0].result, replay.result);
    assert_eq!(stats_cache[0].result, replay.result);
}
