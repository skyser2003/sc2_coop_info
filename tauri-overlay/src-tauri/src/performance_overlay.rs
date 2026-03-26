use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use sysinfo::{Networks, ProcessesToUpdate, System};
use tauri::{Emitter, Manager, Runtime, Wry};

use crate::shared_types::PerformanceVisibilityPayload;

static PERFORMANCE_EDIT_MODE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(crate) const PERFORMANCE_VISIBILITY_EVENT: &str = "sco://performance-visibility";

const DEFAULT_PERFORMANCE_PROCESSES: [&str; 2] = ["SC2_x64.exe", "SC2.exe"];
const DEFAULT_WINDOW_WIDTH: u32 = 780;
const MIN_WINDOW_WIDTH: u32 = 760;
const MIN_WINDOW_HEIGHT: u32 = 860;

#[derive(Serialize)]
struct CpuUsageRow {
    label: String,
    value: String,
    level: &'static str,
}

#[derive(Serialize)]
struct PerformancePayload {
    #[serde(rename = "processTitle")]
    process_title: String,
    #[serde(rename = "sc2Ram")]
    sc2_ram: String,
    #[serde(rename = "sc2Read")]
    sc2_read: String,
    #[serde(rename = "sc2ReadTotal")]
    sc2_read_total: String,
    #[serde(rename = "sc2Write")]
    sc2_write: String,
    #[serde(rename = "sc2WriteTotal")]
    sc2_write_total: String,
    #[serde(rename = "sc2Cpu")]
    sc2_cpu: String,
    #[serde(rename = "sc2CpuLevel")]
    sc2_cpu_level: &'static str,
    #[serde(rename = "systemRam")]
    system_ram: String,
    #[serde(rename = "systemRamLevel")]
    system_ram_level: &'static str,
    #[serde(rename = "systemDown")]
    system_down: String,
    #[serde(rename = "systemDownTotal")]
    system_down_total: String,
    #[serde(rename = "systemUp")]
    system_up: String,
    #[serde(rename = "systemUpTotal")]
    system_up_total: String,
    #[serde(rename = "cpuTotal")]
    cpu_total: String,
    #[serde(rename = "cpuTotalLevel")]
    cpu_total_level: &'static str,
    #[serde(rename = "cpuCores")]
    cpu_cores: Vec<CpuUsageRow>,
}

#[derive(Clone, Copy)]
struct PerformanceGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn required_window_height() -> u32 {
    let cpu_count = std::thread::available_parallelism()
        .map(|value| u32::try_from(value.get()).unwrap_or(16))
        .unwrap_or(16);
    let dynamic_height = 430u32.saturating_add(cpu_count.saturating_mul(28));
    dynamic_height.max(MIN_WINDOW_HEIGHT)
}

fn normalized_geometry(mut geometry: PerformanceGeometry) -> PerformanceGeometry {
    geometry.width = geometry.width.max(MIN_WINDOW_WIDTH);
    geometry.height = geometry.height.max(required_window_height());
    geometry
}

fn performance_show_enabled() -> bool {
    crate::read_settings_file().performance_show
}

fn performance_process_names() -> Vec<String> {
    let names = crate::read_settings_file()
        .performance_processes
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<String>>();
    if names.is_empty() {
        DEFAULT_PERFORMANCE_PROCESSES
            .iter()
            .map(|value| (*value).to_string())
            .collect()
    } else {
        names
    }
}

fn persist_setting_value(key: &str, value: Value) -> Result<(), String> {
    crate::persist_single_setting_value(key, value)
}

fn parse_saved_geometry() -> Option<PerformanceGeometry> {
    let geometry = crate::read_settings_file().performance_geometry?;
    let x = geometry[0];
    let y = geometry[1];
    let width = u32::try_from(geometry[2]).ok()?;
    let height = u32::try_from(geometry[3]).ok()?;

    Some(PerformanceGeometry {
        x,
        y,
        width,
        height,
    })
    .map(normalized_geometry)
}

fn default_geometry(window: &tauri::WebviewWindow<Wry>) -> Result<PerformanceGeometry, String> {
    let monitor_setting = crate::read_settings_file().monitor.max(1);
    let monitor_index = monitor_setting.saturating_sub(1);
    let monitors = window.available_monitors().unwrap_or_default();
    if monitors.is_empty() {
        return Err("No monitors detected".to_string());
    }

    let selected = if monitor_index < monitors.len() {
        &monitors[monitor_index]
    } else {
        &monitors[monitors.len().saturating_sub(1)]
    };
    let size = selected.size();
    let position = selected.position();

    let width = DEFAULT_WINDOW_WIDTH.min(size.width.max(MIN_WINDOW_WIDTH));
    let height = required_window_height().min(size.height.max(MIN_WINDOW_HEIGHT));
    let x = position.x + i32::try_from(size.width.saturating_sub(width)).unwrap_or(0) - 24;
    let y = position.y + 180;

    Ok(normalized_geometry(PerformanceGeometry {
        x,
        y,
        width,
        height,
    }))
}

fn current_geometry(window: &tauri::WebviewWindow<Wry>) -> Option<PerformanceGeometry> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(PerformanceGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

fn apply_geometry(
    window: &tauri::WebviewWindow<Wry>,
    geometry: PerformanceGeometry,
) -> Result<(), String> {
    let geometry = normalized_geometry(geometry);
    window
        .set_size(tauri::PhysicalSize {
            width: geometry.width,
            height: geometry.height,
        })
        .map_err(|error| format!("Failed to set performance overlay size: {error}"))?;
    window
        .set_position(tauri::PhysicalPosition {
            x: geometry.x,
            y: geometry.y,
        })
        .map_err(|error| format!("Failed to set performance overlay position: {error}"))?;
    Ok(())
}

fn level_from_percent(percent: f32) -> &'static str {
    if percent >= 85.0 {
        "high"
    } else if percent <= 15.0 {
        "low"
    } else {
        "normal"
    }
}

fn format_bytes(bytes: u64) -> String {
    let bytes_f = bytes as f64;
    if bytes_f < 0.3 * 1024.0 * 1024.0 {
        format!("{:.1} kB", bytes_f / 1024.0)
    } else if bytes_f < 0.7 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes_f / 1024.0 / 1024.0)
    } else {
        format!("{:.1} GB", bytes_f / 1024.0 / 1024.0 / 1024.0)
    }
}

fn default_payload(system: &System, networks: &Networks) -> PerformancePayload {
    let used_memory = system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let total_memory = system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let system_ram_percent = if system.total_memory() == 0 {
        0.0
    } else {
        (system.used_memory() as f32 / system.total_memory() as f32) * 100.0
    };
    let down_total = networks
        .iter()
        .map(|(_, network)| network.total_received())
        .sum::<u64>();
    let up_total = networks
        .iter()
        .map(|(_, network)| network.total_transmitted())
        .sum::<u64>();
    let down_speed = networks
        .iter()
        .map(|(_, network)| network.received())
        .sum::<u64>();
    let up_speed = networks
        .iter()
        .map(|(_, network)| network.transmitted())
        .sum::<u64>();
    let cpu_cores = system
        .cpus()
        .iter()
        .enumerate()
        .map(|(idx, cpu)| CpuUsageRow {
            label: format!("CPU{idx}"),
            value: format!("{:.1}%", cpu.cpu_usage()),
            level: level_from_percent(cpu.cpu_usage()),
        })
        .collect::<Vec<CpuUsageRow>>();
    let global_cpu = system.global_cpu_usage();

    PerformancePayload {
        process_title: "StarCraft II".to_string(),
        sc2_ram: "-".to_string(),
        sc2_read: "-".to_string(),
        sc2_read_total: "-".to_string(),
        sc2_write: "-".to_string(),
        sc2_write_total: "-".to_string(),
        sc2_cpu: "-".to_string(),
        sc2_cpu_level: "normal",
        system_ram: format!("{used_memory:.1}/{total_memory:.1} GB"),
        system_ram_level: level_from_percent(system_ram_percent),
        system_down: format!("{}/s", format_bytes(down_speed)),
        system_down_total: format_bytes(down_total),
        system_up: format!("{}/s", format_bytes(up_speed)),
        system_up_total: format_bytes(up_total),
        cpu_total: format!("{global_cpu:.1}%"),
        cpu_total_level: level_from_percent(global_cpu),
        cpu_cores,
    }
}

fn build_payload(system: &System, networks: &Networks) -> PerformancePayload {
    let mut payload = default_payload(system, networks);
    let process_names = performance_process_names();
    let process = system.processes().values().find(|candidate| {
        let process_name = candidate.name().to_string_lossy();
        process_names
            .iter()
            .any(|value| process_name.as_ref() == value.as_str())
    });

    let Some(process) = process else {
        return payload;
    };

    let process_name = process.name().to_string_lossy().into_owned();
    let disk_usage = process.disk_usage();
    let process_cpu = process.cpu_usage();
    let process_memory_mb = process.memory() as f64 / 1024.0 / 1024.0;
    let process_memory_percent = if system.total_memory() == 0 {
        0.0
    } else {
        (process.memory() as f32 / system.total_memory() as f32) * 100.0
    };

    payload.process_title = if process_name.contains("SC2") {
        "StarCraft II".to_string()
    } else {
        process_name
    };
    payload.sc2_ram = format!("{process_memory_percent:.0}% | {process_memory_mb:.0} MB");
    payload.sc2_read = format!("{}/s", format_bytes(disk_usage.read_bytes));
    payload.sc2_read_total = format_bytes(disk_usage.total_read_bytes);
    payload.sc2_write = format!("{}/s", format_bytes(disk_usage.written_bytes));
    payload.sc2_write_total = format_bytes(disk_usage.total_written_bytes);
    payload.sc2_cpu = format!("{process_cpu:.1}%");
    payload.sc2_cpu_level = level_from_percent(process_cpu);
    payload
}

pub(crate) fn emit_performance_script<R: Runtime>(app: &tauri::AppHandle<R>, script: &str) {
    if let Some(window) = app.get_webview_window("performance") {
        let _ = window.eval(script);
    }
}

pub(crate) fn apply_saved_geometry(window: &tauri::WebviewWindow<Wry>) -> Result<(), String> {
    let geometry = parse_saved_geometry()
        .map(Ok)
        .unwrap_or_else(|| default_geometry(window))?;
    apply_geometry(window, geometry)
}

pub(crate) fn persist_geometry(window: &tauri::WebviewWindow<Wry>) {
    let Some(geometry) = current_geometry(window) else {
        return;
    };
    let geometry = normalized_geometry(geometry);
    let width = i32::try_from(geometry.width).unwrap_or(i32::MAX);
    let height = i32::try_from(geometry.height).unwrap_or(i32::MAX);
    let value = Value::Array(vec![
        Value::from(geometry.x),
        Value::from(geometry.y),
        Value::from(width),
        Value::from(height),
    ]);
    if let Err(error) = persist_setting_value("performance_geometry", value) {
        crate::sco_log!("[SCO/performance] Failed to save geometry: {error}");
    }
}

pub(crate) fn start_drag(app: &tauri::AppHandle<Wry>) -> Result<(), String> {
    let Some(window) = app.get_webview_window("performance") else {
        return Err("Performance window not available".to_string());
    };
    window
        .start_dragging()
        .map_err(|error| format!("Failed to start dragging performance overlay: {error}"))
}

pub(crate) fn show_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("performance") {
        let _ = window.show();
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.set_focusable(false);
    }
}

pub(crate) fn hide_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    PERFORMANCE_EDIT_MODE.store(false, std::sync::atomic::Ordering::Release);
    if let Some(window) = app.get_webview_window("performance") {
        let _ =
            window.eval("window.setPerformanceEditMode && window.setPerformanceEditMode(false);");
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.set_focusable(false);
        let _ = window.hide();
    }
}

pub(crate) fn set_edit_mode<R: Runtime>(app: &tauri::AppHandle<R>, enabled: bool) {
    PERFORMANCE_EDIT_MODE.store(enabled, std::sync::atomic::Ordering::Release);

    let performance_visible = performance_show_enabled();
    if let Some(window) = app.get_webview_window("performance") {
        if enabled {
            let _ = window.show();
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_focusable(true);
            let _ = window.set_focus();
        } else {
            let _ = window.set_ignore_cursor_events(true);
            let _ = window.set_focusable(false);
            if !performance_visible {
                let _ = window.hide();
            }
        }
    }

    let script = format!(
        "window.setPerformanceEditMode && window.setPerformanceEditMode({});",
        if enabled { "true" } else { "false" }
    );
    emit_performance_script(app, &script);
}

pub(crate) fn toggle_edit_mode<R: Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let next = !PERFORMANCE_EDIT_MODE.load(std::sync::atomic::Ordering::Acquire);
    set_edit_mode(app, next);
    next
}

pub(crate) fn set_visibility<R: Runtime>(
    app: &tauri::AppHandle<R>,
    visible: bool,
    persist_setting: bool,
) -> Result<(), String> {
    if persist_setting {
        persist_setting_value("performance_show", Value::Bool(visible))?;
    }

    if visible {
        show_window(app);
    } else {
        hide_window(app);
    }
    emit_visibility_event(app, visible);
    Ok(())
}

fn emit_visibility_event<R: Runtime>(app: &tauri::AppHandle<R>, visible: bool) {
    let payload = PerformanceVisibilityPayload { visible };
    let _ = app.emit(PERFORMANCE_VISIBILITY_EVENT, payload.clone());
    if let Some(config_window) = app.get_webview_window("config") {
        let _ = config_window.emit(PERFORMANCE_VISIBILITY_EVENT, payload);
        let visible_script = if visible { "true" } else { "false" };
        let _ = config_window.eval(&format!(
            "window.__scoSetPerformanceVisibility && window.__scoSetPerformanceVisibility({visible_script});"
        ));
    }
}

pub(crate) fn apply_settings(app: &tauri::AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("performance") {
        if let Err(error) = apply_saved_geometry(&window) {
            crate::sco_log!("[SCO/performance] Failed to apply geometry: {error}");
        }
    }

    if performance_show_enabled() {
        show_window(app);
    } else {
        hide_window(app);
    }
}

pub(crate) fn spawn_monitor(app: tauri::AppHandle<Wry>) {
    thread::spawn(move || {
        let mut system = System::new_all();
        let mut networks = Networks::new_with_refreshed_list();

        loop {
            system.refresh_cpu_all();
            system.refresh_memory();
            let _ = system.refresh_processes(ProcessesToUpdate::All, true);
            networks.refresh(true);

            let should_emit = performance_show_enabled()
                || PERFORMANCE_EDIT_MODE.load(std::sync::atomic::Ordering::Acquire);
            if should_emit {
                let payload = build_payload(&system, &networks);
                let encoded = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
                emit_performance_script(
                    &app,
                    &format!(
                        "window.updatePerformanceStats && window.updatePerformanceStats({encoded});"
                    ),
                );
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}
