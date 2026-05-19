use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use image::{GrayImage, Luma, Rgba, RgbaImage};
use imageproc::region_labelling::{Connectivity, connected_components};
use std::collections::{BTreeMap, VecDeque};
use xcap::{Monitor, Window};

use crate::{AppSettings, FirstWinBonusTimerPayload, MonitorSettingsOps};

pub const TODAY_WIN_BONUS_SETTINGS_KEY: &str = "latest_today_win_bonus_time";
pub const FIRST_WIN_BONUS_COOLDOWN_HOURS: i64 = 22;
pub const FIRST_WIN_BONUS_COOLDOWN_SECONDS: u64 = FIRST_WIN_BONUS_COOLDOWN_HOURS as u64 * 60 * 60;

const TARGET_TODAY_WIN_BONUS_XP: u32 = 10_000;

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
            return Self::new(true, 0, None);
        };
        let Ok(latest_acquired_time) = DateTime::parse_from_rfc3339(latest_acquired_time) else {
            return Self::new(true, 0, None);
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
        let Some(window) = Self::focused_sc2_window()? else {
            return Ok(None);
        };
        let image = Self::capture_focused_window_visible_region(&window)?;
        let reader = ImageprocTodayWinBonusDigitReader;
        Self::detect_in_left_half_with_reader(&image, &reader).map(Some)
    }

    pub fn focused_sc2_window_active() -> Result<bool, String> {
        Self::focused_sc2_window().map(|window| window.is_some())
    }

    pub fn is_sc2_window_identity(app_name: &str, title: &str) -> bool {
        let normalized_app_name = app_name.trim().to_ascii_lowercase();
        let normalized_title = title.trim().to_ascii_lowercase();

        normalized_app_name == "sc2.exe"
            || normalized_app_name == "sc2_x64.exe"
            || normalized_app_name == "starcraft ii.exe"
            || normalized_app_name == "starcraft ii"
            || normalized_title == "starcraft ii"
    }

    fn focused_sc2_window() -> Result<Option<Window>, String> {
        let windows = Window::all()
            .map_err(|error| format!("Failed to enumerate windows for SC2 capture: {error}"))?;

        Ok(windows.into_iter().find(|window| {
            if !window.is_focused().unwrap_or(false) || window.is_minimized().unwrap_or(true) {
                return false;
            }

            let app_name = window.app_name().unwrap_or_default();
            let title = window.title().unwrap_or_default();
            Self::is_sc2_window_identity(&app_name, &title)
        }))
    }

    fn capture_focused_window_visible_region(window: &Window) -> Result<RgbaImage, String> {
        let monitor = window
            .current_monitor()
            .map_err(|error| format!("Failed to resolve focused SC2 monitor: {error}"))?;
        let window_rect = ScreenRect::new(
            window
                .x()
                .map_err(|error| format!("Failed to read focused SC2 window x: {error}"))?,
            window
                .y()
                .map_err(|error| format!("Failed to read focused SC2 window y: {error}"))?,
            window
                .width()
                .map_err(|error| format!("Failed to read focused SC2 window width: {error}"))?,
            window
                .height()
                .map_err(|error| format!("Failed to read focused SC2 window height: {error}"))?,
        )
        .ok_or_else(|| "Focused SC2 window has invalid dimensions".to_string())?;
        let monitor_rect = ScreenRect::new(
            monitor
                .x()
                .map_err(|error| format!("Failed to read focused SC2 monitor x: {error}"))?,
            monitor
                .y()
                .map_err(|error| format!("Failed to read focused SC2 monitor y: {error}"))?,
            monitor
                .width()
                .map_err(|error| format!("Failed to read focused SC2 monitor width: {error}"))?,
            monitor
                .height()
                .map_err(|error| format!("Failed to read focused SC2 monitor height: {error}"))?,
        )
        .ok_or_else(|| "Focused SC2 monitor has invalid dimensions".to_string())?;
        let region = Self::monitor_capture_region_for_window(window_rect, monitor_rect)
            .ok_or_else(|| "Focused SC2 window is outside its current monitor".to_string())?;

        monitor
            .capture_region(region.x(), region.y(), region.width(), region.height())
            .map_err(|error| format!("Failed to capture focused SC2 monitor region: {error}"))
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

        let index = MonitorSettingsOps::selected_monitor_index(requested_monitor, monitors.len())
            .ok_or_else(|| "No monitors detected for OCR".to_string())?;

        monitors
            .get(index)
            .cloned()
            .ok_or_else(|| "Selected monitor was not found for OCR".to_string())
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
