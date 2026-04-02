mod common;

use common::test_replay_path;
use sco_tauri_overlay::{update_analysis_replay_cache_slots, ReplayInfo};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[test]
fn update_analysis_replay_cache_slots_populates_shared_and_stats_caches() {
    let replays = Arc::new(Mutex::new(HashMap::<String, ReplayInfo>::new()));
    let stats_replays = Arc::new(Mutex::new(HashMap::<String, ReplayInfo>::new()));
    let mut replay = ReplayInfo::default();
    replay.file = test_replay_path("cached.SC2Replay");
    replay.result = "Victory".to_string();

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
    let shared_replay = shared_cache
        .values()
        .next()
        .expect("shared replay cache should contain a replay");
    let stats_replay = stats_cache
        .values()
        .next()
        .expect("stats replay cache should contain a replay");
    assert_eq!(shared_replay.file, replay.file);
    assert_eq!(stats_replay.file, replay.file);
    assert_eq!(shared_replay.result, replay.result);
    assert_eq!(stats_replay.result, replay.result);
}
