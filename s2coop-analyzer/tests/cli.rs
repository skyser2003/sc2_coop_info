use s2coop_analyzer::cache_overall_stats_detailed_analysis::TestCacheOverallStatsDetailedAnalysisArgs;
use s2coop_analyzer::cli::{AnalyzerCli, Command, GenerateCacheArgs};
use std::path::PathBuf;

#[test]
fn parse_help_when_no_args() {
    let args = vec!["s2coop-analyzer-cli".to_string()];
    let command = AnalyzerCli::parse_args(&args).expect("cli should parse");
    assert_eq!(command, Command::Help);
}

#[test]
fn parse_generate_cache_command() {
    let args = vec![
        "s2coop-analyzer-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--output".to_string(),
        "cache_overall_stats".to_string(),
    ];

    let command = AnalyzerCli::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        Command::GenerateCache(GenerateCacheArgs {
            account_dir: PathBuf::from("fixtures/replays"),
            output_file: PathBuf::from("cache_overall_stats"),
            recent_replay_count: None,
        })
    );
}

#[test]
fn parse_generate_cache_command_with_recent_files() {
    let args = vec![
        "s2coop-analyzer-cli".to_string(),
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--output".to_string(),
        "cache_overall_stats".to_string(),
        "--recent-files".to_string(),
        "100".to_string(),
    ];

    let command = AnalyzerCli::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        Command::GenerateCache(GenerateCacheArgs {
            account_dir: PathBuf::from("fixtures/replays"),
            output_file: PathBuf::from("cache_overall_stats"),
            recent_replay_count: Some(100),
        })
    );
}

#[test]
fn parse_test_cache_overall_stats_detailed_analysis_command() {
    let args = vec![
        "s2coop-analyzer-cli".to_string(),
        "test-cache-overall-stats-detailed-analysis".to_string(),
        "--account-dir".to_string(),
        "fixtures/replays".to_string(),
        "--output".to_string(),
        "generated\\cache_overall_stats.json".to_string(),
        "--original".to_string(),
        "..\\original\\cache_overall_stats".to_string(),
    ];

    let command = AnalyzerCli::parse_args(&args).expect("cli should parse");
    assert_eq!(
        command,
        Command::TestCacheOverallStatsDetailedAnalysis(TestCacheOverallStatsDetailedAnalysisArgs {
            account_dir: Some(PathBuf::from("fixtures/replays")),
            output_file: Some(PathBuf::from("generated\\cache_overall_stats.json")),
            original_output: Some(PathBuf::from("..\\original\\cache_overall_stats")),
            help_requested: false,
        })
    );
}
