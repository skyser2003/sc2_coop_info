use rfd::FileDialog;
use std::path::Path;
use tauri::Wry;

use crate::{PathManagerOps, TauriOverlayOps, overlay_info, performance_overlay};

pub struct SystemCommands;

#[tauri::command]
pub fn performance_start_drag(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    SystemCommands::performance_start_drag(app)
}

#[tauri::command]
pub async fn pick_folder(
    title: String,
    directory: Option<String>,
) -> Result<Option<String>, String> {
    SystemCommands::pick_folder(title, directory).await
}

#[tauri::command]
pub fn is_dev() -> bool {
    SystemCommands::is_dev()
}

#[tauri::command]
pub fn save_overlay_screenshot(path: String, png_bytes: Vec<u8>) -> Result<(), String> {
    SystemCommands::save_overlay_screenshot(path, png_bytes)
}

#[tauri::command]
pub fn open_folder_path(path: String) -> Result<(), String> {
    SystemCommands::open_folder_path(path)
}

impl SystemCommands {
    pub fn performance_start_drag(app: tauri::AppHandle<Wry>) -> Result<(), String> {
        performance_overlay::PerformanceOverlayOps::start_drag(&app)
    }

    pub async fn pick_folder(
        title: String,
        directory: Option<String>,
    ) -> Result<Option<String>, String> {
        let start_directory = TauriOverlayOps::folder_dialog_start_directory(directory);
        tauri::async_runtime::spawn_blocking(move || {
            let mut dialog = FileDialog::new().set_title(&title);
            if let Some(start_directory) = start_directory.as_ref() {
                dialog = dialog.set_directory(start_directory);
            }

            Ok(dialog
                .pick_folder()
                .map(|selected| selected.to_string_lossy().to_string()))
        })
        .await
        .map_err(|error| format!("Failed to open folder picker: {error}"))?
    }

    pub fn is_dev() -> bool {
        PathManagerOps::is_dev_env()
    }

    pub fn save_overlay_screenshot(path: String, png_bytes: Vec<u8>) -> Result<(), String> {
        overlay_info::OverlayInfoOps::save_overlay_screenshot(Path::new(&path), &png_bytes)
    }

    pub fn open_folder_path(path: String) -> Result<(), String> {
        overlay_info::OverlayInfoOps::open_folder_in_explorer(&path)
    }
}
