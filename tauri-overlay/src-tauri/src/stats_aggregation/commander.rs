use super::{
    StatsAggregationOps, StatsMasteryDistributionByPrestigeCounts, StatsMasteryDistributionCounts,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

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

impl StatsAggregationOps {
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
