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

#[test]
fn replay_parse_mode_controls_event_streams() {
    let Some(account_dir) = resolve_account_dir() else {
        eprintln!("skipping parse-mode regression test: no SC2 account directory configured");
        return;
    };
    let Some(replay_path) = find_replay(&account_dir, "잘못된 전쟁 (63).SC2Replay") else {
        eprintln!(
            "skipping parse-mode regression test: replay not found under {}",
            account_dir.display()
        );
        return;
    };

    let store = build_protocol_store().expect("protocol store should build");
    let simple = parse_file_with_store(&replay_path, &store, ReplayParseMode::Simple)
        .expect("simple replay parser should read the replay");
    let detailed = parse_file_with_store(&replay_path, &store, ReplayParseMode::Detailed)
        .expect("detailed replay parser should read the replay");

    assert!(simple.game_events().is_empty());
    assert!(simple.tracker_events().is_empty());
    assert!(!detailed.game_events().is_empty());
    assert!(!detailed.tracker_events().is_empty());
}
