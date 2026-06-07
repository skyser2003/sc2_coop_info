use super::*;

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
