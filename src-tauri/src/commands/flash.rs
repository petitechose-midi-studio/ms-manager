use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::services::flash;
use crate::state::AppState;

#[tauri::command]
pub async fn flash_firmware(
    profile: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<ms_manager_core::LastFlashed> {
    let installed = state
        .install_state_get()
        .ok_or_else(|| ApiError::new("not_installed", "install the host bundle first"))?;

    let layout = state.layout_get();
    let last = flash::flash_firmware(&app, &layout, &installed, &profile).await?;
    let _ = state.controller_last_flashed_set(last.clone())?;
    Ok(last)
}
