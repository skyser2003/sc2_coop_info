use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sc2GameState {
    Lobby,
    GameStarting,
    GamePlaying,
    GameEnded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sc2GameStateTransition {
    previous: Sc2GameState,
    current: Sc2GameState,
}

#[derive(Clone, Debug)]
pub struct Sc2GameStateTracker {
    state: Sc2GameState,
    state_entered_at: Instant,
}

impl Sc2GameStateTransition {
    fn new(previous: Sc2GameState, current: Sc2GameState) -> Self {
        Self { previous, current }
    }

    pub fn previous(&self) -> Sc2GameState {
        self.previous
    }

    pub fn current(&self) -> Sc2GameState {
        self.current
    }
}

impl Sc2GameStateTracker {
    pub fn new(now: Instant) -> Self {
        Self {
            state: Sc2GameState::Lobby,
            state_entered_at: now,
        }
    }

    pub fn state(&self) -> Sc2GameState {
        self.state
    }

    pub fn should_poll_live_game(&self) -> bool {
        self.state == Sc2GameState::Lobby
    }

    pub fn transition_to(
        &mut self,
        next_state: Sc2GameState,
        now: Instant,
    ) -> Option<Sc2GameStateTransition> {
        if self.state == next_state {
            return None;
        }

        let transition = Sc2GameStateTransition::new(self.state, next_state);
        self.state = next_state;
        self.state_entered_at = now;
        Some(transition)
    }

    pub fn advance_timed_transitions(
        &mut self,
        now: Instant,
        game_starting_duration: Duration,
        game_ended_duration: Duration,
    ) -> Option<Sc2GameStateTransition> {
        match self.state {
            Sc2GameState::GameStarting
                if now.duration_since(self.state_entered_at) >= game_starting_duration =>
            {
                self.transition_to(Sc2GameState::GamePlaying, now)
            }
            Sc2GameState::GameEnded
                if now.duration_since(self.state_entered_at) >= game_ended_duration =>
            {
                self.transition_to(Sc2GameState::Lobby, now)
            }
            _ => None,
        }
    }
}
