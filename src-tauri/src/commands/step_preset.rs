use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use ms_manager_core::{
    StepPresetCompatibility, StepPresetReport, StepPresetScalePolicy, StepPresetStatus,
    StepPresetTool,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::commands::controller_fs::{controller_fs_client, controller_fs_error};
use crate::commands::local_fs::resolve_local_storage_path;
use crate::services::artifact_resolver;
use crate::services::controller_fs::{
    ControllerFsClient, ControllerFsError, FsCapabilities, FsFileType, FsStatus, FS_RPC_SHA256_SIZE,
};
use crate::state::AppState;

static STEP_PRESET_TRANSACTION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const STEP_PRESET_MAX_BYTES: usize = u16::MAX as usize;
const STEP_PRESET_MAX_TECHNICAL_ID_BYTES: usize = 54;
const STEP_PRESET_MAX_SEMANTIC_NAME_BYTES: usize = 31;
const STEP_PRESET_PREVIEW_KEY_BYTES: usize = 64;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedStepPresetReport {
    #[serde(flatten)]
    pub report: StepPresetReport,
    pub preview_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepPresetInspectRequest {
    pub local_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepPresetRenameRequest {
    pub local_path: String,
    pub semantic_name: String,
    pub expected_technical_id: String,
    pub expected_semantic_name: String,
    pub expected_preview_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepPresetIdentityRequest {
    pub local_path: String,
    pub expected_technical_id: String,
    pub expected_semantic_name: String,
    pub expected_preview_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteStepPresetInspectRequest {
    pub instance_id: String,
    pub control_port: u16,
    pub remote_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteStepPresetRenameRequest {
    pub instance_id: String,
    pub control_port: u16,
    pub remote_path: String,
    pub semantic_name: String,
    pub expected_technical_id: String,
    pub expected_semantic_name: String,
    pub expected_preview_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteStepPresetIdentityRequest {
    pub instance_id: String,
    pub control_port: u16,
    pub remote_path: String,
    pub expected_technical_id: String,
    pub expected_semantic_name: String,
    pub expected_preview_key: String,
}

#[tauri::command]
pub fn step_preset_inspect(
    state: State<'_, AppState>,
    request: StepPresetInspectRequest,
) -> ApiResult<ManagedStepPresetReport> {
    let input = resolve_local_storage_path(&request.local_path)?;
    ensure_local_step_preset_path(&input)?;
    let temp = StepPresetTempDir::create("local-inspect")?;
    let snapshot = temp.path("snapshot.mssp");
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let bytes = read_step_preset_bytes("read Step Preset for inspection", &input)?;
    write_step_preset_snapshot(&snapshot, &bytes)?;
    let report = tool.inspect(&snapshot).map_err(step_preset_error)?;
    ensure_report_operation(&report, "inspect-step-graph-preset")?;
    Ok(managed_step_preset_report(report, &bytes))
}

#[tauri::command]
pub fn step_preset_validate(
    state: State<'_, AppState>,
    request: StepPresetInspectRequest,
) -> ApiResult<ManagedStepPresetReport> {
    let input = resolve_local_storage_path(&request.local_path)?;
    ensure_local_step_preset_path(&input)?;
    let temp = StepPresetTempDir::create("local-validate")?;
    let snapshot = temp.path("snapshot.mssp");
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let bytes = read_step_preset_bytes("read Step Preset for validation", &input)?;
    write_step_preset_snapshot(&snapshot, &bytes)?;
    let report = tool.validate(&snapshot).map_err(step_preset_error)?;
    ensure_report_operation(&report, "validate-step-graph-preset")?;
    Ok(managed_step_preset_report(report, &bytes))
}

#[tauri::command]
pub fn step_preset_rename(
    state: State<'_, AppState>,
    request: StepPresetRenameRequest,
) -> ApiResult<ManagedStepPresetReport> {
    let semantic_name = validate_new_semantic_name(&request.semantic_name)?;
    ensure_valid_confirmation(
        &request.expected_technical_id,
        &request.expected_semantic_name,
        &request.expected_preview_key,
    )?;
    let input = resolve_local_storage_path(&request.local_path)?;
    ensure_local_step_preset_path(&input)?;
    let _transaction_lock = LocalStepPresetLock::acquire(&input)?;
    let temp = StepPresetTempDir::create("local-rename")?;
    let snapshot = temp.path("snapshot.mssp");
    let renamed_output = temp.path("renamed-output.mssp");
    let validated_snapshot = temp.path("renamed-validated.mssp");
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let before = read_step_preset_bytes("read Step Preset before rename", &input)?;
    write_step_preset_snapshot(&snapshot, &before)?;
    let before_report = tool.inspect(&snapshot).map_err(step_preset_error)?;
    ensure_report_operation(&before_report, "inspect-step-graph-preset")?;
    ensure_expected_preview(
        &before_report,
        &before,
        &request.expected_technical_id,
        &request.expected_semantic_name,
        &request.expected_preview_key,
    )?;

    let renamed = tool
        .rename(&snapshot, semantic_name, &renamed_output)
        .map_err(step_preset_error)?;
    let renamed_bytes = read_step_preset_bytes("read renamed Step Preset", &renamed_output)?;
    write_step_preset_snapshot(&validated_snapshot, &renamed_bytes)?;
    let staged = unique_sibling_path(&input, "rename")?;
    remove_file_if_exists(&staged)?;

    let operation = (|| -> ApiResult<ManagedStepPresetReport> {
        ensure_report_operation(&renamed, "rename-step-graph-preset")?;
        ensure_actionable_report(&renamed)?;
        if renamed.technical_id != before_report.technical_id
            || renamed.semantic_name != semantic_name
        {
            return Err(ApiError::new(
                "step_preset_identity_mismatch",
                "renamed Step Preset did not preserve its technical identity",
            ));
        }

        let validated = tool
            .validate(&validated_snapshot)
            .map_err(step_preset_error)?;
        ensure_report_operation(&validated, "validate-step-graph-preset")?;
        ensure_actionable_report(&validated)?;
        if validated.technical_id != renamed.technical_id
            || validated.semantic_name != renamed.semantic_name
        {
            return Err(ApiError::new(
                "step_preset_validation_mismatch",
                "staged Step Preset identity changed during validation",
            ));
        }

        let current = read_step_preset_bytes("re-read Step Preset before rename commit", &input)?;
        if current != before {
            return Err(ApiError::new(
                "step_preset_stale",
                "Step Preset changed while its rename was being prepared; inspect and retry",
            ));
        }

        // Materialize the exact validated bytes on the destination filesystem
        // only after all semantic and stale-preview checks have passed.
        write_step_preset_snapshot(&staged, &renamed_bytes)?;
        replace_local_step_preset(&input, &staged, &before, &renamed_bytes)?;
        Ok(managed_step_preset_report(renamed, &renamed_bytes))
    })();

    let _ = std::fs::remove_file(&staged);
    operation
}

#[tauri::command]
pub fn step_preset_delete(
    state: State<'_, AppState>,
    request: StepPresetIdentityRequest,
) -> ApiResult<ManagedStepPresetReport> {
    ensure_valid_confirmation(
        &request.expected_technical_id,
        &request.expected_semantic_name,
        &request.expected_preview_key,
    )?;
    let input = resolve_local_storage_path(&request.local_path)?;
    ensure_local_step_preset_path(&input)?;
    let _transaction_lock = LocalStepPresetLock::acquire(&input)?;
    let temp = StepPresetTempDir::create("local-delete")?;
    let snapshot = temp.path("snapshot.mssp");
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let before = read_step_preset_bytes("read Step Preset before delete", &input)?;
    write_step_preset_snapshot(&snapshot, &before)?;
    let inspected = tool.inspect(&snapshot).map_err(step_preset_error)?;
    ensure_report_operation(&inspected, "inspect-step-graph-preset")?;
    ensure_expected_preview(
        &inspected,
        &before,
        &request.expected_technical_id,
        &request.expected_semantic_name,
        &request.expected_preview_key,
    )?;

    // Rename first so the exact bytes that were inspected become the deletion
    // target. This avoids deleting a replacement that appeared at the same
    // path between confirmation and commit.
    let quarantine = unique_sibling_path(&input, "delete")?;
    remove_file_if_exists(&quarantine)?;
    durable_move_no_replace(&input, &quarantine)
        .map_err(|err| step_preset_io_error("quarantine Step Preset before delete", &input, err))?;

    let operation = (|| -> ApiResult<()> {
        let moved_bytes = read_step_preset_bytes("read quarantined Step Preset", &quarantine)?;
        if moved_bytes != before {
            return Err(ApiError::new(
                "step_preset_stale",
                "Step Preset changed after confirmation; delete was cancelled",
            ));
        }
        // `moved_bytes` are exactly the immutable, already-inspected preview.
        // Re-running the external tool after quarantine would only enlarge the
        // rollback window without adding evidence.
        std::fs::remove_file(&quarantine)
            .map_err(|err| step_preset_io_error("delete quarantined Step Preset", &quarantine, err))
    })();

    if let Err(err) = operation {
        if input.exists() || !quarantine.exists() {
            return Err(ApiError::new(
                "step_preset_local_rollback_failed",
                format!(
                    "delete Step Preset failed ('{}') and automatic restore was unsafe. Recovery source: {}; intended path: {}",
                    err.message,
                    quarantine.display(),
                    input.display()
                ),
            ));
        }
        if let Err(restore_error) = durable_move_no_replace(&quarantine, &input) {
            return Err(local_rollback_error(
                "delete Step Preset",
                &err,
                &quarantine,
                &input,
                restore_error,
            ));
        }
        return Err(err);
    }
    Ok(managed_step_preset_report(inspected, &before))
}

#[tauri::command]
pub async fn remote_step_preset_inspect(
    state: State<'_, AppState>,
    request: RemoteStepPresetInspectRequest,
) -> ApiResult<ManagedStepPresetReport> {
    ensure_remote_step_preset_path(&request.remote_path)?;
    let temp = StepPresetTempDir::create("inspect")?;
    let input = temp.path("input.mssp");
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let mut client = controller_fs_client(
        &state,
        Some(request.instance_id.clone()),
        Some(request.control_port),
    )?;
    let result = async {
        let bytes = pull_remote_step_preset(&mut client, &request.remote_path, &input).await?;
        let report = tool.inspect(&input).map_err(step_preset_error)?;
        ensure_report_operation(&report, "inspect-step-graph-preset")?;
        Ok(managed_step_preset_report(report, &bytes))
    }
    .await;
    client.close().await;
    result
}

#[tauri::command]
pub async fn remote_step_preset_validate(
    state: State<'_, AppState>,
    request: RemoteStepPresetInspectRequest,
) -> ApiResult<ManagedStepPresetReport> {
    ensure_remote_step_preset_path(&request.remote_path)?;
    let temp = StepPresetTempDir::create("validate")?;
    let input = temp.path("input.mssp");
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let mut client = controller_fs_client(
        &state,
        Some(request.instance_id.clone()),
        Some(request.control_port),
    )?;
    let result = async {
        let bytes = pull_remote_step_preset(&mut client, &request.remote_path, &input).await?;
        let report = tool.validate(&input).map_err(step_preset_error)?;
        ensure_report_operation(&report, "validate-step-graph-preset")?;
        Ok(managed_step_preset_report(report, &bytes))
    }
    .await;
    client.close().await;
    result
}

#[tauri::command]
pub async fn remote_step_preset_rename(
    state: State<'_, AppState>,
    request: RemoteStepPresetRenameRequest,
) -> ApiResult<ManagedStepPresetReport> {
    let semantic_name = validate_new_semantic_name(&request.semantic_name)?.to_string();
    ensure_valid_confirmation(
        &request.expected_technical_id,
        &request.expected_semantic_name,
        &request.expected_preview_key,
    )?;
    ensure_remote_step_preset_path(&request.remote_path)?;
    let temp = StepPresetTempDir::create("rename")?;
    let input = temp.path("input.mssp");
    let staged = temp.path("staged.mssp");
    let staged_check = temp.path("staged-validated.mssp");
    let remote_check = temp.path("remote-staged.mssp");
    let final_check = temp.path("final.mssp");
    let remote_stage = unique_remote_step_preset_path("rename-stage");
    let operation_id = unique_remote_operation_id();

    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let mut client = controller_fs_client(
        &state,
        Some(request.instance_id.clone()),
        Some(request.control_port),
    )?;
    let mut remote_stage_uploaded = false;
    let mut conditional_started = false;
    let mut conditional_reconciled = false;
    let result = async {
        let capabilities = client.capabilities().await.map_err(controller_fs_error)?;
        capabilities
            .require_conditional_mutations()
            .map_err(controller_fs_error)?;
        ensure_remote_paths_fit(&capabilities, &[&request.remote_path, &remote_stage])?;

        let before = pull_remote_step_preset(&mut client, &request.remote_path, &input).await?;
        let before_report = tool.inspect(&input).map_err(step_preset_error)?;
        ensure_report_operation(&before_report, "inspect-step-graph-preset")?;
        ensure_expected_preview(
            &before_report,
            &before,
            &request.expected_technical_id,
            &request.expected_semantic_name,
            &request.expected_preview_key,
        )?;

        let renamed = tool
            .rename(&input, &semantic_name, &staged)
            .map_err(step_preset_error)?;
        ensure_report_operation(&renamed, "rename-step-graph-preset")?;
        ensure_actionable_report(&renamed)?;
        if renamed.technical_id != before_report.technical_id
            || renamed.semantic_name != semantic_name
        {
            return Err(ApiError::new(
                "step_preset_identity_mismatch",
                "renamed Step Preset did not preserve its technical identity",
            ));
        }
        let staged_bytes = read_step_preset_bytes("read staged remote Step Preset", &staged)?;
        write_step_preset_snapshot(&staged_check, &staged_bytes)?;
        let validated = tool.validate(&staged_check).map_err(step_preset_error)?;
        ensure_report_operation(&validated, "validate-step-graph-preset")?;
        ensure_expected_identity(&validated, &renamed.technical_id, &renamed.semantic_name)?;

        // A write-commit timeout can leave the unique stage present even when
        // the upload call reports an error, so mark it for cleanup beforehand.
        remote_stage_uploaded = true;
        push_remote_step_preset(&mut client, &remote_stage, &staged).await?;
        let remote_stage_bytes =
            pull_remote_step_preset(&mut client, &remote_stage, &remote_check).await?;
        if remote_stage_bytes != staged_bytes {
            return Err(ApiError::new(
                "step_preset_remote_transfer_mismatch",
                "controller staging bytes differ from the validated Step Preset",
            ));
        }
        let staged_report = tool.validate(&remote_check).map_err(step_preset_error)?;
        ensure_report_operation(&staged_report, "validate-step-graph-preset")?;
        ensure_expected_identity(
            &staged_report,
            &renamed.technical_id,
            &renamed.semantic_name,
        )?;

        conditional_started = true;
        let commit_result = commit_remote_step_preset_replace(
            &mut client,
            operation_id,
            &request.remote_path,
            &remote_stage,
            &sha256_bytes(&before),
            &sha256_bytes(&staged_bytes),
        )
        .await;

        // Always reconcile the canonical path after a conditional RPC. This
        // resolves the classic "request committed, response timed out" case
        // without rolling back or overwriting concurrent work.
        let final_bytes = match pull_remote_step_preset(
            &mut client,
            &request.remote_path,
            &final_check,
        )
        .await
        {
            Ok(bytes) => {
                conditional_reconciled = true;
                bytes
            }
            Err(verification_error) => {
                return match commit_result {
                    Ok(()) => Err(verification_error),
                    Err(commit_error) => Err(remote_commit_ambiguous_error(
                        "rename",
                        commit_error,
                        verification_error,
                    )),
                };
            }
        };
        if final_bytes != staged_bytes {
            if let Err(commit_error) = commit_result {
                if final_bytes == before {
                    return Err(conditional_step_preset_error(commit_error));
                }
                return Err(ApiError::new(
                    "step_preset_stale",
                    "controller Step Preset changed while rename completion was being reconciled; inspect and retry",
                ));
            }
            return Err(ApiError::new(
                "step_preset_remote_commit_mismatch",
                "committed controller Step Preset differs from the validated staging bytes",
            ));
        }
        // `final_bytes` are byte-identical to the locally validated snapshot
        // and to the remotely re-read stage. A second tool invocation here
        // cannot add evidence and could falsely report failure after a commit
        // that is already proven exact.
        Ok(managed_step_preset_report(renamed, &staged_bytes))
    }
    .await;

    if remote_stage_uploaded && (!conditional_started || conditional_reconciled || result.is_ok()) {
        let _ = client.delete(&remote_stage, false).await;
    }
    client.close().await;
    result
}

#[tauri::command]
pub async fn remote_step_preset_delete(
    state: State<'_, AppState>,
    request: RemoteStepPresetIdentityRequest,
) -> ApiResult<ManagedStepPresetReport> {
    ensure_valid_confirmation(
        &request.expected_technical_id,
        &request.expected_semantic_name,
        &request.expected_preview_key,
    )?;
    ensure_remote_step_preset_path(&request.remote_path)?;
    let temp = StepPresetTempDir::create("delete")?;
    let input = temp.path("input.mssp");
    let reconcile = temp.path("delete-reconcile.mssp");
    let operation_id = unique_remote_operation_id();
    let layout = state.layout_get();
    let tool = step_preset_tool(&layout)?;
    let mut client = controller_fs_client(
        &state,
        Some(request.instance_id.clone()),
        Some(request.control_port),
    )?;

    let result = async {
        let capabilities = client.capabilities().await.map_err(controller_fs_error)?;
        capabilities
            .require_conditional_mutations()
            .map_err(controller_fs_error)?;
        ensure_remote_paths_fit(&capabilities, &[&request.remote_path])?;

        let before = pull_remote_step_preset(&mut client, &request.remote_path, &input).await?;
        let inspected = tool.inspect(&input).map_err(step_preset_error)?;
        ensure_report_operation(&inspected, "inspect-step-graph-preset")?;
        ensure_expected_preview(
            &inspected,
            &before,
            &request.expected_technical_id,
            &request.expected_semantic_name,
            &request.expected_preview_key,
        )?;

        let commit_result = commit_remote_step_preset_delete(
            &mut client,
            operation_id,
            &request.remote_path,
            &sha256_bytes(&before),
        )
        .await;

        // A successful delete must leave the canonical path absent. The same
        // observation also resolves an ambiguous timeout safely.
        let stat = match client.stat(&request.remote_path).await {
            Ok(stat) => stat,
            Err(error) => {
                let verification_error = controller_fs_error(error);
                return match commit_result {
                    Ok(()) => Err(verification_error),
                    Err(commit_error) => Err(remote_commit_ambiguous_error(
                        "delete",
                        commit_error,
                        verification_error,
                    )),
                };
            }
        };
        if stat.status == FsStatus::NotFound {
            return Ok(managed_step_preset_report(inspected, &before));
        }
        if stat.status != FsStatus::Ok {
            let verification_error = ApiError::new(
                "step_preset_remote_verification_failed",
                format!(
                    "controller returned {:?} while verifying Step Preset deletion",
                    stat.status
                ),
            );
            return match commit_result {
                Ok(()) => Err(verification_error),
                Err(commit_error) => Err(remote_commit_ambiguous_error(
                    "delete",
                    commit_error,
                    verification_error,
                )),
            };
        }
        if stat.file_type != FsFileType::File || stat.size_bytes != before.len() as u32 {
            return Err(ApiError::new(
                "step_preset_stale",
                "controller path now identifies different content; no further delete was attempted",
            ));
        }

        let current = match pull_remote_step_preset(
            &mut client,
            &request.remote_path,
            &reconcile,
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(verification_error) => {
                return match commit_result {
                    Ok(()) => Err(verification_error),
                    Err(commit_error) => Err(remote_commit_ambiguous_error(
                        "delete",
                        commit_error,
                        verification_error,
                    )),
                };
            }
        };
        if current != before {
            return Err(ApiError::new(
                "step_preset_stale",
                "controller path now contains different Step Preset bytes; no further delete was attempted",
            ));
        }
        match commit_result {
            Ok(()) => Err(ApiError::new(
                "step_preset_remote_commit_mismatch",
                "controller reported a successful delete but the exact Step Preset is still present",
            )),
            Err(commit_error) => Err(conditional_step_preset_error(commit_error)),
        }
    }
    .await;

    client.close().await;
    result
}

struct StepPresetTempDir {
    root: PathBuf,
}

impl StepPresetTempDir {
    fn create(action: &str) -> ApiResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "ms-manager-step-preset-{action}-{}",
            unique_transaction_suffix()
        ));
        std::fs::create_dir(&root).map_err(|err| {
            step_preset_io_error("create Step Preset transaction directory", &root, err)
        })?;
        Ok(Self { root })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for StepPresetTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn ensure_local_step_preset_path(path: &Path) -> ApiResult<()> {
    let valid = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(valid_step_preset_filename);
    if valid {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_local_path_invalid",
        "local Step Preset must have the .mssp extension",
    ))
}

fn ensure_remote_step_preset_path(path: &str) -> ApiResult<()> {
    let valid = path.starts_with("/midi-studio/")
        && path.len() <= u8::MAX as usize
        && path.bytes().all(|byte| (0x20..0x7f).contains(&byte))
        && !path.contains('\\')
        && !path.contains("//")
        && path
            .rsplit('/')
            .next()
            .is_some_and(valid_step_preset_filename)
        && path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .all(|segment| {
                segment != "."
                    && segment != ".."
                    && !segment.ends_with(' ')
                    && !segment.ends_with('.')
                    && !segment.chars().any(is_reserved_path_character)
            });
    if valid {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_remote_path_invalid",
        "remote Step Preset must be a canonical .mssp path under /midi-studio",
    ))
}

fn valid_step_preset_filename(name: &str) -> bool {
    name.len() > 5
        && name.to_ascii_lowercase().ends_with(".mssp")
        && !name.ends_with(' ')
        && !name.ends_with('.')
        && !name
            .chars()
            .any(|character| character.is_control() || is_reserved_path_character(character))
}

fn is_reserved_path_character(character: char) -> bool {
    matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
}

fn ensure_remote_paths_fit(capabilities: &FsCapabilities, paths: &[&str]) -> ApiResult<()> {
    let max_path_length = usize::from(capabilities.max_path_length);
    if max_path_length > 0 && paths.iter().all(|path| path.len() <= max_path_length) {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_remote_path_too_long",
        format!(
            "controller filesystem supports paths up to {} bytes",
            capabilities.max_path_length
        ),
    ))
}

fn unique_remote_step_preset_path(action: &str) -> String {
    format!(
        "/midi-studio/tmp/msm-{action}-{}.mssp",
        unique_transaction_suffix()
    )
}

fn unique_transaction_suffix() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = STEP_PRESET_TRANSACTION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{stamp}-{sequence}", std::process::id())
}

fn unique_remote_operation_id() -> u32 {
    let digest = Sha256::digest(unique_transaction_suffix().as_bytes());
    let operation_id = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    operation_id.max(1)
}

fn sha256_bytes(bytes: &[u8]) -> [u8; FS_RPC_SHA256_SIZE] {
    Sha256::digest(bytes).into()
}

async fn pull_remote_step_preset(
    client: &mut ControllerFsClient,
    remote_path: &str,
    local_path: &Path,
) -> ApiResult<Vec<u8>> {
    client
        .pull_file_to_path_with_progress_limit(
            remote_path,
            local_path,
            STEP_PRESET_MAX_BYTES as u32,
            |_, _| {},
        )
        .await
        .map_err(controller_fs_error)?;
    read_step_preset_bytes("read downloaded Step Preset", local_path)
}

async fn push_remote_step_preset(
    client: &mut ControllerFsClient,
    remote_path: &str,
    local_path: &Path,
) -> ApiResult<()> {
    client
        .push_file_from_path_with_progress(remote_path, local_path, |_, _| {})
        .await
        .map_err(controller_fs_error)?;
    Ok(())
}

async fn commit_remote_step_preset_replace(
    client: &mut ControllerFsClient,
    operation_id: u32,
    current: &str,
    staging: &str,
    expected_source_sha256: &[u8; FS_RPC_SHA256_SIZE],
    replacement_sha256: &[u8; FS_RPC_SHA256_SIZE],
) -> Result<(), ControllerFsError> {
    let first = client
        .conditional_replace(
            operation_id,
            current,
            staging,
            expected_source_sha256,
            replacement_sha256,
        )
        .await;
    match first {
        Ok(_) => Ok(()),
        Err(error) if conditional_mutation_may_be_committed(&error) => {
            // Replay the exact operation immediately. This next filesystem RPC
            // first runs firmware journal recovery; inserting a capabilities
            // request here could itself fail on the still-pending journal and
            // prevent the idempotent retry.
            client
                .conditional_replace(
                    operation_id,
                    current,
                    staging,
                    expected_source_sha256,
                    replacement_sha256,
                )
                .await
                .map(|_| ())
        }
        Err(error) => Err(error),
    }
}

async fn commit_remote_step_preset_delete(
    client: &mut ControllerFsClient,
    operation_id: u32,
    current: &str,
    expected_source_sha256: &[u8; FS_RPC_SHA256_SIZE],
) -> Result<(), ControllerFsError> {
    let first = client
        .conditional_delete(operation_id, current, expected_source_sha256)
        .await;
    match first {
        Ok(_) => Ok(()),
        Err(error) if conditional_mutation_may_be_committed(&error) => client
            .conditional_delete(operation_id, current, expected_source_sha256)
            .await
            .map(|_| ()),
        Err(error) => Err(error),
    }
}

fn conditional_mutation_may_be_committed(error: &ControllerFsError) -> bool {
    matches!(
        error.kind.as_str(),
        "bridge_timeout"
            | "bridge_unavailable"
            | "controller_rpc_failed"
            | "protocol_error"
            | "invalid_state"
            | "codec_error"
            | "conditional_storage_error"
            | "conditional_invalid_state"
    )
}

fn conditional_step_preset_error(error: ControllerFsError) -> ApiError {
    if error.kind == "precondition_failed" {
        return ApiError::new(
            "step_preset_stale",
            format!(
                "Step Preset changed after confirmation; inspect and retry: {}",
                error.message
            ),
        );
    }
    controller_fs_error(error)
}

fn remote_commit_ambiguous_error(
    action: &str,
    commit_error: ControllerFsError,
    verification_error: ApiError,
) -> ApiError {
    ApiError::new(
        "step_preset_remote_commit_ambiguous",
        format!(
            "controller Step Preset {action} could not be reconciled after '{}' because verification also failed: {}. Reconnect and inspect before retrying",
            commit_error.message, verification_error.message
        ),
    )
}

fn step_preset_tool(layout: &crate::layout::PayloadLayout) -> ApiResult<StepPresetTool> {
    let tool_path = artifact_resolver::resolve_management_core_file_tool_exe(layout)?;
    Ok(StepPresetTool::new(tool_path))
}

fn step_preset_error(err: ms_manager_core::StepPresetError) -> ApiError {
    ApiError::new("step_preset_failed", err.to_string())
}

fn read_step_preset_bytes(action: &str, path: &Path) -> ApiResult<Vec<u8>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|err| step_preset_io_error(action, path, err))?;
    let mut bytes = Vec::with_capacity(STEP_PRESET_MAX_BYTES.min(4096));
    file.take((STEP_PRESET_MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| step_preset_io_error(action, path, err))?;
    if bytes.len() > STEP_PRESET_MAX_BYTES {
        return Err(ApiError::new(
            "step_preset_too_large",
            format!(
                "Step Preset exceeds the {} byte codec limit: {}",
                STEP_PRESET_MAX_BYTES,
                path.display()
            ),
        ));
    }
    Ok(bytes)
}

fn write_step_preset_snapshot(path: &Path, bytes: &[u8]) -> ApiResult<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| step_preset_io_error("create immutable Step Preset snapshot", path, err))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|err| step_preset_io_error("write immutable Step Preset snapshot", path, err))
}

fn step_preset_preview_key(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn managed_step_preset_report(report: StepPresetReport, bytes: &[u8]) -> ManagedStepPresetReport {
    ManagedStepPresetReport {
        report,
        preview_key: step_preset_preview_key(bytes),
    }
}

fn ensure_report_operation(report: &StepPresetReport, expected: &str) -> ApiResult<()> {
    if report.operation == expected {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_tool_contract_mismatch",
        format!(
            "Step Preset tool reported operation '{}' while '{}' was requested",
            report.operation, expected
        ),
    ))
}

fn ensure_actionable_report(report: &StepPresetReport) -> ApiResult<()> {
    let compatible = matches!(
        report.compatibility,
        StepPresetCompatibility::Ready | StepPresetCompatibility::ReadyMixed
    );
    let compatibility_consistent = matches!(
        (report.compatibility, report.mixed_pitch_policy),
        (StepPresetCompatibility::Ready, false) | (StepPresetCompatibility::ReadyMixed, true)
    );
    let pitch_policy_valid = matches!(
        report.scale_policy,
        StepPresetScalePolicy::Chromatic
            | StepPresetScalePolicy::ScaleRelative
            | StepPresetScalePolicy::Mixed
    ) && matches!(
        report.default_scale_policy,
        StepPresetScalePolicy::Chromatic | StepPresetScalePolicy::ScaleRelative
    ) && report.source_scale.root < 12
        && report.source_scale.scale_type <= 13
        && report.source_scale.mode <= 3
        && (report.scale_policy == StepPresetScalePolicy::Mixed) == report.mixed_pitch_policy;
    let flags_consistent = report.flags.graph_payload
        && report.flags.root_values == report.root_values
        && report.flags.mixed_pitch_policy == report.mixed_pitch_policy
        && (!report.root_values || report.root_context);
    if report.status == StepPresetStatus::Ok
        && compatible
        && compatibility_consistent
        && pitch_policy_valid
        && flags_consistent
        && report.file_kind == "step_graph_preset"
        && report.format_version == 2
        && !report.metadata_defaulted
        && valid_step_preset_technical_id(&report.technical_id)
        && valid_step_preset_semantic_name(&report.semantic_name)
    {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_not_actionable",
        format!(
            "Step Preset is not safe to modify: status={:?}, compatibility={:?}",
            report.status, report.compatibility
        ),
    ))
}

fn validate_new_semantic_name(value: &str) -> ApiResult<&str> {
    if valid_step_preset_semantic_name(value) {
        return Ok(value);
    }
    Err(ApiError::new(
        "step_preset_semantic_name_invalid",
        format!(
            "Step Preset name must contain 1 to {STEP_PRESET_MAX_SEMANTIC_NAME_BYTES} UTF-8 bytes, without leading/trailing spaces or control characters"
        ),
    ))
}

fn ensure_valid_confirmation(
    expected_technical_id: &str,
    expected_semantic_name: &str,
    expected_preview_key: &str,
) -> ApiResult<()> {
    let valid_preview_key = expected_preview_key.len() == STEP_PRESET_PREVIEW_KEY_BYTES
        && expected_preview_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid_step_preset_technical_id(expected_technical_id)
        && valid_step_preset_semantic_name(expected_semantic_name)
        && valid_preview_key
    {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_confirmation_invalid",
        "Step Preset confirmation identity or preview key is malformed; inspect again",
    ))
}

fn valid_step_preset_technical_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > STEP_PRESET_MAX_TECHNICAL_ID_BYTES
        || matches!(bytes.first(), Some(b'.' | b' '))
        || matches!(bytes.last(), Some(b'.' | b' '))
        || bytes.windows(2).any(|pair| pair == b"..")
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b' '))
    {
        return false;
    }

    let base = value.split('.').next().unwrap_or(value);
    let base_lower = base.to_ascii_lowercase();
    if matches!(base_lower.as_str(), "con" | "prn" | "aux" | "nul") {
        return false;
    }
    !matches!(
        base_lower.as_bytes(),
        [b'c', b'o', b'm', b'1'..=b'9'] | [b'l', b'p', b't', b'1'..=b'9']
    )
}

fn valid_step_preset_semantic_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= STEP_PRESET_MAX_SEMANTIC_NAME_BYTES
        && !value.starts_with(' ')
        && !value.ends_with(' ')
        && !value.chars().any(|character| {
            character <= '\u{001f}' || ('\u{007f}'..='\u{009f}').contains(&character)
        })
}

fn ensure_expected_identity(
    report: &StepPresetReport,
    expected_technical_id: &str,
    expected_semantic_name: &str,
) -> ApiResult<()> {
    ensure_actionable_report(report)?;
    if report.technical_id == expected_technical_id
        && report.semantic_name == expected_semantic_name
    {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_stale_identity",
        format!(
            "Step Preset identity changed; expected '{}' ({}) but found '{}' ({})",
            expected_semantic_name,
            expected_technical_id,
            report.semantic_name,
            report.technical_id
        ),
    ))
}

fn ensure_expected_preview(
    report: &StepPresetReport,
    bytes: &[u8],
    expected_technical_id: &str,
    expected_semantic_name: &str,
    expected_preview_key: &str,
) -> ApiResult<()> {
    ensure_valid_confirmation(
        expected_technical_id,
        expected_semantic_name,
        expected_preview_key,
    )?;
    ensure_expected_identity(report, expected_technical_id, expected_semantic_name)?;
    if step_preset_preview_key(bytes) == expected_preview_key {
        return Ok(());
    }
    Err(ApiError::new(
        "step_preset_stale_preview",
        "Step Preset content changed after preview; inspect and retry",
    ))
}

fn unique_sibling_path(input: &Path, action: &str) -> ApiResult<PathBuf> {
    let parent = input.parent().ok_or_else(|| {
        ApiError::new("step_preset_path_invalid", "Step Preset path has no parent")
    })?;
    let name = input
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            ApiError::new(
                "step_preset_path_invalid",
                "Step Preset filename is not valid UTF-8",
            )
        })?;
    Ok(parent.join(format!(
        ".{name}.{action}-{}.tmp",
        unique_transaction_suffix()
    )))
}

struct LocalStepPresetLock {
    _file: std::fs::File,
}

impl LocalStepPresetLock {
    fn acquire(input: &Path) -> ApiResult<Self> {
        let parent = input.parent().ok_or_else(|| {
            ApiError::new("step_preset_path_invalid", "Step Preset path has no parent")
        })?;
        let name = input
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                ApiError::new(
                    "step_preset_path_invalid",
                    "Step Preset filename is not valid UTF-8",
                )
            })?;
        let lock_path = parent.join(format!(".{name}.ms-manager.lock"));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|err| {
                step_preset_io_error("open Step Preset transaction lock", &lock_path, err)
            })?;
        FileExt::try_lock_exclusive(&file).map_err(|err| {
            ApiError::new(
                "step_preset_transaction_busy",
                format!(
                    "another Step Preset operation owns {}: {err}",
                    lock_path.display()
                ),
            )
        })?;
        Ok(Self { _file: file })
    }
}

fn remove_file_if_exists(path: &Path) -> ApiResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(step_preset_io_error(
            "remove stale Step Preset staging file",
            path,
            err,
        )),
    }
}

fn replace_local_step_preset(
    input: &Path,
    staged: &Path,
    expected_source: &[u8],
    expected_output: &[u8],
) -> ApiResult<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(staged)
        .and_then(|file| file.sync_all())
        .map_err(|err| step_preset_io_error("sync staged Step Preset", staged, err))?;

    let backup = unique_sibling_path(input, "backup")?;
    let failed = unique_sibling_path(input, "failed")?;
    remove_file_if_exists(&backup)?;
    remove_file_if_exists(&failed)?;

    let current =
        read_step_preset_bytes("verify Step Preset immediately before rename commit", input)?;
    if current != expected_source {
        return Err(ApiError::new(
            "step_preset_stale",
            "Step Preset changed at rename commit; inspect and retry",
        ));
    }
    let staged_at_commit = read_step_preset_bytes(
        "verify staged Step Preset immediately before commit",
        staged,
    )?;
    if staged_at_commit != expected_output {
        return Err(ApiError::new(
            "step_preset_staging_changed",
            "staged Step Preset changed after validation; rename was cancelled",
        ));
    }

    atomic_replace_with_backup(input, staged, &backup, expected_source).map_err(|err| {
        ApiError::new(
            "step_preset_local_atomic_replace_failed",
            format!(
                "atomic Step Preset replacement failed: {err}. Canonical path: {}; recovery backup (if created): {}",
                input.display(),
                backup.display()
            ),
        )
    })?;
    let backed_up = match read_step_preset_bytes("verify Step Preset rename source", &backup) {
        Ok(bytes) => bytes,
        Err(read_error) => {
            return Err(rollback_local_replacement(
                input,
                &backup,
                &failed,
                &read_error,
            ));
        }
    };
    if backed_up != expected_source {
        let stale_error = ApiError::new(
            "step_preset_stale",
            "Step Preset changed at rename commit; inspect and retry",
        );
        return Err(rollback_local_replacement(
            input,
            &backup,
            &failed,
            &stale_error,
        ));
    }

    let verification = read_step_preset_bytes("verify committed Step Preset bytes", input)
        .and_then(|bytes| {
            if bytes == expected_output {
                Ok(())
            } else {
                Err(ApiError::new(
                    "step_preset_local_commit_mismatch",
                    "committed Step Preset differs from the validated staging bytes",
                ))
            }
        });
    if let Err(err) = verification {
        return Err(rollback_local_replacement(input, &backup, &failed, &err));
    }
    if let Err(sync_error) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(input)
        .and_then(|file| file.sync_all())
    {
        let error = step_preset_io_error("flush committed Step Preset", input, sync_error);
        return Err(rollback_local_replacement(input, &backup, &failed, &error));
    }

    // The committed file is already durable and valid. Do not turn a harmless
    // backup-cleanup failure into an ambiguous rename failure in the UI.
    let _ = std::fs::remove_file(&backup);
    Ok(())
}

fn rollback_local_replacement(
    current: &Path,
    backup: &Path,
    failed: &Path,
    operation_error: &ApiError,
) -> ApiError {
    if let Err(cleanup_error) = remove_file_if_exists(failed) {
        return ApiError::new(
            "step_preset_local_rollback_failed",
            format!(
                "cannot prepare rollback after '{}': {}. Recovery backup: {}; intended path: {}",
                operation_error.message,
                cleanup_error.message,
                backup.display(),
                current.display()
            ),
        );
    }
    match atomic_restore_backup(current, backup, failed) {
        Err(restore_error) => local_rollback_error(
            "atomically restore original Step Preset",
            operation_error,
            backup,
            current,
            restore_error,
        ),
        Ok(()) => ApiError::new(
            "step_preset_local_commit_rolled_back",
            format!(
                "Step Preset update was rolled back after '{}'. Original restored at {}; uncommitted bytes preserved at {}",
                operation_error.message,
                current.display(),
                failed.display()
            ),
        ),
    }
}

#[cfg(windows)]
fn durable_move_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn durable_move_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    // hard_link is an atomic no-replace publication on the same filesystem.
    // Removing the original afterwards means a crash exposes either the old
    // canonical name or the quarantine (occasionally both), never data loss.
    std::fs::hard_link(source, destination)?;
    sync_parent_directory(destination)?;
    if let Err(remove_error) = std::fs::remove_file(source) {
        let _ = std::fs::remove_file(destination);
        return Err(remove_error);
    }
    sync_parent_directory(source)
}

#[cfg(windows)]
fn atomic_replace_with_backup(
    current: &Path,
    replacement: &Path,
    backup: &Path,
    _expected_source: &[u8],
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let current_wide: Vec<u16> = current.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement_wide: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let backup_wide: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
    let replaced = unsafe {
        ReplaceFileW(
            current_wide.as_ptr(),
            replacement_wide.as_ptr(),
            backup_wide.as_ptr(),
            // REPLACEFILE_WRITE_THROUGH is explicitly unsupported by Win32.
            // The replacement bytes are flushed before this call and the new
            // canonical handle is flushed again after verification.
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_with_backup(
    current: &Path,
    replacement: &Path,
    backup: &Path,
    expected_source: &[u8],
) -> std::io::Result<()> {
    let mut backup_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(backup)?;
    backup_file.write_all(expected_source)?;
    backup_file.sync_all()?;
    std::fs::rename(replacement, current)?;
    sync_parent_directory(current)
}

#[cfg(windows)]
fn atomic_restore_backup(current: &Path, backup: &Path, failed: &Path) -> std::io::Result<()> {
    atomic_replace_with_backup(current, backup, failed, &[])?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(current)?
        .sync_all()
}

#[cfg(not(windows))]
fn atomic_restore_backup(current: &Path, backup: &Path, failed: &Path) -> std::io::Result<()> {
    let _ = std::fs::copy(current, failed)?;
    std::fs::OpenOptions::new()
        .read(true)
        .open(failed)?
        .sync_all()?;
    std::fs::rename(backup, current)?;
    sync_parent_directory(current)
}

#[cfg(not(windows))]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    std::fs::File::open(parent)?.sync_all()
}

fn local_rollback_error(
    action: &str,
    operation_error: &ApiError,
    recovery_source: &Path,
    recovery_destination: &Path,
    restore_error: std::io::Error,
) -> ApiError {
    ApiError::new(
        "step_preset_local_rollback_failed",
        format!(
            "{action} after '{}' also failed: {restore_error}. Recovery source: {}; intended path: {}",
            operation_error.message,
            recovery_source.display(),
            recovery_destination.display()
        ),
    )
}

fn step_preset_io_error(action: &str, path: &Path, err: std::io::Error) -> ApiError {
    ApiError::new(
        "step_preset_io_failed",
        format!("{action}: {}: {err}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_step_preset_path_requires_asset_extension() {
        assert!(ensure_local_step_preset_path(Path::new("library/groove.mssp")).is_ok());
        assert!(ensure_local_step_preset_path(Path::new("library/GROOVE.MSSP")).is_ok());
        assert!(ensure_local_step_preset_path(Path::new("library/groove.bin")).is_err());
        assert!(ensure_local_step_preset_path(Path::new("library/.mssp")).is_err());
        assert!(ensure_local_step_preset_path(Path::new("library/groove:.mssp")).is_err());
    }

    #[test]
    fn mutation_confirmation_fields_are_fail_closed() {
        let digest = "0".repeat(STEP_PRESET_PREVIEW_KEY_BYTES);
        assert!(ensure_valid_confirmation("human-groove", "Human groove", &digest).is_ok());
        assert!(ensure_valid_confirmation("", "Human groove", &digest).is_err());
        assert!(ensure_valid_confirmation("con", "Human groove", &digest).is_err());
        assert!(ensure_valid_confirmation("human-groove", " Human groove", &digest).is_err());
        assert!(ensure_valid_confirmation("human-groove", "Human groove", "").is_err());
        assert!(ensure_valid_confirmation(
            "human-groove",
            "Human groove",
            &"A".repeat(STEP_PRESET_PREVIEW_KEY_BYTES),
        )
        .is_err());
    }

    #[test]
    fn semantic_name_validation_matches_the_core_codec_contract() {
        assert!(validate_new_semantic_name("Élan").is_ok());
        assert!(validate_new_semantic_name(" leading").is_err());
        assert!(validate_new_semantic_name("trailing ").is_err());
        assert!(validate_new_semantic_name("control\u{0085}").is_err());
        assert!(
            validate_new_semantic_name(&"a".repeat(STEP_PRESET_MAX_SEMANTIC_NAME_BYTES + 1))
                .is_err()
        );
    }

    #[test]
    fn storage_and_cleanup_failures_are_retried_idempotently() {
        for kind in ["conditional_storage_error", "conditional_invalid_state"] {
            assert!(conditional_mutation_may_be_committed(&ControllerFsError {
                kind: kind.to_string(),
                message: "uncertain firmware outcome".to_string(),
            }));
        }
    }

    #[test]
    fn controller_path_capacity_is_checked_before_staging() {
        let capabilities = FsCapabilities {
            status: FsStatus::Ok,
            rpc_schema: 1,
            max_chunk_size: 1024,
            response_buffer_size: 1024,
            max_list_entries: 8,
            max_path_length: 12,
            feature_flags: 1 << 3,
        };
        assert!(ensure_remote_paths_fit(&capabilities, &["/short.mssp"]).is_ok());
        assert!(ensure_remote_paths_fit(&capabilities, &["/too-long-name.mssp"]).is_err());
    }

    #[test]
    fn local_step_preset_reads_are_bounded() {
        let temp = StepPresetTempDir::create("size-test").unwrap();
        let path = temp.path("oversized.mssp");
        std::fs::write(&path, vec![0u8; STEP_PRESET_MAX_BYTES + 1]).unwrap();
        let error = read_step_preset_bytes("test", &path).unwrap_err();
        assert_eq!(error.code, "step_preset_too_large");
    }

    #[test]
    fn durable_move_never_replaces_an_existing_destination() {
        let temp = StepPresetTempDir::create("move-test").unwrap();
        let source = temp.path("source.mssp");
        let destination = temp.path("destination.mssp");
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&destination, b"destination").unwrap();
        assert!(durable_move_no_replace(&source, &destination).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"source");
        assert_eq!(std::fs::read(&destination).unwrap(), b"destination");

        std::fs::remove_file(&destination).unwrap();
        durable_move_no_replace(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"source");
    }

    #[test]
    fn remote_step_preset_path_accepts_only_canonical_product_paths() {
        assert!(
            ensure_remote_step_preset_path("/midi-studio/library/step-presets/groove.mssp").is_ok()
        );

        let too_long = format!("/midi-studio/library/{}.mssp", "a".repeat(240));
        for invalid in [
            "/midi-studio/library/../projects/groove.mssp",
            "/midi-studio//library/groove.mssp",
            "/midi-studio/library\\groove.mssp",
            "/midi-studio/library/groove.bin",
            "/midi-studio/library/.mssp",
            "/midi-studio/library/bad:name.mssp",
            "/midi-studio/library./groove.mssp",
            "/outside/groove.mssp",
            "/midi-studio/library/gr\u{0007}oove.mssp",
            "/midi-studio/library/grôove.mssp",
            too_long.as_str(),
        ] {
            assert!(
                ensure_remote_step_preset_path(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn transaction_paths_are_unique_and_preserve_step_preset_extension() {
        let first = unique_remote_step_preset_path("test");
        let second = unique_remote_step_preset_path("test");
        assert_ne!(first, second);
        assert!(first.starts_with("/midi-studio/tmp/"));
        assert!(first.ends_with(".mssp"));
    }
}
