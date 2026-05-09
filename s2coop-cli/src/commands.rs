use clap::builder::StringValueParser;
use clap::{Arg, ArgAction, Command as ClapCommand, error::ErrorKind};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    GenerateCache(GenerateCacheArgs),
    TestCacheOverallStatsDetailedAnalysis(TestCacheOverallStatsArgs),
    CompareCacheGeneration(CompareCacheGenerationArgs),
    CompareCacheGenerationAlternating(CompareCacheGenerationAlternatingArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateCacheArgs {
    account_dir: PathBuf,
    output_file: PathBuf,
    recent_replay_count: Option<usize>,
    worker_count: usize,
}

impl GenerateCacheArgs {
    pub fn new(
        account_dir: PathBuf,
        output_file: PathBuf,
        recent_replay_count: Option<usize>,
        worker_count: usize,
    ) -> Self {
        Self {
            account_dir,
            output_file,
            recent_replay_count,
            worker_count,
        }
    }

    pub fn account_dir(&self) -> &Path {
        &self.account_dir
    }

    pub fn output_file(&self) -> &Path {
        &self.output_file
    }

    pub fn recent_replay_count(&self) -> Option<usize> {
        self.recent_replay_count
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestCacheOverallStatsArgs {
    account_dir: Option<PathBuf>,
    output_file: Option<PathBuf>,
    original_output: Option<PathBuf>,
}

impl TestCacheOverallStatsArgs {
    pub fn new(
        account_dir: Option<PathBuf>,
        output_file: Option<PathBuf>,
        original_output: Option<PathBuf>,
    ) -> Self {
        Self {
            account_dir,
            output_file,
            original_output,
        }
    }

    pub fn account_dir(&self) -> Option<&Path> {
        self.account_dir.as_deref()
    }

    pub fn output_file(&self) -> Option<&Path> {
        self.output_file.as_deref()
    }

    pub fn original_output(&self) -> Option<&Path> {
        self.original_output.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareCacheGenerationArgs {
    comparison_ref: String,
    recent_replay_count: Option<usize>,
    runs: usize,
    workers: Option<usize>,
    warmup_runs_per_variant: usize,
    analyzer_timings: bool,
    keep_artifacts: bool,
}

impl CompareCacheGenerationArgs {
    pub fn new(
        comparison_ref: String,
        recent_replay_count: Option<usize>,
        runs: usize,
        workers: Option<usize>,
        warmup_runs_per_variant: usize,
        analyzer_timings: bool,
        keep_artifacts: bool,
    ) -> Self {
        Self {
            comparison_ref,
            recent_replay_count,
            runs,
            workers,
            warmup_runs_per_variant,
            analyzer_timings,
            keep_artifacts,
        }
    }

    pub fn comparison_ref(&self) -> &str {
        &self.comparison_ref
    }

    pub fn recent_replay_count(&self) -> Option<usize> {
        self.recent_replay_count
    }

    pub fn runs(&self) -> usize {
        self.runs
    }

    pub fn workers(&self) -> Option<usize> {
        self.workers
    }

    pub fn warmup_runs_per_variant(&self) -> usize {
        self.warmup_runs_per_variant
    }

    pub fn analyzer_timings(&self) -> bool {
        self.analyzer_timings
    }

    pub fn keep_artifacts(&self) -> bool {
        self.keep_artifacts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareCacheGenerationAlternatingArgs {
    comparison_ref: String,
    recent_replay_count: Option<usize>,
    workers: usize,
    analyzer_timings: bool,
    keep_artifacts: bool,
}

impl CompareCacheGenerationAlternatingArgs {
    pub fn new(
        comparison_ref: String,
        recent_replay_count: Option<usize>,
        workers: usize,
        analyzer_timings: bool,
        keep_artifacts: bool,
    ) -> Self {
        Self {
            comparison_ref,
            recent_replay_count,
            workers,
            analyzer_timings,
            keep_artifacts,
        }
    }

    pub fn into_compare_args(self) -> CompareCacheGenerationArgs {
        CompareCacheGenerationArgs::new(
            self.comparison_ref,
            self.recent_replay_count,
            10,
            Some(self.workers),
            1,
            self.analyzer_timings,
            self.keep_artifacts,
        )
    }

    pub fn comparison_ref(&self) -> &str {
        &self.comparison_ref
    }

    pub fn recent_replay_count(&self) -> Option<usize> {
        self.recent_replay_count
    }

    pub fn workers(&self) -> usize {
        self.workers
    }

    pub fn analyzer_timings(&self) -> bool {
        self.analyzer_timings
    }

    pub fn keep_artifacts(&self) -> bool {
        self.keep_artifacts
    }
}

#[derive(Debug, Error)]
pub enum CliParseError {
    #[error(transparent)]
    Clap(#[from] clap::Error),
    #[error("internal parser error: missing parsed subcommand")]
    MissingParsedSubcommand,
}

pub struct CliArguments;

impl CliArguments {
    pub fn default_generate_cache_worker_count() -> usize {
        let cpu_count = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        std::cmp::max(1, cpu_count / 2)
    }

    pub fn default_cargo_jobs() -> usize {
        Self::default_generate_cache_worker_count()
    }

    pub fn parse_args(raw_args: &[String]) -> Result<CliCommand, CliParseError> {
        Self::parse_from(raw_args.iter().cloned())
    }

    pub fn parse_from<I, T>(raw_args: I) -> Result<CliCommand, CliParseError>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let args = raw_args
            .into_iter()
            .map(Into::into)
            .collect::<Vec<OsString>>();
        if args.len() <= 1 {
            return Ok(CliCommand::Help);
        }

        if args
            .get(1)
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == "help")
        {
            return Ok(CliCommand::Help);
        }

        let matches = Self::build_command().try_get_matches_from(args)?;
        let Some((name, sub_matches)) = matches.subcommand() else {
            return Ok(CliCommand::Help);
        };

        match name {
            "generate-cache" => Ok(CliCommand::GenerateCache(GenerateCacheArgs::new(
                required_path(sub_matches, "account-dir")?,
                required_path(sub_matches, "output")?,
                copied_optional_usize(sub_matches, "recent-files"),
                copied_optional_usize(sub_matches, "workers")
                    .unwrap_or_else(CliArguments::default_generate_cache_worker_count),
            ))),
            "test-cache-overall-stats-detailed-analysis" => Ok(
                CliCommand::TestCacheOverallStatsDetailedAnalysis(TestCacheOverallStatsArgs::new(
                    optional_path(sub_matches, "account-dir"),
                    optional_path(sub_matches, "output"),
                    optional_path(sub_matches, "original"),
                )),
            ),
            "compare-cache-generation" => Ok(CliCommand::CompareCacheGeneration(
                CompareCacheGenerationArgs::new(
                    copied_required_string(sub_matches, "comparison-ref")?,
                    copied_optional_usize(sub_matches, "recent-files"),
                    copied_required_usize(sub_matches, "runs")?,
                    copied_optional_usize(sub_matches, "workers"),
                    copied_required_usize(sub_matches, "warmup-runs-per-variant")?,
                    sub_matches.get_flag("analyzer-timings"),
                    sub_matches.get_flag("keep-artifacts"),
                ),
            )),
            "compare-cache-generation-alternating" => {
                Ok(CliCommand::CompareCacheGenerationAlternating(
                    CompareCacheGenerationAlternatingArgs::new(
                        copied_required_string(sub_matches, "comparison-ref")?,
                        copied_optional_usize(sub_matches, "recent-files"),
                        copied_required_usize(sub_matches, "workers")?,
                        !sub_matches.get_flag("no-analyzer-timings"),
                        sub_matches.get_flag("keep-artifacts"),
                    ),
                ))
            }
            _ => Err(CliParseError::MissingParsedSubcommand),
        }
    }

    pub fn usage_text() -> String {
        Self::build_command().render_long_help().to_string()
    }

    pub fn is_help_error(error: &clap::Error) -> bool {
        matches!(
            error.kind(),
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
        )
    }

    fn build_command() -> ClapCommand {
        ClapCommand::new("s2coop-cli")
            .about("StarCraft II co-op replay analysis CLI")
            .disable_help_subcommand(true)
            .subcommand(generate_cache_command())
            .subcommand(test_cache_overall_stats_command())
            .subcommand(compare_cache_generation_command())
            .subcommand(compare_cache_generation_alternating_command())
    }
}

fn generate_cache_command() -> ClapCommand {
    ClapCommand::new("generate-cache")
        .about("Generate deterministic cache_overall_stats entries")
        .arg(path_arg("account-dir", "DIR").required(true))
        .arg(path_arg("output", "FILE").required(true))
        .arg(positive_usize_arg("recent-files", "COUNT").required(false))
        .arg(positive_usize_arg("workers", "COUNT").required(false))
}

fn test_cache_overall_stats_command() -> ClapCommand {
    ClapCommand::new("test-cache-overall-stats-detailed-analysis")
        .about("Run detailed-analysis cache parity validation")
        .arg(path_arg("account-dir", "DIR").required(false))
        .arg(path_arg("output", "FILE").required(false))
        .arg(path_arg("original", "FILE").required(false))
}

fn compare_cache_generation_command() -> ClapCommand {
    ClapCommand::new("compare-cache-generation")
        .about("Compare cache generation between the current workspace and another git ref")
        .arg(
            Arg::new("comparison-ref")
                .long("comparison-ref")
                .alias("head-ref")
                .value_name("REF")
                .value_parser(StringValueParser::new())
                .default_value("HEAD"),
        )
        .arg(positive_usize_arg("recent-files", "COUNT").required(false))
        .arg(
            positive_usize_arg("runs", "COUNT")
                .required(false)
                .default_value("1"),
        )
        .arg(positive_usize_arg("workers", "COUNT").required(false))
        .arg(
            positive_usize_arg("warmup-runs-per-variant", "COUNT")
                .required(false)
                .default_value("1"),
        )
        .arg(
            Arg::new("analyzer-timings")
                .long("analyzer-timings")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("keep-artifacts")
                .long("keep-artifacts")
                .action(ArgAction::SetTrue),
        )
}

fn compare_cache_generation_alternating_command() -> ClapCommand {
    ClapCommand::new("compare-cache-generation-alternating")
        .about("Run the standard alternating cache-generation comparison preset")
        .arg(
            Arg::new("comparison-ref")
                .long("comparison-ref")
                .alias("head-ref")
                .value_name("REF")
                .value_parser(StringValueParser::new())
                .default_value("HEAD"),
        )
        .arg(positive_usize_arg("recent-files", "COUNT").required(false))
        .arg(
            positive_usize_arg("workers", "COUNT")
                .required(false)
                .default_value("8"),
        )
        .arg(
            Arg::new("no-analyzer-timings")
                .long("no-analyzer-timings")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("keep-artifacts")
                .long("keep-artifacts")
                .action(ArgAction::SetTrue),
        )
}

fn path_arg(id: &'static str, value_name: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .value_name(value_name)
        .value_parser(clap::value_parser!(PathBuf))
}

fn positive_usize_arg(id: &'static str, value_name: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .value_name(value_name)
        .value_parser(parse_positive_usize)
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| format!("expected a positive integer, got '{value}': {error}"))?;
    if parsed == 0 {
        return Err("expected a positive integer greater than zero".to_string());
    }
    Ok(parsed)
}

fn required_path(matches: &clap::ArgMatches, id: &'static str) -> Result<PathBuf, CliParseError> {
    matches
        .get_one::<PathBuf>(id)
        .cloned()
        .ok_or(CliParseError::MissingParsedSubcommand)
}

fn optional_path(matches: &clap::ArgMatches, id: &'static str) -> Option<PathBuf> {
    matches.get_one::<PathBuf>(id).cloned()
}

fn copied_required_usize(
    matches: &clap::ArgMatches,
    id: &'static str,
) -> Result<usize, CliParseError> {
    let parsed = matches
        .get_one::<usize>(id)
        .copied()
        .ok_or(CliParseError::MissingParsedSubcommand)?;
    Ok(parsed)
}

fn copied_optional_usize(matches: &clap::ArgMatches, id: &'static str) -> Option<usize> {
    matches.get_one::<usize>(id).copied()
}

fn copied_required_string(
    matches: &clap::ArgMatches,
    id: &'static str,
) -> Result<String, CliParseError> {
    matches
        .get_one::<String>(id)
        .cloned()
        .ok_or(CliParseError::MissingParsedSubcommand)
}
