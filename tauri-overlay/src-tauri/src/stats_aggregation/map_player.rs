use super::StatsAggregationOps;
use std::collections::{BTreeSet, HashMap};

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
pub struct StatsReplaySnapshot {
    pub replay_id: i64,
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
            replay_id: 0,
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
