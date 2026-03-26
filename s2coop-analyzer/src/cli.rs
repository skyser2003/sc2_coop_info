use crate::cache_overall_stats_detailed_analysis::{
    run_test_cache_overall_stats_detailed_analysis, TestCacheOverallStatsDetailedAnalysisArgs,
    TestCacheOverallStatsDetailedAnalysisError,
};
use crate::cache_overall_stats_generator::{
    generate_cache_overall_stats, generate_cache_overall_stats_with_logger, GenerateCacheConfig,
    GenerateCacheError,
};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheArgs {
    pub account_dir: PathBuf,
    pub output_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    GenerateCache(GenerateCacheArgs),
    TestCacheOverallStatsDetailedAnalysis(TestCacheOverallStatsDetailedAnalysisArgs),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CliParseError {
    #[error("unsupported command: {0}")]
    UnsupportedCommand(String),
    #[error("missing value for argument: {0}")]
    MissingArgumentValue(String),
    #[error("unknown argument for generate-cache: {0}")]
    UnknownGenerateCacheArgument(String),
    #[error("unknown argument for test-cache-overall-stats-detailed-analysis: {0}")]
    UnknownTestCacheOverallStatsDetailedAnalysisArgument(String),
    #[error("missing required arguments: {0}")]
    MissingRequiredArguments(String),
}

#[derive(Debug, Error)]
pub enum CliRunError {
    #[error(transparent)]
    Parse(#[from] CliParseError),
    #[error(transparent)]
    Generate(#[from] GenerateCacheError),
    #[error(transparent)]
    TestCacheOverallStatsDetailedAnalysis(#[from] TestCacheOverallStatsDetailedAnalysisError),
}

pub fn parse_cli_args(raw_args: &[String]) -> Result<Command, CliParseError> {
    if raw_args.len() <= 1 {
        return Ok(Command::Help);
    }

    let command = raw_args[1].as_str();
    match command {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "generate-cache" => parse_generate_cache_args(&raw_args[2..]).map(Command::GenerateCache),
        "test-cache-overall-stats-detailed-analysis" => {
            parse_test_cache_overall_stats_detailed_analysis_args(&raw_args[2..])
                .map(Command::TestCacheOverallStatsDetailedAnalysis)
        }
        other => Err(CliParseError::UnsupportedCommand(other.to_string())),
    }
}

fn parse_generate_cache_args(args: &[String]) -> Result<GenerateCacheArgs, CliParseError> {
    let mut account_dir: Option<PathBuf> = None;
    let mut output_file: Option<PathBuf> = None;

    let mut index = 0_usize;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "-h" | "--help" => {
                return Ok(GenerateCacheArgs {
                    account_dir: PathBuf::new(),
                    output_file: PathBuf::new(),
                });
            }
            "--account-dir" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliParseError::MissingArgumentValue(flag.to_string()));
                };
                account_dir = Some(PathBuf::from(value));
                index += 2;
            }
            "--output" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliParseError::MissingArgumentValue(flag.to_string()));
                };
                output_file = Some(PathBuf::from(value));
                index += 2;
            }
            other => {
                return Err(CliParseError::UnknownGenerateCacheArgument(
                    other.to_string(),
                ));
            }
        }
    }

    let missing = [
        ("--account-dir", account_dir.is_none()),
        ("--output", output_file.is_none()),
    ]
    .into_iter()
    .filter_map(|(name, is_missing)| is_missing.then_some(name))
    .collect::<Vec<&str>>();

    if !missing.is_empty() {
        return Err(CliParseError::MissingRequiredArguments(missing.join(", ")));
    }

    Ok(GenerateCacheArgs {
        account_dir: account_dir.expect("validated account_dir"),
        output_file: output_file.expect("validated output_file"),
    })
}

fn parse_test_cache_overall_stats_detailed_analysis_args(
    args: &[String],
) -> Result<TestCacheOverallStatsDetailedAnalysisArgs, CliParseError> {
    let mut parsed = TestCacheOverallStatsDetailedAnalysisArgs::default();

    let mut index = 0_usize;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "-h" | "--help" => {
                parsed.help_requested = true;
                return Ok(parsed);
            }
            "--account-dir" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliParseError::MissingArgumentValue(flag.to_string()));
                };
                parsed.account_dir = Some(PathBuf::from(value));
                index += 2;
            }
            "--output" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliParseError::MissingArgumentValue(flag.to_string()));
                };
                parsed.output_file = Some(PathBuf::from(value));
                index += 2;
            }
            "--original" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliParseError::MissingArgumentValue(flag.to_string()));
                };
                parsed.original_output = Some(PathBuf::from(value));
                index += 2;
            }
            other => {
                return Err(
                    CliParseError::UnknownTestCacheOverallStatsDetailedAnalysisArgument(
                        other.to_string(),
                    ),
                );
            }
        }
    }

    Ok(parsed)
}

pub fn run_cli(raw_args: &[String]) -> Result<String, CliRunError> {
    run_cli_impl(raw_args, None)
}

pub fn run_cli_with_logger(
    raw_args: &[String],
    logger: &(dyn Fn(String) + Send + Sync),
) -> Result<String, CliRunError> {
    run_cli_impl(raw_args, Some(logger))
}

fn run_cli_impl(
    raw_args: &[String],
    logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
) -> Result<String, CliRunError> {
    match parse_cli_args(raw_args)? {
        Command::Help => Ok(usage_text()),
        Command::GenerateCache(args) => {
            if args.account_dir.as_os_str().is_empty() || args.output_file.as_os_str().is_empty() {
                return Ok(usage_text());
            }

            let config = GenerateCacheConfig {
                account_dir: args.account_dir,
                output_file: args.output_file,
            };
            let summary = if let Some(logger) = logger {
                generate_cache_overall_stats_with_logger(&config, logger)?
            } else {
                generate_cache_overall_stats(&config)?
            };

            Ok(format!(
                "Generated cache_overall_stats with {} replay entr{} at {}",
                summary.scanned_replays,
                if summary.scanned_replays == 1 {
                    "y"
                } else {
                    "ies"
                },
                summary.output_file.display()
            ))
        }
        Command::TestCacheOverallStatsDetailedAnalysis(args) => {
            if args.help_requested {
                return Ok(usage_text());
            }

            run_test_cache_overall_stats_detailed_analysis(&args, logger).map_err(Into::into)
        }
    }
}

pub fn usage_text() -> String {
    [
        "Usage:",
        "  s2coop-analyzer-cli generate-cache --account-dir <DIR> --output <FILE>",
        "  s2coop-analyzer-cli test-cache-overall-stats-detailed-analysis [--account-dir <DIR>] [--output <FILE>] [--original <FILE>]",
        "",
        "Notes:",
        "  - This generates deterministic cache_overall_stats entries.",
        "  - Detailed replay analysis is enabled for each replay file.",
        "  - test-cache-overall-stats-detailed-analysis defaults to .env SC2 account paths and ../original/cache_overall_stats.",
    ]
    .join("\n")
}
