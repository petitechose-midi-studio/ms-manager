use tauri::State;

use crate::api_error::ApiResult;
use crate::models::Status;
use crate::state::AppState;

#[tauri::command]
pub fn status_get(state: State<'_, AppState>) -> ApiResult<Status> {
    Ok(Status {
        settings: state.settings_get(),
        installed: state.install_state_get(),
        platform: ms_manager_core::Platform::current()?,
        payload_root: state.layout.root().display().to_string(),
    })
}
