use std::path::PathBuf;

use ms_manager_core::{ProjectMigrationReport, ProjectMigrationTool};
use serde::Deserialize;
use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::commands::local_fs::resolve_local_storage_path;
use crate::services::artifact_resolver;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectMigrationInspectRequest {
    pub local_path: String,
    pub tool_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectMigrationMigrateRequest {
    pub local_path: String,
    pub output_path: String,
    pub tool_path: Option<String>,
    #[serde(default)]
    pub allow_partial: bool,
}

#[tauri::command]
pub fn project_migration_inspect(
    state: State<'_, AppState>,
    request: ProjectMigrationInspectRequest,
) -> ApiResult<ProjectMigrationReport> {
    let input = resolve_local_storage_path(&request.local_path)?;
    let layout = state.layout_get();
    let tool = project_migration_tool(&layout, request.tool_path)?;
    tool.inspect(&input).map_err(project_migration_error)
}

#[tauri::command]
pub fn project_migration_migrate(
    state: State<'_, AppState>,
    request: ProjectMigrationMigrateRequest,
) -> ApiResult<ProjectMigrationReport> {
    let input = resolve_local_storage_path(&request.local_path)?;
    let output = resolve_local_storage_path(&request.output_path)?;
    let layout = state.layout_get();
    let tool = project_migration_tool(&layout, request.tool_path)?;
    tool.migrate(&input, &output, request.allow_partial)
        .map_err(project_migration_error)
}

fn project_migration_tool(
    layout: &crate::layout::PayloadLayout,
    tool_path: Option<String>,
) -> ApiResult<ProjectMigrationTool> {
    if let Some(tool_path) = tool_path {
        return Ok(ProjectMigrationTool::new(PathBuf::from(tool_path)));
    }
    let tool_path = artifact_resolver::resolve_management_core_file_tool_exe(layout)?;
    Ok(ProjectMigrationTool::new(tool_path))
}

fn project_migration_error(err: ms_manager_core::ProjectMigrationError) -> ApiError {
    ApiError::new("project_migration_failed", err.to_string())
}
