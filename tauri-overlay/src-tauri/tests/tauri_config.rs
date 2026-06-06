use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Deserialize)]
struct TauriConfig {
    app: TauriAppConfig,
}

impl TauriConfig {
    fn load() -> Self {
        let config_text =
            std::fs::read_to_string(Self::path()).expect("tauri.conf.json should be readable");
        serde_json::from_str(&config_text).expect("tauri.conf.json should be valid JSON")
    }

    fn path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json")
    }
}

struct FrontendIndex;

impl FrontendIndex {
    fn load() -> String {
        std::fs::read_to_string(Self::path()).expect("frontend index.html should be readable")
    }

    fn path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("index.html")
    }
}

#[derive(Deserialize)]
struct TauriAppConfig {
    security: TauriSecurityConfig,
}

#[derive(Deserialize)]
struct TauriSecurityConfig {
    csp: String,
    capabilities: Vec<TauriCapability>,
}

impl TauriSecurityConfig {
    fn csp_directives(&self) -> HashMap<String, HashSet<String>> {
        self.csp
            .split(';')
            .filter_map(|directive| {
                let mut tokens = directive.split_whitespace();
                let name = tokens.next()?;
                let values = tokens.map(ToString::to_string).collect();
                Some((name.to_string(), values))
            })
            .collect()
    }

    fn capability(&self, identifier: &str) -> Option<&TauriCapability> {
        self.capabilities
            .iter()
            .find(|capability| capability.matches_identifier(identifier))
    }
}

#[derive(Deserialize)]
struct TauriCapability {
    identifier: String,
    permissions: Vec<TauriPermission>,
}

impl TauriCapability {
    fn matches_identifier(&self, identifier: &str) -> bool {
        self.identifier == identifier
    }

    fn permission_identifiers(&self) -> HashSet<&str> {
        self.permissions
            .iter()
            .map(TauriPermission::identifier)
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TauriPermission {
    Identifier(String),
    Scoped(TauriScopedPermission),
}

impl TauriPermission {
    fn identifier(&self) -> &str {
        match self {
            Self::Identifier(identifier) => identifier,
            Self::Scoped(permission) => permission.identifier(),
        }
    }
}

#[derive(Deserialize)]
struct TauriScopedPermission {
    identifier: String,
}

impl TauriScopedPermission {
    fn identifier(&self) -> &str {
        &self.identifier
    }
}

#[test]
fn packaged_csp_allows_overlay_screenshot_data_images() {
    let config = TauriConfig::load();
    let directives = config.app.security.csp_directives();
    let img_src = directives
        .get("img-src")
        .expect("packaged CSP should declare img-src");

    assert!(img_src.contains("'self'"));
    assert!(img_src.contains("data:"));
    assert!(img_src.contains("blob:"));
}

#[test]
fn packaged_csp_allows_overlay_font_assets() {
    let config = TauriConfig::load();
    let directives = config.app.security.csp_directives();
    let font_src = directives
        .get("font-src")
        .expect("packaged CSP should declare font-src");

    assert!(font_src.contains("'self'"));
    assert!(font_src.contains("data:"));
}

#[test]
fn packaged_index_contains_initial_google_analytics_tag() {
    let index_html = FrontendIndex::load();

    assert!(index_html.contains("https://www.googletagmanager.com/gtag/js?id=G-K12WZBGJF7"));
    assert!(index_html.contains("<script nonce=\"c2NvT3ZlcmxheUdh\">"));
    assert!(index_html.contains("gtag(\"config\", \"G-K12WZBGJF7\");"));
}

#[test]
fn packaged_csp_allows_initial_google_analytics_tag() {
    let config = TauriConfig::load();
    let directives = config.app.security.csp_directives();
    let script_src = directives
        .get("script-src")
        .expect("packaged CSP should declare script-src");
    let connect_src = directives
        .get("connect-src")
        .expect("packaged CSP should declare connect-src");
    let img_src = directives
        .get("img-src")
        .expect("packaged CSP should declare img-src");

    assert!(script_src.contains("'self'"));
    assert!(script_src.contains("'nonce-c2NvT3ZlcmxheUdh'"));
    assert!(script_src.contains("https://*.googletagmanager.com"));
    assert!(!script_src.contains("'unsafe-inline'"));

    assert!(connect_src.contains("'self'"));
    assert!(connect_src.contains("data:"));
    assert!(connect_src.contains("https://*.google-analytics.com"));
    assert!(connect_src.contains("https://*.analytics.google.com"));
    assert!(connect_src.contains("https://*.googletagmanager.com"));

    assert!(img_src.contains("https://*.google-analytics.com"));
    assert!(img_src.contains("https://*.googletagmanager.com"));
}

#[test]
fn packaged_capability_allows_autostart_plugin_commands() {
    let config = TauriConfig::load();
    let capability = config
        .app
        .security
        .capability("event-handler")
        .expect("event-handler capability should be configured");
    let permissions = capability.permission_identifiers();

    assert!(permissions.contains("autostart:default"));
}
