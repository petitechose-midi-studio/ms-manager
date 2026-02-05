use tauri::State;

use crate::api_error::ApiResult;
use crate::models::BridgeStatus;
use crate::services::bridge_status;
use crate::state::AppState;

#[tauri::command]
pub async fn bridge_status_get(state: State<'_, AppState>) -> ApiResult<BridgeStatus> {
    let layout = state.layout_get();
    Ok(bridge_status::bridge_status(&layout).await)
}
