use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{ReplayInfo, ReplayPlayerInfo};

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

    fn record_commander_mastery(
        mastery_counts: &mut [f64; 6],
        mastery_distribution_counts: &mut StatsMasteryDistributionCounts,
        mastery_distribution_by_prestige_counts: &mut StatsMasteryDistributionByPrestigeCounts,
        mastery_by_prestige_counts: &mut [[f64; 6]; 4],
        prestige_counts: &mut [u64; 4],
        record: StatsCommanderPlayerRecord<'_>,
    ) {
        let normalized_masteries = Self::normalize_mastery_vector(record.masteries);
        Self::record_mastery_counts(mastery_counts, &normalized_masteries);
        Self::record_mastery_distribution(mastery_distribution_counts, record.masteries);
        Self::record_mastery_distribution_by_prestige(
            mastery_distribution_by_prestige_counts,
            record.prestige,
            record.masteries,
        );
        Self::record_mastery_by_prestige(
            mastery_by_prestige_counts,
            record.prestige,
            &normalized_masteries,
        );
        if record.include_prestige {
            Self::record_prestige_count(prestige_counts, record.prestige);
        }
    }
}

#[derive(Default)]
pub struct StatsWinLossAggregate {
    wins: u64,
    losses: u64,
}

impl StatsWinLossAggregate {
    pub fn record_result(&mut self, is_victory: bool) {
        if is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
    }

    pub fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }

    pub fn wins(&self) -> u64 {
        self.wins
    }

    pub fn losses(&self) -> u64 {
        self.losses
    }
}

#[derive(Default)]
pub struct StatsRegionAggregate {
    wins: u64,
    losses: u64,
    max_asc: u64,
    max_com: BTreeSet<String>,
    prestiges: HashMap<String, u64>,
}

impl StatsRegionAggregate {
    pub fn record_result(&mut self, is_victory: bool) {
        if is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
    }

    pub fn record_player(
        &mut self,
        mastery_level: u64,
        commander_level: u64,
        commander_text: &str,
        commander_name: &str,
        prestige: u64,
    ) {
        self.max_asc = self.max_asc.max(mastery_level);
        if commander_level == 15 && !commander_text.is_empty() {
            self.max_com.insert(commander_text.to_string());
        }
        if !commander_name.is_empty() {
            let value = prestige.min(3);
            self.prestiges
                .entry(commander_name.to_string())
                .and_modify(|current| *current = (*current).max(value))
                .or_insert(value);
        }
    }

    pub fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }

    pub fn wins(&self) -> u64 {
        self.wins
    }

    pub fn losses(&self) -> u64 {
        self.losses
    }

    pub fn max_asc(&self) -> u64 {
        self.max_asc
    }

    pub fn max_com(&self) -> &BTreeSet<String> {
        &self.max_com
    }

    pub fn prestiges(&self) -> &HashMap<String, u64> {
        &self.prestiges
    }
}

#[derive(Clone, Debug, Default)]
pub struct StatsPlayerSnapshot {
    pub pid: u8,
    pub name: String,
    pub handle: String,
    pub commander: String,
    pub apm: u64,
    pub kills: u64,
    pub commander_level: u64,
    pub mastery_level: u64,
    pub prestige: u64,
    pub masteries: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct StatsPlayerUnitSnapshot {
    pub pid: u8,
    pub unit_name: String,
    pub created_hidden: bool,
    pub created_count: i64,
    pub lost_hidden: bool,
    pub lost_count: i64,
    pub kills: u64,
}

#[derive(Clone, Debug)]
pub struct StatsAmonUnitSnapshot {
    pub unit_name: String,
    pub created_hidden: bool,
    pub created_count: i64,
    pub lost_hidden: bool,
    pub lost_count: i64,
    pub kills: i64,
}

#[derive(Clone, Debug)]
pub struct StatsReplaySnapshot {
    pub file: String,
    pub map_name: String,
    pub result: String,
    pub difficulty: String,
    pub enemy_race: String,
    pub date_seconds: u64,
    pub detailed_analysis: bool,
    pub brutal_plus: u64,
    pub extension: bool,
    pub length_realtime: f64,
    pub bonus_completed: u64,
    pub main: StatsPlayerSnapshot,
    pub ally: StatsPlayerSnapshot,
    pub player_units: Vec<StatsPlayerUnitSnapshot>,
    pub amon_units: Vec<StatsAmonUnitSnapshot>,
}

#[derive(Default)]
pub struct StatsCommanderAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    mastery_counts: [f64; 6],
    mastery_distribution_counts: StatsMasteryDistributionCounts,
    mastery_distribution_by_prestige_counts: StatsMasteryDistributionByPrestigeCounts,
    mastery_by_prestige_counts: [[f64; 6]; 4],
    prestige_counts: [u64; 4],
    detailed_count: u64,
}

#[derive(Clone, Copy)]
pub struct StatsCommanderPlayerRecord<'a> {
    replay_is_victory: bool,
    detailed_analysis: bool,
    apm: u64,
    kill_fraction: f64,
    prestige: u64,
    masteries: &'a [u64],
    include_prestige: bool,
}

#[derive(Default)]
pub struct StatsCommanderTotals {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    mastery_counts: [f64; 6],
    mastery_distribution_counts: StatsMasteryDistributionCounts,
    mastery_distribution_by_prestige_counts: StatsMasteryDistributionByPrestigeCounts,
    mastery_by_prestige_counts: [[f64; 6]; 4],
    prestige_counts: [u64; 4],
}

pub struct StatsCommanderDataInput<'a> {
    aggregates: &'a BTreeMap<String, StatsCommanderAggregate>,
    total_games: u64,
    totals: &'a StatsCommanderTotals,
    main_frequency: Option<&'a HashMap<String, f64>>,
}

#[derive(Default)]
pub struct StatsPlayerAggregate {
    wins: u64,
    losses: u64,
    apm_values: Vec<u64>,
    kill_fractions: Vec<f64>,
    last_seen: u64,
    handles: BTreeSet<String>,
    names: HashMap<String, u64>,
    commander: String,
    commander_counts: HashMap<String, u64>,
}

pub struct StatsPlayerRecord<'a> {
    player_name: &'a str,
    handle: &'a str,
    commander: &'a str,
    replay_is_victory: bool,
    apm: u64,
    kill_fraction: f64,
    replay_date: u64,
}

#[derive(Default)]
pub struct StatsMapAggregate {
    wins: u64,
    losses: u64,
    victory_length_sum: f64,
    victory_games: u64,
    bonus_fraction_sum: f64,
    bonus_games: u64,
    detailed_count: u64,
    fastest: Option<StatsReplaySnapshot>,
}

impl<'a> StatsCommanderPlayerRecord<'a> {
    pub fn new(
        replay_is_victory: bool,
        detailed_analysis: bool,
        apm: u64,
        kill_fraction: f64,
        prestige: u64,
        masteries: &'a [u64],
        include_prestige: bool,
    ) -> Self {
        Self {
            replay_is_victory,
            detailed_analysis,
            apm,
            kill_fraction,
            prestige,
            masteries,
            include_prestige,
        }
    }
}

impl<'a> StatsCommanderDataInput<'a> {
    pub fn new(
        aggregates: &'a BTreeMap<String, StatsCommanderAggregate>,
        total_games: u64,
        totals: &'a StatsCommanderTotals,
        main_frequency: Option<&'a HashMap<String, f64>>,
    ) -> Self {
        Self {
            aggregates,
            total_games,
            totals,
            main_frequency,
        }
    }
}

impl<'a> StatsPlayerRecord<'a> {
    pub fn new(
        player_name: &'a str,
        handle: &'a str,
        commander: &'a str,
        replay_is_victory: bool,
        apm: u64,
        kill_fraction: f64,
        replay_date: u64,
    ) -> Self {
        Self {
            player_name,
            handle,
            commander,
            replay_is_victory,
            apm,
            kill_fraction,
            replay_date,
        }
    }
}

impl StatsCommanderAggregate {
    pub fn record_player(&mut self, record: StatsCommanderPlayerRecord<'_>) {
        if record.replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
        if record.detailed_analysis {
            self.detailed_count = self.detailed_count.saturating_add(1);
            self.kill_fractions.push(record.kill_fraction);
        }
        self.apm_values.push(record.apm);
        StatsAggregationOps::record_commander_mastery(
            &mut self.mastery_counts,
            &mut self.mastery_distribution_counts,
            &mut self.mastery_distribution_by_prestige_counts,
            &mut self.mastery_by_prestige_counts,
            &mut self.prestige_counts,
            record,
        );
    }

    pub fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }
}

impl StatsCommanderTotals {
    pub fn record_player(&mut self, record: StatsCommanderPlayerRecord<'_>) {
        if record.replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
        self.apm_values.push(record.apm);
        if record.detailed_analysis {
            self.kill_fractions.push(record.kill_fraction);
        }
        StatsAggregationOps::record_commander_mastery(
            &mut self.mastery_counts,
            &mut self.mastery_distribution_counts,
            &mut self.mastery_distribution_by_prestige_counts,
            &mut self.mastery_by_prestige_counts,
            &mut self.prestige_counts,
            record,
        );
    }

    pub fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }
}

impl StatsMapAggregate {
    pub fn record_snapshot(
        &mut self,
        snapshot: &StatsReplaySnapshot,
        replay_is_victory: bool,
        bonus_total: Option<u64>,
        zero_date_ties_can_replace: bool,
    ) {
        if snapshot.detailed_analysis {
            self.detailed_count = self.detailed_count.saturating_add(1);
        }

        if replay_is_victory {
            self.victory_games = self.victory_games.saturating_add(1);
            self.victory_length_sum += snapshot.length_realtime;
            if snapshot.detailed_analysis
                && let Some(total) = bonus_total
                && total > 0
            {
                let completed = snapshot.bonus_completed.min(total);
                self.bonus_fraction_sum += completed as f64 / total as f64;
                self.bonus_games = self.bonus_games.saturating_add(1);
            }
            if self.should_replace_fastest(snapshot, zero_date_ties_can_replace) {
                self.fastest = Some(snapshot.clone());
            }
        }

        if replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
    }

    pub fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }

    pub fn average_victory_time(&self) -> f64 {
        if self.victory_games == 0 {
            999_999.0
        } else {
            self.victory_length_sum / self.victory_games as f64
        }
    }

    pub fn bonus_rate(&self) -> f64 {
        if self.bonus_games == 0 {
            0.0
        } else {
            self.bonus_fraction_sum / self.bonus_games as f64
        }
    }

    pub fn wins(&self) -> u64 {
        self.wins
    }

    pub fn losses(&self) -> u64 {
        self.losses
    }

    pub fn detailed_count(&self) -> u64 {
        self.detailed_count
    }

    pub fn fastest_or_default(&self) -> StatsReplaySnapshot {
        self.fastest.clone().unwrap_or_else(|| StatsReplaySnapshot {
            file: String::new(),
            map_name: String::new(),
            result: String::new(),
            difficulty: String::new(),
            enemy_race: String::new(),
            date_seconds: 0,
            detailed_analysis: false,
            brutal_plus: 0,
            extension: false,
            length_realtime: 999_999.0,
            bonus_completed: 0,
            main: StatsPlayerSnapshot::default(),
            ally: StatsPlayerSnapshot::default(),
            player_units: Vec::new(),
            amon_units: Vec::new(),
        })
    }

    fn should_replace_fastest(
        &self,
        snapshot: &StatsReplaySnapshot,
        zero_date_ties_can_replace: bool,
    ) -> bool {
        self.fastest.as_ref().is_none_or(|fastest| {
            if !fastest.length_realtime.is_finite() {
                return true;
            }
            snapshot.length_realtime < fastest.length_realtime
                || ((snapshot.length_realtime - fastest.length_realtime).abs() < f64::EPSILON
                    && if zero_date_ties_can_replace {
                        snapshot.date_seconds < fastest.date_seconds
                    } else {
                        snapshot.date_seconds > 0
                            && (fastest.date_seconds == 0
                                || snapshot.date_seconds < fastest.date_seconds)
                    })
        })
    }
}

impl StatsPlayerAggregate {
    pub fn record_replay(&mut self, record: StatsPlayerRecord<'_>) {
        if !record.player_name.is_empty() {
            self.names
                .entry(record.player_name.to_string())
                .and_modify(|last_seen| *last_seen = (*last_seen).max(record.replay_date))
                .or_insert(record.replay_date);
        }
        if !record.handle.is_empty() {
            self.handles.insert(record.handle.to_string());
        }
        if !record.commander.is_empty() {
            self.commander = record.commander.to_string();
            self.commander_counts
                .entry(record.commander.to_string())
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }
        if record.replay_is_victory {
            self.wins = self.wins.saturating_add(1);
        } else {
            self.losses = self.losses.saturating_add(1);
        }
        self.apm_values.push(record.apm);
        self.kill_fractions.push(record.kill_fraction);
        self.last_seen = self.last_seen.max(record.replay_date);
    }

    pub fn dominant_commander(&self) -> (String, f64) {
        let games = self.games();
        let Some((commander, count)) = self
            .commander_counts
            .iter()
            .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        else {
            return (self.commander.clone(), 0.0);
        };
        (commander.clone(), StatsAggregationOps::ratio(*count, games))
    }

    pub fn names_by_recency(&self) -> Vec<String> {
        let mut names = self
            .names
            .iter()
            .map(|(name, last_seen)| (name.clone(), *last_seen))
            .collect::<Vec<_>>();
        names.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        names.into_iter().map(|(name, _)| name).collect()
    }

    pub fn games(&self) -> u64 {
        self.wins.saturating_add(self.losses)
    }

    pub fn wins(&self) -> u64 {
        self.wins
    }

    pub fn losses(&self) -> u64 {
        self.losses
    }

    pub fn apm_values(&self) -> &[u64] {
        &self.apm_values
    }

    pub fn kill_fractions(&self) -> &[f64] {
        &self.kill_fractions
    }

    pub fn last_seen(&self) -> u64 {
        self.last_seen
    }

    pub fn handles(&self) -> &BTreeSet<String> {
        &self.handles
    }
}

impl StatsAggregationOps {
    pub fn build_commander_data(input: StatsCommanderDataInput<'_>) -> Map<String, Value> {
        let corrected_frequency = input
            .aggregates
            .iter()
            .map(|(name, aggregate)| {
                let games = aggregate.games() as f64;
                let corrected = if let Some(main_frequency) = input.main_frequency {
                    let divisor = 1.0 - main_frequency.get(name).copied().unwrap_or(0.0);
                    if divisor <= f64::EPSILON {
                        0.0
                    } else {
                        games / divisor
                    }
                } else {
                    games
                };
                (name.clone(), corrected)
            })
            .collect::<HashMap<_, _>>();
        let corrected_total = corrected_frequency.values().sum::<f64>();

        let mut rows = Map::new();
        for (commander, aggregate) in input.aggregates {
            let games = aggregate.games();
            let prestige_games = aggregate.prestige_counts.iter().sum::<u64>();
            let frequency = if input.main_frequency.is_some() {
                Self::ratio_f64(
                    corrected_frequency.get(commander).copied().unwrap_or(0.0),
                    corrected_total,
                )
            } else {
                Self::ratio(games, input.total_games)
            };
            rows.insert(
                commander.clone(),
                Self::to_value(&StatsAggregateCommanderDataRow {
                    frequency,
                    victory: aggregate.wins,
                    defeat: aggregate.losses,
                    winrate: Self::ratio(aggregate.wins, games),
                    median_apm: Self::median_u64(&aggregate.apm_values),
                    kill_fraction: Self::median_f64(&aggregate.kill_fractions),
                    mastery: Self::build_mastery_ratio_map(&aggregate.mastery_counts),
                    mastery_distribution: Self::build_mastery_distribution_map(
                        &aggregate.mastery_distribution_counts,
                    ),
                    mastery_distribution_by_prestige:
                        Self::build_mastery_distribution_by_prestige_map(
                            &aggregate.mastery_distribution_by_prestige_counts,
                        ),
                    prestige: Self::build_ratio_map(&aggregate.prestige_counts, prestige_games),
                    mastery_by_prestige: Self::build_mastery_by_prestige_ratio_map(
                        &aggregate.mastery_by_prestige_counts,
                    ),
                    detailed_count: aggregate.detailed_count,
                }),
            );
        }

        let total_commander_games = input.totals.games();
        let detailed_count = input
            .aggregates
            .values()
            .map(|value| value.detailed_count)
            .sum();
        rows.insert(
            "any".to_string(),
            Self::to_value(&StatsAggregateCommanderDataRow {
                frequency: if total_commander_games == 0 { 0.0 } else { 1.0 },
                victory: input.totals.wins,
                defeat: input.totals.losses,
                winrate: Self::ratio(input.totals.wins, total_commander_games),
                median_apm: Self::median_u64(&input.totals.apm_values),
                kill_fraction: Self::median_f64(&input.totals.kill_fractions),
                mastery: Self::build_mastery_ratio_map(&input.totals.mastery_counts),
                mastery_distribution: Self::build_mastery_distribution_map(
                    &input.totals.mastery_distribution_counts,
                ),
                mastery_distribution_by_prestige: Self::build_mastery_distribution_by_prestige_map(
                    &input.totals.mastery_distribution_by_prestige_counts,
                ),
                prestige: Self::build_ratio_map(
                    &input.totals.prestige_counts,
                    input.totals.prestige_counts.iter().sum::<u64>(),
                ),
                mastery_by_prestige: Self::build_mastery_by_prestige_ratio_map(
                    &input.totals.mastery_by_prestige_counts,
                ),
                detailed_count,
            }),
        );
        rows
    }

    fn to_value<T: Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

impl StatsReplaySnapshot {
    pub fn to_replay_info(&self) -> ReplayInfo {
        ReplayInfo {
            file: self.file.clone(),
            date: self.date_seconds,
            map: self.map_name.clone(),
            result: self.result.clone(),
            difficulty: self.difficulty.clone(),
            enemy: self.enemy_race.clone(),
            length: if self.length_realtime.is_finite() && self.length_realtime > 0.0 {
                self.length_realtime.floor() as u64
            } else {
                0
            },
            accurate_length: self.length_realtime,
            slot1: self.replay_player_info(&self.main),
            slot2: self.replay_player_info(&self.ally),
            main_slot: 0,
            amon_units: self.amon_units_payload(),
            player_stats: Value::Object(Map::new()),
            extension: self.extension,
            brutal_plus: self.brutal_plus,
            weekly: self.extension,
            weekly_name: None,
            mutators: Vec::new(),
            comp: String::new(),
            bonus: vec![1; usize::try_from(self.bonus_completed).unwrap_or(0)],
            bonus_total: None,
            messages: Vec::new(),
            is_detailed: self.detailed_analysis,
        }
    }

    fn replay_player_info(&self, player: &StatsPlayerSnapshot) -> ReplayPlayerInfo {
        ReplayPlayerInfo {
            name: player.name.clone(),
            handle: player.handle.clone(),
            apm: player.apm,
            kills: player.kills,
            commander: player.commander.clone(),
            commander_level: player.commander_level,
            mastery_level: player.mastery_level,
            prestige: player.prestige,
            masteries: player.masteries.clone(),
            units: self.player_units_payload(player.pid),
            icons: Value::Object(Map::new()),
        }
    }

    fn player_units_payload(&self, pid: u8) -> Value {
        let mut units = Map::new();
        for unit in self.player_units.iter().filter(|unit| unit.pid == pid) {
            units.insert(
                unit.unit_name.clone(),
                Value::Array(vec![
                    Self::unit_count_value(unit.created_count, unit.created_hidden),
                    Self::unit_count_value(unit.lost_count, unit.lost_hidden),
                    Value::from(unit.kills),
                ]),
            );
        }
        Value::Object(units)
    }

    fn amon_units_payload(&self) -> Value {
        let mut units = Map::new();
        for unit in &self.amon_units {
            units.insert(
                unit.unit_name.clone(),
                Value::Array(vec![
                    Self::unit_count_value(unit.created_count, unit.created_hidden),
                    Self::unit_count_value(unit.lost_count, unit.lost_hidden),
                    Value::from(unit.kills),
                ]),
            );
        }
        Value::Object(units)
    }

    fn unit_count_value(value: i64, hidden: bool) -> Value {
        if hidden {
            Value::String("-".to_string())
        } else {
            Value::from(value)
        }
    }
}

#[derive(Serialize)]
pub struct StatsAggregateFastestMapDetails {
    length: f64,
    file: String,
    date: u64,
    difficulty: String,
    players: Vec<Value>,
    enemy_race: String,
}

#[derive(Clone, Copy)]
pub struct StatsResultSummary {
    victory: u64,
    defeat: u64,
    winrate: f64,
}

#[derive(Serialize)]
pub struct StatsAggregateMapDataRow {
    id: String,
    average_victory_time: f64,
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
    bonus: f64,
    #[serde(rename = "detailedCount")]
    detailed_count: u64,
    #[serde(rename = "Fastest")]
    fastest: StatsAggregateFastestMapDetails,
}

#[derive(Serialize)]
pub struct StatsAggregateCommanderDataRow {
    #[serde(rename = "Frequency")]
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
    #[serde(rename = "MedianAPM")]
    median_apm: f64,
    #[serde(rename = "KillFraction")]
    kill_fraction: f64,
    #[serde(rename = "Mastery")]
    mastery: Map<String, Value>,
    #[serde(rename = "MasteryDistribution")]
    mastery_distribution: Map<String, Value>,
    #[serde(rename = "MasteryDistributionByPrestige")]
    mastery_distribution_by_prestige: Map<String, Value>,
    #[serde(rename = "Prestige")]
    prestige: Map<String, Value>,
    #[serde(rename = "MasteryByPrestige")]
    mastery_by_prestige: Map<String, Value>,
    #[serde(rename = "detailedCount")]
    detailed_count: u64,
}

#[derive(Serialize)]
pub struct StatsAggregateDifficultyDataRow {
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
}

#[derive(Serialize)]
pub struct StatsAggregateRegionDataRow {
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    winrate: f64,
    max_asc: u64,
    prestiges: Map<String, Value>,
    max_com: Vec<String>,
}

#[derive(Serialize)]
pub struct StatsAggregatePlayerDataRow {
    wins: u64,
    losses: u64,
    winrate: f64,
    kills: f64,
    apm: f64,
    frequency: f64,
    last_seen: u64,
    commander: String,
}

#[derive(Serialize)]
pub struct StatsAggregateUnitDataPayload {
    main: Value,
    ally: Value,
    amon: Value,
}

#[derive(Serialize)]
pub struct StatsAggregateAnalysisPayload {
    #[serde(rename = "MapData")]
    map_data: Map<String, Value>,
    #[serde(rename = "CommanderData")]
    commander_data: Map<String, Value>,
    #[serde(rename = "AllyCommanderData")]
    ally_commander_data: Map<String, Value>,
    #[serde(rename = "DifficultyData")]
    difficulty_data: Map<String, Value>,
    #[serde(rename = "RegionData")]
    region_data: Map<String, Value>,
    #[serde(rename = "PlayerData")]
    player_data: Map<String, Value>,
    #[serde(rename = "AmonData")]
    amon_data: Map<String, Value>,
    #[serde(rename = "UnitData")]
    unit_data: Value,
    #[serde(rename = "MapDataReady")]
    map_data_ready: bool,
}

impl StatsAggregateFastestMapDetails {
    pub fn new(
        length: f64,
        file: String,
        date: u64,
        difficulty: String,
        players: Vec<Value>,
        enemy_race: String,
    ) -> Self {
        Self {
            length,
            file,
            date,
            difficulty,
            players,
            enemy_race,
        }
    }
}

impl StatsResultSummary {
    pub fn new(victory: u64, defeat: u64, winrate: f64) -> Self {
        Self {
            victory,
            defeat,
            winrate,
        }
    }
}

impl StatsAggregateMapDataRow {
    pub fn new(
        id: String,
        average_victory_time: f64,
        frequency: f64,
        result: StatsResultSummary,
        bonus: f64,
        detailed_count: u64,
        fastest: StatsAggregateFastestMapDetails,
    ) -> Self {
        Self {
            id,
            average_victory_time,
            frequency,
            victory: result.victory,
            defeat: result.defeat,
            winrate: result.winrate,
            bonus,
            detailed_count,
            fastest,
        }
    }
}

impl StatsAggregateDifficultyDataRow {
    pub fn new(result: StatsResultSummary) -> Self {
        Self {
            victory: result.victory,
            defeat: result.defeat,
            winrate: result.winrate,
        }
    }
}

impl StatsAggregateRegionDataRow {
    pub fn new(
        frequency: f64,
        result: StatsResultSummary,
        max_asc: u64,
        prestiges: Map<String, Value>,
        max_com: Vec<String>,
    ) -> Self {
        Self {
            frequency,
            victory: result.victory,
            defeat: result.defeat,
            winrate: result.winrate,
            max_asc,
            prestiges,
            max_com,
        }
    }
}

impl StatsAggregatePlayerDataRow {
    pub fn new(
        result: StatsResultSummary,
        kills: f64,
        apm: f64,
        frequency: f64,
        last_seen: u64,
        commander: String,
    ) -> Self {
        Self {
            wins: result.victory,
            losses: result.defeat,
            winrate: result.winrate,
            kills,
            apm,
            frequency,
            last_seen,
            commander,
        }
    }
}

impl StatsAggregateUnitDataPayload {
    pub fn new(main: Value, ally: Value, amon: Value) -> Self {
        Self { main, ally, amon }
    }
}

impl StatsAggregateAnalysisPayload {
    pub fn new_ready_map_data(
        map_data: Map<String, Value>,
        commander_data: Map<String, Value>,
        ally_commander_data: Map<String, Value>,
        difficulty_data: Map<String, Value>,
        region_data: Map<String, Value>,
        player_data: Map<String, Value>,
        unit_data: Value,
    ) -> Self {
        Self {
            map_data,
            commander_data,
            ally_commander_data,
            difficulty_data,
            region_data,
            player_data,
            amon_data: Map::new(),
            unit_data,
            map_data_ready: true,
        }
    }
}
