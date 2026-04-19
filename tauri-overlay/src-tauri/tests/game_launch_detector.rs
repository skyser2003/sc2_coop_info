use std::time::{Duration, Instant};

use sco_tauri_overlay::{GameLaunchDetector, GameLaunchStatus};

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

    detector.observe_non_live_state();

    assert_eq!(
        detector.update_display_time_status(5),
        GameLaunchStatus::Idle
    );
    assert_eq!(
        detector.update_display_time_status(6),
        GameLaunchStatus::Started
    );
    assert_eq!(
        detector.update_display_time_status(6),
        GameLaunchStatus::Started
    );
}

#[test]
fn startup_midgame_is_suppressed_until_display_time_halts() {
    let mut detector = GameLaunchDetector::new(Instant::now());

    assert_eq!(
        detector.update_display_time_status(120),
        GameLaunchStatus::Running
    );
    assert_eq!(
        detector.update_display_time_status(121),
        GameLaunchStatus::Running
    );
    assert_eq!(
        detector.update_display_time_status(121),
        GameLaunchStatus::Running
    );
    assert_eq!(
        detector.update_display_time_status(121),
        GameLaunchStatus::Running
    );
    assert_eq!(
        detector.update_display_time_status(121),
        GameLaunchStatus::Ended
    );
    assert_eq!(
        detector.update_display_time_status(12),
        GameLaunchStatus::Idle
    );
    assert_eq!(
        detector.update_display_time_status(13),
        GameLaunchStatus::Started
    );
}

#[test]
fn launch_stays_armed_until_popup_is_recorded() {
    let mut detector = GameLaunchDetector::new(Instant::now());

    detector.observe_non_live_state();

    assert_eq!(
        detector.update_display_time_status(10),
        GameLaunchStatus::Idle
    );
    assert_eq!(
        detector.update_display_time_status(11),
        GameLaunchStatus::Started
    );
    assert_eq!(
        detector.update_display_time_status(12),
        GameLaunchStatus::Started
    );

    detector.record_popup_shown(4);

    assert_eq!(
        detector.update_display_time_status(13),
        GameLaunchStatus::Running
    );
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

#[test]
fn non_live_gap_does_not_require_zero_display_time() {
    let mut detector = GameLaunchDetector::new(Instant::now());

    detector.observe_non_live_state();

    assert_eq!(
        detector.update_display_time_status(8),
        GameLaunchStatus::Idle
    );
    assert_eq!(
        detector.update_display_time_status(9),
        GameLaunchStatus::Started
    );
}
