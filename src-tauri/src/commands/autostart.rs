use crate::api_error::{ApiError, ApiResult};
use crate::services::manager_autostart;

#[tauri::command]
pub fn manager_autostart_get() -> ApiResult<bool> {
    Ok(manager_autostart::is_installed())
}

#[tauri::command]
pub fn manager_autostart_set(enabled: bool) -> ApiResult<bool> {
    if enabled {
        manager_autostart::install().map_err(|e| {
            ApiError::new(
                "autostart_install_failed",
                format!("install autostart: {e}"),
            )
        })?;
    } else {
        manager_autostart::uninstall().map_err(|e| {
            ApiError::new(
                "autostart_uninstall_failed",
                format!("uninstall autostart: {e}"),
            )
        })?;
    }

    Ok(manager_autostart::is_installed())
}
