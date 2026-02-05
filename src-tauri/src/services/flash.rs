use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, BufReader};

use ms_manager_core::{InstallState, LastFlashed};
use tauri::Emitter;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::services::bridge_ctl;
use crate::models::FlashEvent;
use crate::services::process;

const FLASH_EVENT: &str = "ms-manager://flash";

pub async fn flash_firmware(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    installed: &InstallState,
    profile: &str,
) -> ApiResult<LastFlashed> {
    if profile.trim().is_empty() {
        return Err(ApiError::new("invalid_profile", "profile cannot be empty"));
    }

    let loader = loader_path(layout);
    if !loader.exists() {
        return Err(ApiError::new(
            "loader_missing",
            "midi-studio-loader not found (install the host bundle first)",
        ));
    }

    let firmware = firmware_path_for_profile(layout, &installed.tag, profile)?;

    // Best-effort: ask oc-bridge to release the serial port before flashing.
    // This keeps the UX smooth when the bridge is running in the background.
    let _ = bridge_ctl::send_command(
        bridge_ctl::DEFAULT_CONTROL_PORT,
        "pause",
        std::time::Duration::from_secs(2),
    )
    .await;

    let _ = app.emit(
        FLASH_EVENT,
        FlashEvent::Begin {
            channel: installed.channel,
            tag: installed.tag.clone(),
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
    let _ = bridge_ctl::send_command(
        bridge_ctl::DEFAULT_CONTROL_PORT,
        "resume",
        std::time::Duration::from_millis(600),
    )
    .await;

    let stderr = stderr_task.await.unwrap_or_default();

    let ok = status.success();
    let _ = app.emit(FLASH_EVENT, FlashEvent::Done { ok });

    if !ok {
        return Err(ApiError::new("flash_failed", "firmware flash failed").with_details(
            serde_json::json!({
                "exit_code": status.code(),
                "stderr": stderr,
                "firmware": firmware.display().to_string(),
            }),
        ));
    }

    Ok(LastFlashed {
        channel: installed.channel,
        tag: installed.tag.clone(),
        profile: profile.to_string(),
        flashed_at_ms: now_ms(),
    })
}

fn loader_path(layout: &PayloadLayout) -> PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("midi-studio-loader.exe")
    } else {
        bin.join("midi-studio-loader")
    }
}

fn firmware_path_for_profile(layout: &PayloadLayout, tag: &str, profile: &str) -> ApiResult<PathBuf> {
    let dir = layout.version_dir(tag).join("firmware");
    if !dir.exists() {
        return Err(ApiError::new(
            "firmware_missing",
            "firmware directory not found for installed version",
        )
        .with_details(serde_json::json!({"dir": dir.display().to_string()})));
    }

    let mut candidates = Vec::<PathBuf>::new();
    let read = std::fs::read_dir(&dir).map_err(|e| {
        ApiError::new(
            "io_read_failed",
            format!("read dir {}: {e}", dir.display()),
        )
    })?;
    for entry in read {
        let entry = entry.map_err(|e| ApiError::new("io_read_failed", e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).unwrap_or("") != "hex" {
            continue;
        }
        candidates.push(path);
    }

    let needle = profile.to_lowercase();
    let mut matches = candidates
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase()
                .contains(&needle)
        })
        .collect::<Vec<_>>();

    if matches.len() == 1 {
        return Ok(matches.remove(0));
    }

    let available = list_firmware_files(&dir);
    Err(ApiError::new(
        "firmware_missing",
        "firmware for selected profile is not installed (install that profile first)",
    )
    .with_details(serde_json::json!({
        "profile": profile,
        "dir": dir.display().to_string(),
        "available": available,
    })))
}

fn list_firmware_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()).unwrap_or("") != "hex" {
            continue;
        }
        if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            out.push(n.to_string());
        }
    }
    out.sort();
    out
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
