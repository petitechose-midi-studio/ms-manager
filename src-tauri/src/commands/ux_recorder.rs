use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::commands::payload::open_path_inner;
use crate::services::ux_recorder::{self, UxRecordingSessionInfo};
use crate::state::AppState;

#[tauri::command]
pub fn ux_recordings_open(state: State<'_, AppState>) -> ApiResult<()> {
    let dir = ux_recorder::open_recordings_folder(&state.layout_get())?;
    open_path_inner(&dir)
}

#[tauri::command]
pub fn ux_recording_session_rotate(
    instance_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<UxRecordingSessionInfo> {
    let layout = state.layout_get();
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

    ux_recorder::rotate_session(&app, &layout, &binding)
}
