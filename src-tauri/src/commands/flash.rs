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
    let settings = state.settings_get();
    let installed = state.install_state_get();
    let layout = state.layout_get();
    if settings.artifact_source == ms_manager_core::ArtifactSource::Installed && installed.is_none() {
        return Err(ApiError::new("not_installed", "install the host bundle first"));
    }

    let last = flash::flash_firmware(&app, &layout, &settings, installed.as_ref(), &profile).await?;
    let _ = state.controller_last_flashed_set(last.clone())?;
    Ok(last)
}
