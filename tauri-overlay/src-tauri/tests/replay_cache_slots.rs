use sco_tauri_overlay::TestHelperOps;
use sco_tauri_overlay::{BackendState, ReplayInfo, TauriOverlayOps};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn replay_with_file(file_name: &str, result: &str) -> ReplayInfo {
    let mut replay = ReplayInfo::default();
    replay.set_file(TestHelperOps::test_replay_path(file_name));
    replay.set_result(result);
    replay
}

#[test]
fn update_analysis_replay_cache_slots_populates_shared_cache() {
    let replays = Arc::new(Mutex::new(HashMap::<String, ReplayInfo>::new()));
    let replay = replay_with_file("cached.SC2Replay", "Victory");

    TauriOverlayOps::update_analysis_replay_cache_slots(&[replay.clone()], &replays);

    let shared_cache = replays
        .lock()
        .expect("replays mutex should not be poisoned")
        .clone();

    assert_eq!(shared_cache.len(), 1);
    let shared_replay = shared_cache
        .values()
        .next()
        .expect("shared replay cache should contain a replay");
    assert_eq!(shared_replay.file(), replay.file());
    assert_eq!(shared_replay.result(), replay.result());
}

#[test]
fn update_analysis_replay_cache_slots_preserves_entries_missing_from_update() {
    let replays = Arc::new(Mutex::new(HashMap::<String, ReplayInfo>::new()));
    let existing = replay_with_file("existing.SC2Replay", "Victory");
    let incoming = replay_with_file("incoming.SC2Replay", "Defeat");

    TauriOverlayOps::update_analysis_replay_cache_slots(&[existing.clone()], &replays);
    TauriOverlayOps::update_analysis_replay_cache_slots(&[incoming.clone()], &replays);

    let shared_cache = replays
        .lock()
        .expect("replays mutex should not be poisoned")
        .clone();
    let shared_paths = shared_cache
        .values()
        .map(|replay| PathBuf::from(replay.file()))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(shared_cache.len(), 2);
    assert!(shared_paths.contains(&PathBuf::from(existing.file())));
    assert!(shared_paths.contains(&PathBuf::from(incoming.file())));
}

#[test]
fn limited_replay_cache_snapshot_does_not_shrink_shared_cache() {
    let state = BackendState::new();
    let existing = replay_with_file("existing.SC2Replay", "Victory");
    let incoming = replay_with_file("incoming.SC2Replay", "Defeat");

    {
        let replay_state = state.get_replay_state();
        let replay_state = replay_state
            .lock()
            .expect("replay state mutex should not be poisoned");
        let replays = replay_state.replays_handle();
        let mut cache = replays
            .lock()
            .expect("replays mutex should not be poisoned");
        cache.insert("existing".to_string(), existing.clone());
        cache.insert("incoming".to_string(), incoming.clone());
    }

    let limited = state.sync_replay_cache_slots(1);

    assert_eq!(limited.len(), 1);
    let replay_state = state.get_replay_state();
    let replay_state = replay_state
        .lock()
        .expect("replay state mutex should not be poisoned");
    let replays = replay_state.replays_handle();
    let cache = replays
        .lock()
        .expect("replays mutex should not be poisoned");

    assert_eq!(cache.len(), 2);
}
