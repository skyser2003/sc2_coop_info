use image::{Rgba, RgbaImage};
use sco_tauri_overlay::{
    ImageprocTodayWinBonusDigitReader, TodayWinBonusDetection, TodayWinBonusDetector,
    TodayWinBonusDigitReader,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

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
    let mut fixture_paths = fs::read_dir(&fixture_dir)
        .unwrap_or_else(|error| {
            panic!(
                "today win bonus fixture group directory should read: {}: {error}",
                fixture_dir.display()
            )
        })
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
fn detects_all_first_win_fixture_images_without_external_ocr() {
    let fixture_paths = first_win_fixture_paths("true");
    assert!(
        !fixture_paths.is_empty(),
        "expected at least one positive today win bonus fixture image"
    );
    for fixture_path in fixture_paths {
        assert_detects_first_win_fixture(&fixture_path);
    }
}

#[test]
fn ignores_all_non_first_win_fixture_images_without_external_ocr() {
    let fixture_paths = first_win_fixture_paths("false");
    assert!(
        !fixture_paths.is_empty(),
        "expected at least one negative today win bonus fixture image"
    );
    for fixture_path in fixture_paths {
        assert_does_not_detect_first_win_fixture(&fixture_path);
    }
}
