use tauri::State;

use crate::api_error::ApiResult;
use crate::models::Status;
use crate::services::device;
use crate::services::bridge_status;
use crate::state::AppState;

#[tauri::command]
pub async fn status_get(state: State<'_, AppState>) -> ApiResult<Status> {
    status_get_internal(&state).await
}

pub(crate) async fn status_get_internal(state: &AppState) -> ApiResult<Status> {
    let layout = state.layout_get();
    let installed = state.install_state_get();
    let host_installed = installed
        .as_ref()
        .is_some_and(|i| layout.version_dir(&i.tag).exists())
        && layout.current_dir().join("bin").exists();

    let bridge = if !host_installed {
        crate::models::BridgeStatus {
            installed: false,
            running: false,
            paused: false,
            serial_open: false,
            version: None,
            message: Some("host not installed".to_string()),
        }
    } else {
        bridge_status::bridge_status(&layout).await
    };

    Ok(Status {
        settings: state.settings_get(),
        installed,
        host_installed,
        platform: ms_manager_core::Platform::current()?,
        payload_root: layout.root().display().to_string(),
        device: device::device_status(&layout).await,
        last_flashed: state.controller_last_flashed(),
        bridge,
    })
}
