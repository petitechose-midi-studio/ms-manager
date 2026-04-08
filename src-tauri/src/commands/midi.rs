use tauri::State;

use crate::api_error::ApiResult;
use crate::models::MidiInventoryStatus;
use crate::services::{device, midi_inventory};
use crate::state::AppState;

#[tauri::command]
pub async fn midi_inventory_get(state: State<'_, AppState>) -> ApiResult<MidiInventoryStatus> {
    let layout = state.layout_get();
    let device = device::device_status(&layout).await;
    Ok(midi_inventory::midi_inventory(&device))
}
