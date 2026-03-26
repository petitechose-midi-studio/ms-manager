use tauri::State;

use crate::api_error::ApiResult;
use crate::models::Status;
use crate::services::artifact_resolver;
use crate::services::bridge_status;
use crate::services::device;
use crate::state::AppState;

#[tauri::command]
pub async fn status_get(state: State<'_, AppState>) -> ApiResult<Status> {
    status_get_internal(&state).await
}

pub(crate) async fn status_get_internal(state: &AppState) -> ApiResult<Status> {
    let layout = state.layout_get();
    let settings = state.settings_get();
    let installed = state.install_state_get();
    let artifact_health = artifact_resolver::artifact_health(&settings, &layout, installed.as_ref());
    let host_installed = artifact_health.ready;

    let bridge = if !host_installed {
        crate::models::BridgeStatus {
            installed: false,
            running: false,
            paused: false,
            serial_open: false,
            version: None,
            message: Some(
                artifact_health
                    .message
                    .clone()
                    .unwrap_or_else(|| "host artifacts are not available".to_string()),
            ),
            instances: Vec::new(),
        }
    } else {
        bridge_status::bridge_status(&settings, &layout, &state.bridge_instances_get()).await
    };
    let device = device::device_status(&settings, &layout).await;

    Ok(Status {
        settings,
        installed,
        host_installed,
        artifact_source: artifact_health.source,
        artifact_config_path: artifact_health
            .config_path
            .map(|path| path.display().to_string()),
        artifact_message: artifact_health.message,
        platform: ms_manager_core::Platform::current()?,
        payload_root: layout.root().display().to_string(),
        device,
        last_flashed: state.controller_last_flashed(),
        bridge,
    })
}
