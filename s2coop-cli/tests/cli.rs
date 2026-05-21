use s2coop_analyzer::cache_overall_stats_detailed_analysis::TestCacheOverallStatsDetailedAnalysisArgs;
use s2coop_cli::commands::{
    CliArguments, CliCommand, CompareCacheGenerationAlternatingArgs, CompareCacheGenerationArgs,
    GenerateCacheArgs, TestCacheOverallStatsArgs,
};
use s2coop_cli::comparison::{
    CliBinarySpec, ComparisonVariant, RecentReplaySubset, alternating_variant,
    parse_timing_metric_seconds,
};
use s2coop_cli::env_file::EnvFileLoader;
use s2coop_cli::progress::{CacheProgressUpdate, CliProgressBar};
use std::fs;
use std::path::{Path, PathBuf};
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
fn parse_help_when_no_args() {
    let args = vec!["s2coop-cli".to_string()];
    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(command, CliCommand::Help);
}

#[test]
fn parse_generate_cache_command() {
    let args = vec![
        "s2coop-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
    ];

    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        CliCommand::GenerateCache(GenerateCacheArgs::new(
            PathBuf::from("fixtures/replays"),
            None,
            CliArguments::default_generate_cache_worker_count(),
        ))
    );
}

#[test]
fn parse_generate_cache_command_with_recent_files() {
    let args = vec![
        "s2coop-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--recent-files".to_string(),
        "100".to_string(),
    ];

    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        CliCommand::GenerateCache(GenerateCacheArgs::new(
            PathBuf::from("fixtures/replays"),
            Some(100),
            CliArguments::default_generate_cache_worker_count(),
        ))
    );
}

#[test]
fn parse_generate_cache_command_with_workers() {
    let args = vec![
        "s2coop-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--workers".to_string(),
        "8".to_string(),
    ];

    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        CliCommand::GenerateCache(GenerateCacheArgs::new(
            PathBuf::from("fixtures/replays"),
            None,
            8,
        ))
    );
}

#[test]
fn parse_generate_cache_command_rejects_output_option() {
    let args = vec![
        "s2coop-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--output".to_string(),
        "cache_overall_stats.json".to_string(),
    ];

    let error = CliArguments::parse_args(&args).expect_err("output option should be removed");
    assert!(error.to_string().contains("unexpected argument"));
}

#[test]
fn parse_test_cache_overall_stats_detailed_analysis_command() {
    let args = vec![
        "s2coop-cli".to_string(),
        "test-cache-overall-stats-detailed-analysis".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--output".to_string(),
        "generated\\cache_overall_stats.json".to_string(),
        "--original".to_string(),
        "..\\original\\cache_overall_stats".to_string(),
    ];

    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        CliCommand::TestCacheOverallStatsDetailedAnalysis(TestCacheOverallStatsArgs::new(
            Some(PathBuf::from("fixtures/replays")),
            Some(PathBuf::from("generated\\cache_overall_stats.json")),
            Some(PathBuf::from("..\\original\\cache_overall_stats")),
        ))
    );

    let analyzer_args = TestCacheOverallStatsDetailedAnalysisArgs {
        account_dir: Some(PathBuf::from("fixtures/replays")),
        output_file: Some(PathBuf::from("generated\\cache_overall_stats.json")),
        original_output: Some(PathBuf::from("..\\original\\cache_overall_stats")),
        help_requested: false,
    };
    assert_eq!(
        analyzer_args.account_dir,
        Some(PathBuf::from("fixtures/replays"))
    );
}

#[test]
fn parse_compare_cache_generation_command() {
    let args = vec![
        "s2coop-cli".to_string(),
        "compare-cache-generation".to_string(),
        "--comparison-ref".to_string(),
        "main".to_string(),
        "--recent-files".to_string(),
        "25".to_string(),
        "--runs".to_string(),
        "4".to_string(),
        "--workers".to_string(),
        "6".to_string(),
        "--warmup-runs-per-variant".to_string(),
        "2".to_string(),
        "--analyzer-timings".to_string(),
        "--keep-artifacts".to_string(),
    ];

    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        CliCommand::CompareCacheGeneration(CompareCacheGenerationArgs::new(
            "main".to_string(),
            Some(25),
            4,
            Some(6),
            2,
            true,
            true,
        ))
    );
}

#[test]
fn parse_compare_cache_generation_alternating_command() {
    let args = vec![
        "s2coop-cli".to_string(),
        "compare-cache-generation-alternating".to_string(),
        "--comparison-ref".to_string(),
        "HEAD~1".to_string(),
        "--recent-files".to_string(),
        "10".to_string(),
        "--workers".to_string(),
        "3".to_string(),
        "--no-analyzer-timings".to_string(),
        "--keep-artifacts".to_string(),
    ];

    let command = CliArguments::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        CliCommand::CompareCacheGenerationAlternating(CompareCacheGenerationAlternatingArgs::new(
            "HEAD~1".to_string(),
            Some(10),
            3,
            false,
            true,
        ))
    );
}

#[test]
fn parser_rejects_zero_numeric_values() {
    let args = vec![
        "s2coop-cli".to_string(),
        "compare-cache-generation".to_string(),
        "--runs".to_string(),
        "0".to_string(),
    ];

    let error = CliArguments::parse_args(&args).expect_err("zero should fail validation");
    assert!(error.to_string().contains("positive integer"));
}

#[test]
fn progress_update_parses_eta_and_counts() {
    let parsed =
        CliProgressBar::parse_update("Estimated remaining time: 00:01:03\nRunning... 4/12 replays")
            .expect("progress should parse");

    assert_eq!(
        parsed,
        CacheProgressUpdate::new(4, 12, Some("00:01:03".to_string()))
    );
}

#[test]
fn alternating_run_order_matches_script_pattern() {
    let variants = (0..8).map(alternating_variant).collect::<Vec<_>>();
    assert_eq!(
        variants,
        vec![
            ComparisonVariant::Comparison,
            ComparisonVariant::Current,
            ComparisonVariant::Current,
            ComparisonVariant::Comparison,
            ComparisonVariant::Comparison,
            ComparisonVariant::Current,
            ComparisonVariant::Current,
            ComparisonVariant::Comparison,
        ]
    );
}

#[test]
fn timing_metric_parser_reads_named_seconds() {
    let output = "hotspot hints: total=12.345s decode_ordered=7.500s detailed_report=1.250s";

    assert_eq!(parse_timing_metric_seconds(output, "total"), Some(12.345));
    assert_eq!(
        parse_timing_metric_seconds(output, "decode_ordered"),
        Some(7.5)
    );
    assert_eq!(
        parse_timing_metric_seconds(output, "detailed_report"),
        Some(1.25)
    );
    assert_eq!(parse_timing_metric_seconds(output, "missing"), None);
}

#[test]
fn env_file_parser_supports_exports_and_quotes() {
    let assignments = EnvFileLoader::parse_content(
        "\n# comment\nSC2_ACCOUNT_PATH=\"/tmp/accounts\"\nexport SC2_ACCOUNT_PATH_LINUX='/mnt/accounts'\n",
    );

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].name(), "SC2_ACCOUNT_PATH");
    assert_eq!(assignments[0].value(), "/tmp/accounts");
    assert_eq!(assignments[1].name(), "SC2_ACCOUNT_PATH_LINUX");
    assert_eq!(assignments[1].value(), "/mnt/accounts");
}

#[test]
fn recent_replay_subset_copies_newest_files_with_relative_paths() {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let source = temp_dir.path().join("source");
    let destination = temp_dir.path().join("destination");
    let old_replay = source.join("1-S2-1-1").join("old.SC2Replay");
    let middle_replay = source.join("1-S2-1-1").join("middle.SC2Replay");
    let new_replay = source.join("1-S2-1-2").join("new.SC2Replay");

    write_replay_file(&old_replay);
    thread::sleep(Duration::from_millis(20));
    write_replay_file(&middle_replay);
    thread::sleep(Duration::from_millis(20));
    write_replay_file(&new_replay);

    let copied = RecentReplaySubset::copy_recent_replays(&source, &destination, 2)
        .expect("subset copy should succeed");

    assert_eq!(copied, 2);
    assert!(destination.join("1-S2-1-2").join("new.SC2Replay").is_file());
    assert!(
        destination
            .join("1-S2-1-1")
            .join("middle.SC2Replay")
            .is_file()
    );
    assert!(!destination.join("1-S2-1-1").join("old.SC2Replay").is_file());
}

#[test]
fn comparison_binary_spec_falls_back_to_legacy_analyzer_cli() {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let worktree = temp_dir.path();
    fs::create_dir_all(worktree.join("s2coop-analyzer")).expect("failed to create legacy dir");
    fs::write(
        worktree.join("s2coop-analyzer").join("Cargo.toml"),
        "[package]\nname = \"s2coop-analyzer\"\n",
    )
    .expect("failed to write legacy manifest");

    let spec = CliBinarySpec::for_comparison_worktree(worktree);

    assert_eq!(spec.bin_name(), "s2coop-analyzer-cli");
    assert!(spec.manifest_path().ends_with("s2coop-analyzer/Cargo.toml"));
}
