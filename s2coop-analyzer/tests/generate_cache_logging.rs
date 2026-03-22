use s2coop_analyzer::cli::run_cli_with_logger;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
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
fn cli_generate_cache_emits_legacy_style_logs() {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let account_dir = temp_dir.path().join("Accounts");
    for index in 0..12 {
        let account_id = if index % 2 == 0 {
            "2-S2-1-111"
        } else {
            "1-S2-1-42"
        };
        let replay_name = format!("Replay_{index:02}.SC2Replay");
        write_replay_file(&account_dir.join(account_id).join(replay_name));
    }
    let output_file = temp_dir.path().join("cache_overall_stats");
    let args = vec![
        "s2coop-analyzer-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        account_dir.display().to_string(),
        "--output".to_string(),
        output_file.display().to_string(),
    ];

    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let logger_messages = Arc::clone(&messages);
    let logger = move |message: String| {
        logger_messages
            .lock()
            .expect("logger messages lock should succeed")
            .push(message);
    };

    let output = run_cli_with_logger(&args, &logger).expect("cli should run");
    assert!(output.contains("Generated cache_overall_stats"));
    assert!(output_file.is_file());

    let messages = messages
        .lock()
        .expect("logger messages lock should succeed");
    assert!(
        !messages.is_empty(),
        "the logger should emit at least one message"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("Detailed analysis completed! ")),
        "completion log should be emitted"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("Detailed analysis completed in ")),
        "elapsed-time log should be emitted"
    );
}
