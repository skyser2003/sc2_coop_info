mod common;

use s2coop_analyzer::cache_overall_stats_generator::pretty_output_path;
use s2coop_analyzer::detailed_replay_analysis::GenerateCacheConfig;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write_replay_file(path: &Path) {
    fs::create_dir_all(
        path.parent()
            .expect("test replay path must have parent directory"),
    )
    .expect("failed to create replay directory");
    fs::write(path, b"SC2ReplayTestData").expect("failed to write replay file");
}

fn read_json(path: &Path) -> serde_json::Value {
    let payload = fs::read_to_string(path).expect("json file should be readable");
    serde_json::from_str(&payload).expect("json file should deserialize")
}

#[test]
fn generate_cache_writes_pretty_sibling_file() {
    let resources = common::load_replay_resources();
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");
    write_replay_file(&account_dir.join("1-S2-1-42").join("single.SC2Replay"));

    let output_file = temp_dir.path().join("cache_overall_stats");
    let summary = GenerateCacheConfig::new(account_dir, output_file.clone())
        .generate(&resources)
        .expect("cache generation should succeed");

    let pretty_file = pretty_output_path(summary.output_file());
    assert!(pretty_file.is_file(), "pretty cache file should be created");
    assert_eq!(read_json(summary.output_file()), read_json(&pretty_file));

    let pretty_text = fs::read_to_string(&pretty_file).expect("pretty file should be readable");
    assert!(
        pretty_text.ends_with('\n'),
        "pretty cache file should end with a newline"
    );
}
