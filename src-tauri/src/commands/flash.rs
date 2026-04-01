use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::services::flash;
use crate::state::AppState;

#[tauri::command]
pub async fn flash_bridge_instance(
    instance_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<ms_manager_core::LastFlashed> {
    let layout = state.layout_get();
    let installed = state.install_state_get();
    let binding = state
        .bridge_instances_get()
        .instances
        .into_iter()
        .find(|binding| binding.instance_id == instance_id)
        .ok_or_else(|| {
            ApiError::new(
                "bridge_instance_not_found",
                format!("unknown instance_id: {instance_id}"),
            )
        })?;

    let last =
        flash::flash_firmware_for_binding(&app, &layout, installed.as_ref(), &binding).await?;
    let _ = state.controller_last_flashed_set(&binding.instance_id, last.clone())?;
    Ok(last)
}
