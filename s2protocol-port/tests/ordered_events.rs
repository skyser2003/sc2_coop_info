use s2protocol_port::{
    build_protocol_store, parse_file_with_store, parse_file_with_store_ordered_events,
    parse_ordered_events_with_store, parse_ordered_events_with_store_filtered, ReplayEvent,
    ReplayParseMode,
};
use std::fs;
use std::path::{Path, PathBuf};

fn read_env_file_value(env_file: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(env_file).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((current_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if current_key.trim() != key {
            continue;
        }
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        return Some(value.to_string());
    }
    None
}

fn resolve_account_dir() -> Option<PathBuf> {
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .to_path_buf();
    let env_path = repo_root.join(".env");
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Some(value) = read_env_file_value(&env_path, key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    None
}

fn find_replay(root: &Path, replay_name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if metadata.is_file()
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value == replay_name)
            {
                return Some(path);
            }
        }
    }

    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventKey {
    kind: &'static str,
    game_loop: i64,
    event_id: u32,
    event: String,
}

fn ordered_key(event: &ReplayEvent) -> EventKey {
    match event {
        ReplayEvent::Game(event) => EventKey {
            kind: "game",
            game_loop: event.game_loop,
            event_id: event.event_id,
            event: event.event.clone(),
        },
        ReplayEvent::Tracker(event) => EventKey {
            kind: "tracker",
            game_loop: event.game_loop,
            event_id: event.event_id,
            event: event.event.clone(),
        },
    }
}

#[test]
fn ordered_event_parse_matches_split_detailed_events() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping ordered-event regression test: no SC2 account directory configured");
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping ordered-event regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let store = build_protocol_store().expect("protocol store should build");
    let split = parse_file_with_store(&replay_path, &store, ReplayParseMode::Detailed)
        .expect("split detailed replay parser should read the replay");
    let ordered = parse_file_with_store_ordered_events(&replay_path, &store)
        .expect("ordered replay parser should read the replay");

    let mut expected = Vec::new();
    expected.extend(split.game_events().iter().map(|event| EventKey {
        kind: "game",
        game_loop: event.game_loop,
        event_id: event.event_id,
        event: event.event.clone(),
    }));
    expected.extend(split.tracker_events().iter().map(|event| EventKey {
        kind: "tracker",
        game_loop: event.game_loop,
        event_id: event.event_id,
        event: event.event.clone(),
    }));
    expected.sort_by_key(|event| event.game_loop);

    let actual = ordered.events().iter().map(ordered_key).collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn events_only_parse_matches_ordered_replay_events() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping events-only regression test: no SC2 account directory configured");
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping events-only regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let store = build_protocol_store().expect("protocol store should build");
    let ordered = parse_file_with_store_ordered_events(&replay_path, &store)
        .expect("ordered replay parser should read the replay");
    let events_only = parse_ordered_events_with_store(&replay_path, &store)
        .expect("events-only replay parser should read the replay");

    let expected = ordered.events().iter().map(ordered_key).collect::<Vec<_>>();
    let actual = events_only.iter().map(ordered_key).collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn filtered_events_only_parse_matches_filtered_ordered_replay_events() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!(
            "skipping filtered events-only regression test: no SC2 account directory configured"
        );
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping filtered events-only regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let include_event = |event: &str| {
        matches!(
            event,
            "NNet.Game.SGameUserLeaveEvent"
                | "NNet.Game.SSelectionDeltaEvent"
                | "NNet.Game.STriggerDialogControlEvent"
                | "NNet.Game.SCmdEvent"
                | "NNet.Game.SCmdUpdateTargetUnitEvent"
                | "NNet.Replay.Tracker.SPlayerStatsEvent"
                | "NNet.Replay.Tracker.SUpgradeEvent"
                | "NNet.Replay.Tracker.SUnitBornEvent"
                | "NNet.Replay.Tracker.SUnitInitEvent"
                | "NNet.Replay.Tracker.SUnitTypeChangeEvent"
                | "NNet.Replay.Tracker.SUnitOwnerChangeEvent"
                | "NNet.Replay.Tracker.SUnitDiedEvent"
        )
    };

    let store = build_protocol_store().expect("protocol store should build");
    let ordered = parse_file_with_store_ordered_events(&replay_path, &store)
        .expect("ordered replay parser should read the replay");
    let filtered = parse_ordered_events_with_store_filtered(&replay_path, &store, include_event)
        .expect("filtered events-only replay parser should read the replay");

    let expected = ordered
        .events()
        .iter()
        .filter(|event| include_event(event._event()))
        .map(ordered_key)
        .collect::<Vec<_>>();
    let actual = filtered.iter().map(ordered_key).collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
