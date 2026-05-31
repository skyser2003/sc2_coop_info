use chrono::{TimeZone, Utc};
use image::{Rgba, RgbaImage};
use sco_tauri_overlay::{
    FirstWinBonusAcquiredTime, FirstWinBonusTimerStatus, ImageprocTodayWinBonusDigitReader,
    ScreenRect, TodayWinBonusCaptureFallbackState, TodayWinBonusDetection, TodayWinBonusDetector,
    TodayWinBonusDigitReader, WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration as StdDuration, UNIX_EPOCH};

struct SequenceDigitReader {
    values: RefCell<VecDeque<Option<u32>>>,
    calls: Cell<usize>,
}

impl SequenceDigitReader {
    fn new(values: Vec<Option<u32>>) -> Self {
        Self {
            values: RefCell::new(values.into()),
            calls: Cell::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.get()
    }
}

impl TodayWinBonusDigitReader for SequenceDigitReader {
    fn read_xp_value(&self, _line_image: &RgbaImage) -> Result<Option<u32>, String> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.values.borrow_mut().pop_front().flatten())
    }
}

fn draw_rect(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    for yy in y..(y + height) {
        for xx in x..(x + width) {
            image.put_pixel(xx, yy, color);
        }
    }
}

fn draw_green_label(image: &mut RgbaImage, x: u32, y: u32) {
    let green = Rgba([112, 236, 78, 255]);
    draw_rect(image, x, y, 92, 4, green);
    draw_rect(image, x + 5, y + 6, 118, 4, green);
    draw_rect(image, x + 1, y + 12, 76, 4, green);
}

fn draw_xp_line(image: &mut RgbaImage, x: u32, y: u32) {
    let white_green = Rgba([218, 255, 210, 255]);
    draw_rect(image, x, y, 135, 5, white_green);
    draw_rect(image, x + 2, y + 8, 152, 5, white_green);
    draw_rect(image, x + 4, y + 16, 126, 5, white_green);
}

fn detect(image: &RgbaImage, reader: &SequenceDigitReader) -> TodayWinBonusDetection {
    TodayWinBonusDetector::detect_in_left_half_with_reader(image, reader)
        .expect("synthetic detection should not fail")
}

fn today_win_bonus_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("today_win_bonus")
}

fn today_win_bonus_fixture_group_dir(group_name: &str) -> PathBuf {
    today_win_bonus_fixture_dir().join(group_name)
}

fn is_supported_image_fixture(path: &Path) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => matches!(
            extension.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg"
        ),
        None => false,
    }
}

fn first_win_fixture_paths(group_name: &str) -> Vec<PathBuf> {
    let fixture_dir = today_win_bonus_fixture_group_dir(group_name);
    let entries = match fs::read_dir(&fixture_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            panic!(
                "today win bonus fixture group directory should read: {}: {error}",
                fixture_dir.display()
            )
        }
    };

    let mut fixture_paths = entries
        .map(|entry_result| {
            entry_result
                .unwrap_or_else(|error| {
                    panic!("today win bonus fixture entry should read: {error}")
                })
                .path()
        })
        .filter(|path| path.is_file() && is_supported_image_fixture(path))
        .collect::<Vec<_>>();
    fixture_paths.sort();
    fixture_paths
}

fn detect_first_win_fixture(fixture_path: &Path) -> TodayWinBonusDetection {
    let image = image::open(fixture_path)
        .unwrap_or_else(|error| {
            panic!(
                "first win fixture should load: {}: {error}",
                fixture_path.display()
            )
        })
        .to_rgba8();
    let reader = ImageprocTodayWinBonusDigitReader;

    TodayWinBonusDetector::detect_in_left_half_with_reader(&image, &reader).unwrap_or_else(
        |error| {
            panic!(
                "fixture detection should not fail: {}: {error}",
                fixture_path.display()
            )
        },
    )
}

fn assert_detects_first_win_fixture(fixture_path: &Path) {
    let detection = detect_first_win_fixture(fixture_path);

    assert!(
        detection.found_today_win_bonus(),
        "fixture should detect today's win bonus: {}",
        fixture_path.display()
    );
    assert_eq!(
        detection.xp(),
        Some(10_000),
        "fixture should detect 10,000 XP: {}",
        fixture_path.display()
    );
}

fn assert_does_not_detect_first_win_fixture(fixture_path: &Path) {
    let detection = detect_first_win_fixture(fixture_path);

    assert!(
        !detection.found_today_win_bonus(),
        "fixture should not detect today's win bonus: {}",
        fixture_path.display()
    );
    assert_eq!(
        detection.xp(),
        None,
        "fixture should not detect XP: {}",
        fixture_path.display()
    );
}

#[test]
fn detects_only_green_label_with_ten_thousand_xp_underneath() {
    let mut image = RgbaImage::from_pixel(500, 280, Rgba([0, 0, 0, 255]));
    draw_green_label(&mut image, 40, 70);
    draw_xp_line(&mut image, 40, 96);
    draw_green_label(&mut image, 42, 150);
    draw_xp_line(&mut image, 42, 176);
    let reader = SequenceDigitReader::new(vec![Some(5_500), Some(10_000)]);

    let detection = detect(&image, &reader);

    assert!(detection.found_today_win_bonus());
    assert_eq!(detection.xp(), Some(10_000));
    assert_eq!(reader.calls(), 2);
}

#[test]
fn ignores_ten_thousand_without_green_label_pair() {
    let mut image = RgbaImage::from_pixel(500, 220, Rgba([0, 0, 0, 255]));
    draw_xp_line(&mut image, 40, 96);
    let reader = SequenceDigitReader::new(vec![Some(10_000)]);

    let detection = detect(&image, &reader);

    assert!(!detection.found_today_win_bonus());
    assert_eq!(reader.calls(), 0);
}

#[test]
fn ignores_ten_thousand_when_it_is_not_directly_under_green_label() {
    let mut image = RgbaImage::from_pixel(500, 300, Rgba([0, 0, 0, 255]));
    draw_green_label(&mut image, 40, 70);
    draw_xp_line(&mut image, 40, 220);
    let reader = SequenceDigitReader::new(vec![Some(10_000)]);

    let detection = detect(&image, &reader);

    assert!(!detection.found_today_win_bonus());
    assert_eq!(reader.calls(), 0);
}

#[test]
fn normalizes_common_ocr_digit_shapes() {
    assert_eq!(
        TodayWinBonusDetector::normalize_xp_value("+1O,OOO XP"),
        Some(10_000)
    );
    assert_eq!(TodayWinBonusDetector::normalize_xp_value("XP"), None);
}

#[test]
fn identifies_sc2_window_names_for_focused_capture() {
    assert!(TodayWinBonusDetector::is_sc2_window_identity(
        "SC2_x64.exe",
        ""
    ));
    assert!(TodayWinBonusDetector::is_sc2_window_identity(
        "StarCraft II",
        ""
    ));
    assert!(TodayWinBonusDetector::is_sc2_window_identity(
        "",
        "StarCraft II"
    ));
    assert!(!TodayWinBonusDetector::is_sc2_window_identity(
        "notepad.exe",
        "StarCraft notes"
    ));
}

#[test]
fn monitor_capture_region_uses_visible_window_intersection() {
    let region = TodayWinBonusDetector::monitor_capture_region_for_window(
        ScreenRect::new(1900, 100, 400, 300).expect("valid window rect"),
        ScreenRect::new(1920, 0, 1920, 1080).expect("valid monitor rect"),
    )
    .expect("window intersects monitor");

    assert_eq!(region.x(), 0);
    assert_eq!(region.y(), 100);
    assert_eq!(region.width(), 380);
    assert_eq!(region.height(), 300);
}

#[test]
fn monitor_capture_region_rejects_window_outside_monitor() {
    assert_eq!(
        TodayWinBonusDetector::monitor_capture_region_for_window(
            ScreenRect::new(0, 0, 100, 100).expect("valid window rect"),
            ScreenRect::new(1920, 0, 1920, 1080).expect("valid monitor rect")
        ),
        None
    );
}

#[test]
fn rejects_blank_window_capture_as_unusable() {
    let image = RgbaImage::from_pixel(320, 180, Rgba([0, 0, 0, 255]));

    assert!(!TodayWinBonusDetector::capture_image_looks_usable(&image));
}

#[test]
fn accepts_varied_window_capture_as_usable() {
    let mut image = RgbaImage::from_pixel(320, 180, Rgba([0, 0, 0, 255]));
    draw_rect(&mut image, 30, 30, 90, 40, Rgba([40, 90, 180, 255]));
    draw_rect(&mut image, 180, 80, 70, 50, Rgba([220, 210, 180, 255]));

    assert!(TodayWinBonusDetector::capture_image_looks_usable(&image));
}

#[test]
fn window_capture_fallback_starts_after_five_failures() {
    let mut fallback_state = TodayWinBonusCaptureFallbackState::new();

    for expected_failures in 1..WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK {
        fallback_state.record_window_capture_failure();

        assert_eq!(
            fallback_state.consecutive_window_capture_failures(),
            expected_failures
        );
        assert!(!fallback_state.region_capture_fallback());
        assert!(fallback_state.should_try_window_capture());
    }

    fallback_state.record_window_capture_failure();

    assert_eq!(
        fallback_state.consecutive_window_capture_failures(),
        WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK
    );
    assert!(fallback_state.region_capture_fallback());
    assert!(!fallback_state.should_try_window_capture());
}

#[test]
fn window_capture_success_resets_failure_count() {
    let mut fallback_state = TodayWinBonusCaptureFallbackState::new();
    fallback_state.record_window_capture_failure();
    fallback_state.record_window_capture_failure();

    fallback_state.record_window_capture_success();

    assert_eq!(fallback_state.consecutive_window_capture_failures(), 0);
    assert!(!fallback_state.region_capture_fallback());
    assert!(fallback_state.should_try_window_capture());
}

#[test]
fn window_capture_reports_initial_capture_method() {
    #[cfg(windows)]
    assert_eq!(
        sco_tauri_overlay::TodayWinBonusWindowCapture::initial_capture_method(),
        "gdi_window_dc"
    );

    #[cfg(not(windows))]
    assert_eq!(
        sco_tauri_overlay::TodayWinBonusWindowCapture::initial_capture_method(),
        "monitor_region"
    );
}

#[test]
fn window_capture_reports_selected_fallback_method() {
    let mut fallback_state = TodayWinBonusCaptureFallbackState::new();

    assert_eq!(fallback_state.selected_fallback_method(), "none");
    assert_eq!(
        fallback_state.active_capture_method(),
        sco_tauri_overlay::TodayWinBonusWindowCapture::initial_capture_method()
    );

    for _ in 0..WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK {
        fallback_state.record_window_capture_failure();
    }

    assert_eq!(fallback_state.selected_fallback_method(), "monitor_region");
    assert_eq!(fallback_state.active_capture_method(), "monitor_region");
}

#[test]
fn first_win_bonus_timer_uses_twenty_two_hour_cooldown() {
    let now = Utc
        .with_ymd_and_hms(2026, 5, 19, 12, 0, 0)
        .single()
        .expect("valid test time");
    let latest = Utc
        .with_ymd_and_hms(2026, 5, 19, 0, 0, 0)
        .single()
        .expect("valid latest time")
        .to_rfc3339();

    let status = FirstWinBonusTimerStatus::from_latest_acquired_time(Some(&latest), now);

    assert!(!status.available());
    assert_eq!(status.seconds_until_available(), 10 * 60 * 60);
}

#[test]
fn first_win_bonus_timer_is_unavailable_without_saved_time() {
    let now = Utc
        .with_ymd_and_hms(2026, 5, 19, 12, 0, 0)
        .single()
        .expect("valid test time");

    let status = FirstWinBonusTimerStatus::from_latest_acquired_time(None, now);

    assert!(!status.available());
    assert_eq!(status.seconds_until_available(), 0);
}

#[test]
fn first_win_bonus_timer_is_unavailable_with_invalid_saved_time() {
    let now = Utc
        .with_ymd_and_hms(2026, 5, 19, 12, 0, 0)
        .single()
        .expect("valid test time");

    let status = FirstWinBonusTimerStatus::from_latest_acquired_time(Some("not-a-time"), now);

    assert!(!status.available());
    assert_eq!(status.seconds_until_available(), 0);
}

#[test]
fn first_win_bonus_acquired_time_formats_replay_file_modified_time() {
    let replay_file_modified_time = u64::try_from(
        Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0)
            .single()
            .expect("valid replay file modified time")
            .timestamp(),
    )
    .expect("test replay file modified time should be positive");

    let saved_time = FirstWinBonusAcquiredTime::from_replay_file_modified_time_seconds(
        replay_file_modified_time,
    )
    .expect("non-zero replay file modified time should be accepted");

    assert_eq!(
        saved_time.replay_file_modified_time_seconds(),
        replay_file_modified_time
    );
    assert_eq!(
        saved_time.to_rfc3339().as_deref(),
        Some("2026-05-19T00:00:00Z")
    );
}

#[test]
fn first_win_bonus_acquired_time_formats_system_time_as_utc() {
    let replay_file_modified_time = u64::try_from(
        Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0)
            .single()
            .expect("valid replay file modified time")
            .timestamp(),
    )
    .expect("test replay file modified time should be positive");
    let system_time = UNIX_EPOCH + StdDuration::from_secs(replay_file_modified_time);

    let saved_time = FirstWinBonusAcquiredTime::from_system_time(system_time)
        .expect("system time should convert")
        .expect("non-zero replay file modified time should be accepted");

    assert_eq!(
        saved_time.to_rfc3339().as_deref(),
        Some("2026-05-19T00:00:00Z")
    );
}

#[test]
fn first_win_bonus_acquired_time_selects_latest_replay_file_modified_time() {
    let older = u64::try_from(
        Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0)
            .single()
            .expect("valid older replay time")
            .timestamp(),
    )
    .expect("older replay time should be positive");
    let newer = u64::try_from(
        Utc.with_ymd_and_hms(2026, 5, 19, 1, 0, 0)
            .single()
            .expect("valid newer replay time")
            .timestamp(),
    )
    .expect("newer replay time should be positive");

    let saved_time =
        FirstWinBonusAcquiredTime::latest_replay_file_modified_time(Some(older), Some(newer))
            .expect("latest replay file modified time should be selected");

    assert_eq!(saved_time.replay_file_modified_time_seconds(), newer);
}

#[test]
fn first_win_bonus_acquired_time_uses_cache_fallback_only_when_observed_time_is_missing() {
    let observed = u64::try_from(
        Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0)
            .single()
            .expect("valid observed replay time")
            .timestamp(),
    )
    .expect("observed replay time should be positive");
    let fallback = u64::try_from(
        Utc.with_ymd_and_hms(2026, 5, 19, 1, 0, 0)
            .single()
            .expect("valid fallback replay time")
            .timestamp(),
    )
    .expect("fallback replay time should be positive");

    let fallback_saved_time =
        FirstWinBonusAcquiredTime::latest_replay_time_with_fallback(None, None, Some(fallback))
            .expect("cache fallback replay time should be used when observed time is missing");
    assert_eq!(
        fallback_saved_time.replay_file_modified_time_seconds(),
        fallback
    );

    let observed_saved_time = FirstWinBonusAcquiredTime::latest_replay_time_with_fallback(
        None,
        Some(observed),
        Some(fallback),
    )
    .expect("observed replay time should be used before cache fallback");
    assert_eq!(
        observed_saved_time.replay_file_modified_time_seconds(),
        observed
    );
}

#[test]
fn first_win_bonus_acquired_time_rejects_missing_replay_file_modified_time() {
    assert_eq!(
        FirstWinBonusAcquiredTime::from_replay_file_modified_time_seconds(0),
        None
    );
    assert_eq!(
        FirstWinBonusAcquiredTime::latest_replay_file_modified_time(None, Some(0)),
        None
    );
}

#[test]
fn detects_all_first_win_fixture_images_without_external_ocr() {
    let fixture_paths = first_win_fixture_paths("true");
    for fixture_path in fixture_paths {
        assert_detects_first_win_fixture(&fixture_path);
    }
}

#[test]
fn ignores_all_non_first_win_fixture_images_without_external_ocr() {
    let fixture_paths = first_win_fixture_paths("false");
    for fixture_path in fixture_paths {
        assert_does_not_detect_first_win_fixture(&fixture_path);
    }
}
