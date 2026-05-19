use std::time::{Duration, Instant};

use sco_tauri_overlay::{Sc2GameState, Sc2GameStateTracker};

#[test]
fn sc2_game_state_starts_in_lobby() {
    let tracker = Sc2GameStateTracker::new(Instant::now());

    assert_eq!(tracker.state(), Sc2GameState::Lobby);
    assert!(tracker.should_poll_live_game());
}

#[test]
fn game_starting_transitions_to_game_playing_after_display_duration() {
    let started_at = Instant::now();
    let mut tracker = Sc2GameStateTracker::new(started_at);

    tracker.transition_to(Sc2GameState::GameStarting, started_at);

    assert_eq!(
        tracker.advance_timed_transitions(
            started_at + Duration::from_secs(11),
            Duration::from_secs(12),
            Duration::from_secs(30),
        ),
        None
    );
    assert_eq!(
        tracker
            .advance_timed_transitions(
                started_at + Duration::from_secs(12),
                Duration::from_secs(12),
                Duration::from_secs(30),
            )
            .map(|transition| transition.current()),
        Some(Sc2GameState::GamePlaying)
    );
    assert_eq!(tracker.state(), Sc2GameState::GamePlaying);
    assert!(!tracker.should_poll_live_game());
}

#[test]
fn game_ended_transitions_to_lobby_after_display_duration() {
    let ended_at = Instant::now();
    let mut tracker = Sc2GameStateTracker::new(ended_at);

    tracker.transition_to(Sc2GameState::GameEnded, ended_at);

    assert_eq!(
        tracker.advance_timed_transitions(
            ended_at + Duration::from_secs(29),
            Duration::from_secs(12),
            Duration::from_secs(30),
        ),
        None
    );
    assert_eq!(
        tracker
            .advance_timed_transitions(
                ended_at + Duration::from_secs(30),
                Duration::from_secs(12),
                Duration::from_secs(30),
            )
            .map(|transition| transition.current()),
        Some(Sc2GameState::Lobby)
    );
    assert_eq!(tracker.state(), Sc2GameState::Lobby);
    assert!(tracker.should_poll_live_game());
}
