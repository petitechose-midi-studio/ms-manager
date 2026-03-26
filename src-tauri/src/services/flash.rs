use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, BufReader};

use ms_manager_core::{ArtifactSource, BridgeInstanceBinding, InstallState, LastFlashed};
use tauri::Emitter;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::models::FlashEvent;
use crate::services::{artifact_resolver, bridge_ctl, device};
use crate::services::process;

const FLASH_EVENT: &str = "ms-manager://flash";

pub async fn flash_firmware_for_binding(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> ApiResult<LastFlashed> {
    let loader = artifact_resolver::resolve_loader_exe_for_binding(layout, binding)?;
    let firmware = artifact_resolver::resolve_firmware_for_binding(layout, installed, binding)?;
    let device_target = resolve_flash_target(&loader, binding).await?;
    pause_bridge_instance(binding).await;

    let profile = binding.target.profile_id().to_string();
    let channel = binding.installed_channel.unwrap_or(ms_manager_core::Channel::Stable);
    let tag = flash_tag(installed, binding);

    let _ = app.emit(
        FLASH_EVENT,
        FlashEvent::Begin {
            channel,
            tag: tag.clone(),
            profile: profile.clone(),
        },
    );

    let mut cmd = tokio::process::Command::new(&loader);
    process::no_console_window(&mut cmd);
    let mut child = cmd
        .args([
            "flash",
            "--json",
            "--json-progress",
            "percent",
            "--wait",
            "--wait-timeout-ms",
            "60000",
        ])
        .arg("--device")
        .arg(&device_target.target_id)
        .arg(&firmware)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ApiError::new("io_exec_failed", format!("spawn flash: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ApiError::new("internal_error", "missing flash stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ApiError::new("internal_error", "missing flash stderr"))?;

    let stderr_task = tokio::spawn(async move {
        let mut s = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str(&line);
        }
        s
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut stdout_lines = Vec::new();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| ApiError::new("io_read_failed", format!("flash stdout: {e}")))? 
    {
        if line.trim().is_empty() {
            continue;
        }
        stdout_lines.push(line.clone());
        let _ = app.emit(FLASH_EVENT, FlashEvent::Output { line });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| ApiError::new("io_exec_failed", format!("wait flash: {e}")))?;

    resume_bridge_instance(binding).await;

    let stderr = stderr_task.await.unwrap_or_default();

    let ok = status.success();
    let _ = app.emit(FLASH_EVENT, FlashEvent::Done { ok });

    if !ok {
        let summary = summarize_flash_failure(&stdout_lines, &stderr);
        return Err(
            ApiError::new("flash_failed", format!("firmware flash failed: {summary}")).with_details(
                serde_json::json!({
                    "exit_code": status.code(),
                    "stderr": stderr,
                    "stdout": stdout_lines,
                    "firmware": firmware.display().to_string(),
                    "instance_id": binding.instance_id,
                    "device_target_id": device_target.target_id,
                }),
            ),
        );
    }

    Ok(LastFlashed {
        channel,
        tag,
        profile,
        flashed_at_ms: now_ms(),
    })
}

async fn resolve_flash_target(
    loader: &std::path::Path,
    binding: &BridgeInstanceBinding,
) -> ApiResult<crate::models::DeviceTarget> {
    let targets = device::list_targets_with_loader(loader).await?;
    device::select_serial_target(&targets, &binding.controller_serial).map_err(|err| {
        err.with_details(serde_json::json!({
            "loader": loader.display().to_string(),
            "controller_serial": binding.controller_serial,
            "instance_id": binding.instance_id,
        }))
    })
}

fn flash_tag(installed: Option<&InstallState>, binding: &BridgeInstanceBinding) -> String {
    match binding.artifact_source {
        ArtifactSource::Workspace => "workspace".to_string(),
        ArtifactSource::Installed => binding
            .installed_pinned_tag
            .as_ref()
            .map(|tag| tag.trim())
            .filter(|tag| !tag.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| installed.map(|v| v.tag.clone()))
            .unwrap_or_else(|| "installed".to_string()),
    }
}

fn summarize_flash_failure(stdout_lines: &[String], stderr: &str) -> String {
    stdout_lines
        .iter()
        .rev()
        .find_map(|line| loader_message_from_json(line))
        .or_else(|| last_non_empty_line(stderr).map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown loader error".to_string())
}

fn loader_message_from_json(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let obj = value.as_object()?;
    obj.get("message")
        .and_then(|message| message.as_str())
        .map(ToOwned::to_owned)
}

fn last_non_empty_line(text: &str) -> Option<&str> {
    text.lines().rev().find(|line| !line.trim().is_empty())
}

async fn pause_bridge_instance(binding: &BridgeInstanceBinding) {
    if !binding.enabled {
        return;
    }

    let _ = bridge_ctl::send_command(
        binding.control_port,
        "pause",
        std::time::Duration::from_secs(2),
    )
    .await;
}

async fn resume_bridge_instance(binding: &BridgeInstanceBinding) {
    if !binding.enabled {
        return;
    }

    let _ = bridge_ctl::send_command(
        binding.control_port,
        "resume",
        std::time::Duration::from_millis(600),
    )
    .await;
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_flash_failure_prefers_loader_json_message() {
        let lines = vec![
            "{\"event\":\"error\",\"message\":\"target selection failed\"}".to_string(),
            "{\"event\":\"operation_summary\",\"message\":\"multiple targets detected (2); use --device or --all\"}".to_string(),
        ];

        let summary = summarize_flash_failure(&lines, "");
        assert_eq!(summary, "multiple targets detected (2); use --device or --all");
    }

    #[test]
    fn flash_tag_uses_workspace_for_workspace_instances() {
        let binding = BridgeInstanceBinding {
            instance_id: "bitwig-hardware-17076520".to_string(),
            display_name: None,
            app: ms_manager_core::BridgeApp::Bitwig,
            mode: ms_manager_core::BridgeMode::Hardware,
            controller_serial: "17076520".to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
            target: ms_manager_core::FirmwareTarget::Bitwig,
            artifact_source: ArtifactSource::Workspace,
            installed_channel: None,
            installed_pinned_tag: None,
            host_udp_port: 9000,
            control_port: 7999,
            log_broadcast_port: 9999,
            enabled: true,
        };

        assert_eq!(flash_tag(None, &binding), "workspace");
    }
}
