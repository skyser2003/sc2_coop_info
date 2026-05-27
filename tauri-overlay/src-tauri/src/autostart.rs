use tauri::{AppHandle, Wry};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_autostart::ManagerExt;

use crate::{AppSettings, TauriOverlayOps};

struct AutostartOps;

impl AutostartOps {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn sync_tauri_registration(app: &AppHandle<Wry>, enabled: bool) -> Result<(), String> {
        let manager = app.autolaunch();
        if enabled {
            manager
                .enable()
                .map_err(|error| format!("Failed to enable autostart: {error}"))
        } else {
            manager
                .disable()
                .map_err(|error| format!("Failed to disable autostart: {error}"))
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn sync_tauri_registration(_app: &AppHandle<Wry>, _enabled: bool) -> Result<(), String> {
        Ok(())
    }
}

impl AutostartOps {
    #[cfg(target_os = "windows")]
    fn remove_legacy_windows_registration_if_needed() -> Result<(), String> {
        use winreg::{
            RegKey,
            enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE},
        };

        if !TauriOverlayOps::should_remove_legacy_windows_startup_registration() {
            return Ok(());
        }

        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match current_user.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_QUERY_VALUE | KEY_SET_VALUE,
        ) {
            Ok(key) => key,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Failed to open legacy Windows startup registry key: {error}"
                ));
            }
        };

        let legacy_name = TauriOverlayOps::legacy_windows_startup_registration_name();
        match run_key.get_raw_value(legacy_name) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "Failed to query legacy Windows startup registry value: {error}"
                ));
            }
        }

        match run_key.delete_value(legacy_name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Failed to remove legacy Windows startup registry value: {error}"
            )),
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn remove_legacy_windows_registration_if_needed() -> Result<(), String> {
        Ok(())
    }
}

impl TauriOverlayOps {
    pub fn autostart_registration_name() -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    pub fn legacy_windows_startup_registration_name() -> &'static str {
        "SCO Overlay"
    }

    pub fn should_remove_legacy_windows_startup_registration() -> bool {
        Self::autostart_registration_name() != Self::legacy_windows_startup_registration_name()
    }

    pub fn sync_start_with_windows_registration(
        app: &AppHandle<Wry>,
        settings: &AppSettings,
    ) -> Result<(), String> {
        AutostartOps::sync_tauri_registration(app, settings.start_with_windows())?;
        AutostartOps::remove_legacy_windows_registration_if_needed()
    }
}
