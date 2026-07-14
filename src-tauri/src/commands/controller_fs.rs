use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::api_error::{ApiError, ApiResult};
use crate::commands::local_fs::resolve_local_storage_path;
use crate::services::controller_fs::{
    BridgeBinaryClient, ControllerFsClient, ControllerFsError, FsCapabilities, FsListEntry,
    DEFAULT_BRIDGE_CONTROL_PORT, DEFAULT_CONTROL_TIMEOUT, DEFAULT_PIPELINE_WINDOW,
    FS_RPC_MAX_CHUNK_SIZE,
};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct ControllerFsBridgeRequest {
    pub instance_id: Option<String>,
    pub control_port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControllerFsPathRequest {
    pub instance_id: Option<String>,
    pub control_port: Option<u16>,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControllerFsDeleteRequest {
    pub instance_id: Option<String>,
    pub control_port: Option<u16>,
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControllerFsRenameRequest {
    pub instance_id: Option<String>,
    pub control_port: Option<u16>,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControllerFsPullFileRequest {
    pub instance_id: Option<String>,
    pub control_port: Option<u16>,
    pub remote_path: String,
    pub local_path: String,
    pub transfer_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControllerFsPushFileRequest {
    pub instance_id: Option<String>,
    pub control_port: Option<u16>,
    pub local_path: String,
    pub remote_path: String,
    pub transfer_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerFsTransferResponse {
    pub remote_path: String,
    pub local_path: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerFsTransferProgressEvent {
    pub transfer_id: String,
    pub direction: &'static str,
    pub remote_path: String,
    pub local_path: String,
    pub bytes_done: usize,
    pub bytes_total: usize,
}

#[tauri::command]
pub async fn controller_fs_capabilities_get(
    state: State<'_, AppState>,
    request: ControllerFsBridgeRequest,
) -> ApiResult<FsCapabilities> {
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    let capabilities = client.capabilities().await.map_err(controller_fs_error)?;
    client.close().await;
    Ok(capabilities)
}

#[tauri::command]
pub async fn controller_fs_list(
    state: State<'_, AppState>,
    request: ControllerFsPathRequest,
) -> ApiResult<Vec<FsListEntry>> {
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    client
        .list(&request.path)
        .await
        .map_err(controller_fs_error)
}

#[tauri::command]
pub async fn controller_fs_mkdir(
    state: State<'_, AppState>,
    request: ControllerFsPathRequest,
) -> ApiResult<()> {
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    client
        .mkdir(&request.path)
        .await
        .map_err(controller_fs_error)
}

#[tauri::command]
pub async fn controller_fs_delete(
    state: State<'_, AppState>,
    request: ControllerFsDeleteRequest,
) -> ApiResult<()> {
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    client
        .delete(&request.path, request.recursive)
        .await
        .map_err(controller_fs_error)
}

#[tauri::command]
pub async fn controller_fs_rename(
    state: State<'_, AppState>,
    request: ControllerFsRenameRequest,
) -> ApiResult<()> {
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    client
        .rename(&request.from_path, &request.to_path)
        .await
        .map_err(controller_fs_error)
}

#[tauri::command]
pub async fn controller_fs_pull_file(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ControllerFsPullFileRequest,
) -> ApiResult<ControllerFsTransferResponse> {
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    let transfer_id = request.transfer_id.clone().unwrap_or_else(|| {
        format!(
            "pull:{}:{}",
            request.remote_path.as_str(),
            request.local_path.as_str()
        )
    });
    let local_path = resolve_local_storage_path(&request.local_path)?;
    let remote_path = request.remote_path.clone();
    let local_path_for_event = request.local_path.clone();
    let bytes = client
        .pull_file_to_path_with_progress(
            &request.remote_path,
            &local_path,
            |bytes_done, bytes_total| {
                let _ = app.emit(
                    "controller-fs-transfer-progress",
                    ControllerFsTransferProgressEvent {
                        transfer_id: transfer_id.clone(),
                        direction: "pull",
                        remote_path: remote_path.clone(),
                        local_path: local_path_for_event.clone(),
                        bytes_done,
                        bytes_total,
                    },
                );
            },
        )
        .await
        .map_err(controller_fs_error)?;

    Ok(ControllerFsTransferResponse {
        remote_path: request.remote_path,
        local_path: request.local_path,
        bytes,
    })
}

#[tauri::command]
pub async fn controller_fs_push_file(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ControllerFsPushFileRequest,
) -> ApiResult<ControllerFsTransferResponse> {
    let local_path = resolve_local_storage_path(&request.local_path)?;
    let mut client = controller_fs_client(&state, request.instance_id, request.control_port)?;
    let transfer_id = request.transfer_id.clone().unwrap_or_else(|| {
        format!(
            "push:{}:{}",
            request.local_path.as_str(),
            request.remote_path.as_str()
        )
    });
    let local_path_for_event = request.local_path.clone();
    let remote_path = request.remote_path.clone();
    let bytes = client
        .push_file_from_path_with_progress(
            &request.remote_path,
            &local_path,
            |bytes_done, bytes_total| {
                let _ = app.emit(
                    "controller-fs-transfer-progress",
                    ControllerFsTransferProgressEvent {
                        transfer_id: transfer_id.clone(),
                        direction: "push",
                        remote_path: remote_path.clone(),
                        local_path: local_path_for_event.clone(),
                        bytes_done,
                        bytes_total,
                    },
                );
            },
        )
        .await
        .map_err(controller_fs_error)?;

    Ok(ControllerFsTransferResponse {
        remote_path: request.remote_path,
        local_path: request.local_path,
        bytes,
    })
}

pub(crate) fn controller_fs_client(
    state: &AppState,
    instance_id: Option<String>,
    control_port: Option<u16>,
) -> ApiResult<ControllerFsClient> {
    let control_port = resolve_control_port(state, instance_id.as_deref(), control_port)?;
    ControllerFsClient::new(
        BridgeBinaryClient::new(control_port).with_timeout(DEFAULT_CONTROL_TIMEOUT),
    )
    .with_chunk_size(FS_RPC_MAX_CHUNK_SIZE)
    .map_err(controller_fs_error)?
    .with_pipeline_window(DEFAULT_PIPELINE_WINDOW)
    .map_err(controller_fs_error)
}

fn resolve_control_port(
    state: &AppState,
    instance_id: Option<&str>,
    control_port: Option<u16>,
) -> ApiResult<u16> {
    if control_port == Some(0) {
        return Err(ApiError::new(
            "controller_fs_control_port_invalid",
            "bridge control port cannot be 0",
        ));
    }

    let bridge_state = state.bridge_instances_get();
    if let Some(instance_id) = instance_id {
        let instance = bridge_state
            .instances
            .iter()
            .find(|instance| instance.instance_id == instance_id)
            .ok_or_else(|| {
                ApiError::new(
                    "controller_fs_instance_missing",
                    format!("bridge instance not found: {instance_id}"),
                )
            })?;
        if let Some(requested_port) = control_port {
            if requested_port != instance.control_port {
                return Err(ApiError::new(
                    "controller_fs_endpoint_mismatch",
                    format!(
                        "bridge instance {instance_id} uses control port {}, not {requested_port}",
                        instance.control_port
                    ),
                ));
            }
        }
        return Ok(instance.control_port);
    }

    if let Some(port) = control_port {
        return Ok(port);
    }

    if bridge_state.instances.is_empty() {
        return Ok(DEFAULT_BRIDGE_CONTROL_PORT);
    }

    let enabled: Vec<_> = bridge_state
        .instances
        .iter()
        .filter(|instance| instance.enabled)
        .collect();
    if enabled.len() == 1 {
        return Ok(enabled[0].control_port);
    }
    if bridge_state.instances.len() == 1 {
        return Ok(bridge_state.instances[0].control_port);
    }

    Err(ApiError::new(
        "controller_fs_instance_required",
        "multiple bridge instances are configured; provide instance_id or control_port",
    ))
}

pub(crate) fn controller_fs_error(err: ControllerFsError) -> ApiError {
    ApiError::new(err.kind, err.message)
}
