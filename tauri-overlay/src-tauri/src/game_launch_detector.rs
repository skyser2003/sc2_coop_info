use std::time::{Duration, Instant};

const INITIAL_REPLAY_LOOKBACK: Duration = Duration::from_secs(60);
const REPLAY_SETTLE_DELAY: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub struct GameLaunchDetector {
    last_game_time: Option<u64>,
    last_popup_replay_count: usize,
    last_observed_replay_count: usize,
    last_replay_time: Instant,
}

impl GameLaunchDetector {
    pub fn new(now: Instant) -> Self {
        Self {
            last_game_time: None,
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

    pub fn observe_display_time(&mut self, display_time: u64) -> bool {
        if self.last_game_time.is_none() || display_time == 0 {
            self.last_game_time = Some(display_time);
            return false;
        }
        if self.last_game_time == Some(display_time) {
            return false;
        }

        self.last_game_time = Some(display_time);
        true
    }

    pub fn replay_change_settled(&self, now: Instant) -> bool {
        now.duration_since(self.last_replay_time) >= REPLAY_SETTLE_DELAY
    }

    pub fn record_popup_shown(&mut self, replay_count: usize) {
        self.last_popup_replay_count = replay_count;
    }
}
