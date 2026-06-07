use super::*;

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
