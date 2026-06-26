use std::path::PathBuf;

use ms_manager_core::{StepPresetReport, StepPresetTool};
use serde::Deserialize;
use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::commands::local_fs::resolve_local_storage_path;
use crate::services::artifact_resolver;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct StepPresetInspectRequest {
    pub local_path: String,
    pub tool_path: Option<String>,
}

#[tauri::command]
pub fn step_preset_inspect(
    state: State<'_, AppState>,
    request: StepPresetInspectRequest,
) -> ApiResult<StepPresetReport> {
    let input = resolve_local_storage_path(&request.local_path)?;
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout, request.tool_path)?;
    tool.inspect(&input).map_err(step_preset_error)
}

#[tauri::command]
pub fn step_preset_validate(
    state: State<'_, AppState>,
    request: StepPresetInspectRequest,
) -> ApiResult<StepPresetReport> {
    let input = resolve_local_storage_path(&request.local_path)?;
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout, request.tool_path)?;
    tool.validate(&input).map_err(step_preset_error)
}

fn step_preset_tool(
    layout: &crate::layout::PayloadLayout,
    tool_path: Option<String>,
) -> ApiResult<StepPresetTool> {
    if let Some(tool_path) = tool_path {
        return Ok(StepPresetTool::new(PathBuf::from(tool_path)));
    }
    let tool_path = artifact_resolver::resolve_management_core_file_tool_exe(layout)?;
    Ok(StepPresetTool::new(tool_path))
}

fn step_preset_error(err: ms_manager_core::StepPresetError) -> ApiError {
    ApiError::new("step_preset_failed", err.to_string())
}
