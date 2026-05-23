use serde::Serialize;
use serde_json::Value;

use crate::{OverlayActionResponse, OverlayActionResult, TauriOverlayOps};

impl OverlayActionResponse {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: true,
                path: None,
            },
            message: message.into(),
            randomizer: None,
        }
    }

    pub fn success_with_path(message: impl Into<String>, path: String) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: true,
                path: Some(path),
            },
            message: message.into(),
            randomizer: None,
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: false,
                path: None,
            },
            message: message.into(),
            randomizer: None,
        }
    }

    pub fn failure_with_path(message: impl Into<String>, path: String) -> Self {
        Self {
            status: "ok",
            result: OverlayActionResult {
                ok: false,
                path: Some(path),
            },
            message: message.into(),
            randomizer: None,
        }
    }
}

impl TauriOverlayOps {
    pub fn to_json_value<T: Serialize>(value: T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}
