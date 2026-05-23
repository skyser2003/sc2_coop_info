use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

use crate::{CommanderUnitRollup, UnitStatsRollup, stats_aggregation::StatsAggregationOps};

pub struct StatsUnitDataOps;

impl StatsUnitDataOps {
    fn to_stats_json_value<T: Serialize>(value: T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }

    fn units_to_stats_with_dictionary(dictionary: &Sc2DictionaryData) -> HashSet<String> {
        dictionary.units_to_stats.clone()
    }

    fn unit_excluded_from_stats_for_commander(commander: &str, unit: &str) -> bool {
        (unit == "MULE" && commander != "Raynor")
            || (unit == "Spider Mine" && commander != "Raynor" && commander != "Nova")
            || (unit == "Omega Worm" && commander != "Kerrigan")
            || (unit == "Nydus Worm" && commander != "Abathur")
    }

    fn unit_excluded_from_sum_for_commander(commander: &str, unit: &str) -> bool {
        matches!(
            unit,
            "Mecha Infestor"
                | "Havoc"
                | "SCV"
                | "Probe"
                | "Drone"
                | "Mecha Drone"
                | "Primal Drone"
                | "Infested SCV"
                | "Probius"
                | "Dominion Laborer"
                | "Primal Hive"
                | "Primal Warden"
                | "Imperial Intercessor"
                | "Archangel"
        ) || (commander != "Tychus" && unit == "Auto-Turret")
    }

    fn unit_rollup_count_value(value: i64, hidden: bool) -> Value {
        if hidden {
            Value::String("-".to_string())
        } else {
            Value::from(value)
        }
    }

    pub fn build_amon_unit_data(amon_rollup: BTreeMap<String, UnitStatsRollup>) -> Value {
        #[derive(Serialize)]
        struct AmonUnitRow {
            created: i64,
            lost: i64,
            kills: i64,
            #[serde(rename = "KD")]
            kd: Value,
        }

        const AMON_KD_MUTATORS: [&str; 4] = [
            "Twister",
            "Purifier Beam",
            "Moebius Corps Laser Drill",
            "Blizzard",
        ];

        const AMON_REMOVED_UNITS: [&str; 3] = [
            "AdeptPhaseShift",
            "Drakken Pulse Cannon",
            "James 'Sirius' Sykes",
        ];

        let mut rows = amon_rollup
            .into_iter()
            .collect::<Vec<(String, UnitStatsRollup)>>();

        rows.sort_by(|(left_name, left), (right_name, right)| {
            right
                .created
                .cmp(&left.created)
                .then_with(|| left_name.cmp(right_name))
        });

        let mut out = Map::new();
        let mut total = UnitStatsRollup::default();

        for (unit, mut row) in rows {
            if AMON_REMOVED_UNITS
                .iter()
                .any(|removed| removed == &unit.as_str())
            {
                continue;
            }

            if AMON_KD_MUTATORS
                .iter()
                .any(|mutator| mutator == &unit.as_str())
            {
                row.lost = 0;
                out.insert(
                    unit,
                    Self::to_stats_json_value(AmonUnitRow {
                        created: row.created,
                        lost: row.lost,
                        kills: row.kills,
                        kd: Value::String("-".to_string()),
                    }),
                );
            } else {
                out.insert(
                    unit,
                    Self::to_stats_json_value(AmonUnitRow {
                        created: row.created,
                        lost: row.lost,
                        kills: row.kills,
                        kd: Value::from(if row.lost <= 0 {
                            0.0
                        } else {
                            row.kills as f64 / row.lost as f64
                        }),
                    }),
                );
            }

            total.created = total.created.saturating_add(row.created);
            total.lost = total.lost.saturating_add(row.lost);
            total.kills = total.kills.saturating_add(row.kills);
        }

        out.insert(
            "sum".to_string(),
            Self::to_stats_json_value(AmonUnitRow {
                created: total.created,
                lost: total.lost,
                kills: total.kills,
                kd: Value::from(if total.lost <= 0 {
                    0.0
                } else {
                    total.kills as f64 / total.lost as f64
                }),
            }),
        );

        Value::Object(out)
    }

    pub fn build_commander_unit_data_with_dictionary(
        side_rollup: BTreeMap<String, CommanderUnitRollup>,
        dictionary: &Sc2DictionaryData,
    ) -> Value {
        #[derive(Serialize)]
        struct CommanderUnitRow {
            created: Value,
            made: f64,
            lost: Value,
            lost_percent: Option<f64>,
            kills: i64,
            #[serde(rename = "KD")]
            kd: Option<f64>,
            kill_percentage: f64,
        }

        let mut out = Map::new();

        for (commander, entry) in side_rollup {
            let mut rows = Map::new();
            let mut totals = UnitStatsRollup::default();
            let mut units_to_delete = HashSet::new();
            let mut units = entry.units.into_iter().collect::<Vec<_>>();
            let stats_units = Self::units_to_stats_with_dictionary(dictionary);

            units.sort_by(|(left_name, left), (right_name, right)| {
                right
                    .kills
                    .cmp(&left.kills)
                    .then_with(|| right.created.cmp(&left.created))
                    .then_with(|| left_name.cmp(right_name))
            });

            for (unit, unit_row) in units {
                if unit_row.kills == 0 && !stats_units.contains(unit.as_str()) {
                    units_to_delete.insert(unit);
                    continue;
                }

                if Self::unit_excluded_from_stats_for_commander(&commander, &unit) {
                    units_to_delete.insert(unit);
                    continue;
                }

                let made = if entry.count == 0 {
                    0.0
                } else {
                    unit_row.made as f64 / entry.count as f64
                };
                let lost_percent =
                    if !unit_row.created_hidden && !unit_row.lost_hidden && unit_row.created > 0 {
                        Some(unit_row.lost as f64 / unit_row.created as f64)
                    } else {
                        None
                    };
                let kd = if !unit_row.lost_hidden && unit_row.lost > 0 {
                    Some(unit_row.kills as f64 / unit_row.lost as f64)
                } else {
                    None
                };
                let kill_percentage = if unit_row.kill_percentages.is_empty() {
                    0.0
                } else {
                    StatsAggregationOps::median_f64(&unit_row.kill_percentages)
                };

                if !Self::unit_excluded_from_sum_for_commander(&commander, &unit) {
                    if !unit_row.created_hidden {
                        totals.created = totals.created.saturating_add(unit_row.created);
                    }
                    if !unit_row.lost_hidden {
                        totals.lost = totals.lost.saturating_add(unit_row.lost);
                    }
                    totals.kills = totals.kills.saturating_add(unit_row.kills);
                }

                rows.insert(
                    unit,
                    Self::to_stats_json_value(CommanderUnitRow {
                        created: Self::unit_rollup_count_value(
                            unit_row.created,
                            unit_row.created_hidden,
                        ),
                        made,
                        lost: Self::unit_rollup_count_value(unit_row.lost, unit_row.lost_hidden),
                        lost_percent,
                        kills: unit_row.kills,
                        kd,
                        kill_percentage,
                    }),
                );
            }

            for unit in units_to_delete {
                rows.remove(&unit);
            }

            let total_lost_percent = if totals.created == 0 {
                0.0
            } else {
                totals.lost as f64 / totals.created as f64
            };
            let total_kd = if totals.lost <= 0 {
                0.0
            } else {
                totals.kills as f64 / totals.lost as f64
            };
            rows.insert(
                "sum".to_string(),
                Self::to_stats_json_value(CommanderUnitRow {
                    created: Value::from(totals.created),
                    made: 1.0,
                    lost: Value::from(totals.lost),
                    lost_percent: Some(total_lost_percent),
                    kills: totals.kills,
                    kd: Some(total_kd),
                    kill_percentage: 1.0,
                }),
            );
            rows.insert("count".to_string(), Value::from(entry.count));
            out.insert(commander, Value::Object(rows));
        }

        Value::Object(out)
    }
}
