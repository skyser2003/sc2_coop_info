use mpq::Archive;
use s2protocol_port::{build_protocol_store, parse_file_with_store, ReplayParseMode};
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

fn read_mpq_file(path: &Path, filename: &str) -> Option<Vec<u8>> {
    let mut archive = Archive::open(path).ok()?;
    let file = archive.open_file(filename).ok()?;
    let size = usize::try_from(file.size()).ok()?;
    let mut data = vec![0_u8; size];
    let read = file.read(&mut archive, &mut data).ok()?;
    data.truncate(read);
    Some(data)
}

#[test]
fn malwarfare_weekly_replay_has_tracker_events() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping tracker-event regression test: no SC2 account directory configured");
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping tracker-event regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let store = build_protocol_store().expect("protocol store should build");
    let parsed = parse_file_with_store(&replay_path, &store, ReplayParseMode::Detailed)
        .expect("full replay parser should read the replay");
    assert!(!parsed.tracker_events().is_empty());

    let raw = read_mpq_file(&replay_path, "replay.tracker.events")
        .expect("tracker stream should be present in replay archive");
    assert!(!raw.is_empty());
    let protocol = store.build(86383).expect("protocol build should exist");
    let events = protocol
        .decode_replay_tracker_events(&raw)
        .expect("exact replay build should decode tracker events");

    assert!(
        !events.is_empty(),
        "expected tracker events for {}",
        replay_path.display()
    );
}
