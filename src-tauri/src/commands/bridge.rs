use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::models::BridgeStatus;
use crate::services::bridge_status;
use crate::state::AppState;

#[tauri::command]
pub async fn bridge_status_get(state: State<'_, AppState>) -> ApiResult<BridgeStatus> {
    let layout = state.layout_get();
    Ok(bridge_status::bridge_status(&layout).await)
}

#[tauri::command]
pub fn bridge_log_open() -> ApiResult<()> {
    let dir = oc_bridge_config_dir()?;
    let log = dir.join("bridge.log");

    // If there's no log file yet, open the directory.
    let target = if log.exists() { log } else { dir };

    // Best-effort: ensure directory exists so the opener doesn't error.
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut cmd = if cfg!(windows) {
        // Select the file in Explorer when possible.
        if target.is_file() {
            let mut c = std::process::Command::new("explorer");
            c.arg(format!("/select,{}", target.display()));
            c
        } else {
            let mut c = std::process::Command::new("explorer");
            c.arg(&target);
            c
        }
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&target);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&target);
        c
    };

    cmd.spawn()
        .map_err(|e| ApiError::new("open_failed", format!("open {}: {e}", target.display())))?;

    Ok(())
}

fn oc_bridge_config_dir() -> ApiResult<std::path::PathBuf> {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA")
            .ok_or_else(|| ApiError::new("env_missing", "APPDATA is not set"))?;
        return Ok(std::path::PathBuf::from(base)
            .join("OpenControl")
            .join("oc-bridge"));
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| ApiError::new("env_missing", "HOME is not set"))?;
        return Ok(std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("OpenControl")
            .join("oc-bridge"));
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(v) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(std::path::PathBuf::from(v)
                .join("opencontrol")
                .join("oc-bridge"));
        }
        let home = std::env::var_os("HOME")
            .ok_or_else(|| ApiError::new("env_missing", "HOME is not set"))?;
        return Ok(std::path::PathBuf::from(home)
            .join(".config")
            .join("opencontrol")
            .join("oc-bridge"));
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Err(ApiError::new(
            "unsupported_platform",
            "unsupported platform",
        ))
    }
}
