use super::{
    CacheGenerationComparisonConfig, ComparisonError, ComparisonVariant, GenerateCacheRunResult,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct ComparisonRunRow {
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
    pub(super) fn new(
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
pub(super) struct ComparisonSummary {
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
pub(super) struct ComparisonStats {
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
    pub(super) fn from_rows(run_rows: &[ComparisonRunRow]) -> Self {
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

    pub(super) fn to_summary(
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

pub(super) fn write_run_csv(
    path: &Path,
    run_rows: &[ComparisonRunRow],
) -> Result<(), ComparisonError> {
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

pub(super) fn write_summary_json(
    path: &Path,
    summary: &ComparisonSummary,
) -> Result<(), ComparisonError> {
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

pub(super) fn format_seconds_number(value: f64) -> String {
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

pub(super) fn format_run_line(row: &ComparisonRunRow) -> String {
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

pub(super) fn format_summary_lines(
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
