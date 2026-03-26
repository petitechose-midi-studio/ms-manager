use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::models::{
    BridgeInstanceArtifactSourceSetRequest, BridgeInstanceBindRequest,
    BridgeInstanceBindingResponse, BridgeInstanceInstalledReleaseSetRequest,
    BridgeInstanceNameSetRequest, BridgeInstanceTargetSetRequest, BridgeInstancesResponse,
};
use crate::services::bridge_instances;
use crate::state::AppState;

#[tauri::command]
pub fn bridge_instances_get(state: State<'_, AppState>) -> ApiResult<BridgeInstancesResponse> {
    Ok(BridgeInstancesResponse {
        state: state.bridge_instances_get(),
    })
}

#[tauri::command]
pub fn bridge_instance_bind(
    state: State<'_, AppState>,
    request: BridgeInstanceBindRequest,
) -> ApiResult<BridgeInstanceBindingResponse> {
    let current = state.bridge_instances_get();
    let binding = bridge_instances::build_binding(
        &current,
        request.app,
        request.mode,
        &request.controller_serial,
        request.controller_vid,
        request.controller_pid,
        ms_manager_core::FirmwareTarget::Bitwig,
        ms_manager_core::ArtifactSource::Installed,
        Some(ms_manager_core::Channel::Stable),
        None,
    )
    .map_err(|reason| ApiError::new("bridge_instance_bind_failed", reason))?;

    let state_next = state.bridge_instance_upsert(binding.clone())?;
    let binding = state_next
        .instances
        .into_iter()
        .find(|instance| instance.instance_id == binding.instance_id)
        .ok_or_else(|| {
            ApiError::new("bridge_instance_missing", "binding disappeared after save")
        })?;

    Ok(BridgeInstanceBindingResponse { binding })
}

#[tauri::command]
pub fn bridge_instance_remove(
    state: State<'_, AppState>,
    instance_id: String,
) -> ApiResult<BridgeInstancesResponse> {
    let state = state.bridge_instance_remove(&instance_id)?;
    Ok(BridgeInstancesResponse { state })
}

#[tauri::command]
pub fn bridge_instance_enable_set(
    state: State<'_, AppState>,
    instance_id: String,
    enabled: bool,
) -> ApiResult<BridgeInstancesResponse> {
    let state = state.bridge_instance_set_enabled(&instance_id, enabled)?;
    Ok(BridgeInstancesResponse { state })
}

#[tauri::command]
pub fn bridge_instance_target_set(
    state: State<'_, AppState>,
    request: BridgeInstanceTargetSetRequest,
) -> ApiResult<BridgeInstancesResponse> {
    let state = state.bridge_instance_set_target(&request.instance_id, request.target)?;
    Ok(BridgeInstancesResponse { state })
}

#[tauri::command]
pub fn bridge_instance_artifact_source_set(
    state: State<'_, AppState>,
    request: BridgeInstanceArtifactSourceSetRequest,
) -> ApiResult<BridgeInstancesResponse> {
    let state =
        state.bridge_instance_set_artifact_source(&request.instance_id, request.artifact_source)?;
    Ok(BridgeInstancesResponse { state })
}

#[tauri::command]
pub fn bridge_instance_installed_release_set(
    state: State<'_, AppState>,
    request: BridgeInstanceInstalledReleaseSetRequest,
) -> ApiResult<BridgeInstancesResponse> {
    let state = state.bridge_instance_set_installed_release(
        &request.instance_id,
        request.channel,
        request.pinned_tag,
    )?;
    Ok(BridgeInstancesResponse { state })
}

#[tauri::command]
pub fn bridge_instance_name_set(
    state: State<'_, AppState>,
    request: BridgeInstanceNameSetRequest,
) -> ApiResult<BridgeInstancesResponse> {
    let state = state.bridge_instance_set_display_name(&request.instance_id, request.display_name)?;
    Ok(BridgeInstancesResponse { state })
}
