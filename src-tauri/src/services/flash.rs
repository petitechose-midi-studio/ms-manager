use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, BufReader};

use ms_manager_core::{InstallState, LastFlashed, Settings};
use tauri::{Emitter, Manager};

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::models::FlashEvent;
use crate::services::{artifact_resolver, bridge_ctl};
use crate::services::process;
use crate::state::AppState;

const FLASH_EVENT: &str = "ms-manager://flash";

pub async fn flash_firmware(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    settings: &Settings,
    installed: Option<&InstallState>,
    profile: &str,
) -> ApiResult<LastFlashed> {
    if profile.trim().is_empty() {
        return Err(ApiError::new("invalid_profile", "profile cannot be empty"));
    }

    let loader = artifact_resolver::resolve_loader_exe(&settings, layout)?;
    let firmware = artifact_resolver::resolve_firmware_for_profile(
        settings,
        layout,
        installed,
        profile,
    )?;

    // Best-effort: ask oc-bridge to release the serial port before flashing.
    // This keeps the UX smooth when the bridge is running in the background.
    for control_port in bridge_control_ports(app) {
        let _ = bridge_ctl::send_command(control_port, "pause", std::time::Duration::from_secs(2))
            .await;
    }

    let _ = app.emit(
        FLASH_EVENT,
        FlashEvent::Begin {
            channel: installed.map(|v| v.channel).unwrap_or(settings.channel),
            tag: installed
                .map(|v| v.tag.clone())
                .unwrap_or_else(|| "workspace".to_string()),
            profile: profile.to_string(),
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
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| ApiError::new("io_read_failed", format!("flash stdout: {e}")))?
    {
        if line.trim().is_empty() {
            continue;
        }
        let _ = app.emit(FLASH_EVENT, FlashEvent::Output { line });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| ApiError::new("io_exec_failed", format!("wait flash: {e}")))?;

    // Best-effort: re-open serial after flashing.
    for control_port in bridge_control_ports(app) {
        let _ = bridge_ctl::send_command(
            control_port,
            "resume",
            std::time::Duration::from_millis(600),
        )
        .await;
    }

    let stderr = stderr_task.await.unwrap_or_default();

    let ok = status.success();
    let _ = app.emit(FLASH_EVENT, FlashEvent::Done { ok });

    if !ok {
        return Err(
            ApiError::new("flash_failed", "firmware flash failed").with_details(
                serde_json::json!({
                    "exit_code": status.code(),
                    "stderr": stderr,
                    "firmware": firmware.display().to_string(),
                }),
            ),
        );
    }

    Ok(LastFlashed {
        channel: installed.map(|v| v.channel).unwrap_or(settings.channel),
        tag: installed
            .map(|v| v.tag.clone())
            .unwrap_or_else(|| "workspace".to_string()),
        profile: profile.to_string(),
        flashed_at_ms: now_ms(),
    })
}

fn bridge_control_ports(app: &tauri::AppHandle) -> Vec<u16> {
    let mut ports = app
        .state::<AppState>()
        .bridge_instances_get()
        .instances
        .into_iter()
        .filter(|binding| binding.enabled)
        .map(|binding| binding.control_port)
        .collect::<Vec<_>>();

    if ports.is_empty() {
        ports.push(bridge_ctl::DEFAULT_CONTROL_PORT);
    }

    ports.sort_unstable();
    ports.dedup();
    ports
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
