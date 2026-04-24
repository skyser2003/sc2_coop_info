use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Deserialize)]
struct TauriConfig {
    app: TauriAppConfig,
}

#[derive(Deserialize)]
struct TauriAppConfig {
    security: TauriSecurityConfig,
}

#[derive(Deserialize)]
struct TauriSecurityConfig {
    csp: String,
}

fn tauri_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json")
}

fn tauri_config() -> TauriConfig {
    let config_text =
        std::fs::read_to_string(tauri_config_path()).expect("tauri.conf.json should be readable");
    serde_json::from_str(&config_text).expect("tauri.conf.json should be valid JSON")
}

fn csp_directives(csp: &str) -> HashMap<String, HashSet<String>> {
    csp.split(';')
        .filter_map(|directive| {
            let mut tokens = directive.split_whitespace();
            let name = tokens.next()?;
            let values = tokens.map(ToString::to_string).collect();
            Some((name.to_string(), values))
        })
        .collect()
}

#[test]
fn packaged_csp_allows_overlay_screenshot_data_images() {
    let config = tauri_config();
    let directives = csp_directives(&config.app.security.csp);
    let img_src = directives
        .get("img-src")
        .expect("packaged CSP should declare img-src");

    assert!(img_src.contains("'self'"));
    assert!(img_src.contains("data:"));
    assert!(img_src.contains("blob:"));
}

#[test]
fn packaged_csp_allows_overlay_font_assets() {
    let config = tauri_config();
    let directives = csp_directives(&config.app.security.csp);
    let font_src = directives
        .get("font-src")
        .expect("packaged CSP should declare font-src");

    assert!(font_src.contains("'self'"));
    assert!(font_src.contains("data:"));
}
