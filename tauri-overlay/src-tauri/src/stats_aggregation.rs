mod commander;
mod map_player;
mod payload;

pub use commander::{
    StatsCommanderAggregate, StatsCommanderDataInput, StatsCommanderPlayerRecord,
    StatsCommanderTotals,
};
pub use map_player::{
    StatsMapAggregate, StatsPlayerAggregate, StatsPlayerRecord, StatsPlayerSnapshot,
    StatsRegionAggregate, StatsReplaySnapshot, StatsWinLossAggregate,
};
pub use payload::{
    StatsAggregateAnalysisPayload, StatsAggregateDifficultyDataRow,
    StatsAggregateFastestMapDetails, StatsAggregateMapDataRow, StatsAggregatePlayerDataRow,
    StatsAggregateRegionDataRow, StatsAggregateUnitDataPayload, StatsResultSummary,
};

use serde_json::{Map, Value};
use std::collections::BTreeMap;

const PRESTIGE_TRACKING_START_YMD: u32 = 20200726;
const MASTERY_DISTRIBUTION_RATIO_SCALE: u64 = 100_000;

pub type StatsMasteryDistributionCounts = [BTreeMap<u64, u64>; 3];
pub type StatsMasteryDistributionByPrestigeCounts = [StatsMasteryDistributionCounts; 4];

pub struct StatsAggregationOps;

impl StatsAggregationOps {
    pub fn ratio(numerator: u64, denominator: u64) -> f64 {
        if denominator == 0 {
            0.0
        } else {
            numerator as f64 / denominator as f64
        }
    }

    pub fn ratio_f64(numerator: f64, denominator: f64) -> f64 {
        if denominator <= f64::EPSILON {
            0.0
        } else {
            numerator / denominator
        }
    }

    pub fn median_u64(values: &[u64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mut sorted = values.to_vec();
        sorted.sort_unstable();

        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            sorted[mid] as f64
        } else {
            (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
        }
    }

    pub fn median_f64(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mut sorted = values.to_vec();
        sorted.sort_by(|left, right| left.total_cmp(right));

        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            sorted[mid]
        } else {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        }
    }

    pub fn mastery_distribution_ratio_key(bucket: u64) -> String {
        let integer = bucket / 1_000;
        let fractional = bucket % 1_000;
        if fractional == 0 {
            return integer.to_string();
        }

        format!("{integer}.{fractional:03}")
            .trim_end_matches('0')
            .to_string()
    }

    pub fn mastery_points_invested(raw_values: &[u64]) -> u64 {
        raw_values.iter().take(6).copied().sum::<u64>()
    }

    pub fn normalize_mastery_vector(raw_values: &[u64]) -> [f64; 6] {
        let mut normalized = [0f64; 6];
        let total_points = Self::mastery_points_invested(raw_values) as f64;
        if total_points <= f64::EPSILON {
            return normalized;
        }

        for (idx, raw) in raw_values.iter().take(6).enumerate() {
            normalized[idx] = *raw as f64 / total_points;
        }
        normalized
    }

    pub fn normalize_mastery_values(raw_values: &[u64]) -> Vec<u64> {
        let mut values = vec![0u64; 6];
        for (index, value) in raw_values.iter().take(6).enumerate() {
            values[index] = *value;
        }
        values
    }

    pub fn record_mastery_counts(target: &mut [f64; 6], values: &[f64; 6]) {
        for (idx, value) in values.iter().enumerate() {
            target[idx] += *value;
        }
    }

    pub fn record_prestige_count(target: &mut [u64; 4], prestige: u64) {
        let prestige = usize::try_from(prestige.min(3)).unwrap_or(3);
        target[prestige] = target[prestige].saturating_add(1);
    }

    pub fn record_mastery_by_prestige(
        target: &mut [[f64; 6]; 4],
        prestige: u64,
        values: &[f64; 6],
    ) {
        let prestige = usize::try_from(prestige.min(3)).unwrap_or(3);
        for (idx, value) in values.iter().enumerate() {
            target[prestige][idx] += *value;
        }
    }

    pub fn record_mastery_distribution(
        target: &mut StatsMasteryDistributionCounts,
        raw_values: &[u64],
    ) {
        for (pair_index, counts) in target.iter_mut().enumerate().take(3) {
            let left = raw_values.get(pair_index * 2).copied().unwrap_or(0);
            let right = raw_values.get(pair_index * 2 + 1).copied().unwrap_or(0);
            let pair_total = left.saturating_add(right);
            if pair_total == 0 {
                continue;
            }
            let bucket = left
                .saturating_mul(MASTERY_DISTRIBUTION_RATIO_SCALE)
                .saturating_add(pair_total / 2)
                .checked_div(pair_total)
                .unwrap_or(0)
                .min(MASTERY_DISTRIBUTION_RATIO_SCALE);
            counts
                .entry(bucket)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }
    }

    pub fn record_mastery_distribution_by_prestige(
        target: &mut StatsMasteryDistributionByPrestigeCounts,
        prestige: u64,
        raw_values: &[u64],
    ) {
        let prestige = usize::try_from(prestige.min(3)).unwrap_or(3);
        Self::record_mastery_distribution(&mut target[prestige], raw_values);
    }

    pub fn build_ratio_map(values: &[u64], total_games: u64) -> Map<String, Value> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    index.to_string(),
                    Value::from(Self::ratio(*value, total_games)),
                )
            })
            .collect()
    }

    pub fn build_mastery_ratio_map(values: &[f64; 6]) -> Map<String, Value> {
        let mut result = Map::new();
        for pair_index in 0..3 {
            let left_idx = pair_index * 2;
            let right_idx = left_idx + 1;
            let pair_total = values[left_idx] + values[right_idx];
            result.insert(
                left_idx.to_string(),
                Value::from(Self::ratio_f64(values[left_idx], pair_total)),
            );
            result.insert(
                right_idx.to_string(),
                Value::from(Self::ratio_f64(values[right_idx], pair_total)),
            );
        }
        result
    }

    pub fn build_mastery_distribution_map(
        values: &StatsMasteryDistributionCounts,
    ) -> Map<String, Value> {
        let mut result = Map::new();
        for (pair_index, pair_counts) in values.iter().enumerate() {
            let pair_total = pair_counts.values().sum::<u64>();
            let buckets = pair_counts
                .iter()
                .map(|(bucket, count)| {
                    (
                        Self::mastery_distribution_ratio_key(*bucket),
                        Value::from(Self::ratio(*count, pair_total)),
                    )
                })
                .collect::<Map<String, Value>>();
            result.insert(pair_index.to_string(), Value::Object(buckets));
        }
        result
    }

    pub fn build_mastery_distribution_by_prestige_map(
        values: &StatsMasteryDistributionByPrestigeCounts,
    ) -> Map<String, Value> {
        values
            .iter()
            .enumerate()
            .map(|(prestige, distribution)| {
                (
                    prestige.to_string(),
                    Value::Object(Self::build_mastery_distribution_map(distribution)),
                )
            })
            .collect()
    }

    pub fn build_mastery_by_prestige_ratio_map(values: &[[f64; 6]; 4]) -> Map<String, Value> {
        values
            .iter()
            .enumerate()
            .map(|(prestige, mastery_values)| {
                (
                    prestige.to_string(),
                    Value::Object(Self::build_mastery_ratio_map(mastery_values)),
                )
            })
            .collect()
    }

    pub fn ymd_from_unix_seconds(seconds: u64) -> Option<u32> {
        let days = i64::try_from(seconds / 86_400).ok()?;
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        let year = year + if month <= 2 { 1 } else { 0 };
        if year < 0 {
            return None;
        }
        let year_u32 = u32::try_from(year).ok()?;
        let month_u32 = u32::try_from(month).ok()?;
        let day_u32 = u32::try_from(day).ok()?;
        if !(1..=12).contains(&month_u32) || !(1..=31).contains(&day_u32) {
            return None;
        }
        year_u32
            .checked_mul(10_000)
            .and_then(|value| {
                month_u32
                    .checked_mul(100)
                    .and_then(|month| value.checked_add(month))
            })
            .and_then(|value| value.checked_add(day_u32))
    }

    pub fn should_count_prestige(date_seconds: u64) -> bool {
        Self::ymd_from_unix_seconds(date_seconds)
            .is_some_and(|value| value > PRESTIGE_TRACKING_START_YMD)
    }
}
