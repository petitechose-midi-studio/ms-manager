use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::models::FlashMessageLevel;
use crate::services::{flash, workspace_firmware};
use crate::state::AppState;

#[tauri::command]
pub async fn workspace_firmware_profiles(
    target: ms_manager_core::FirmwareTarget,
) -> ApiResult<Vec<workspace_firmware::WorkspaceFirmwareProfile>> {
    workspace_firmware::profiles(target).await
}

#[tauri::command]
pub async fn build_workspace_firmware(
    target: ms_manager_core::FirmwareTarget,
    build_profile: String,
    app: tauri::AppHandle,
) -> ApiResult<workspace_firmware::WorkspaceFirmwareProfile> {
    workspace_firmware::build(&app, target, &build_profile).await
}

#[tauri::command]
pub async fn flash_bridge_instance(
    instance_id: String,
    build_profile: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<ms_manager_core::LastFlashed> {
    let layout = state.layout_get();
    let installed = state.install_state_get();
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

    let firmware_override = if binding.artifact_source == ms_manager_core::ArtifactSource::Workspace
    {
        let profile_id = build_profile
            .as_deref()
            .map(str::trim)
            .filter(|profile| !profile.is_empty())
            .ok_or_else(|| {
                ApiError::new(
                    "firmware_profile_required",
                    "Select a development firmware profile before flashing.",
                )
            })?;
        let mut profile = workspace_firmware::profile(binding.target, profile_id).await?;
        if profile.artifact_ready && profile.source_dirty {
            flash::emit_flash_message(
                &app,
                FlashMessageLevel::Warn,
                format!(
                    "Source repository has uncommitted changes; Flash will use the existing {profile_id} artifact. Run Build Firmware first if it should include those edits."
                ),
            );
        }
        if !profile.artifact_ready {
            flash::emit_flash_message(
                &app,
                FlashMessageLevel::Info,
                format!("Firmware artifact missing; building {profile_id}..."),
            );
            profile = match workspace_firmware::build(&app, binding.target, profile_id).await {
                Ok(profile) => profile,
                Err(error) => {
                    flash::emit_flash_done(&app, false);
                    return Err(error);
                }
            };
            flash::emit_flash_message(
                &app,
                FlashMessageLevel::Info,
                format!("Build complete: {}", profile.id),
            );
        }
        Some((profile.artifact_path, profile.id))
    } else {
        None
    };

    let last = flash::flash_firmware_for_binding(
        &app,
        &layout,
        installed.as_ref(),
        &binding,
        firmware_override,
    )
    .await?;
    let _ = state.controller_last_flashed_set(&binding.instance_id, last.clone())?;
    Ok(last)
}
