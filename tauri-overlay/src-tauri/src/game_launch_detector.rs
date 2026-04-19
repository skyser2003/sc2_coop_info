use std::time::{Duration, Instant};

const INITIAL_REPLAY_LOOKBACK: Duration = Duration::from_secs(60);
const REPLAY_SETTLE_DELAY: Duration = Duration::from_secs(15);
const HALTED_DISPLAY_TIME_TICKS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameLaunchStatus {
    Unknown,
    Idle,
    Started,
    Running,
    Ended,
}

#[derive(Clone, Debug)]
pub struct GameLaunchDetector {
    status: GameLaunchStatus,
    last_display_time: Option<u64>,
    halted_display_time_ticks: u8,
    last_popup_replay_count: usize,
    last_observed_replay_count: usize,
    last_replay_time: Instant,
}

impl GameLaunchDetector {
    pub fn new(now: Instant) -> Self {
        Self {
            status: GameLaunchStatus::Unknown,
            last_display_time: None,
            halted_display_time_ticks: 0,
            last_popup_replay_count: 0,
            last_observed_replay_count: 0,
            last_replay_time: now.checked_sub(INITIAL_REPLAY_LOOKBACK).unwrap_or(now),
        }
    }

    pub fn observe_replay_count(&mut self, replay_count: usize, now: Instant) {
        if replay_count > self.last_observed_replay_count {
            self.last_observed_replay_count = replay_count;
            self.last_replay_time = now;
        }
    }

    pub fn should_attempt_popup(&self, has_player_rows: bool, replay_count: usize) -> bool {
        has_player_rows && replay_count != self.last_popup_replay_count
    }

    pub fn observe_non_live_state(&mut self) {
        self.status = GameLaunchStatus::Idle;
        self.last_display_time = None;
        self.halted_display_time_ticks = 0;
    }

    pub fn update_display_time_status(&mut self, display_time: u64) -> GameLaunchStatus {
        match self.last_display_time {
            None => {
                self.halted_display_time_ticks = 0;
                self.status = match self.status {
                    GameLaunchStatus::Idle | GameLaunchStatus::Ended => GameLaunchStatus::Idle,
                    _ => GameLaunchStatus::Running,
                };
            }
            Some(last_game_time) if display_time > last_game_time => {
                self.halted_display_time_ticks = 0;
                self.status = match self.status {
                    GameLaunchStatus::Idle | GameLaunchStatus::Ended => GameLaunchStatus::Started,
                    GameLaunchStatus::Started => GameLaunchStatus::Started,
                    _ => GameLaunchStatus::Running,
                };
            }
            Some(last_game_time) if display_time == last_game_time => {
                self.halted_display_time_ticks = self.halted_display_time_ticks.saturating_add(1);
                if self.halted_display_time_ticks >= HALTED_DISPLAY_TIME_TICKS {
                    self.status = GameLaunchStatus::Ended;
                }
            }
            Some(_) => {
                self.halted_display_time_ticks = 0;
                self.status = match self.status {
                    GameLaunchStatus::Idle | GameLaunchStatus::Ended => GameLaunchStatus::Idle,
                    GameLaunchStatus::Started => GameLaunchStatus::Started,
                    _ => GameLaunchStatus::Running,
                };
            }
        }

        self.last_display_time = Some(display_time);
        self.status
    }

    pub fn replay_change_settled(&self, now: Instant) -> bool {
        now.duration_since(self.last_replay_time) >= REPLAY_SETTLE_DELAY
    }

    pub fn record_popup_shown(&mut self, replay_count: usize) {
        self.last_popup_replay_count = replay_count;
        self.status = GameLaunchStatus::Running;
    }
}
