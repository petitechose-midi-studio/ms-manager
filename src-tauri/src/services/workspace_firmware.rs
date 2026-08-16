use std::path::{Path, PathBuf};
use std::process::Stdio;

use ms_manager_core::FirmwareTarget;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::api_error::{ApiError, ApiResult};
use crate::models::FlashMessageLevel;
use crate::services::{artifact_paths, flash, process};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceFirmwareProfile {
    pub id: String,
    pub source_path: PathBuf,
    pub artifact_path: PathBuf,
    pub artifact_ready: bool,
    pub artifact_built_at_ms: Option<u64>,
    pub source_dirty: bool,
}

pub async fn profiles(target: FirmwareTarget) -> ApiResult<Vec<WorkspaceFirmwareProfile>> {
    let root = workspace_root()?;
    let output = run_ms(&root, &["profiles", app_name(target), "--json"]).await?;
    if !output.status.success() {
        return Err(command_error(
            "firmware_profile_discovery_failed",
            "Unable to discover development firmware profiles.",
            &output,
        ));
    }

    serde_json::from_slice(&output.stdout).map_err(|error| {
        ApiError::new(
            "firmware_profile_discovery_failed",
            format!("invalid profile list from ms-dev-env: {error}"),
        )
    })
}

pub async fn build(
    app: &tauri::AppHandle,
    target: FirmwareTarget,
    profile_id: &str,
) -> ApiResult<WorkspaceFirmwareProfile> {
    let selected_profile = profile(target, profile_id).await?;

    if selected_profile.source_dirty {
        flash::emit_flash_message(
            app,
            FlashMessageLevel::Warn,
            format!(
                "Building {}/{} from a source repository with uncommitted changes; this firmware will not map to a clean commit.",
                app_name(target), selected_profile.id
            ),
        );
    }

    let root = workspace_root()?;
    let output = run_ms_streaming(
        app,
        &root,
        &[
            "build",
            app_name(target),
            "--target",
            "teensy",
            "--env",
            &selected_profile.id,
            "--stream",
        ],
    )
    .await?;
    if !output.status.success() {
        return Err(command_error(
            "firmware_build_failed",
            &format!(
                "Development firmware build failed for {}.",
                selected_profile.id
            ),
            &output,
        ));
    }

    let profile = profile(target, profile_id).await?;
    if !profile.artifact_ready {
        return Err(ApiError::new(
            "firmware_build_failed",
            format!(
                "build completed without firmware artifact: {}",
                profile.artifact_path.display()
            ),
        ));
    }

    Ok(profile)
}

pub async fn profile(
    target: FirmwareTarget,
    profile_id: &str,
) -> ApiResult<WorkspaceFirmwareProfile> {
    let profile_id = profile_id.trim();
    profiles(target)
        .await?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| {
            ApiError::new(
                "firmware_profile_invalid",
                format!("development firmware profile is not available: {profile_id}"),
            )
        })
}

fn app_name(target: FirmwareTarget) -> &'static str {
    match target {
        FirmwareTarget::Standalone => "core",
        FirmwareTarget::Bitwig => "bitwig",
    }
}

fn workspace_root() -> ApiResult<PathBuf> {
    artifact_paths::dev_artifacts_path()
        .ancestors()
        .find(|path| path.join(".ms-workspace").is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            ApiError::new(
                "development_environment_missing",
                "MIDI Studio development workspace was not found.",
            )
        })
}

async fn run_ms(root: &Path, args: &[&str]) -> ApiResult<std::process::Output> {
    ms_command(root, args)?.output().await.map_err(|error| {
        ApiError::new(
            "development_command_failed",
            format!("unable to run ms-dev-env: {error}"),
        )
    })
}

async fn run_ms_streaming(
    app: &tauri::AppHandle,
    root: &Path,
    args: &[&str],
) -> ApiResult<std::process::Output> {
    let mut command = ms_command(root, args)?;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ApiError::new(
                "development_command_failed",
                format!("unable to run ms-dev-env: {error}"),
            )
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ApiError::new("internal_error", "missing build stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ApiError::new("internal_error", "missing build stderr"))?;
    let stdout_task = tokio::spawn(stream_lines(stdout, app.clone()));
    let stderr_task = tokio::spawn(stream_lines(stderr, app.clone()));
    let status = child.wait().await.map_err(|error| {
        ApiError::new(
            "development_command_failed",
            format!("unable to wait for ms-dev-env: {error}"),
        )
    })?;
    let stdout = stdout_task
        .await
        .map_err(|error| ApiError::new("io_read_failed", format!("build stdout: {error}")))?
        .map_err(|error| ApiError::new("io_read_failed", format!("build stdout: {error}")))?;
    let stderr = stderr_task
        .await
        .map_err(|error| ApiError::new("io_read_failed", format!("build stderr: {error}")))?
        .map_err(|error| ApiError::new("io_read_failed", format!("build stderr: {error}")))?;

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

async fn stream_lines<R>(reader: R, app: tauri::AppHandle) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        output.extend_from_slice(line.as_bytes());
        output.push(b'\n');
        if !line.trim().is_empty() {
            flash::emit_flash_message(&app, FlashMessageLevel::Info, line);
        }
    }
    Ok(output)
}

fn ms_command(root: &Path, args: &[&str]) -> ApiResult<tokio::process::Command> {
    let python = if cfg!(windows) {
        root.join(".venv/Scripts/python.exe")
    } else {
        root.join(".venv/bin/python")
    };
    if !python.is_file() {
        return Err(ApiError::new(
            "development_environment_missing",
            format!("ms-dev-env Python runtime not found: {}", python.display()),
        ));
    }

    let mut command = tokio::process::Command::new(&python);
    process::no_console_window(&mut command);
    command
        .arg("-m")
        .arg("ms")
        .arg("--workspace")
        .arg(root)
        .args(args)
        .env("PYTHONUNBUFFERED", "1")
        .current_dir(root);
    Ok(command)
}

fn command_error(code: &str, message: &str, output: &std::process::Output) -> ApiError {
    ApiError::new(code, message).with_details(serde_json::json!({
        "exit_code": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
        "suggested_actions": [
            "Open the activity details to inspect the build error.",
            "Fix the reported source or toolchain error, then retry the build."
        ]
    }))
}
