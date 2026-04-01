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
    let installed = state.install_state_get();
    let artifact_health =
        artifact_resolver::management_artifact_health(&layout, installed.as_ref());
    let host_installed = artifact_health.ready;

    let bindings = state.bridge_instances_get();
    let controller_state = state.controller_state_get();
    let bridge_layout = layout.clone();
    let bridge_installed = installed.clone();
    let bridge_task = tauri::async_runtime::spawn(async move {
        bridge_status::bridge_status(
            &bridge_layout,
            bridge_installed.as_ref(),
            &bindings,
            &controller_state,
        )
        .await
    });
    let device_layout = layout.clone();
    let device_task =
        tauri::async_runtime::spawn(async move { device::device_status(&device_layout).await });
    let bridge = bridge_task
        .await
        .map_err(|e| crate::api_error::ApiError::new("task_join_failed", e.to_string()))?;
    let device = device_task
        .await
        .map_err(|e| crate::api_error::ApiError::new("task_join_failed", e.to_string()))?;
    Ok(Status {
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
        bridge,
    })
}
