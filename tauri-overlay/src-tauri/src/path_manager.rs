use std::{
    env::{self, current_exe},
    path::PathBuf,
};

pub fn is_dev_env() -> bool {
    !is_prod_env()
}

fn is_windows() -> bool {
    cfg!(windows)
}

fn is_prod_env() -> bool {
    if let Ok(mode) = env::var("SC2_RUNTIME_MODE") {
        if mode.to_lowercase() == "production" {
            return true;
        }
    }

    return !tauri::is_dev();
}

fn write_data_dir() -> PathBuf {
    let default = "./".to_string();

    if is_prod_env() {
        if is_windows() {
            PathBuf::from(env::var("localappdata").unwrap_or(default)).join("SC2_Coop_Info")
        } else {
            PathBuf::from(env::var("HOME").unwrap_or(default)).join(".sc2_coop_info")
        }
    } else {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or(default))
    }
}

pub(crate) fn get_json_data_dir() -> PathBuf {
    let default = "./".to_string();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR");

    if let Ok(manifest_dir) = manifest_dir {
        return PathBuf::from(manifest_dir)
            .join("..")
            .join("..")
            .join("s2coop-analyzer")
            .join("data");
    } else {
        let cur_path = current_exe();

        if let Ok(cur_path) = cur_path {
            if let Some(cur_dir) = cur_path.parent() {
                return cur_dir.join("data");
            }
        }
    }

    return PathBuf::from(default).join("data");
}

pub(crate) fn get_settings_path() -> PathBuf {
    let filename = "settings.json";
    write_data_dir().join(filename)
}

pub fn get_cache_path() -> PathBuf {
    let filename = "cache_overall_stats.json";
    write_data_dir().join("generated").join(filename)
}

pub(crate) fn get_pretty_cache_path() -> PathBuf {
    let filename = "cache_overall_stats_pretty.json";
    write_data_dir().join("generated").join(filename)
}

pub(crate) fn get_log_path() -> PathBuf {
    let filename = "logs.txt";
    write_data_dir().join(filename)
}
