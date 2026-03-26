use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::models::{
    BridgeInstanceBindRequest, BridgeInstanceBindingResponse, BridgeInstancesResponse,
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
    let mut bindings = state.bridge_instances_get();
    let Some(binding) = bindings
        .instances
        .iter_mut()
        .find(|binding| binding.instance_id == instance_id)
    else {
        return Err(ApiError::new(
            "bridge_instance_not_found",
            format!("unknown instance_id: {instance_id}"),
        ));
    };

    binding.enabled = enabled;
    let state = state.bridge_instances_set(bindings)?;
    Ok(BridgeInstancesResponse { state })
}
