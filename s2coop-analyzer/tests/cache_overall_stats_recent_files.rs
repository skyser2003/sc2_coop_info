use s2coop_analyzer::cache_overall_stats_generator::GenerateCacheConfig;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn write_replay_file(path: &Path) {
    fs::create_dir_all(
        path.parent()
            .expect("test replay path must have parent directory"),
    )
    .expect("failed to create replay directory");
    fs::write(path, b"SC2ReplayTestData").expect("failed to write replay file");
}

#[test]
fn generate_cache_collects_only_requested_recent_replays() {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");
    let replay_dir = account_dir.join("1-S2-1-42");

    let oldest = replay_dir.join("oldest.SC2Replay");
    write_replay_file(&oldest);
    thread::sleep(Duration::from_millis(1100));

    let middle = replay_dir.join("middle.SC2Replay");
    write_replay_file(&middle);
    thread::sleep(Duration::from_millis(1100));

    let newest = replay_dir.join("newest.SC2Replay");
    write_replay_file(&newest);

    let replay_files = GenerateCacheConfig {
        account_dir,
        output_file: temp_dir.path().join("cache_overall_stats"),
        recent_replay_count: Some(2),
    }
    .collect_replay_files();

    let replay_names = replay_files
        .iter()
        .map(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .expect("selected replay path should include a valid file name")
                .to_string()
        })
        .collect::<Vec<String>>();

    assert_eq!(
        replay_names,
        vec![
            "newest.SC2Replay".to_string(),
            "middle.SC2Replay".to_string()
        ]
    );
}
