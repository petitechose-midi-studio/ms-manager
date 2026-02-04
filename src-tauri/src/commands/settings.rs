use tauri::State;

use ms_manager_core::{Channel, Settings};

use crate::api_error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> ApiResult<Settings> {
    Ok(state.settings_get())
}

#[tauri::command]
pub fn settings_set_channel(channel: Channel, state: State<'_, AppState>) -> ApiResult<Settings> {
    state.settings_set_channel(channel)
}

#[tauri::command]
pub fn settings_set_profile(profile: String, state: State<'_, AppState>) -> ApiResult<Settings> {
    state.settings_set_profile(profile)
}
