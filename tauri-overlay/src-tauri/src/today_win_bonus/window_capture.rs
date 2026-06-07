use super::*;

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
