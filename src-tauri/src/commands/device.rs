use tauri::State;

use crate::api_error::ApiResult;
use crate::models::DeviceStatus;
use crate::services::device;
use crate::state::AppState;

#[tauri::command]
pub async fn device_status_get(state: State<'_, AppState>) -> ApiResult<DeviceStatus> {
    let layout = state.layout_get();
    Ok(device::device_status(&layout).await)
}
