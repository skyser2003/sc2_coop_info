use crate::cache_overall_stats_generator::{pretty_output_path, write_pretty_cache_file};
use crate::detailed_replay_analysis::{
    analyze_full_detailed, GenerateCacheConfig, GenerateCacheError, GenerateCacheRuntimeOptions,
    ReplayAnalysisResources,
};
use crate::dictionary_data::Sc2DictionaryData;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use walkdir::WalkDir;

const NUMERIC_ABS_TOLERANCE: f64 = 1e-9;
const PYTHON_TEST_ID: &str =
    "tests.test_cache_overall_stats_detailed_analysis.TestCacheOverallStatsDetailedAnalysis.test_detailed_analysis_generates_cache_overall_stats";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestCacheOverallStatsDetailedAnalysisArgs {
    pub account_dir: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub original_output: Option<PathBuf>,
    pub help_requested: bool,
}

#[derive(Debug, Error)]
pub enum TestCacheOverallStatsDetailedAnalysisError {
    #[error(transparent)]
    Generate(#[from] GenerateCacheError),
    #[error("missing original cache_overall_stats file: {0}")]
    MissingOriginalCache(PathBuf),
    #[error("generated cache_overall_stats file was not created: {0}")]
    MissingGeneratedCache(PathBuf),
    #[error("generated pretty cache_overall_stats file was not created: {0}")]
    MissingGeneratedPrettyCache(PathBuf),
    #[error("generated cache file is empty: {0}")]
    EmptyGeneratedCache(PathBuf),
    #[error("generated cache has no detailed-analysis replay entries: {0}")]
    MissingDetailedAnalysisEntries(PathBuf),
    #[error("generated cache payload should be a json array: {0}")]
    GeneratedCacheNotArray(PathBuf),
    #[error("failed to read cache file '{0}': {1}")]
    ReadFailed(PathBuf, #[source] io::Error),
    #[error("failed to parse cache json '{0}': {1}")]
    ParseFailed(PathBuf, #[source] serde_json::Error),
    #[error("{details}")]
    ComparisonFailed { details: String },
}

#[derive(Debug)]
struct NumericTypeDiff {
    path: String,
    expected_type: &'static str,
    actual_type: &'static str,
    expected_value: String,
    actual_value: String,
}

#[derive(Debug)]
struct FieldDifference {
    path: String,
    expected: String,
    actual: String,
    abs_delta: Option<f64>,
}

#[derive(Debug)]
struct CacheSummaryPaths {
    generated: PathBuf,
    pretty: PathBuf,
    original: PathBuf,
}

impl CacheSummaryPaths {
    fn format_line(&self) -> String {
        format!(
            "Cache comparison summary: generated={}, pretty={}, original={}",
            self.generated.display(),
            self.pretty.display(),
            self.original.display()
        )
    }
}

pub fn run_test_cache_overall_stats_detailed_analysis(
    args: &TestCacheOverallStatsDetailedAnalysisArgs,
    logger: Option<&(dyn Fn(String) + Send + Sync + '_)>,
) -> Result<String, TestCacheOverallStatsDetailedAnalysisError> {
    let Some(account_dir) = args.account_dir.clone().or_else(resolve_account_dir) else {
        return Ok("skipping exact parity test: no SC2 account directory configured".to_string());
    };

    let replay_count = count_replays(&account_dir);
    if replay_count == 0 {
        return Ok(format!(
            "skipping exact parity test: no replay files found under {}",
            account_dir.display()
        ));
    }

    let start = Instant::now();
    let generated_output = args
        .output_file
        .clone()
        .unwrap_or_else(default_generated_output);
    let generated_pretty = pretty_output_path(&generated_output);
    let original_output = args
        .original_output
        .clone()
        .unwrap_or_else(default_original_output);
    let original_pretty = pretty_output_path(&original_output);
    let summary_paths = CacheSummaryPaths {
        generated: generated_output.clone(),
        pretty: generated_pretty.clone(),
        original: original_output.clone(),
    };

    if !original_output.is_file() {
        return Err(
            TestCacheOverallStatsDetailedAnalysisError::MissingOriginalCache(original_output),
        );
    }

    let config = GenerateCacheConfig::new(account_dir, generated_output.clone());
    let dictionary_data = Arc::new(
        Sc2DictionaryData::load(None)
            .map_err(|error| GenerateCacheError::DetailedAnalysisConfig(error.to_string()))?,
    );
    let resources = ReplayAnalysisResources::from_dictionary_data(dictionary_data)
        .map_err(|error| GenerateCacheError::DetailedAnalysisConfig(error.to_string()))?;
    let runtime = GenerateCacheRuntimeOptions::default();
    let summary = analyze_full_detailed(&config, &resources, logger, &runtime)?;

    if !generated_output.is_file() {
        return Err(
            TestCacheOverallStatsDetailedAnalysisError::MissingGeneratedCache(generated_output),
        );
    }
    if !generated_pretty.is_file() {
        return Err(
            TestCacheOverallStatsDetailedAnalysisError::MissingGeneratedPrettyCache(
                generated_pretty,
            ),
        );
    }

    let generated_payload = read_json_payload(summary.output_file())?;
    let entries = generated_payload.as_array().ok_or_else(|| {
        TestCacheOverallStatsDetailedAnalysisError::GeneratedCacheNotArray(
            summary.output_file().to_path_buf(),
        )
    })?;
    if entries.is_empty() {
        return Err(
            TestCacheOverallStatsDetailedAnalysisError::EmptyGeneratedCache(
                summary.output_file().to_path_buf(),
            ),
        );
    }
    if !entries.iter().any(|entry| {
        entry
            .get("detailed_analysis")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }) {
        return Err(
            TestCacheOverallStatsDetailedAnalysisError::MissingDetailedAnalysisEntries(
                generated_output,
            ),
        );
    }

    let regenerated_original_pretty =
        write_pretty_cache_file(&original_output, Some(&original_pretty))
            .map_err(GenerateCacheError::from)?;

    let comparison_report = compare_cache_payloads_with_report(
        &generated_output,
        &original_output,
        "Generated cache_overall_stats and original/cache_overall_stats",
        NUMERIC_ABS_TOLERANCE,
        Some(&generated_pretty),
        Some(&regenerated_original_pretty),
    )
    .map_err(|error| error.with_context(&summary_paths, start.elapsed().as_secs_f64()))?;

    let _ = summary;

    Ok([
        comparison_report,
        summary_paths.format_line(),
        format!(
            "{PYTHON_TEST_ID}: elapsed={:.3}s",
            start.elapsed().as_secs_f64()
        ),
    ]
    .join("\n"))
}

impl TestCacheOverallStatsDetailedAnalysisError {
    fn with_context(self, summary: &CacheSummaryPaths, elapsed_seconds: f64) -> Self {
        match self {
            Self::ComparisonFailed { details } => Self::ComparisonFailed {
                details: format!(
                    "{details}\n{}\n{PYTHON_TEST_ID}: elapsed={elapsed_seconds:.3}s",
                    summary.format_line()
                ),
            },
            other => other,
        }
    }
}

pub fn runtime_root() -> PathBuf {
    let manifest_dir_str = std::env::var("CARGO_MANIFEST_DIR");

    match manifest_dir_str {
        Ok(manifest_dir_str) => PathBuf::from(manifest_dir_str),
        Err(_) => {
            if let Ok(abs) = std::env::current_exe() {
                if let Some(parent) = abs.parent() {
                    return parent.to_path_buf();
                }
            }

            return PathBuf::from("./");
        }
    }
}

pub fn repo_root() -> PathBuf {
    runtime_root()
        .parent()
        .expect("crate manifest directory should have repo root parent")
        .to_path_buf()
}

fn default_generated_output() -> PathBuf {
    return runtime_root().join("cache_overall_stats.json");
}

fn default_original_output() -> PathBuf {
    repo_root()
        .join("original")
        .join("cache_overall_stats.json")
}

fn read_env_file_value(env_file: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(env_file).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((current_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if current_key.trim() != key {
            continue;
        }
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        return Some(value.to_string());
    }
    None
}

fn resolve_account_dir() -> Option<PathBuf> {
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    let env_path = repo_root().join(".env");
    for key in [
        "SC2_ACCOUNT_PATH",
        "SC2_ACCOUNT_PATH_WINDOWS",
        "SC2_ACCOUNT_PATH_LINUX",
    ] {
        if let Some(value) = read_env_file_value(&env_path, key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    None
}

fn count_replays(account_dir: &Path) -> usize {
    WalkDir::new(account_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension == "SC2Replay")
        })
        .count()
}

fn read_json_payload(path: &Path) -> Result<Value, TestCacheOverallStatsDetailedAnalysisError> {
    let payload = fs::read(path).map_err(|error| {
        TestCacheOverallStatsDetailedAnalysisError::ReadFailed(path.to_path_buf(), error)
    })?;
    serde_json::from_slice(&payload).map_err(|error| {
        TestCacheOverallStatsDetailedAnalysisError::ParseFailed(path.to_path_buf(), error)
    })
}

fn number_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .or_else(|| number.as_i64().map(|inner| inner as f64))
            .or_else(|| number.as_u64().map(|inner| inner as f64)),
        _ => None,
    }
}

fn is_number(value: &Value) -> bool {
    number_value(value).is_some()
}

fn numeric_type_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                Some("int")
            } else if number.is_f64() {
                Some("float")
            } else {
                Some("number")
            }
        }
        _ => None,
    }
}

fn python_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(_) => numeric_type_name(value).unwrap_or("float"),
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

fn python_string_repr(value: &str) -> String {
    let mut output = String::from("'");
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '\'' => output.push_str("\\'"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other if other.is_control() => output.push_str(&format!("\\x{:02x}", other as u32)),
            other => output.push(other),
        }
    }
    output.push('\'');
    output
}

fn trim_trailing_zeroes(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }

    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn format_python_general(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    let abs_value = value.abs();
    if !(1e-4..1e16).contains(&abs_value) {
        let scientific = format!("{value:.16e}");
        let (mantissa, exponent) = scientific
            .split_once('e')
            .expect("scientific float formatting should contain exponent");
        let trimmed_mantissa = trim_trailing_zeroes(mantissa);
        let sign = if exponent.starts_with('-') { '-' } else { '+' };
        let digits = exponent
            .trim_start_matches(['-', '+'])
            .trim_start_matches('0');
        let normalized_digits = if digits.is_empty() { "0" } else { digits };
        return format!("{trimmed_mantissa}e{sign}{normalized_digits:0>2}");
    }

    trim_trailing_zeroes(&format!("{value:.16}"))
}

fn python_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(boolean) => {
            if *boolean {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Number(number) => number.to_string(),
        Value::String(text) => python_string_repr(text),
        Value::Array(items) => {
            let parts = items.iter().map(python_repr).collect::<Vec<String>>();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts = map
                .iter()
                .map(|(key, inner)| format!("{}: {}", python_string_repr(key), python_repr(inner)))
                .collect::<Vec<String>>();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn root_or_path(path: &str) -> String {
    if path.is_empty() {
        "<root>".to_string()
    } else {
        path.to_string()
    }
}

fn key_path(base_path: &str, key: &str) -> String {
    if base_path.is_empty() {
        key.to_string()
    } else {
        format!("{base_path}.{key}")
    }
}

fn index_path(base_path: &str, index: usize) -> String {
    if base_path.is_empty() {
        format!("[{index}]")
    } else {
        format!("{base_path}[{index}]")
    }
}

fn collect_numeric_type_differences(
    expected: &Value,
    actual: &Value,
    path: &str,
    out: &mut Vec<NumericTypeDiff>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }

    if is_number(expected) && is_number(actual) {
        let Some(expected_value) = number_value(expected) else {
            return;
        };
        let Some(actual_value) = number_value(actual) else {
            return;
        };
        if expected_value == actual_value
            && numeric_type_name(expected) != numeric_type_name(actual)
        {
            out.push(NumericTypeDiff {
                path: root_or_path(path),
                expected_type: numeric_type_name(expected).unwrap_or("number"),
                actual_type: numeric_type_name(actual).unwrap_or("number"),
                expected_value: python_repr(expected),
                actual_value: python_repr(actual),
            });
        }
        return;
    }

    match (expected, actual) {
        (Value::Object(expected_map), Value::Object(actual_map)) => {
            let expected_keys = expected_map.keys().cloned().collect::<BTreeSet<String>>();
            let actual_keys = actual_map.keys().cloned().collect::<BTreeSet<String>>();
            if expected_keys != actual_keys {
                return;
            }

            for key in expected_keys {
                collect_numeric_type_differences(
                    expected_map.get(&key).expect("expected key must exist"),
                    actual_map.get(&key).expect("actual key must exist"),
                    &key_path(path, &key),
                    out,
                    limit,
                );
                if out.len() >= limit {
                    return;
                }
            }
        }
        (Value::Array(expected_items), Value::Array(actual_items)) => {
            if expected_items.len() != actual_items.len() {
                return;
            }
            for (index, (expected_item, actual_item)) in
                expected_items.iter().zip(actual_items.iter()).enumerate()
            {
                collect_numeric_type_differences(
                    expected_item,
                    actual_item,
                    &index_path(path, index),
                    out,
                    limit,
                );
                if out.len() >= limit {
                    return;
                }
            }
        }
        _ => {}
    }
}

fn format_numeric_type_differences(diffs: &[NumericTypeDiff]) -> String {
    if diffs.is_empty() {
        return "No numeric type-only differences.".to_string();
    }

    let mut lines = vec![
        "Numeric type-only differences (same numeric value, different number type):".to_string(),
    ];
    for diff in diffs {
        lines.push(format!(
            "  - {}: expected {}({}) actual {}({})",
            diff.path, diff.expected_type, diff.expected_value, diff.actual_type, diff.actual_value
        ));
    }
    lines.join("\n")
}

fn collect_first_field_differences(
    expected: &Value,
    actual: &Value,
    path: &str,
    out: &mut Vec<FieldDifference>,
    limit: usize,
    abs_tolerance: f64,
) {
    if out.len() >= limit {
        return;
    }

    if is_number(expected) && is_number(actual) {
        let Some(expected_value) = number_value(expected) else {
            return;
        };
        let Some(actual_value) = number_value(actual) else {
            return;
        };
        let delta = (expected_value - actual_value).abs();
        if delta <= abs_tolerance {
            return;
        }
        out.push(FieldDifference {
            path: root_or_path(path),
            expected: python_repr(expected),
            actual: python_repr(actual),
            abs_delta: Some(delta),
        });
        return;
    }

    match (expected, actual) {
        (Value::Object(expected_map), Value::Object(actual_map)) => {
            let keys = expected_map
                .keys()
                .chain(actual_map.keys())
                .cloned()
                .collect::<BTreeSet<String>>();
            for key in keys {
                let next_path = key_path(path, &key);
                match (expected_map.get(&key), actual_map.get(&key)) {
                    (Some(expected_value), Some(actual_value)) => {
                        collect_first_field_differences(
                            expected_value,
                            actual_value,
                            &next_path,
                            out,
                            limit,
                            abs_tolerance,
                        );
                    }
                    (Some(expected_value), None) => out.push(FieldDifference {
                        path: next_path,
                        expected: python_repr(expected_value),
                        actual: python_string_repr("<missing>"),
                        abs_delta: None,
                    }),
                    (None, Some(actual_value)) => out.push(FieldDifference {
                        path: next_path,
                        expected: python_string_repr("<missing>"),
                        actual: python_repr(actual_value),
                        abs_delta: None,
                    }),
                    (None, None) => {}
                }
                if out.len() >= limit {
                    return;
                }
            }
        }
        (Value::Array(expected_items), Value::Array(actual_items)) => {
            if expected_items.len() != actual_items.len() {
                out.push(FieldDifference {
                    path: if path.is_empty() {
                        "<root>.length".to_string()
                    } else {
                        format!("{path}.length")
                    },
                    expected: expected_items.len().to_string(),
                    actual: actual_items.len().to_string(),
                    abs_delta: None,
                });
                return;
            }

            for (index, (expected_item, actual_item)) in
                expected_items.iter().zip(actual_items.iter()).enumerate()
            {
                collect_first_field_differences(
                    expected_item,
                    actual_item,
                    &index_path(path, index),
                    out,
                    limit,
                    abs_tolerance,
                );
                if out.len() >= limit {
                    return;
                }
            }
        }
        _ => {
            let same_type = python_type_name(expected) == python_type_name(actual);
            let same_value = expected == actual;
            if !same_type || !same_value {
                out.push(FieldDifference {
                    path: root_or_path(path),
                    expected: python_repr(expected),
                    actual: python_repr(actual),
                    abs_delta: None,
                });
            }
        }
    }
}

fn format_cache_diff_report(
    expected_payload: &Value,
    actual_payload: &Value,
    replay_limit: usize,
    field_limit_per_replay: usize,
    abs_tolerance: f64,
) -> String {
    let (Value::Array(expected_replays), Value::Array(actual_replays)) =
        (expected_payload, actual_payload)
    else {
        return format!(
            "Unable to diff cache payloads as replay lists. expected_type={} actual_type={}",
            python_type_name(expected_payload),
            python_type_name(actual_payload)
        );
    };

    let mut lines = Vec::new();
    if expected_replays.len() != actual_replays.len() {
        lines.push(format!(
            "- replay_count differs: expected={} actual={}",
            expected_replays.len(),
            actual_replays.len()
        ));
    }

    let mut replay_diffs = 0_usize;
    for (replay_index, (expected_replay, actual_replay)) in expected_replays
        .iter()
        .zip(actual_replays.iter())
        .enumerate()
    {
        let mut replay_probe = Vec::new();
        collect_first_field_differences(
            expected_replay,
            actual_replay,
            "",
            &mut replay_probe,
            1,
            abs_tolerance,
        );
        if replay_probe.is_empty() {
            continue;
        }

        let replay_name = expected_replay
            .get("file")
            .and_then(Value::as_str)
            .or_else(|| actual_replay.get("file").and_then(Value::as_str))
            .unwrap_or("<unknown>");

        lines.push(format!("- replay[{replay_index}] file={replay_name}"));

        let mut field_differences = Vec::new();
        collect_first_field_differences(
            expected_replay,
            actual_replay,
            "",
            &mut field_differences,
            field_limit_per_replay,
            abs_tolerance,
        );

        if field_differences.is_empty() {
            lines.push("  field=<unknown> expected!=actual".to_string());
        } else {
            for field_difference in field_differences {
                match field_difference.abs_delta {
                    Some(delta) => lines.push(format!(
                        "  field={} expected={} actual={} abs_delta={}",
                        field_difference.path,
                        field_difference.expected,
                        field_difference.actual,
                        format_python_general(delta)
                    )),
                    None => lines.push(format!(
                        "  field={} expected={} actual={}",
                        field_difference.path, field_difference.expected, field_difference.actual
                    )),
                }
            }
        }

        replay_diffs += 1;
        if replay_diffs >= replay_limit {
            break;
        }
    }

    if replay_diffs == 0 && lines.is_empty() {
        "No replay-level differences found.".to_string()
    } else {
        lines.join("\n")
    }
}

fn truncate_line(text: &str, max_length: usize) -> String {
    if text.len() <= max_length {
        text.to_string()
    } else {
        format!("{}...", &text[..max_length - 3])
    }
}

fn first_differing_pretty_line(
    expected_path: &Path,
    actual_path: &Path,
) -> Result<String, TestCacheOverallStatsDetailedAnalysisError> {
    let expected_lines = fs::read_to_string(expected_path)
        .map_err(|error| {
            TestCacheOverallStatsDetailedAnalysisError::ReadFailed(
                expected_path.to_path_buf(),
                error,
            )
        })?
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<String>>();
    let actual_lines = fs::read_to_string(actual_path)
        .map_err(|error| {
            TestCacheOverallStatsDetailedAnalysisError::ReadFailed(actual_path.to_path_buf(), error)
        })?
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<String>>();

    let min_count = expected_lines.len().min(actual_lines.len());
    for line_index in 0..min_count {
        if expected_lines[line_index] != actual_lines[line_index] {
            return Ok(format!(
                "first_differing_pretty_line={}\nexpected_line={}\nactual_line={}",
                line_index + 1,
                truncate_line(&expected_lines[line_index], 240),
                truncate_line(&actual_lines[line_index], 240)
            ));
        }
    }

    if expected_lines.len() != actual_lines.len() {
        return Ok(format!(
            "pretty_line_count_diff expected_lines={} actual_lines={}",
            expected_lines.len(),
            actual_lines.len()
        ));
    }

    Ok("pretty_line_report_unavailable".to_string())
}

fn compare_cache_payloads_with_report(
    actual_path: &Path,
    expected_path: &Path,
    label: &str,
    abs_tolerance: f64,
    actual_pretty_path: Option<&Path>,
    expected_pretty_path: Option<&Path>,
) -> Result<String, TestCacheOverallStatsDetailedAnalysisError> {
    if let (Some(actual_pretty_path), Some(expected_pretty_path)) =
        (actual_pretty_path, expected_pretty_path)
    {
        if actual_pretty_path.is_file()
            && expected_pretty_path.is_file()
            && fs::read(actual_pretty_path).map_err(|error| {
                TestCacheOverallStatsDetailedAnalysisError::ReadFailed(
                    actual_pretty_path.to_path_buf(),
                    error,
                )
            })? == fs::read(expected_pretty_path).map_err(|error| {
                TestCacheOverallStatsDetailedAnalysisError::ReadFailed(
                    expected_pretty_path.to_path_buf(),
                    error,
                )
            })?
        {
            return Ok(format!("{label}: pretty-identical; no differences."));
        }
    }

    let actual_bytes = fs::read(actual_path).map_err(|error| {
        TestCacheOverallStatsDetailedAnalysisError::ReadFailed(actual_path.to_path_buf(), error)
    })?;
    let expected_bytes = fs::read(expected_path).map_err(|error| {
        TestCacheOverallStatsDetailedAnalysisError::ReadFailed(expected_path.to_path_buf(), error)
    })?;
    if actual_bytes == expected_bytes {
        return Ok(format!("{label}: byte-identical; no differences."));
    }

    let actual_payload: Value = serde_json::from_slice(&actual_bytes).map_err(|error| {
        TestCacheOverallStatsDetailedAnalysisError::ParseFailed(actual_path.to_path_buf(), error)
    })?;
    let expected_payload: Value = serde_json::from_slice(&expected_bytes).map_err(|error| {
        TestCacheOverallStatsDetailedAnalysisError::ParseFailed(expected_path.to_path_buf(), error)
    })?;

    let mut numeric_type_differences = Vec::new();
    collect_numeric_type_differences(
        &expected_payload,
        &actual_payload,
        "",
        &mut numeric_type_differences,
        16,
    );

    let all_differences_report =
        format_cache_diff_report(&expected_payload, &actual_payload, 8, 3, abs_tolerance);

    let pretty_line_report = if let (Some(actual_pretty_path), Some(expected_pretty_path)) =
        (actual_pretty_path, expected_pretty_path)
    {
        if actual_pretty_path.is_file() && expected_pretty_path.is_file() {
            format!(
                "Pretty file line diff:\n{}\n",
                first_differing_pretty_line(expected_pretty_path, actual_pretty_path)?
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let mut significant_differences = Vec::new();
    collect_first_field_differences(
        &expected_payload,
        &actual_payload,
        "",
        &mut significant_differences,
        1,
        abs_tolerance,
    );

    if significant_differences.is_empty() {
        let mut lines = vec![
            format!(
                "{label}: byte-level differences are numeric-equivalent.\nexpected={}\nactual={}\n{}Diff summary (all differences):\n{}",
                expected_path.display(),
                actual_path.display(),
                pretty_line_report,
                all_differences_report
            ),
            format!(
                "{label}: differences are within numeric_abs_tolerance={}; treating as equivalent.",
                format_python_general(abs_tolerance)
            ),
        ];
        if !numeric_type_differences.is_empty() {
            lines.push(format_numeric_type_differences(&numeric_type_differences));
        }
        return Ok(lines.join("\n"));
    }

    Err(TestCacheOverallStatsDetailedAnalysisError::ComparisonFailed {
        details: format!(
            "{label}: byte-level difference detected.\nexpected={}\nactual={}\n{}Diff summary (all differences):\n{}\n{label} are different.\nexpected={}\nactual={}\nnumeric_abs_tolerance={}\nDiff summary:\n{}",
            expected_path.display(),
            actual_path.display(),
            pretty_line_report,
            all_differences_report,
            expected_path.display(),
            actual_path.display(),
            format_python_general(abs_tolerance),
            all_differences_report,
        ),
    })
}
