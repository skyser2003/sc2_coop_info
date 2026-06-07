use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, TimeZone, Utc};
use image::{GrayImage, Luma, Rgba, RgbaImage};
use imageproc::region_labelling::{Connectivity, connected_components};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use xcap::Monitor;

use crate::{ActiveWindowDetector, AppSettings, FirstWinBonusTimerPayload, MonitorSettingsOps};

mod detector;
mod digit_reader;
mod window_capture;

pub use detector::TodayWinBonusDetector;
pub use digit_reader::ImageprocTodayWinBonusDigitReader;
pub use window_capture::{TodayWinBonusCaptureFallbackState, TodayWinBonusWindowCapture};

pub const TODAY_WIN_BONUS_SETTINGS_KEY: &str = "latest_today_win_bonus_time";
pub const FIRST_WIN_BONUS_COOLDOWN_HOURS: i64 = 22;
pub const FIRST_WIN_BONUS_COOLDOWN_SECONDS: u64 = FIRST_WIN_BONUS_COOLDOWN_HOURS as u64 * 60 * 60;
pub const FIRST_WIN_BONUS_TIMER_POLL_INTERVAL: Duration = Duration::from_millis(500);

const TARGET_TODAY_WIN_BONUS_XP: u32 = 10_000;
pub const WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK: u8 = 5;
pub const CAPTURE_METHOD_GDI_WINDOW_DC: &str = "gdi_window_dc";
pub const CAPTURE_METHOD_MONITOR_REGION: &str = "monitor_region";
pub const CAPTURE_FALLBACK_METHOD_NONE: &str = "none";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodayWinBonusDetection {
    found_today_win_bonus: bool,
    xp: Option<u32>,
}

impl TodayWinBonusDetection {
    pub fn new(found_today_win_bonus: bool, xp: Option<u32>) -> Self {
        Self {
            found_today_win_bonus,
            xp,
        }
    }

    pub fn not_found() -> Self {
        Self::new(false, None)
    }

    pub fn found(xp: u32) -> Self {
        Self::new(true, Some(xp))
    }

    pub fn found_today_win_bonus(&self) -> bool {
        self.found_today_win_bonus
    }

    pub fn xp(&self) -> Option<u32> {
        self.xp
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstWinBonusTimerStatus {
    available: bool,
    seconds_until_available: u64,
    next_available_time: Option<DateTime<Utc>>,
}

impl FirstWinBonusTimerStatus {
    pub fn new(
        available: bool,
        seconds_until_available: u64,
        next_available_time: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            available,
            seconds_until_available,
            next_available_time,
        }
    }

    pub fn from_latest_acquired_time(
        latest_acquired_time: Option<&str>,
        now: DateTime<Utc>,
    ) -> Self {
        let Some(latest_acquired_time) = latest_acquired_time else {
            return Self::new(false, 0, None);
        };
        let Ok(latest_acquired_time) = DateTime::parse_from_rfc3339(latest_acquired_time) else {
            return Self::new(false, 0, None);
        };

        let next_available_time = latest_acquired_time.with_timezone(&Utc)
            + ChronoDuration::seconds(FIRST_WIN_BONUS_COOLDOWN_SECONDS as i64);
        let seconds_until_available = next_available_time
            .signed_duration_since(now)
            .num_seconds()
            .max(0) as u64;

        Self::new(
            seconds_until_available == 0,
            seconds_until_available,
            Some(next_available_time),
        )
    }

    pub fn available(&self) -> bool {
        self.available
    }

    pub fn seconds_until_available(&self) -> u64 {
        self.seconds_until_available
    }

    pub fn next_available_time(&self) -> Option<DateTime<Utc>> {
        self.next_available_time
    }

    pub fn into_payload(self, visible: bool) -> FirstWinBonusTimerPayload {
        FirstWinBonusTimerPayload {
            visible,
            available: self.available,
            seconds_until_available: self.seconds_until_available,
            next_available_time: self
                .next_available_time
                .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirstWinBonusAcquiredTime {
    replay_file_modified_time_seconds: u64,
}

impl FirstWinBonusAcquiredTime {
    pub fn from_replay_file_modified_time_seconds(
        replay_file_modified_time_seconds: u64,
    ) -> Option<Self> {
        if replay_file_modified_time_seconds == 0 {
            return None;
        }

        Some(Self {
            replay_file_modified_time_seconds,
        })
    }

    pub fn from_replay_file_modified_time(replay_file: &Path) -> Result<Option<Self>, String> {
        let modified = replay_file
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                format!(
                    "Failed to read replay file modified time '{}': {error}",
                    replay_file.display()
                )
            })?;
        Self::from_system_time(modified)
    }

    pub fn from_system_time(time: SystemTime) -> Result<Option<Self>, String> {
        let seconds = time
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("Replay file modified time is before Unix epoch: {error}"))?
            .as_secs();
        Ok(Self::from_replay_file_modified_time_seconds(seconds))
    }

    pub fn latest_replay_file_modified_time(
        first_replay_file_modified_time_seconds: Option<u64>,
        second_replay_file_modified_time_seconds: Option<u64>,
    ) -> Option<Self> {
        [
            first_replay_file_modified_time_seconds,
            second_replay_file_modified_time_seconds,
        ]
        .into_iter()
        .flatten()
        .filter(|seconds| *seconds > 0)
        .max()
        .and_then(Self::from_replay_file_modified_time_seconds)
    }

    pub fn latest_replay_time_with_fallback(
        first_replay_file_modified_time_seconds: Option<u64>,
        second_replay_file_modified_time_seconds: Option<u64>,
        fallback_replay_time_seconds: Option<u64>,
    ) -> Option<Self> {
        Self::latest_replay_file_modified_time(
            first_replay_file_modified_time_seconds,
            second_replay_file_modified_time_seconds,
        )
        .or_else(|| {
            fallback_replay_time_seconds
                .filter(|seconds| *seconds > 0)
                .and_then(Self::from_replay_file_modified_time_seconds)
        })
    }

    pub fn replay_file_modified_time_seconds(&self) -> u64 {
        self.replay_file_modified_time_seconds
    }

    pub fn to_rfc3339(&self) -> Option<String> {
        let seconds = i64::try_from(self.replay_file_modified_time_seconds).ok()?;
        Utc.timestamp_opt(seconds, 0)
            .single()
            .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImageRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl ImageRect {
    fn new(x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    fn from_bounds(left: u32, top: u32, right: u32, bottom: u32) -> Option<Self> {
        if right <= left || bottom <= top {
            return None;
        }

        Self::new(left, top, right - left, bottom - top)
    }

    fn x(&self) -> u32 {
        self.x
    }

    fn y(&self) -> u32 {
        self.y
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn right(&self) -> u32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(&self) -> u32 {
        self.y.saturating_add(self.height)
    }

    fn expanded(&self, left: u32, top: u32, right: u32, bottom: u32) -> Self {
        let x = self.x.saturating_sub(left);
        let y = self.y.saturating_sub(top);
        let expanded_right = self.right().saturating_add(right);
        let expanded_bottom = self.bottom().saturating_add(bottom);

        Self {
            x,
            y,
            width: expanded_right.saturating_sub(x),
            height: expanded_bottom.saturating_sub(y),
        }
    }

    fn clamped(&self, image_width: u32, image_height: u32) -> Option<Self> {
        let left = self.x.min(image_width);
        let top = self.y.min(image_height);
        let right = self.right().min(image_width);
        let bottom = self.bottom().min(image_height);
        Self::from_bounds(left, top, right, bottom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RowRange {
    top: u32,
    bottom: u32,
}

impl RowRange {
    fn new(top: u32, bottom: u32) -> Option<Self> {
        if bottom <= top {
            return None;
        }

        Some(Self { top, bottom })
    }

    fn top(&self) -> u32 {
        self.top
    }

    fn bottom(&self) -> u32 {
        self.bottom
    }

    fn height(&self) -> u32 {
        self.bottom - self.top
    }
}

#[derive(Clone)]
struct CaptureMonitor {
    monitor: Monitor,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl CaptureMonitor {
    fn from_monitor(monitor: Monitor) -> Result<Self, String> {
        let name = monitor
            .friendly_name()
            .or_else(|_| monitor.name())
            .unwrap_or_default();
        let x = monitor
            .x()
            .map_err(|error| format!("Failed to read monitor x: {error}"))?;
        let y = monitor
            .y()
            .map_err(|error| format!("Failed to read monitor y: {error}"))?;
        let width = monitor
            .width()
            .map_err(|error| format!("Failed to read monitor width: {error}"))?;
        let height = monitor
            .height()
            .map_err(|error| format!("Failed to read monitor height: {error}"))?;

        Ok(Self {
            monitor,
            name,
            x,
            y,
            width,
            height,
        })
    }

    fn monitor(&self) -> &Monitor {
        &self.monitor
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn x(&self) -> i32 {
        self.x
    }

    fn y(&self) -> i32 {
        self.y
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl ScreenRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn x(&self) -> i32 {
        self.x
    }

    pub fn y(&self) -> i32 {
        self.y
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorCaptureRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl MonitorCaptureRegion {
    fn new(x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }

        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn x(&self) -> u32 {
        self.x
    }

    pub fn y(&self) -> u32 {
        self.y
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

pub trait TodayWinBonusDigitReader {
    fn read_xp_value(&self, line_image: &RgbaImage) -> Result<Option<u32>, String>;
}
