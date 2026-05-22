use crate::commands::{CliArguments, CliCommand, CliParseError};
use crate::comparison::{
    CacheGenerationComparisonConfig, CacheGenerationComparisonRunner, ComparisonError,
    SystemCommandRunner,
};
use crate::env_file::{EnvFileError, EnvFileLoader};
use s2coop_analyzer::cache_overall_stats_detailed_analysis::{
    CacheOverallStatsDetailedAnalysis, TestCacheOverallStatsDetailedAnalysisArgs,
    TestCacheOverallStatsDetailedAnalysisError,
};
use s2coop_analyzer::detailed_replay_analysis::{
    DetailedReplayAnalyzer, GenerateCacheConfig, GenerateCacheError, GenerateCacheRuntimeOptions,
    ReplayAnalysisResources,
};
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliRunError {
    #[error(transparent)]
    Parse(#[from] CliParseError),
    #[error(transparent)]
    Env(#[from] EnvFileError),
    #[error(transparent)]
    Generate(#[from] GenerateCacheError),
    #[error(transparent)]
    TestCacheOverallStatsDetailedAnalysis(#[from] TestCacheOverallStatsDetailedAnalysisError),
    #[error(transparent)]
    Compare(#[from] ComparisonError),
}

pub struct CliApplication;

impl CliApplication {
    pub fn run(raw_args: &[String]) -> Result<String, CliRunError> {
        Self::run_impl(raw_args, None)
    }

    pub fn run_with_logger(
        raw_args: &[String],
        logger: &(dyn Fn(String) + Send + Sync),
    ) -> Result<String, CliRunError> {
        Self::run_impl(raw_args, Some(logger))
    }

    fn run_impl(
        raw_args: &[String],
        logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
    ) -> Result<String, CliRunError> {
        let command = match CliArguments::parse_args(raw_args) {
            Ok(command) => command,
            Err(CliParseError::Clap(error)) if CliArguments::is_help_error(&error) => {
                return Ok(error.to_string());
            }
            Err(error) => return Err(error.into()),
        };

        if matches!(command, CliCommand::Help) {
            return Ok(CliArguments::usage_text());
        }

        let repo_root = RepositoryRootResolver::resolve();
        EnvFileLoader::load_repo_env_files(&repo_root)?;

        match command {
            CliCommand::Help => Ok(CliArguments::usage_text()),
            CliCommand::GenerateCache(args) => {
                let config = GenerateCacheConfig::new(
                    args.account_dir().to_path_buf(),
                    generate_cache_output_placeholder(),
                )
                .with_recent_replay_count(args.recent_replay_count());
                let dictionary_data = Arc::new(Sc2DictionaryData::load(None).map_err(|error| {
                    GenerateCacheError::DetailedAnalysisConfig(error.to_string())
                })?);
                let resources = ReplayAnalysisResources::from_dictionary_data(dictionary_data)
                    .map_err(|error| {
                        GenerateCacheError::DetailedAnalysisConfig(error.to_string())
                    })?;
                let runtime =
                    GenerateCacheRuntimeOptions::default().with_worker_count(args.worker_count());
                let summary = DetailedReplayAnalyzer::analyze_full_detailed(
                    &config, &resources, logger, &runtime,
                )?;

                let mut output = format!(
                    "Analyzed cache entries from {} replay entr{}",
                    summary.scanned_replays(),
                    if summary.scanned_replays() == 1 {
                        "y"
                    } else {
                        "ies"
                    }
                );
                if summary.timing_report().enabled() {
                    output.push('\n');
                    output.push_str(&summary.timing_report().format_amdahl_summary());
                }
                Ok(output)
            }
            CliCommand::TestCacheOverallStatsDetailedAnalysis(args) => {
                let analyzer_args = TestCacheOverallStatsDetailedAnalysisArgs {
                    account_dir: args.account_dir().map(ToOwned::to_owned),
                    output_file: args.output_file().map(ToOwned::to_owned),
                    original_output: args.original_output().map(ToOwned::to_owned),
                    help_requested: false,
                };
                CacheOverallStatsDetailedAnalysis::run(&analyzer_args, logger).map_err(Into::into)
            }
            CliCommand::CompareCacheGeneration(args) => {
                let system_runner = SystemCommandRunner;
                let comparison_runner = CacheGenerationComparisonRunner::new(&system_runner);
                let config = CacheGenerationComparisonConfig::from_cli_args(&args, repo_root);
                comparison_runner.run(&config).map_err(Into::into)
            }
            CliCommand::CompareCacheGenerationAlternating(args) => {
                let system_runner = SystemCommandRunner;
                let comparison_runner = CacheGenerationComparisonRunner::new(&system_runner);
                let compare_args = args.into_compare_args();
                let config =
                    CacheGenerationComparisonConfig::from_cli_args(&compare_args, repo_root);
                comparison_runner.run(&config).map_err(Into::into)
            }
        }
    }
}

fn generate_cache_output_placeholder() -> PathBuf {
    std::env::temp_dir().join("s2coop-cli-cache-output-unused.sqlite3")
}

struct RepositoryRootResolver;

impl RepositoryRootResolver {
    fn resolve() -> PathBuf {
        if let Ok(current_dir) = std::env::current_dir()
            && let Some(repo_root) = Self::find_repo_root_from(&current_dir)
        {
            return repo_root;
        }

        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest_path = PathBuf::from(manifest_dir);
            if let Some(repo_root) = Self::find_repo_root_from(&manifest_path) {
                return repo_root;
            }
        }

        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    fn find_repo_root_from(start: &std::path::Path) -> Option<PathBuf> {
        start.ancestors().find_map(|candidate| {
            let workspace_manifest = candidate.join("Cargo.toml");
            let analyzer_dir = candidate.join("s2coop-analyzer");
            let protocol_dir = candidate.join("s2protocol-port");
            (workspace_manifest.is_file() && analyzer_dir.is_dir() && protocol_dir.is_dir())
                .then(|| candidate.to_path_buf())
        })
    }
}
