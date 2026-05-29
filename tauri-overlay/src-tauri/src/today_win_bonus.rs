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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ComponentDraft {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    pixels: u32,
}

impl ComponentDraft {
    fn new(x: u32, y: u32) -> Self {
        Self {
            left: x,
            top: y,
            right: x.saturating_add(1),
            bottom: y.saturating_add(1),
            pixels: 1,
        }
    }

    fn include(&mut self, x: u32, y: u32) {
        self.left = self.left.min(x);
        self.top = self.top.min(y);
        self.right = self.right.max(x.saturating_add(1));
        self.bottom = self.bottom.max(y.saturating_add(1));
        self.pixels = self.pixels.saturating_add(1);
    }

    fn into_component(self, has_hole: bool) -> Option<GlyphComponent> {
        ImageRect::from_bounds(self.left, self.top, self.right, self.bottom)
            .map(|rect| GlyphComponent::new(rect, self.pixels, has_hole))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GlyphComponent {
    rect: ImageRect,
    pixels: u32,
    has_hole: bool,
}

impl GlyphComponent {
    fn new(rect: ImageRect, pixels: u32, has_hole: bool) -> Self {
        Self {
            rect,
            pixels,
            has_hole,
        }
    }

    fn rect(&self) -> ImageRect {
        self.rect
    }

    fn pixels(&self) -> u32 {
        self.pixels
    }

    fn has_hole(&self) -> bool {
        self.has_hole
    }
}

pub struct ImageprocTodayWinBonusDigitReader;

impl ImageprocTodayWinBonusDigitReader {
    fn line_to_binary(line_image: &RgbaImage) -> GrayImage {
        let mut binary = GrayImage::new(line_image.width(), line_image.height());
        for y in 0..line_image.height() {
            for x in 0..line_image.width() {
                let value = if Self::is_digit_core_pixel(line_image.get_pixel(x, y)) {
                    255
                } else {
                    0
                };
                binary.put_pixel(x, y, Luma([value]));
            }
        }

        binary
    }

    fn components(binary: &GrayImage) -> Vec<GlyphComponent> {
        let labels = connected_components(binary, Connectivity::Eight, Luma([0_u8]));
        let mut drafts = BTreeMap::<u32, ComponentDraft>::new();

        for (x, y, pixel) in labels.enumerate_pixels() {
            let label = pixel.0[0];
            if label == 0 {
                continue;
            }
            drafts
                .entry(label)
                .and_modify(|draft| draft.include(x, y))
                .or_insert_with(|| ComponentDraft::new(x, y));
        }

        let max_height = drafts
            .values()
            .map(|draft| draft.bottom.saturating_sub(draft.top))
            .max()
            .unwrap_or(0);
        if max_height == 0 {
            return Vec::new();
        }

        let mut components = drafts
            .into_values()
            .filter_map(|draft| {
                let rect =
                    ImageRect::from_bounds(draft.left, draft.top, draft.right, draft.bottom)?;
                if rect.height() < 5 || rect.height().saturating_mul(100) < max_height * 50 {
                    return None;
                }
                if rect.width() < 2 || rect.width() > binary.width() / 3 {
                    return None;
                }
                if draft.pixels < 12 {
                    return None;
                }

                draft.into_component(Self::component_has_hole(binary, &rect))
            })
            .collect::<Vec<_>>();

        components.sort_by(|left, right| {
            left.rect()
                .x()
                .cmp(&right.rect().x())
                .then(left.rect().y().cmp(&right.rect().y()))
        });
        components
    }

    fn contains_ten_thousand_pattern(components: &[GlyphComponent]) -> bool {
        if Self::contains_low_resolution_ten_thousand_pattern(components) {
            return true;
        }

        if components.len() < 5 {
            return false;
        }

        let baseline_height = components
            .iter()
            .map(|component| component.rect().height())
            .max()
            .unwrap_or(0);
        if baseline_height == 0 {
            return false;
        }

        let digit_components = Self::without_digit_separators(components, baseline_height);
        if digit_components.len() < 5 {
            return false;
        }

        for start in 0..digit_components.len().saturating_sub(4) {
            let window = &digit_components[start..(start + 5)];
            if !Self::is_one_like(&window[0], baseline_height) {
                continue;
            }
            if !window[1..]
                .iter()
                .all(|component| Self::is_zero_like(component, baseline_height))
            {
                continue;
            }
            if Self::digit_gaps_are_reasonable(window, baseline_height) {
                return true;
            }
        }

        false
    }

    fn without_digit_separators(
        components: &[GlyphComponent],
        baseline_height: u32,
    ) -> Vec<GlyphComponent> {
        components
            .iter()
            .copied()
            .filter(|component| !Self::is_digit_separator_like(component, baseline_height))
            .collect()
    }

    fn is_digit_separator_like(component: &GlyphComponent, baseline_height: u32) -> bool {
        let rect = component.rect();
        rect.width().saturating_mul(100) <= baseline_height.saturating_mul(45)
            && rect.height().saturating_mul(100) <= baseline_height.saturating_mul(80)
    }

    fn contains_low_resolution_ten_thousand_pattern(components: &[GlyphComponent]) -> bool {
        let max_height = components
            .iter()
            .map(|component| component.rect().height())
            .max()
            .unwrap_or(0);
        if !(5..=9).contains(&max_height) {
            return false;
        }

        Self::contains_low_resolution_split_digit_pattern(components, max_height)
            || Self::contains_low_resolution_merged_zero_pattern(components, max_height)
    }

    fn contains_low_resolution_split_digit_pattern(
        components: &[GlyphComponent],
        max_height: u32,
    ) -> bool {
        if components.len() != 6 {
            return false;
        }

        let Some(first) = components.first() else {
            return false;
        };
        let Some(last) = components.last() else {
            return false;
        };

        let span = last.rect().right().saturating_sub(first.rect().x());
        if span > max_height.saturating_mul(12) {
            return false;
        }

        components.iter().all(|component| {
            let rect = component.rect();
            let height_close = rect.height().saturating_mul(100) >= max_height.saturating_mul(75);
            let width_reasonable =
                rect.width() >= 2 && rect.width() <= max_height.saturating_mul(2);
            let fill_reasonable = component.pixels().saturating_mul(100)
                <= rect
                    .width()
                    .saturating_mul(rect.height())
                    .saturating_mul(90);

            height_close && width_reasonable && fill_reasonable
        })
    }

    fn contains_low_resolution_merged_zero_pattern(
        components: &[GlyphComponent],
        max_height: u32,
    ) -> bool {
        if components.len() < 3 {
            return false;
        }

        components.windows(3).any(|window| {
            let one = window[0];
            let first_zero = window[1];
            let merged_zeroes = window[2];
            Self::is_low_resolution_one_like(&one, max_height)
                && Self::is_low_resolution_single_zero_like(&first_zero, max_height)
                && Self::is_low_resolution_merged_zeroes_like(&merged_zeroes, max_height)
                && Self::digit_gaps_are_reasonable(window, max_height)
        })
    }

    fn is_low_resolution_one_like(component: &GlyphComponent, max_height: u32) -> bool {
        let rect = component.rect();
        rect.height().saturating_mul(100) >= max_height.saturating_mul(75)
            && rect.width() >= 2
            && rect.width() <= max_height
            && component.pixels().saturating_mul(100)
                <= rect
                    .width()
                    .saturating_mul(rect.height())
                    .saturating_mul(85)
    }

    fn is_low_resolution_single_zero_like(component: &GlyphComponent, max_height: u32) -> bool {
        let rect = component.rect();
        rect.height().saturating_mul(100) >= max_height.saturating_mul(75)
            && rect.width() >= max_height
            && rect.width() <= max_height.saturating_mul(2)
            && component.pixels().saturating_mul(100)
                <= rect
                    .width()
                    .saturating_mul(rect.height())
                    .saturating_mul(90)
    }

    fn is_low_resolution_merged_zeroes_like(component: &GlyphComponent, max_height: u32) -> bool {
        let rect = component.rect();
        rect.height().saturating_mul(100) >= max_height.saturating_mul(75)
            && rect.width() >= max_height.saturating_mul(3)
            && rect.width() <= max_height.saturating_mul(6)
            && component.pixels().saturating_mul(100)
                <= rect
                    .width()
                    .saturating_mul(rect.height())
                    .saturating_mul(90)
    }

    fn digit_gaps_are_reasonable(window: &[GlyphComponent], baseline_height: u32) -> bool {
        window.windows(2).all(|pair| {
            let left = pair[0].rect();
            let right = pair[1].rect();
            if right.x() <= left.right() {
                return true;
            }
            right.x().saturating_sub(left.right()) <= baseline_height.saturating_mul(2)
        })
    }

    fn is_one_like(component: &GlyphComponent, baseline_height: u32) -> bool {
        let rect = component.rect();
        if rect.height().saturating_mul(100) < baseline_height.saturating_mul(65) {
            return false;
        }
        if component.has_hole() {
            return false;
        }

        rect.width().saturating_mul(100) <= rect.height().saturating_mul(75)
            && component.pixels().saturating_mul(100)
                <= rect
                    .width()
                    .saturating_mul(rect.height())
                    .saturating_mul(72)
    }

    fn is_zero_like(component: &GlyphComponent, baseline_height: u32) -> bool {
        let rect = component.rect();
        if rect.height().saturating_mul(100) < baseline_height.saturating_mul(65) {
            return false;
        }
        if !component.has_hole() {
            return false;
        }

        let width_to_height = rect.width().saturating_mul(100) / rect.height().max(1);
        (42..=130).contains(&width_to_height)
    }

    fn component_has_hole(binary: &GrayImage, rect: &ImageRect) -> bool {
        if rect.width() < 5 || rect.height() < 8 {
            return false;
        }

        let width = rect.width() as usize;
        let height = rect.height() as usize;
        let mut visited = vec![false; width.saturating_mul(height)];
        let mut queue = VecDeque::<(u32, u32)>::new();

        for x in 0..rect.width() {
            Self::enqueue_background(binary, rect, x, 0, &mut visited, &mut queue);
            Self::enqueue_background(
                binary,
                rect,
                x,
                rect.height().saturating_sub(1),
                &mut visited,
                &mut queue,
            );
        }
        for y in 0..rect.height() {
            Self::enqueue_background(binary, rect, 0, y, &mut visited, &mut queue);
            Self::enqueue_background(
                binary,
                rect,
                rect.width().saturating_sub(1),
                y,
                &mut visited,
                &mut queue,
            );
        }

        while let Some((x, y)) = queue.pop_front() {
            if x > 0 {
                Self::enqueue_background(binary, rect, x - 1, y, &mut visited, &mut queue);
            }
            if y > 0 {
                Self::enqueue_background(binary, rect, x, y - 1, &mut visited, &mut queue);
            }
            if x + 1 < rect.width() {
                Self::enqueue_background(binary, rect, x + 1, y, &mut visited, &mut queue);
            }
            if y + 1 < rect.height() {
                Self::enqueue_background(binary, rect, x, y + 1, &mut visited, &mut queue);
            }
        }

        for y in 1..rect.height().saturating_sub(1) {
            for x in 1..rect.width().saturating_sub(1) {
                if Self::is_binary_background(binary, rect, x, y)
                    && !visited[Self::visited_index(rect.width(), x, y)]
                {
                    return true;
                }
            }
        }

        false
    }

    fn enqueue_background(
        binary: &GrayImage,
        rect: &ImageRect,
        x: u32,
        y: u32,
        visited: &mut [bool],
        queue: &mut VecDeque<(u32, u32)>,
    ) {
        let index = Self::visited_index(rect.width(), x, y);
        if visited[index] || !Self::is_binary_background(binary, rect, x, y) {
            return;
        }

        visited[index] = true;
        queue.push_back((x, y));
    }

    fn visited_index(width: u32, x: u32, y: u32) -> usize {
        y as usize * width as usize + x as usize
    }

    fn is_binary_background(binary: &GrayImage, rect: &ImageRect, x: u32, y: u32) -> bool {
        binary.get_pixel(rect.x() + x, rect.y() + y).0[0] == 0
    }

    fn is_digit_core_pixel(pixel: &Rgba<u8>) -> bool {
        let [r, g, b, a] = pixel.0;
        if a < 32 {
            return false;
        }

        let max_channel = r.max(g).max(b);
        let min_channel = r.min(g).min(b);
        let white_core = min_channel >= 170 && max_channel >= 205;
        let green_core = g >= 190 && r >= 120 && b >= 80;
        let blue_white_core = b >= 205 && r >= 130 && g >= 150;

        white_core || green_core || blue_white_core
    }
}

impl TodayWinBonusDigitReader for ImageprocTodayWinBonusDigitReader {
    fn read_xp_value(&self, line_image: &RgbaImage) -> Result<Option<u32>, String> {
        let binary = Self::line_to_binary(line_image);
        let components = Self::components(&binary);
        if Self::contains_ten_thousand_pattern(&components) {
            Ok(Some(TARGET_TODAY_WIN_BONUS_XP))
        } else {
            Ok(None)
        }
    }
}

pub struct TodayWinBonusDetector;

impl TodayWinBonusDetector {
    pub fn capture_selected_monitor_detection(
        settings: &AppSettings,
    ) -> Result<TodayWinBonusDetection, String> {
        let image = Self::capture_selected_monitor_left_half(settings.monitor())?;
        let reader = ImageprocTodayWinBonusDigitReader;
        Self::detect_in_reward_region_with_reader(&image, &reader)
    }

    pub fn capture_focused_sc2_window_detection() -> Result<Option<TodayWinBonusDetection>, String>
    {
        TodayWinBonusWindowCapture::new().capture_focused_sc2_window_detection()
    }

    pub fn focused_sc2_window_active() -> Result<bool, String> {
        ActiveWindowDetector::focused_window_is_sc2()
    }

    pub fn focused_sc2_window_rect() -> Result<Option<ScreenRect>, String> {
        Self::active_sc2_window_rect()
    }

    pub fn sc2_window_rect() -> Result<Option<ScreenRect>, String> {
        Self::active_sc2_window_rect()
    }

    pub fn is_sc2_window_identity(app_name: &str, title: &str) -> bool {
        ActiveWindowDetector::is_sc2_window_identity(app_name, title)
    }

    fn active_sc2_window_rect() -> Result<Option<ScreenRect>, String> {
        let Some(info) = ActiveWindowDetector::focused_sc2_window_info()? else {
            return Ok(None);
        };
        let Some(rect) = info.rect() else {
            return Ok(None);
        };

        ScreenRect::new(rect.x(), rect.y(), rect.width(), rect.height())
            .ok_or_else(|| "Active SC2 window has invalid dimensions".to_string())
            .map(Some)
    }

    #[cfg(not(windows))]
    fn capture_focused_window_visible_region(window_rect: ScreenRect) -> Result<RgbaImage, String> {
        let monitor = Self::capture_monitor_for_window(window_rect)?;
        let monitor_rect =
            ScreenRect::new(monitor.x(), monitor.y(), monitor.width(), monitor.height())
                .ok_or_else(|| "Focused SC2 monitor has invalid dimensions".to_string())?;
        let region = Self::monitor_capture_region_for_window(window_rect, monitor_rect)
            .ok_or_else(|| "Focused SC2 window is outside its current monitor".to_string())?;

        monitor
            .monitor()
            .capture_region(region.x(), region.y(), region.width(), region.height())
            .map_err(|error| format!("Failed to capture focused SC2 monitor region: {error}"))
    }

    #[cfg(not(windows))]
    fn capture_monitor_for_window(window_rect: ScreenRect) -> Result<CaptureMonitor, String> {
        Self::capture_monitors()?
            .into_iter()
            .filter_map(|monitor| {
                let monitor_rect =
                    ScreenRect::new(monitor.x(), monitor.y(), monitor.width(), monitor.height())?;
                let area = Self::screen_rect_intersection_area(window_rect, monitor_rect);
                (area > 0).then_some((area, monitor))
            })
            .max_by_key(|(area, _monitor)| *area)
            .map(|(_area, monitor)| monitor)
            .ok_or_else(|| "Focused SC2 window does not intersect any monitor".to_string())
    }

    pub fn capture_image_looks_usable(image: &RgbaImage) -> bool {
        if image.width() < 16 || image.height() < 16 {
            return false;
        }

        let step_x = (image.width() / 96).max(1);
        let step_y = (image.height() / 96).max(1);
        let mut sampled = 0_u32;
        let mut min_luma = u8::MAX;
        let mut max_luma = u8::MIN;
        let mut min_red = u8::MAX;
        let mut max_red = u8::MIN;
        let mut min_green = u8::MAX;
        let mut max_green = u8::MIN;
        let mut min_blue = u8::MAX;
        let mut max_blue = u8::MIN;

        for y in (0..image.height()).step_by(step_y as usize) {
            for x in (0..image.width()).step_by(step_x as usize) {
                let [red, green, blue, alpha] = image.get_pixel(x, y).0;
                if alpha < 32 {
                    continue;
                }

                sampled = sampled.saturating_add(1);
                min_red = min_red.min(red);
                max_red = max_red.max(red);
                min_green = min_green.min(green);
                max_green = max_green.max(green);
                min_blue = min_blue.min(blue);
                max_blue = max_blue.max(blue);

                let luma = ((u16::from(red) * 30 + u16::from(green) * 59 + u16::from(blue) * 11)
                    / 100) as u8;
                min_luma = min_luma.min(luma);
                max_luma = max_luma.max(luma);
            }
        }

        if sampled < 64 {
            return false;
        }

        let luma_range = max_luma.saturating_sub(min_luma);
        let color_range = max_red
            .saturating_sub(min_red)
            .max(max_green.saturating_sub(min_green))
            .max(max_blue.saturating_sub(min_blue));

        luma_range >= 12 || color_range >= 20
    }

    pub fn monitor_capture_region_for_window(
        window_rect: ScreenRect,
        monitor_rect: ScreenRect,
    ) -> Option<MonitorCaptureRegion> {
        let window_left = i64::from(window_rect.x());
        let window_top = i64::from(window_rect.y());
        let window_right = window_left.checked_add(i64::from(window_rect.width()))?;
        let window_bottom = window_top.checked_add(i64::from(window_rect.height()))?;

        let monitor_left = i64::from(monitor_rect.x());
        let monitor_top = i64::from(monitor_rect.y());
        let monitor_right = monitor_left.checked_add(i64::from(monitor_rect.width()))?;
        let monitor_bottom = monitor_top.checked_add(i64::from(monitor_rect.height()))?;

        let capture_left = window_left.max(monitor_left);
        let capture_top = window_top.max(monitor_top);
        let capture_right = window_right.min(monitor_right);
        let capture_bottom = window_bottom.min(monitor_bottom);

        if capture_right <= capture_left || capture_bottom <= capture_top {
            return None;
        }

        MonitorCaptureRegion::new(
            u32::try_from(capture_left - monitor_left).ok()?,
            u32::try_from(capture_top - monitor_top).ok()?,
            u32::try_from(capture_right - capture_left).ok()?,
            u32::try_from(capture_bottom - capture_top).ok()?,
        )
    }

    #[cfg(not(windows))]
    fn screen_rect_intersection_area(left: ScreenRect, right: ScreenRect) -> u64 {
        let left_x = i64::from(left.x());
        let left_y = i64::from(left.y());
        let left_right = left_x.saturating_add(i64::from(left.width()));
        let left_bottom = left_y.saturating_add(i64::from(left.height()));
        let right_x = i64::from(right.x());
        let right_y = i64::from(right.y());
        let right_right = right_x.saturating_add(i64::from(right.width()));
        let right_bottom = right_y.saturating_add(i64::from(right.height()));

        let width = left_right.min(right_right) - left_x.max(right_x);
        let height = left_bottom.min(right_bottom) - left_y.max(right_y);
        if width <= 0 || height <= 0 {
            return 0;
        }

        u64::try_from(width)
            .ok()
            .and_then(|width| u64::try_from(height).ok().map(|height| width * height))
            .unwrap_or(0)
    }

    fn capture_selected_monitor_left_half(requested_monitor: usize) -> Result<RgbaImage, String> {
        let monitor = Self::selected_capture_monitor(requested_monitor)?;
        let width = (monitor.width() / 2).max(1);
        let height = monitor.height().max(1);
        monitor
            .monitor()
            .capture_region(0, 0, width, height)
            .map_err(|error| format!("Failed to capture monitor left half: {error}"))
    }

    fn selected_capture_monitor(requested_monitor: usize) -> Result<CaptureMonitor, String> {
        let monitors = Self::capture_monitors()?;

        let index = MonitorSettingsOps::selected_monitor_index(requested_monitor, monitors.len())
            .ok_or_else(|| "No monitors detected for OCR".to_string())?;

        monitors
            .get(index)
            .cloned()
            .ok_or_else(|| "Selected monitor was not found for OCR".to_string())
    }

    fn capture_monitors() -> Result<Vec<CaptureMonitor>, String> {
        let mut monitors = Monitor::all()
            .map_err(|error| format!("Failed to enumerate monitors for OCR: {error}"))?
            .into_iter()
            .filter_map(|monitor| CaptureMonitor::from_monitor(monitor).ok())
            .collect::<Vec<_>>();

        monitors.sort_by(|left, right| {
            left.x()
                .cmp(&right.x())
                .then(left.y().cmp(&right.y()))
                .then(left.name().cmp(right.name()))
        });

        Ok(monitors)
    }

    pub fn detect_in_left_half_with_reader<R: TodayWinBonusDigitReader>(
        image: &RgbaImage,
        reader: &R,
    ) -> Result<TodayWinBonusDetection, String> {
        let left_width = (image.width() / 2).max(1);
        let left_half =
            image::imageops::crop_imm(image, 0, 0, left_width, image.height()).to_image();

        Self::detect_in_reward_region_with_reader(&left_half, reader)
    }

    pub fn detect_in_reward_region_with_reader<R: TodayWinBonusDigitReader>(
        image: &RgbaImage,
        reader: &R,
    ) -> Result<TodayWinBonusDetection, String> {
        for label_rect in Self::find_green_label_rows(image) {
            let Some(xp_rect) = Self::find_xp_row_below(image, &label_rect) else {
                continue;
            };
            let ocr_rect = xp_rect
                .expanded(16, 5, 48, 5)
                .clamped(image.width(), image.height())
                .unwrap_or(xp_rect);
            let line_image = image::imageops::crop_imm(
                image,
                ocr_rect.x(),
                ocr_rect.y(),
                ocr_rect.width(),
                ocr_rect.height(),
            )
            .to_image();
            let Some(xp) = reader.read_xp_value(&line_image)? else {
                continue;
            };
            if xp == TARGET_TODAY_WIN_BONUS_XP {
                return Ok(TodayWinBonusDetection::found(xp));
            }
        }

        Ok(TodayWinBonusDetection::not_found())
    }

    pub fn normalize_xp_value(text: &str) -> Option<u32> {
        let digits = text
            .chars()
            .filter_map(|value| {
                if value.is_ascii_digit() {
                    Some(value)
                } else {
                    match value {
                        'O' | 'o' => Some('0'),
                        'I' | 'l' | '|' => Some('1'),
                        _ => None,
                    }
                }
            })
            .collect::<String>();

        if digits.is_empty() {
            return None;
        }

        digits.parse::<u32>().ok()
    }

    fn find_green_label_rows(image: &RgbaImage) -> Vec<ImageRect> {
        let search_right = (image.width() / 2).max(1);
        let min_pixels = (search_right / 120).max(8);
        Self::row_ranges_with_pixels(
            image,
            0,
            image.height(),
            0,
            search_right,
            min_pixels,
            Self::is_green_label_pixel,
        )
        .into_iter()
        .filter_map(|range| {
            if range.height() > 36 {
                return None;
            }
            let rect = Self::pixel_bounds_for_range(
                image,
                &range,
                0,
                search_right,
                Self::is_green_label_pixel,
            )?;
            if rect.width() < 18 || rect.height() < 4 {
                return None;
            }
            Some(rect)
        })
        .collect()
    }

    fn find_xp_row_below(image: &RgbaImage, label_rect: &ImageRect) -> Option<ImageRect> {
        let search_top = label_rect.bottom().saturating_add(3);
        if search_top >= image.height() {
            return None;
        }

        let max_gap = (image.height() / 12).clamp(48, 96);
        let search_bottom = search_top.saturating_add(max_gap).min(image.height());
        let search_left = label_rect.x().saturating_sub(40);
        let search_right = label_rect.right().saturating_add(280).min(image.width());
        if search_right <= search_left {
            return None;
        }

        let min_pixels = (label_rect.width() / 5).max(10);
        let ranges = Self::row_ranges_with_pixels(
            image,
            search_top,
            search_bottom,
            search_left,
            search_right,
            min_pixels,
            Self::is_xp_line_pixel,
        );

        ranges.into_iter().find_map(|range| {
            if !(6..=48).contains(&range.height()) {
                return None;
            }
            let rect = Self::pixel_bounds_for_range(
                image,
                &range,
                search_left,
                search_right,
                Self::is_xp_line_pixel,
            )?;
            if rect.width() < 32 {
                return None;
            }
            let has_expected_alignment = rect.x() <= label_rect.right().saturating_add(80)
                && rect.right() >= label_rect.x().saturating_sub(24);
            if !has_expected_alignment {
                return None;
            }
            Some(rect)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodayWinBonusCaptureFallbackState {
    consecutive_window_capture_failures: u8,
    region_capture_fallback: bool,
}

impl TodayWinBonusCaptureFallbackState {
    pub fn new() -> Self {
        Self {
            consecutive_window_capture_failures: 0,
            region_capture_fallback: false,
        }
    }

    pub fn consecutive_window_capture_failures(&self) -> u8 {
        self.consecutive_window_capture_failures
    }

    pub fn region_capture_fallback(&self) -> bool {
        self.region_capture_fallback
    }

    pub fn should_try_window_capture(&self) -> bool {
        !self.region_capture_fallback
    }

    pub fn selected_fallback_method(&self) -> &'static str {
        if self.region_capture_fallback() {
            CAPTURE_METHOD_MONITOR_REGION
        } else {
            CAPTURE_FALLBACK_METHOD_NONE
        }
    }

    pub fn active_capture_method(&self) -> &'static str {
        if self.region_capture_fallback() {
            CAPTURE_METHOD_MONITOR_REGION
        } else {
            TodayWinBonusWindowCapture::initial_capture_method()
        }
    }

    pub fn record_window_capture_success(&mut self) {
        self.consecutive_window_capture_failures = 0;
    }

    pub fn record_window_capture_failure(&mut self) {
        self.consecutive_window_capture_failures =
            self.consecutive_window_capture_failures.saturating_add(1);
        if self.consecutive_window_capture_failures
            >= WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK
        {
            self.region_capture_fallback = true;
        }
    }
}

impl Default for TodayWinBonusCaptureFallbackState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TodayWinBonusWindowCapture {
    fallback_state: TodayWinBonusCaptureFallbackState,
}

impl TodayWinBonusWindowCapture {
    pub fn new() -> Self {
        Self {
            fallback_state: TodayWinBonusCaptureFallbackState::new(),
        }
    }

    pub fn initial_capture_method() -> &'static str {
        #[cfg(windows)]
        {
            CAPTURE_METHOD_GDI_WINDOW_DC
        }
        #[cfg(not(windows))]
        {
            CAPTURE_METHOD_MONITOR_REGION
        }
    }

    pub fn fallback_state(&self) -> &TodayWinBonusCaptureFallbackState {
        &self.fallback_state
    }

    pub fn selected_fallback_method(&self) -> &'static str {
        self.fallback_state.selected_fallback_method()
    }

    pub fn active_capture_method(&self) -> &'static str {
        self.fallback_state.active_capture_method()
    }

    pub fn capture_focused_sc2_window_detection(
        &mut self,
    ) -> Result<Option<TodayWinBonusDetection>, String> {
        let image = self.capture_focused_window_image()?;
        let Some(image) = image else {
            return Ok(None);
        };
        let reader = ImageprocTodayWinBonusDigitReader;
        TodayWinBonusDetector::detect_in_left_half_with_reader(&image, &reader).map(Some)
    }

    #[cfg(windows)]
    fn capture_focused_window_image(&mut self) -> Result<Option<RgbaImage>, String> {
        let Some(window_rect) = TodayWinBonusDetector::focused_sc2_window_rect()? else {
            return Ok(None);
        };

        if self.fallback_state.should_try_window_capture() {
            match windows_gdi_window_dc_capture::capture_focused_sc2_window(window_rect) {
                Ok(image) if TodayWinBonusDetector::capture_image_looks_usable(&image) => {
                    self.fallback_state.record_window_capture_success();
                    return Ok(Some(image));
                }
                Ok(_image) => {
                    self.fallback_state.record_window_capture_failure();
                    if self.fallback_state.region_capture_fallback() {
                        return windows_gdi_window_dc_capture::capture_focused_sc2_window_region(
                            window_rect,
                        )
                        .map(Some);
                    }

                    return Err(format!(
                        "GDI window capture produced unusable image ({}/{})",
                        self.fallback_state.consecutive_window_capture_failures(),
                        WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK
                    ));
                }
                Err(error) => {
                    self.fallback_state.record_window_capture_failure();
                    if self.fallback_state.region_capture_fallback() {
                        return windows_gdi_window_dc_capture::capture_focused_sc2_window_region(
                            window_rect,
                        )
                        .map(Some);
                    }

                    return Err(format!(
                        "GDI window capture failed ({}/{}): {error}",
                        self.fallback_state.consecutive_window_capture_failures(),
                        WINDOW_CAPTURE_FAILURES_BEFORE_REGION_FALLBACK
                    ));
                }
            }
        }

        windows_gdi_window_dc_capture::capture_focused_sc2_window_region(window_rect).map(Some)
    }

    #[cfg(not(windows))]
    fn capture_focused_window_image(&mut self) -> Result<Option<RgbaImage>, String> {
        let Some(window_rect) = TodayWinBonusDetector::focused_sc2_window_rect()? else {
            return Ok(None);
        };

        TodayWinBonusDetector::capture_focused_window_visible_region(window_rect).map(Some)
    }
}

impl Default for TodayWinBonusWindowCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl TodayWinBonusDetector {
    fn row_ranges_with_pixels<F>(
        image: &RgbaImage,
        top: u32,
        bottom: u32,
        left: u32,
        right: u32,
        min_pixels: u32,
        predicate: F,
    ) -> Vec<RowRange>
    where
        F: Fn(&Rgba<u8>) -> bool,
    {
        let mut ranges = Vec::<RowRange>::new();
        let mut active_top = None::<u32>;
        let mut last_active = None::<u32>;

        for y in top..bottom {
            let count = (left..right)
                .filter(|x| predicate(image.get_pixel(*x, y)))
                .count();
            let active = count >= min_pixels as usize;
            if active {
                if active_top.is_none() {
                    active_top = Some(y);
                }
                last_active = Some(y);
                continue;
            }

            if let (Some(start), Some(last)) = (active_top, last_active)
                && y.saturating_sub(last) > 2
            {
                if let Some(range) = RowRange::new(start, last.saturating_add(1)) {
                    ranges.push(range);
                }
                active_top = None;
                last_active = None;
            }
        }

        if let (Some(start), Some(last)) = (active_top, last_active)
            && let Some(range) = RowRange::new(start, last.saturating_add(1))
        {
            ranges.push(range);
        }

        ranges
    }

    fn pixel_bounds_for_range<F>(
        image: &RgbaImage,
        range: &RowRange,
        left: u32,
        right: u32,
        predicate: F,
    ) -> Option<ImageRect>
    where
        F: Fn(&Rgba<u8>) -> bool,
    {
        let mut min_x = right;
        let mut min_y = range.bottom();
        let mut max_x = left;
        let mut max_y = range.top();
        let mut found = false;

        for y in range.top()..range.bottom() {
            for x in left..right {
                if !predicate(image.get_pixel(x, y)) {
                    continue;
                }
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }

        if !found {
            return None;
        }

        ImageRect::from_bounds(
            min_x,
            min_y,
            max_x.saturating_add(1),
            max_y.saturating_add(1),
        )
    }

    fn is_green_label_pixel(pixel: &Rgba<u8>) -> bool {
        let [r, g, b, a] = pixel.0;
        if a < 32 || g < 90 {
            return false;
        }

        let r = u16::from(r);
        let g = u16::from(g);
        let b = u16::from(b);

        g > r.saturating_add(24) && g > b.saturating_add(8)
    }

    fn is_xp_line_pixel(pixel: &Rgba<u8>) -> bool {
        let [r, g, b, a] = pixel.0;
        if a < 32 {
            return false;
        }

        let max_channel = r.max(g).max(b);
        let min_channel = r.min(g).min(b);
        let bright_text = max_channel >= 150 && max_channel.saturating_sub(min_channel) >= 18;
        let white_text = min_channel >= 170;
        let green_glow = g >= 115 && g > r.saturating_add(18) && g > b.saturating_add(4);

        bright_text || white_text || green_glow
    }
}

#[cfg(windows)]
mod windows_gdi_window_dc_capture {
    use image::RgbaImage;
    use std::mem;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetWindowDC, HBITMAP, HDC,
        HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsIconic};

    use super::ScreenRect;

    struct WindowDc {
        hwnd: HWND,
        hdc: HDC,
    }

    impl WindowDc {
        fn new(hwnd: HWND) -> Result<Self, String> {
            let hdc = unsafe { GetWindowDC(Some(hwnd)) };
            if hdc.is_invalid() {
                return Err("GetWindowDC failed for GDI window capture".to_string());
            }

            Ok(Self { hwnd, hdc })
        }

        fn hdc(&self) -> HDC {
            self.hdc
        }
    }

    impl Drop for WindowDc {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseDC(Some(self.hwnd), self.hdc);
            }
        }
    }

    struct ScreenDc {
        hdc: HDC,
    }

    impl ScreenDc {
        fn new() -> Result<Self, String> {
            let hdc = unsafe { GetDC(None) };
            if hdc.is_invalid() {
                return Err("GetDC failed for GDI screen capture".to_string());
            }

            Ok(Self { hdc })
        }

        fn hdc(&self) -> HDC {
            self.hdc
        }
    }

    impl Drop for ScreenDc {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseDC(None, self.hdc);
            }
        }
    }

    struct MemoryDc {
        hdc: HDC,
    }

    impl MemoryDc {
        fn new(source_dc: HDC) -> Result<Self, String> {
            let hdc = unsafe { CreateCompatibleDC(Some(source_dc)) };
            if hdc.is_invalid() {
                return Err("CreateCompatibleDC failed for GDI window capture".to_string());
            }

            Ok(Self { hdc })
        }

        fn hdc(&self) -> HDC {
            self.hdc
        }
    }

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteDC(self.hdc);
            }
        }
    }

    struct Bitmap {
        bitmap: HBITMAP,
    }

    impl Bitmap {
        fn new(source_dc: HDC, width: i32, height: i32) -> Result<Self, String> {
            let bitmap = unsafe { CreateCompatibleBitmap(source_dc, width, height) };
            if bitmap.is_invalid() {
                return Err("CreateCompatibleBitmap failed for GDI window capture".to_string());
            }

            Ok(Self { bitmap })
        }

        fn handle(&self) -> HBITMAP {
            self.bitmap
        }
    }

    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(self.bitmap.into());
            }
        }
    }

    struct SelectObjectGuard {
        hdc: HDC,
        previous_object: HGDIOBJ,
    }

    impl SelectObjectGuard {
        fn new(hdc: HDC, bitmap: HBITMAP) -> Result<Self, String> {
            let previous_object = unsafe { SelectObject(hdc, bitmap.into()) };
            if previous_object.is_invalid() {
                return Err("SelectObject failed for GDI window capture".to_string());
            }

            Ok(Self {
                hdc,
                previous_object,
            })
        }
    }

    impl Drop for SelectObjectGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.hdc, self.previous_object);
            }
        }
    }

    pub fn capture_focused_sc2_window(window_rect: ScreenRect) -> Result<RgbaImage, String> {
        let Some(hwnd) = focused_sc2_hwnd() else {
            return Err("Foreground window is not SC2".to_string());
        };
        let (width, height) = capture_dimensions(window_rect, "GDI window capture")?;

        capture_window_dc(hwnd, width, height)
    }

    pub fn capture_focused_sc2_window_region(window_rect: ScreenRect) -> Result<RgbaImage, String> {
        if focused_sc2_hwnd().is_none() {
            return Err("Foreground window is not SC2".to_string());
        }

        capture_screen_region(window_rect)
    }

    fn focused_sc2_hwnd() -> Option<HWND> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() || unsafe { IsIconic(hwnd).as_bool() } || !window_is_sc2(hwnd) {
            return None;
        }

        Some(hwnd)
    }

    fn window_is_sc2(hwnd: HWND) -> bool {
        crate::ActiveWindowDetector::windows_window_info(hwnd).is_sc2_window()
    }

    fn capture_dimensions(rect: ScreenRect, context: &str) -> Result<(i32, i32), String> {
        let width = i32::try_from(rect.width())
            .map_err(|_| format!("SC2 window is too wide for {context}"))?;
        let height = i32::try_from(rect.height())
            .map_err(|_| format!("SC2 window is too tall for {context}"))?;
        if width <= 0 || height <= 0 {
            return Err(format!("SC2 window has invalid bounds for {context}"));
        }

        Ok((width, height))
    }

    fn capture_window_dc(hwnd: HWND, width: i32, height: i32) -> Result<RgbaImage, String> {
        let window_dc = WindowDc::new(hwnd)?;
        capture_dc_region(window_dc.hdc(), 0, 0, width, height, "GDI window capture")
    }

    fn capture_screen_region(window_rect: ScreenRect) -> Result<RgbaImage, String> {
        let (width, height) = capture_dimensions(window_rect, "GDI screen capture")?;
        let screen_dc = ScreenDc::new()?;
        capture_dc_region(
            screen_dc.hdc(),
            window_rect.x(),
            window_rect.y(),
            width,
            height,
            "GDI screen capture",
        )
    }

    fn capture_dc_region(
        source_hdc: HDC,
        source_x: i32,
        source_y: i32,
        width: i32,
        height: i32,
        context: &str,
    ) -> Result<RgbaImage, String> {
        let memory_dc = MemoryDc::new(source_hdc)?;
        let bitmap = Bitmap::new(source_hdc, width, height)?;
        let _selected = SelectObjectGuard::new(memory_dc.hdc(), bitmap.handle())?;

        unsafe {
            BitBlt(
                memory_dc.hdc(),
                0,
                0,
                width,
                height,
                Some(source_hdc),
                source_x,
                source_y,
                SRCCOPY,
            )
            .map_err(|error| format!("BitBlt failed for {context}: {error}"))?;
        }

        to_rgba_image(memory_dc.hdc(), bitmap.handle(), width, height, context)
    }

    fn to_rgba_image(
        hdc: HDC,
        bitmap: HBITMAP,
        width: i32,
        height: i32,
        context: &str,
    ) -> Result<RgbaImage, String> {
        let width_u32 = u32::try_from(width).map_err(|_| "Invalid bitmap width".to_string())?;
        let height_u32 = u32::try_from(height).map_err(|_| "Invalid bitmap height".to_string())?;
        let buffer_size = width_u32
            .checked_mul(height_u32)
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| format!("{context} bitmap is too large"))?;
        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: buffer_size,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buffer = vec![0_u8; buffer_size as usize];

        let scan_lines = unsafe {
            GetDIBits(
                hdc,
                bitmap,
                0,
                height_u32,
                Some(buffer.as_mut_ptr().cast()),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            )
        };
        if scan_lines == 0 {
            return Err(format!("GetDIBits failed for {context}"));
        }

        for pixel in buffer.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }

        RgbaImage::from_raw(width_u32, height_u32, buffer)
            .ok_or_else(|| format!("RgbaImage::from_raw failed for {context}"))
    }
}
