use std::time::{Duration, Instant};

use sco_tauri_overlay::GameLaunchDetector;

#[test]
fn replay_count_changes_refresh_settle_timer() {
    let started_at = Instant::now();
    let mut detector = GameLaunchDetector::new(started_at);

    detector.observe_replay_count(3, started_at + Duration::from_secs(1));

    assert!(!detector.replay_change_settled(started_at + Duration::from_secs(10)));
    assert!(detector.replay_change_settled(started_at + Duration::from_secs(16)));
}

#[test]
fn display_time_must_change_before_popup_can_trigger() {
    let mut detector = GameLaunchDetector::new(Instant::now());

    assert!(!detector.observe_display_time(0));
    assert!(detector.observe_display_time(5));
    assert!(!detector.observe_display_time(5));
    assert!(detector.observe_display_time(6));
}

#[test]
fn popup_attempts_are_deduped_by_replay_count() {
    let started_at = Instant::now();
    let mut detector = GameLaunchDetector::new(started_at);

    assert!(detector.should_attempt_popup(true, 7));
    detector.record_popup_shown(7);

    assert!(!detector.should_attempt_popup(true, 7));
    assert!(detector.should_attempt_popup(true, 8));
    assert!(!detector.should_attempt_popup(false, 8));
}
