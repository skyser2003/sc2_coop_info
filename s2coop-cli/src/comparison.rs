use crate::commands::{CliArguments, CompareCacheGenerationArgs};
use crate::env_file::EnvAssignment;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use walkdir::WalkDir;

const ACCOUNT_ENV_KEYS: [&str; 3] = [
    "SC2_ACCOUNT_PATH",
    "SC2_ACCOUNT_PATH_WINDOWS",
    "SC2_ACCOUNT_PATH_LINUX",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonVariant {
    Current,
    Comparison,
}

impl ComparisonVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Comparison => "comparison",
        }
    }
}

impl fmt::Display for ComparisonVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliBinarySpec {
    manifest_path: PathBuf,
    bin_name: String,
}

impl CliBinarySpec {
    pub fn new(manifest_path: PathBuf, bin_name: String) -> Self {
        Self {
            manifest_path,
            bin_name,
        }
    }

    pub fn current(repo_root: &Path) -> Self {
        Self::new(
            repo_root.join("s2coop-cli").join("Cargo.toml"),
            "s2coop-cli".to_string(),
        )
    }

    pub fn for_comparison_worktree(worktree: &Path) -> Self {
        let s2coop_cli_manifest = worktree.join("s2coop-cli").join("Cargo.toml");
        if s2coop_cli_manifest.is_file() {
            return Self::new(s2coop_cli_manifest, "s2coop-cli".to_string());
        }

        Self::new(
            worktree.join("s2coop-analyzer").join("Cargo.toml"),
            "s2coop-analyzer-cli".to_string(),
        )
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn bin_name(&self) -> &str {
        &self.bin_name
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    program: PathBuf,
    arguments: Vec<String>,
    working_directory: PathBuf,
    env_vars: Vec<EnvAssignment>,
}

impl CommandRequest {
    pub fn new(
        program: impl Into<PathBuf>,
        arguments: Vec<String>,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            arguments,
            working_directory: working_directory.into(),
            env_vars: Vec::new(),
        }
    }

    pub fn with_env_vars(mut self, env_vars: Vec<EnvAssignment>) -> Self {
        self.env_vars = env_vars;
        self
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub fn env_vars(&self) -> &[EnvAssignment] {
        &self.env_vars
    }

    pub fn display_command(&self) -> String {
        let mut parts = Vec::with_capacity(self.arguments.len() + 1);
        parts.push(self.program.display().to_string());
        parts.extend(self.arguments.iter().cloned());
        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    pub fn new(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self {
            exit_code,
            stdout,
            stderr,
        }
    }

    pub fn success(stdout: String) -> Self {
        Self::new(0, stdout, String::new())
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    pub fn combined_text(&self) -> String {
        match (self.stdout.is_empty(), self.stderr.is_empty()) {
            (true, true) => String::new(),
            (false, true) => self.stdout.clone(),
            (true, false) => self.stderr.clone(),
            (false, false) => format!("{}\n{}", self.stdout, self.stderr),
        }
    }
}

pub trait CommandRunner {
    fn run(&self, request: &CommandRequest) -> Result<CommandOutput, io::Error>;
}

#[derive(Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, request: &CommandRequest) -> Result<CommandOutput, io::Error> {
        let mut command = Command::new(request.program());
        command
            .args(request.arguments())
            .current_dir(request.working_directory());
        for env_var in request.env_vars() {
            command.env(env_var.name(), env_var.value());
        }

        let output = command.output()?;
        Ok(CommandOutput::new(
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheGenerationComparisonConfig {
    repo_root: PathBuf,
    comparison_ref: String,
    recent_replay_count: Option<usize>,
    runs: usize,
    workers: Option<usize>,
    warmup_runs_per_variant: usize,
    analyzer_timings: bool,
    keep_artifacts: bool,
    cargo_jobs: usize,
}

impl CacheGenerationComparisonConfig {
    pub fn from_cli_args(args: &CompareCacheGenerationArgs, repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            comparison_ref: args.comparison_ref().to_string(),
            recent_replay_count: args.recent_replay_count(),
            runs: args.runs(),
            workers: args.workers(),
            warmup_runs_per_variant: args.warmup_runs_per_variant(),
            analyzer_timings: args.analyzer_timings(),
            keep_artifacts: args.keep_artifacts(),
            cargo_jobs: CliArguments::default_cargo_jobs(),
        }
    }

    pub fn with_cargo_jobs(mut self, cargo_jobs: usize) -> Self {
        self.cargo_jobs = cargo_jobs;
        self
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
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

    pub fn cargo_jobs(&self) -> usize {
        self.cargo_jobs
    }

    fn validate(&self) -> Result<(), ComparisonError> {
        if self.runs == 0 {
            return Err(ComparisonError::InvalidRuns);
        }
        if self.workers.is_some_and(|workers| workers == 0) {
            return Err(ComparisonError::InvalidWorkers);
        }
        if self.warmup_runs_per_variant == 0 {
            return Err(ComparisonError::InvalidWarmupRuns);
        }
        if self.cargo_jobs == 0 {
            return Err(ComparisonError::InvalidCargoJobs);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateCacheRunResult {
    elapsed_seconds: f64,
    entry_count: usize,
    analyzer_total_seconds: Option<f64>,
    decode_ordered_seconds: Option<f64>,
    detailed_report_seconds: Option<f64>,
    output: String,
}

impl GenerateCacheRunResult {
    fn new(elapsed_seconds: f64, entry_count: usize, output: String) -> Self {
        Self {
            elapsed_seconds,
            entry_count,
            analyzer_total_seconds: parse_timing_metric_seconds(&output, "total"),
            decode_ordered_seconds: parse_timing_metric_seconds(&output, "decode_ordered"),
            detailed_report_seconds: parse_timing_metric_seconds(&output, "detailed_report"),
            output,
        }
    }

    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    pub fn analyzer_total_seconds(&self) -> Option<f64> {
        self.analyzer_total_seconds
    }

    pub fn decode_ordered_seconds(&self) -> Option<f64> {
        self.decode_ordered_seconds
    }

    pub fn detailed_report_seconds(&self) -> Option<f64> {
        self.detailed_report_seconds
    }

    pub fn output(&self) -> &str {
        &self.output
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ComparisonRunRow {
    run: usize,
    variant: String,
    elapsed_seconds: f64,
    analyzer_total_seconds: Option<f64>,
    decode_ordered_seconds: Option<f64>,
    detailed_report_seconds: Option<f64>,
    entry_count: usize,
    output_file: String,
}

impl ComparisonRunRow {
    fn new(
        run: usize,
        variant: ComparisonVariant,
        cache_run: GenerateCacheRunResult,
        output_file: &Path,
    ) -> Self {
        Self {
            run,
            variant: variant.as_str().to_string(),
            elapsed_seconds: cache_run.elapsed_seconds(),
            analyzer_total_seconds: cache_run.analyzer_total_seconds(),
            decode_ordered_seconds: cache_run.decode_ordered_seconds(),
            detailed_report_seconds: cache_run.detailed_report_seconds(),
            entry_count: cache_run.entry_count(),
            output_file: output_file.display().to_string(),
        }
    }

    fn variant(&self) -> &str {
        &self.variant
    }

    fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    fn analyzer_total_seconds(&self) -> Option<f64> {
        self.analyzer_total_seconds
    }

    fn decode_ordered_seconds(&self) -> Option<f64> {
        self.decode_ordered_seconds
    }

    fn detailed_report_seconds(&self) -> Option<f64> {
        self.detailed_report_seconds
    }

    fn entry_count(&self) -> usize {
        self.entry_count
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ComparisonSummary {
    comparison_ref: String,
    comparison_commit: String,
    runs: usize,
    warmup_runs_per_variant: usize,
    workers: Option<usize>,
    analyzer_timings: bool,
    current_mean_seconds: Option<f64>,
    comparison_mean_seconds: Option<f64>,
    delta_seconds: Option<f64>,
    runtime_ratio: Option<f64>,
    current_analyzer_mean_seconds: Option<f64>,
    comparison_analyzer_mean_seconds: Option<f64>,
    current_decode_ordered_mean_seconds: Option<f64>,
    comparison_decode_ordered_mean_seconds: Option<f64>,
    current_detailed_report_mean_seconds: Option<f64>,
    comparison_detailed_report_mean_seconds: Option<f64>,
    entry_counts: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct ComparisonStats {
    current_rows: Vec<ComparisonRunRow>,
    comparison_rows: Vec<ComparisonRunRow>,
    current_mean: Option<f64>,
    comparison_mean: Option<f64>,
    current_analyzer_mean: Option<f64>,
    comparison_analyzer_mean: Option<f64>,
    current_decode_mean: Option<f64>,
    comparison_decode_mean: Option<f64>,
    current_detailed_mean: Option<f64>,
    comparison_detailed_mean: Option<f64>,
    entry_counts: Vec<usize>,
    delta_seconds: Option<f64>,
    runtime_ratio: Option<f64>,
}

impl ComparisonStats {
    fn from_rows(run_rows: &[ComparisonRunRow]) -> Self {
        let current_rows = run_rows
            .iter()
            .filter(|row| row.variant() == ComparisonVariant::Current.as_str())
            .cloned()
            .collect::<Vec<ComparisonRunRow>>();
        let comparison_rows = run_rows
            .iter()
            .filter(|row| row.variant() == ComparisonVariant::Comparison.as_str())
            .cloned()
            .collect::<Vec<ComparisonRunRow>>();
        let current_mean = mean_by(&current_rows, |row| Some(row.elapsed_seconds()));
        let comparison_mean = mean_by(&comparison_rows, |row| Some(row.elapsed_seconds()));
        let delta_seconds = current_mean
            .zip(comparison_mean)
            .map(|(current, base)| current - base);
        let runtime_ratio = current_mean
            .zip(comparison_mean)
            .and_then(|(current, base)| (base > 0.0).then_some(current / base));

        let entry_counts = run_rows
            .iter()
            .map(ComparisonRunRow::entry_count)
            .collect::<BTreeSet<usize>>()
            .into_iter()
            .collect::<Vec<usize>>();

        Self {
            current_analyzer_mean: mean_by(&current_rows, ComparisonRunRow::analyzer_total_seconds),
            comparison_analyzer_mean: mean_by(
                &comparison_rows,
                ComparisonRunRow::analyzer_total_seconds,
            ),
            current_decode_mean: mean_by(&current_rows, ComparisonRunRow::decode_ordered_seconds),
            comparison_decode_mean: mean_by(
                &comparison_rows,
                ComparisonRunRow::decode_ordered_seconds,
            ),
            current_detailed_mean: mean_by(
                &current_rows,
                ComparisonRunRow::detailed_report_seconds,
            ),
            comparison_detailed_mean: mean_by(
                &comparison_rows,
                ComparisonRunRow::detailed_report_seconds,
            ),
            current_rows,
            comparison_rows,
            current_mean,
            comparison_mean,
            entry_counts,
            delta_seconds,
            runtime_ratio,
        }
    }

    fn to_summary(
        &self,
        config: &CacheGenerationComparisonConfig,
        comparison_commit: &str,
    ) -> ComparisonSummary {
        ComparisonSummary {
            comparison_ref: config.comparison_ref().to_string(),
            comparison_commit: comparison_commit.to_string(),
            runs: config.runs(),
            warmup_runs_per_variant: config.warmup_runs_per_variant(),
            workers: config.workers(),
            analyzer_timings: config.analyzer_timings(),
            current_mean_seconds: self.current_mean,
            comparison_mean_seconds: self.comparison_mean,
            delta_seconds: self.delta_seconds,
            runtime_ratio: self.runtime_ratio,
            current_analyzer_mean_seconds: self.current_analyzer_mean,
            comparison_analyzer_mean_seconds: self.comparison_analyzer_mean,
            current_decode_ordered_mean_seconds: self.current_decode_mean,
            comparison_decode_ordered_mean_seconds: self.comparison_decode_mean,
            current_detailed_report_mean_seconds: self.current_detailed_mean,
            comparison_detailed_report_mean_seconds: self.comparison_detailed_mean,
            entry_counts: self.entry_counts.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparisonExecutionReport {
    output: String,
    keep_artifacts: bool,
}

impl ComparisonExecutionReport {
    fn new(output: String, keep_artifacts: bool) -> Self {
        Self {
            output,
            keep_artifacts,
        }
    }
}

#[derive(Debug, Error)]
pub enum ComparisonError {
    #[error("runs must be greater than zero")]
    InvalidRuns,
    #[error("recent replay count must be greater than zero when supplied")]
    InvalidRecentReplayCount,
    #[error("workers must be greater than zero when supplied")]
    InvalidWorkers,
    #[error("warmup runs per variant must be greater than zero")]
    InvalidWarmupRuns,
    #[error("cargo jobs must be greater than zero")]
    InvalidCargoJobs,
    #[error("failed to create temporary comparison directory '{0}': {1}")]
    TempDirectoryCreateFailed(PathBuf, #[source] io::Error),
    #[error("failed to remove temporary comparison directory '{0}': {1}")]
    TempDirectoryRemoveFailed(PathBuf, #[source] io::Error),
    #[error("no valid SC2 account directory found in .env or current environment")]
    MissingAccountDir,
    #[error("no replay files found under account directory: {0}")]
    NoReplayFiles(PathBuf),
    #[error("failed to create directory '{0}': {1}")]
    CreateDirectoryFailed(PathBuf, #[source] io::Error),
    #[error("failed to copy replay '{source_path}' to '{destination}': {error}")]
    ReplayCopyFailed {
        source_path: PathBuf,
        destination: PathBuf,
        error: io::Error,
    },
    #[error("failed to start command '{command}': {error}")]
    CommandStartFailed { command: String, error: io::Error },
    #[error("command failed with exit code {exit_code}: {command}\n{output}")]
    CommandFailed {
        command: String,
        exit_code: i32,
        output: String,
    },
    #[error("failed to resolve git ref '{0}'")]
    EmptyComparisonRef(String),
    #[error("generate-cache output did not include an entry count")]
    MissingEntryCount,
    #[error("failed to create CSV writer '{0}': {1}")]
    CsvCreateFailed(PathBuf, #[source] csv::Error),
    #[error("failed to write CSV row '{0}': {1}")]
    CsvWriteFailed(PathBuf, #[source] csv::Error),
    #[error("failed to flush CSV '{0}': {1}")]
    CsvFlushFailed(PathBuf, #[source] io::Error),
    #[error("failed to serialize summary json: {0}")]
    SummarySerializeFailed(#[source] serde_json::Error),
    #[error("failed to write file '{0}': {1}")]
    WriteFileFailed(PathBuf, #[source] io::Error),
}

pub struct CacheGenerationComparisonRunner<'a> {
    command_runner: &'a dyn CommandRunner,
}

impl<'a> CacheGenerationComparisonRunner<'a> {
    pub fn new(command_runner: &'a dyn CommandRunner) -> Self {
        Self { command_runner }
    }

    pub fn run(&self, config: &CacheGenerationComparisonConfig) -> Result<String, ComparisonError> {
        config.validate()?;

        let temp_root = create_temp_root()?;
        let comparison_worktree = temp_root.join("comparison-worktree");
        let mut should_keep_artifacts = config.keep_artifacts();
        let result = self.run_in_temp_root(
            config,
            &temp_root,
            &comparison_worktree,
            &mut should_keep_artifacts,
        );

        if comparison_worktree.exists() {
            let _ = self.run_command(CommandRequest::new(
                "git",
                vec![
                    "-C".to_string(),
                    config.repo_root().display().to_string(),
                    "worktree".to_string(),
                    "remove".to_string(),
                    "--force".to_string(),
                    comparison_worktree.display().to_string(),
                ],
                config.repo_root(),
            ));
        }

        match result {
            Ok(report) => {
                should_keep_artifacts = should_keep_artifacts || report.keep_artifacts;
                if !should_keep_artifacts {
                    remove_temp_root(&temp_root)?;
                }
                Ok(report.output)
            }
            Err(error) => {
                if !should_keep_artifacts {
                    remove_temp_root(&temp_root)?;
                }
                Err(error)
            }
        }
    }

    fn run_in_temp_root(
        &self,
        config: &CacheGenerationComparisonConfig,
        temp_root: &Path,
        comparison_worktree: &Path,
        should_keep_artifacts: &mut bool,
    ) -> Result<ComparisonExecutionReport, ComparisonError> {
        let mut lines = Vec::<String>::new();
        let account_dir =
            AccountDirectoryResolver::resolve().ok_or(ComparisonError::MissingAccountDir)?;
        let mut benchmark_account_dir = account_dir.clone();
        let mut selected_replay_count = None;
        if let Some(recent_replay_count) = config.recent_replay_count() {
            let subset_root = temp_root.join("StarCraft II");
            benchmark_account_dir = subset_root.join("Accounts");
            let selected = RecentReplaySubset::copy_recent_replays(
                &account_dir,
                &benchmark_account_dir,
                recent_replay_count,
            )?;
            selected_replay_count = Some(selected);
        }

        let comparison_commit = self.resolve_comparison_commit(config)?;
        self.build_current_cli(config)?;
        self.add_comparison_worktree(config, comparison_worktree, &comparison_commit)?;
        self.build_comparison_cli(config, comparison_worktree)?;

        let current_spec = CliBinarySpec::current(config.repo_root());
        let comparison_spec = CliBinarySpec::for_comparison_worktree(comparison_worktree);
        let current_exe =
            release_executable(&config.repo_root().join("target"), current_spec.bin_name());
        let comparison_exe = release_executable(
            &comparison_worktree.join("target"),
            comparison_spec.bin_name(),
        );

        for _ in 0..config.warmup_runs_per_variant() {
            self.invoke_warmup_generate_cache(
                ComparisonVariant::Comparison,
                &comparison_exe,
                &benchmark_account_dir,
                config,
                temp_root,
                &mut lines,
            )?;
            self.invoke_warmup_generate_cache(
                ComparisonVariant::Current,
                &current_exe,
                &benchmark_account_dir,
                config,
                temp_root,
                &mut lines,
            )?;
        }

        let mut run_rows = Vec::<ComparisonRunRow>::new();
        for run_index in 0..config.runs() {
            let run_number = run_index + 1;
            let variant = if config.runs() == 1 {
                ComparisonVariant::Comparison
            } else {
                alternating_variant(run_index)
            };
            let exe_path = match variant {
                ComparisonVariant::Current => &current_exe,
                ComparisonVariant::Comparison => &comparison_exe,
            };
            let row = self.invoke_recorded_generate_cache(
                run_number,
                variant,
                exe_path,
                &benchmark_account_dir,
                config,
                temp_root,
            )?;
            lines.push(format_run_line(&row));
            run_rows.push(row);
        }

        if config.runs() == 1 {
            let current_output = temp_root.join("current-cache-output");
            let row = self.invoke_recorded_generate_cache_at_output(
                2,
                ComparisonVariant::Current,
                &current_exe,
                &benchmark_account_dir,
                config,
                &current_output,
            )?;
            lines.push(format_run_line(&row));
            run_rows.push(row);
        }

        let stats = ComparisonStats::from_rows(&run_rows);
        let csv_path = temp_root.join("cache-generation-comparison-runs.csv");
        let summary_path = temp_root.join("cache-generation-comparison-summary.json");
        write_run_csv(&csv_path, &run_rows)?;
        write_summary_json(&summary_path, &stats.to_summary(config, &comparison_commit))?;

        lines.extend(format_summary_lines(
            config,
            &comparison_commit,
            &account_dir,
            selected_replay_count,
            &benchmark_account_dir,
            &stats,
        ));

        if config.keep_artifacts() {
            *should_keep_artifacts = true;
            lines.push(format!(
                "Artifacts kept by request: {}",
                temp_root.display()
            ));
        }

        if *should_keep_artifacts {
            lines.push(format!("Run CSV: {}", csv_path.display()));
            lines.push(format!("Summary JSON: {}", summary_path.display()));
        } else {
            lines.push(
                "Run CSV and summary JSON are temporary; pass --keep-artifacts to keep them."
                    .to_string(),
            );
        }

        Ok(ComparisonExecutionReport::new(
            lines.join("\n"),
            *should_keep_artifacts,
        ))
    }

    fn resolve_comparison_commit(
        &self,
        config: &CacheGenerationComparisonConfig,
    ) -> Result<String, ComparisonError> {
        let output = self.run_checked(CommandRequest::new(
            "git",
            vec![
                "-C".to_string(),
                config.repo_root().display().to_string(),
                "rev-parse".to_string(),
                config.comparison_ref().to_string(),
            ],
            config.repo_root(),
        ))?;
        let commit = output.stdout().trim().to_string();
        if commit.is_empty() {
            return Err(ComparisonError::EmptyComparisonRef(
                config.comparison_ref().to_string(),
            ));
        }
        Ok(commit)
    }

    fn add_comparison_worktree(
        &self,
        config: &CacheGenerationComparisonConfig,
        comparison_worktree: &Path,
        comparison_commit: &str,
    ) -> Result<(), ComparisonError> {
        self.run_checked(CommandRequest::new(
            "git",
            vec![
                "-C".to_string(),
                config.repo_root().display().to_string(),
                "worktree".to_string(),
                "add".to_string(),
                "--detach".to_string(),
                comparison_worktree.display().to_string(),
                comparison_commit.to_string(),
            ],
            config.repo_root(),
        ))?;
        Ok(())
    }

    fn build_current_cli(
        &self,
        config: &CacheGenerationComparisonConfig,
    ) -> Result<(), ComparisonError> {
        let spec = CliBinarySpec::current(config.repo_root());
        self.build_cli(
            config.repo_root(),
            &config.repo_root().join("target"),
            &spec,
            config,
        )
    }

    fn build_comparison_cli(
        &self,
        config: &CacheGenerationComparisonConfig,
        comparison_worktree: &Path,
    ) -> Result<(), ComparisonError> {
        let spec = CliBinarySpec::for_comparison_worktree(comparison_worktree);
        self.build_cli(
            comparison_worktree,
            &comparison_worktree.join("target"),
            &spec,
            config,
        )
    }

    fn build_cli(
        &self,
        working_directory: &Path,
        target_dir: &Path,
        spec: &CliBinarySpec,
        config: &CacheGenerationComparisonConfig,
    ) -> Result<(), ComparisonError> {
        self.run_checked(CommandRequest::new(
            "cargo",
            vec![
                "build".to_string(),
                "--release".to_string(),
                "--jobs".to_string(),
                config.cargo_jobs().to_string(),
                "--target-dir".to_string(),
                target_dir.display().to_string(),
                "--manifest-path".to_string(),
                spec.manifest_path().display().to_string(),
                "--bin".to_string(),
                spec.bin_name().to_string(),
            ],
            working_directory,
        ))?;
        Ok(())
    }

    fn invoke_warmup_generate_cache(
        &self,
        variant: ComparisonVariant,
        exe_path: &Path,
        account_dir: &Path,
        config: &CacheGenerationComparisonConfig,
        temp_root: &Path,
        lines: &mut Vec<String>,
    ) -> Result<(), ComparisonError> {
        let output_file = temp_root.join(format!("warmup-{variant}-cache-output"));
        lines.push(format!("Warm-up {variant}: starting"));
        let run = self.invoke_generate_cache(exe_path, account_dir, &output_file, config)?;
        lines.push(format!(
            "Warm-up {variant}: elapsed={}s entries={} discarded",
            format_seconds_number(run.elapsed_seconds()),
            run.entry_count()
        ));
        remove_generated_cache_outputs(&output_file)?;
        Ok(())
    }

    fn invoke_recorded_generate_cache(
        &self,
        run_number: usize,
        variant: ComparisonVariant,
        exe_path: &Path,
        account_dir: &Path,
        config: &CacheGenerationComparisonConfig,
        temp_root: &Path,
    ) -> Result<ComparisonRunRow, ComparisonError> {
        let output_prefix = format!("{run_number:02}-{variant}");
        let output_file = temp_root.join(format!("{output_prefix}-cache-output"));
        self.invoke_recorded_generate_cache_at_output(
            run_number,
            variant,
            exe_path,
            account_dir,
            config,
            &output_file,
        )
    }

    fn invoke_recorded_generate_cache_at_output(
        &self,
        run_number: usize,
        variant: ComparisonVariant,
        exe_path: &Path,
        account_dir: &Path,
        config: &CacheGenerationComparisonConfig,
        output_file: &Path,
    ) -> Result<ComparisonRunRow, ComparisonError> {
        let run = self.invoke_generate_cache(exe_path, account_dir, output_file, config)?;
        remove_generated_cache_outputs(output_file)?;
        Ok(ComparisonRunRow::new(run_number, variant, run, output_file))
    }

    fn invoke_generate_cache(
        &self,
        exe_path: &Path,
        account_dir: &Path,
        output_file: &Path,
        config: &CacheGenerationComparisonConfig,
    ) -> Result<GenerateCacheRunResult, ComparisonError> {
        let env_vars = if config.analyzer_timings() {
            vec![EnvAssignment::new(
                "S2COOP_ANALYZER_TIMINGS".to_string(),
                "1".to_string(),
            )]
        } else {
            Vec::new()
        };

        let started = Instant::now();
        let output = match self.run_checked(
            CommandRequest::new(
                exe_path,
                generate_cache_arguments(account_dir, None, config.workers()),
                config.repo_root(),
            )
            .with_env_vars(env_vars.clone()),
        ) {
            Ok(output) => output,
            Err(error) if should_retry_generate_cache_with_legacy_output(&error) => self
                .run_checked(
                    CommandRequest::new(
                        exe_path,
                        generate_cache_arguments(account_dir, Some(output_file), config.workers()),
                        config.repo_root(),
                    )
                    .with_env_vars(env_vars),
                )?,
            Err(error) => return Err(error),
        };
        let elapsed_seconds = started.elapsed().as_secs_f64();
        let output_text = output.combined_text();
        let entry_count = parse_generate_cache_entry_count(&output_text)
            .ok_or(ComparisonError::MissingEntryCount)?;
        Ok(GenerateCacheRunResult::new(
            elapsed_seconds,
            entry_count,
            output_text,
        ))
    }

    fn run_checked(&self, request: CommandRequest) -> Result<CommandOutput, ComparisonError> {
        let output = self.run_command(request.clone())?;
        if output.exit_code() != 0 {
            return Err(ComparisonError::CommandFailed {
                command: request.display_command(),
                exit_code: output.exit_code(),
                output: output.combined_text(),
            });
        }
        Ok(output)
    }

    fn run_command(&self, request: CommandRequest) -> Result<CommandOutput, ComparisonError> {
        self.command_runner
            .run(&request)
            .map_err(|error| ComparisonError::CommandStartFailed {
                command: request.display_command(),
                error,
            })
    }
}

fn generate_cache_arguments(
    account_dir: &Path,
    legacy_output_file: Option<&Path>,
    workers: Option<usize>,
) -> Vec<String> {
    let mut arguments = vec![
        "generate-cache".to_string(),
        "--account-dir".to_string(),
        account_dir.display().to_string(),
    ];
    if let Some(output_file) = legacy_output_file {
        arguments.extend(["--output".to_string(), output_file.display().to_string()]);
    }
    if let Some(workers) = workers {
        arguments.extend(["--workers".to_string(), workers.to_string()]);
    }
    arguments
}

fn should_retry_generate_cache_with_legacy_output(error: &ComparisonError) -> bool {
    let ComparisonError::CommandFailed { output, .. } = error else {
        return false;
    };
    output.contains("--output") || output.contains("output")
}

pub struct AccountDirectoryResolver;

impl AccountDirectoryResolver {
    pub fn resolve() -> Option<PathBuf> {
        for key in ACCOUNT_ENV_KEYS {
            let Ok(value) = std::env::var(key) else {
                continue;
            };
            let candidate = PathBuf::from(value.trim().trim_matches('"').trim_matches('\''));
            if candidate.is_dir() {
                return candidate.canonicalize().ok().or(Some(candidate));
            }
        }
        None
    }
}

pub struct RecentReplaySubset;

impl RecentReplaySubset {
    pub fn copy_recent_replays(
        source_account_dir: &Path,
        destination_account_dir: &Path,
        replay_count: usize,
    ) -> Result<usize, ComparisonError> {
        if replay_count == 0 {
            return Err(ComparisonError::InvalidRecentReplayCount);
        }

        let mut replay_files = WalkDir::new(source_account_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
            .filter(|path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("SC2Replay"))
            })
            .map(|path| ReplayFileCandidate::from_path(path.as_path()))
            .collect::<Vec<ReplayFileCandidate>>();
        replay_files.sort_by(ReplayFileCandidate::compare_recent_first);
        replay_files.truncate(replay_count);

        if replay_files.is_empty() {
            return Err(ComparisonError::NoReplayFiles(
                source_account_dir.to_path_buf(),
            ));
        }

        for replay_file in &replay_files {
            let relative_path = replay_file
                .path()
                .strip_prefix(source_account_dir)
                .unwrap_or_else(|_| replay_file.path());
            let destination_path = destination_account_dir.join(relative_path);
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    ComparisonError::CreateDirectoryFailed(parent.to_path_buf(), error)
                })?;
            }
            fs::copy(replay_file.path(), &destination_path).map_err(|error| {
                ComparisonError::ReplayCopyFailed {
                    source_path: replay_file.path().to_path_buf(),
                    destination: destination_path,
                    error,
                }
            })?;
        }

        Ok(replay_files.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayFileCandidate {
    path: PathBuf,
    modified_millis: u128,
    normalized_path: String,
}

impl ReplayFileCandidate {
    fn from_path(path: &Path) -> Self {
        let modified_millis = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        Self {
            path: path.to_path_buf(),
            modified_millis,
            normalized_path: path.to_string_lossy().to_lowercase(),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn compare_recent_first(left: &Self, right: &Self) -> std::cmp::Ordering {
        right
            .modified_millis
            .cmp(&left.modified_millis)
            .then_with(|| left.normalized_path.cmp(&right.normalized_path))
    }
}

pub fn alternating_variant(run_index: usize) -> ComparisonVariant {
    let phase = run_index % 4;
    if phase == 0 || phase == 3 {
        ComparisonVariant::Comparison
    } else {
        ComparisonVariant::Current
    }
}

pub fn parse_timing_metric_seconds(output: &str, metric_name: &str) -> Option<f64> {
    let pattern = format!("{metric_name}=");
    let start = output.find(&pattern)? + pattern.len();
    let rest = &output[start..];
    let number_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .map(char::len_utf8)
        .sum::<usize>();
    if number_len == 0 || !rest[number_len..].starts_with('s') {
        return None;
    }
    rest[..number_len].parse::<f64>().ok()
}

fn parse_generate_cache_entry_count(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        parse_count_after_prefix(line, "Analyzed cache entries from ")
            .or_else(|| parse_count_after_prefix(line, "Generated cache_overall_stats with "))
    })
}

fn parse_count_after_prefix(line: &str, prefix: &str) -> Option<usize> {
    let rest = line.strip_prefix(prefix)?;
    let number_len = rest
        .chars()
        .take_while(char::is_ascii_digit)
        .map(char::len_utf8)
        .sum::<usize>();
    if number_len == 0 {
        return None;
    }
    rest[..number_len].parse::<usize>().ok()
}

fn create_temp_root() -> Result<PathBuf, ComparisonError> {
    let base = std::env::temp_dir();
    let process_id = std::process::id();
    for attempt in 0..100_u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = base.join(format!(
            "sc2coop-cache-compare-{process_id}-{nanos}-{attempt}"
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ComparisonError::TempDirectoryCreateFailed(path, error)),
        }
    }

    let fallback = base.join(format!("sc2coop-cache-compare-{process_id}-fallback"));
    fs::create_dir(&fallback)
        .map_err(|error| ComparisonError::TempDirectoryCreateFailed(fallback.clone(), error))?;
    Ok(fallback)
}

fn remove_temp_root(path: &Path) -> Result<(), ComparisonError> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path)
        .map_err(|error| ComparisonError::TempDirectoryRemoveFailed(path.to_path_buf(), error))
}

fn release_executable(target_dir: &Path, bin_name: &str) -> PathBuf {
    target_dir
        .join("release")
        .join(format!("{bin_name}{}", std::env::consts::EXE_SUFFIX))
}

fn pretty_output_file(output_file: &Path) -> PathBuf {
    let directory = output_file.parent().unwrap_or_else(|| Path::new(""));
    let stem = output_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let extension = output_file
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    directory.join(format!("{stem}_pretty{extension}"))
}

fn remove_generated_cache_outputs(output_file: &Path) -> Result<(), ComparisonError> {
    for path in [output_file.to_path_buf(), pretty_output_file(output_file)] {
        if path.is_file() {
            fs::remove_file(&path)
                .map_err(|error| ComparisonError::WriteFileFailed(path.clone(), error))?;
        }
    }
    Ok(())
}

fn write_run_csv(path: &Path, run_rows: &[ComparisonRunRow]) -> Result<(), ComparisonError> {
    let mut writer = csv::Writer::from_path(path)
        .map_err(|error| ComparisonError::CsvCreateFailed(path.to_path_buf(), error))?;
    for row in run_rows {
        writer
            .serialize(row)
            .map_err(|error| ComparisonError::CsvWriteFailed(path.to_path_buf(), error))?;
    }
    writer
        .flush()
        .map_err(|error| ComparisonError::CsvFlushFailed(path.to_path_buf(), error))
}

fn write_summary_json(path: &Path, summary: &ComparisonSummary) -> Result<(), ComparisonError> {
    let content =
        serde_json::to_string_pretty(summary).map_err(ComparisonError::SummarySerializeFailed)?;
    fs::write(path, content)
        .map_err(|error| ComparisonError::WriteFileFailed(path.to_path_buf(), error))
}

fn mean_by<F>(rows: &[ComparisonRunRow], value: F) -> Option<f64>
where
    F: Fn(&ComparisonRunRow) -> Option<f64>,
{
    let values = rows.iter().filter_map(value).collect::<Vec<f64>>();
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn format_optional_seconds(value: Option<f64>) -> String {
    value
        .map(format_seconds_number)
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_seconds_number(value: f64) -> String {
    format!("{value:.3}")
}

fn format_optional_ratio(value: Option<f64>) -> String {
    value
        .map(|inner| format!("{inner:.4}x"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn format_run_line(row: &ComparisonRunRow) -> String {
    format!(
        "Run {:02} {}: elapsed={}s analyzer_total={} decode_ordered={} detailed_report={} entries={}",
        row.run,
        row.variant(),
        format_seconds_number(row.elapsed_seconds()),
        format_optional_seconds(row.analyzer_total_seconds()),
        format_optional_seconds(row.decode_ordered_seconds()),
        format_optional_seconds(row.detailed_report_seconds()),
        row.entry_count()
    )
}

fn format_summary_lines(
    config: &CacheGenerationComparisonConfig,
    comparison_commit: &str,
    account_dir: &Path,
    selected_replay_count: Option<usize>,
    benchmark_account_dir: &Path,
    stats: &ComparisonStats,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("Comparison ref: {}", config.comparison_ref()));
    lines.push(format!("Comparison commit: {comparison_commit}"));
    lines.push(format!("Account dir: {}", account_dir.display()));
    if let Some(selected) = selected_replay_count {
        lines.push(format!("Replay scope: recent {selected} files"));
        lines.push(format!(
            "Benchmark account dir: {}",
            benchmark_account_dir.display()
        ));
    } else {
        lines.push("Replay scope: all replay files".to_string());
    }
    lines.push(format!("Runs: {}", config.runs()));
    lines.push(format!(
        "Warm-up runs per variant: {}",
        config.warmup_runs_per_variant()
    ));
    if let Some(workers) = config.workers() {
        lines.push(format!("Workers: {workers}"));
    }
    lines.push(format!(
        "Analyzer timings: {}",
        format_bool(config.analyzer_timings())
    ));
    lines.push(format!("Current runs: {}", stats.current_rows.len()));
    lines.push(format!("Comparison runs: {}", stats.comparison_rows.len()));
    lines.push(format!(
        "Entry counts: {}",
        stats
            .entry_counts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(", ")
    ));
    lines.push("Cache output byte comparison: not compared".to_string());
    lines.push(format!(
        "Current elapsed mean seconds: {}",
        format_optional_seconds(stats.current_mean)
    ));
    lines.push(format!(
        "Comparison elapsed mean seconds: {}",
        format_optional_seconds(stats.comparison_mean)
    ));
    lines.push(format!(
        "Delta mean seconds (current - comparison): {}",
        format_optional_seconds(stats.delta_seconds)
    ));
    lines.push(format!(
        "Runtime ratio (current / comparison): {}",
        format_optional_ratio(stats.runtime_ratio)
    ));
    if config.analyzer_timings() {
        lines.push(format!(
            "Current analyzer total mean seconds: {}",
            format_optional_seconds(stats.current_analyzer_mean)
        ));
        lines.push(format!(
            "Comparison analyzer total mean seconds: {}",
            format_optional_seconds(stats.comparison_analyzer_mean)
        ));
        lines.push(format!(
            "Current decode_ordered mean seconds: {}",
            format_optional_seconds(stats.current_decode_mean)
        ));
        lines.push(format!(
            "Comparison decode_ordered mean seconds: {}",
            format_optional_seconds(stats.comparison_decode_mean)
        ));
        lines.push(format!(
            "Current detailed_report mean seconds: {}",
            format_optional_seconds(stats.current_detailed_mean)
        ));
        lines.push(format!(
            "Comparison detailed_report mean seconds: {}",
            format_optional_seconds(stats.comparison_detailed_mean)
        ));
    }
    lines
}
