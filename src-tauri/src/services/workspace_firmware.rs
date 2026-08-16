use std::path::{Path, PathBuf};

use ms_manager_core::FirmwareTarget;
use serde::{Deserialize, Serialize};

use crate::api_error::{ApiError, ApiResult};
use crate::services::{artifact_paths, process};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceFirmwareProfile {
    pub id: String,
    pub label: String,
    pub artifact_path: PathBuf,
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
    target: FirmwareTarget,
    profile_id: &str,
) -> ApiResult<WorkspaceFirmwareProfile> {
    let profile_id = profile_id.trim();
    let profile = profiles(target)
        .await?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| {
            ApiError::new(
                "firmware_profile_invalid",
                format!("development firmware profile is not available: {profile_id}"),
            )
        })?;

    let root = workspace_root()?;
    let output = run_ms(
        &root,
        &[
            "build",
            app_name(target),
            "--target",
            "teensy",
            "--env",
            &profile.id,
        ],
    )
    .await?;
    if !output.status.success() {
        return Err(command_error(
            "firmware_build_failed",
            &format!("Development firmware build failed for {}.", profile.label),
            &output,
        ));
    }

    if !profile.artifact_path.is_file() {
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
        .current_dir(root);
    command.output().await.map_err(|error| {
        ApiError::new(
            "development_command_failed",
            format!("unable to run ms-dev-env: {error}"),
        )
    })
}

fn command_error(code: &str, message: &str, output: &std::process::Output) -> ApiError {
    ApiError::new(code, message).with_details(serde_json::json!({
        "exit_code": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
        "suggested_actions": [
            "Open the activity details to inspect the build error.",
            "Fix the reported source or toolchain error, then retry Build & Flash."
        ]
    }))
}
