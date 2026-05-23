use std::time::{SystemTime, UNIX_EPOCH};

use crate::TauriOverlayOps;

impl TauriOverlayOps {
    pub fn format_date_from_system_time(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }

    fn normalize_region_code(code: &str) -> Option<&'static str> {
        match code {
            "1" => Some("NA"),
            "2" => Some("EU"),
            "3" => Some("KR"),
            "4" => Some("SEA"),
            "5" => Some("CN"),
            "6" => Some("CN"),
            "8" => Some("KR"),
            "98" => Some("PTR"),
            _ => None,
        }
    }

    pub fn infer_region_from_handle(handle: &str) -> Option<String> {
        let region_code = handle.split('-').next().map(str::trim)?;
        if region_code.is_empty() {
            return None;
        }
        TauriOverlayOps::normalize_region_code(region_code).map(|region| region.to_string())
    }

    pub fn ratio(numerator: u64, denominator: u64) -> f64 {
        if denominator == 0 {
            0.0
        } else {
            numerator as f64 / denominator as f64
        }
    }

    pub fn median_f64(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));

        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            sorted[mid]
        } else {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        }
    }

    pub fn median_u64(values: &[u64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mut sorted = values.to_vec();
        sorted.sort_unstable();

        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            sorted[mid] as f64
        } else {
            (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
        }
    }

    pub fn kill_fraction(main_kills: u64, ally_kills: u64) -> f64 {
        let total = main_kills + ally_kills;
        if total == 0 {
            0.0
        } else {
            main_kills as f64 / total as f64
        }
    }

    pub fn result_is_victory(result: &str) -> Option<bool> {
        let normalized = result.trim().to_ascii_lowercase();
        if matches!(normalized.as_str(), "victory" | "win" | "1" | "true") {
            Some(true)
        } else if matches!(
            normalized.as_str(),
            "defeat" | "loss" | "lose" | "0" | "false"
        ) {
            Some(false)
        } else {
            None
        }
    }
}
