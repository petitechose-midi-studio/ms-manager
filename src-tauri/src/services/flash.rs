use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, BufReader};

use ms_manager_core::{ArtifactSource, BridgeInstanceBinding, InstallState, LastFlashed};
use tauri::Emitter;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::models::{FlashEvent, FlashMessageLevel};
use crate::services::process;
use crate::services::{artifact_resolver, bridge_ctl, bridge_status, device, ux_recorder};

const FLASH_EVENT: &str = "ms-manager://flash";
const POST_FLASH_READY_TIMEOUT: Duration = Duration::from_secs(20);
const POST_FLASH_POLL_INTERVAL: Duration = Duration::from_millis(250);
const POST_FLASH_BRIDGE_STATUS_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BridgeControlAction {
    Pause,
    Resume,
}

impl BridgeControlAction {
    fn label(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
        }
    }

    fn request_message(self) -> &'static str {
        match self {
            Self::Pause => "Requesting bridge pause before flash...",
            Self::Resume => "Requesting bridge resume after flash...",
        }
    }

    fn success_message(self) -> &'static str {
        match self {
            Self::Pause => "Bridge pause confirmed",
            Self::Resume => "Bridge resume acknowledged",
        }
    }

    fn continuation_suffix(self) -> &'static str {
        match self {
            Self::Pause => " Flash will continue.",
            Self::Resume => " Post-flash verification will continue.",
        }
    }
}

pub async fn flash_firmware_for_binding(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> ApiResult<LastFlashed> {
    let loader = artifact_resolver::resolve_loader_exe_for_binding(layout, binding)
        .map_err(|error| make_actionable_flash_error(error, binding))?;
    let firmware = artifact_resolver::resolve_firmware_for_binding(layout, installed, binding)
        .map_err(|error| make_actionable_flash_error(error, binding))?;
    let device_target = resolve_flash_target(&loader, binding)
        .await
        .map_err(|error| make_actionable_flash_error(error, binding))?;
    let mut bridge_transition_warnings = Vec::new();
    ux_recorder::close_session_for_instance(app, layout, &binding.instance_id, "flash_begin");
    if let Some(warning) = run_bridge_control_action(app, binding, BridgeControlAction::Pause).await
    {
        bridge_transition_warnings.push(warning);
    }

    let profile = binding.target.profile_id().to_string();
    let channel = binding
        .installed_channel
        .unwrap_or(ms_manager_core::Channel::Stable);
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
        .map_err(|e| {
            make_actionable_flash_error(
                ApiError::new("io_exec_failed", format!("spawn flash: {e}")),
                binding,
            )
        })?;

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

    let status = child.wait().await.map_err(|e| {
        make_actionable_flash_error(
            ApiError::new("io_exec_failed", format!("wait flash: {e}")),
            binding,
        )
    })?;

    if let Some(warning) =
        run_bridge_control_action(app, binding, BridgeControlAction::Resume).await
    {
        bridge_transition_warnings.push(warning);
    }

    let stderr = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let _ = app.emit(FLASH_EVENT, FlashEvent::Done { ok: false });
        let summary = summarize_flash_failure(&stdout_lines, &stderr);
        return Err(make_actionable_flash_error(
            ApiError::new("flash_failed", format!("firmware flash failed: {summary}"))
                .with_details(serde_json::json!({
                    "exit_code": status.code(),
                    "stderr": stderr,
                    "stdout": stdout_lines,
                    "firmware": firmware.display().to_string(),
                    "instance_id": binding.instance_id,
                    "device_target_id": device_target.target_id,
                    "bridge_transition_warnings": bridge_transition_warnings,
                })),
            binding,
        ));
    }

    if let Err(error) =
        verify_post_flash_health(app, &loader, binding, &bridge_transition_warnings).await
    {
        let _ = app.emit(FLASH_EVENT, FlashEvent::Done { ok: false });
        return Err(make_actionable_flash_error(error, binding));
    }
    let _ = app.emit(FLASH_EVENT, FlashEvent::Done { ok: true });

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

async fn verify_post_flash_health(
    app: &tauri::AppHandle,
    loader: &std::path::Path,
    binding: &BridgeInstanceBinding,
    bridge_transition_warnings: &[String],
) -> ApiResult<()> {
    if binding.enabled {
        emit_flash_output(app, "Verifying bridge reconnect after flash...");
        wait_for_bridge_ready_after_flash(binding, bridge_transition_warnings).await?;
        emit_flash_output(app, "Bridge healthy after flash");
        return Ok(());
    }

    emit_flash_output(app, "Verifying controller reconnect after flash...");
    wait_for_device_ready_after_flash(loader, binding, bridge_transition_warnings).await?;
    emit_flash_output(app, "Controller reconnected after flash");
    Ok(())
}

async fn wait_for_bridge_ready_after_flash(
    binding: &BridgeInstanceBinding,
    bridge_transition_warnings: &[String],
) -> ApiResult<()> {
    let deadline = tokio::time::Instant::now() + POST_FLASH_READY_TIMEOUT;
    let mut last_runtime: Option<bridge_status::BridgeRuntimeState> = None;
    let mut last_error: Option<String> = None;

    loop {
        match bridge_ctl::send_command(
            binding.control_port,
            "status",
            POST_FLASH_BRIDGE_STATUS_TIMEOUT,
        )
        .await
        {
            Ok(value) => {
                let runtime = bridge_status::BridgeRuntimeState::from_value(value);
                if runtime.is_ready_after_flash_for(binding) {
                    return Ok(());
                }
                last_runtime = Some(runtime);
            }
            Err(error) => {
                last_error = Some(error);
            }
        }

        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(POST_FLASH_POLL_INTERVAL).await;
    }

    let summary = last_runtime
        .as_ref()
        .and_then(|runtime| runtime.post_flash_message_for(binding))
        .or(last_error.clone())
        .unwrap_or_else(|| "bridge did not return to a healthy state after flash".to_string());

    Err(ApiError::new(
        "post_flash_check_failed",
        format!("post-flash health check failed: {summary}"),
    )
    .with_details(serde_json::json!({
        "instance_id": binding.instance_id,
        "controller_serial": binding.controller_serial,
        "control_port": binding.control_port,
        "timeout_ms": POST_FLASH_READY_TIMEOUT.as_millis(),
        "bridge_transition_warnings": bridge_transition_warnings,
        "last_bridge_error": last_error,
        "last_runtime": last_runtime.as_ref().map(|runtime| serde_json::json!({
            "ok": runtime.ok,
            "paused": runtime.paused,
            "serial_open": runtime.serial_open,
            "version": runtime.version,
            "resolved_serial_port": runtime.resolved_serial_port,
            "instance_id": runtime.reported_instance_id,
            "controller_serial": runtime.connected_serial,
            "message": runtime.message,
        })),
    })))
}

async fn wait_for_device_ready_after_flash(
    loader: &std::path::Path,
    binding: &BridgeInstanceBinding,
    bridge_transition_warnings: &[String],
) -> ApiResult<()> {
    let deadline = tokio::time::Instant::now() + POST_FLASH_READY_TIMEOUT;

    loop {
        match resolve_flash_target(loader, binding).await {
            Ok(_) => return Ok(()),
            Err(error) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(ApiError::new(
                        "post_flash_check_failed",
                        format!("post-flash device check failed: {}", error.message),
                    )
                    .with_details(serde_json::json!({
                        "instance_id": binding.instance_id,
                        "controller_serial": binding.controller_serial,
                        "loader": loader.display().to_string(),
                        "timeout_ms": POST_FLASH_READY_TIMEOUT.as_millis(),
                        "bridge_transition_warnings": bridge_transition_warnings,
                        "last_error": error,
                    })));
                }
            }
        }
        tokio::time::sleep(POST_FLASH_POLL_INTERVAL).await;
    }
}

fn emit_flash_output(app: &tauri::AppHandle, line: impl Into<String>) {
    let _ = app.emit(FLASH_EVENT, FlashEvent::Output { line: line.into() });
}

fn make_actionable_flash_error(error: ApiError, binding: &BridgeInstanceBinding) -> ApiError {
    match error.code.as_str() {
        "artifact_missing" => actionable_error(
            error,
            artifact_missing_user_message(binding),
            artifact_missing_actions(binding),
        ),
        "device_not_found" => actionable_error(
            error,
            format!(
                "Controller {} was not found for flashing.",
                binding.controller_serial
            ),
            vec![
                "Check that the controller is connected over USB and powered on.",
                "If it is rebooting or entering bootloader mode, wait a few seconds and retry.",
                "Close any app that may still be using the controller, then retry the flash.",
            ],
        ),
        "device_ambiguous" => actionable_error(
            error,
            "Multiple matching flash targets were detected for this controller.".to_string(),
            vec![
                "Disconnect extra controllers or bootloaders.",
                "Leave only the controller you want to flash connected.",
                "Retry the flash once only one matching target remains.",
            ],
        ),
        "device_list_failed" => actionable_error(
            error,
            "Unable to inspect connected controllers before flashing.".to_string(),
            vec![
                "Reconnect the controller and retry.",
                "Check that the firmware loader is available in the selected artifact source.",
                "If the problem persists, open the activity log and capture the loader details.",
            ],
        ),
        "io_exec_failed" => actionable_error(
            error,
            "Unable to start the firmware flashing tool.".to_string(),
            vec![
                "Check that the selected firmware artifacts are installed and intact.",
                "Retry the flash after closing any app that may still be using the controller.",
                "If it still fails immediately, open the activity log and capture the error details.",
            ],
        ),
        "flash_failed" => classify_loader_failure(error),
        "post_flash_check_failed" => actionable_error(
            error,
            "The firmware was written, but the controller did not return to a healthy state in time."
                .to_string(),
            vec![
                "Wait a few seconds for the controller to reconnect, then refresh and retry if needed.",
                "If the controller stays unavailable, disconnect and reconnect it.",
                "If the issue repeats, capture the activity log and bridge details for diagnosis.",
            ],
        ),
        _ => error,
    }
}

fn classify_loader_failure(error: ApiError) -> ApiError {
    let original_message = error.message.to_ascii_lowercase();

    if original_message.contains("multiple targets detected")
        || original_message.contains("multiple halfkay devices")
        || original_message.contains("multiple targets matched")
    {
        return actionable_error(
            error,
            "More than one flash target was detected.".to_string(),
            vec![
                "Disconnect extra controllers or Teensy bootloaders.",
                "Leave only the controller you want to flash connected.",
                "Retry the flash once the target list is unambiguous.",
            ],
        );
    }

    if original_message.contains("no targets found")
        || original_message.contains("no halfkay device found")
        || original_message.contains("halfkay did not appear after soft reboot")
        || original_message.contains("unable to open halfkay device")
    {
        return actionable_error(
            error,
            "The controller did not enter bootloader mode for flashing.".to_string(),
            vec![
                "Retry the flash and wait for the controller to switch into bootloader mode.",
                "If needed, disconnect and reconnect the controller before trying again.",
                "If the controller still does not appear, press its bootloader button and retry.",
            ],
        );
    }

    actionable_error(
        error,
        "Firmware flashing failed.".to_string(),
        vec![
            "Retry the flash once the controller is connected and idle.",
            "If the error repeats, check the selected firmware source and release.",
            "Capture the activity log for diagnosis if the failure persists.",
        ],
    )
}

fn actionable_error(
    error: ApiError,
    user_message: String,
    suggested_actions: Vec<&str>,
) -> ApiError {
    let original_message = error.message.clone();
    let details = merge_error_details(
        error.details,
        serde_json::json!({
            "original_message": original_message,
            "suggested_actions": suggested_actions,
        }),
    );

    ApiError {
        code: error.code,
        message: user_message,
        details: Some(details),
    }
}

fn merge_error_details(
    existing: Option<serde_json::Value>,
    additions: serde_json::Value,
) -> serde_json::Value {
    let mut merged = serde_json::Map::new();

    if let Some(existing) = existing {
        match existing {
            serde_json::Value::Object(map) => merged.extend(map),
            other => {
                merged.insert("original_details".to_string(), other);
            }
        }
    }

    if let serde_json::Value::Object(map) = additions {
        merged.extend(map);
    }

    serde_json::Value::Object(merged)
}

fn artifact_missing_user_message(binding: &BridgeInstanceBinding) -> String {
    match binding.artifact_source {
        ArtifactSource::Installed => {
            "The selected firmware release is not available locally for flashing.".to_string()
        }
        ArtifactSource::Workspace => match binding.target {
            ms_manager_core::FirmwareTarget::Standalone => {
                "The standalone firmware artifact is missing from the workspace.".to_string()
            }
            ms_manager_core::FirmwareTarget::Bitwig => {
                "The Bitwig firmware artifact is missing from the workspace.".to_string()
            }
        },
    }
}

fn artifact_missing_actions(binding: &BridgeInstanceBinding) -> Vec<&'static str> {
    match binding.artifact_source {
        ArtifactSource::Installed => vec![
            "Download the selected installed release before flashing.",
            "Check that the selected release and channel are correct for this controller.",
            "Retry the flash once the artifacts are available locally.",
        ],
        ArtifactSource::Workspace => vec![
            "Build the selected firmware target in the workspace.",
            "Check that the expected .hex artifact exists in the configured workspace artifacts.",
            "Retry the flash once the workspace artifact is present.",
        ],
    }
}

fn emit_flash_message(
    app: &tauri::AppHandle,
    level: FlashMessageLevel,
    message: impl Into<String>,
) {
    let _ = app.emit(
        FLASH_EVENT,
        FlashEvent::Message {
            level,
            message: message.into(),
        },
    );
}

async fn run_bridge_control_action(
    app: &tauri::AppHandle,
    binding: &BridgeInstanceBinding,
    action: BridgeControlAction,
) -> Option<String> {
    if !binding.enabled {
        return None;
    }

    emit_flash_message(app, FlashMessageLevel::Info, action.request_message());

    let timeout = match action {
        BridgeControlAction::Pause => Duration::from_secs(2),
        BridgeControlAction::Resume => Duration::from_millis(600),
    };

    match bridge_ctl::send_command(binding.control_port, action.command(), timeout).await {
        Ok(value) => {
            let runtime = bridge_status::BridgeRuntimeState::from_value(value);
            if runtime.ok {
                emit_flash_message(app, FlashMessageLevel::Info, action.success_message());
                None
            } else {
                let warning = format_bridge_control_warning(action, runtime.message.as_deref());
                emit_flash_message(app, FlashMessageLevel::Warn, warning.clone());
                Some(warning)
            }
        }
        Err(error) => {
            let warning = format_bridge_control_warning(action, Some(&error));
            emit_flash_message(app, FlashMessageLevel::Warn, warning.clone());
            Some(warning)
        }
    }
}

fn format_bridge_control_warning(action: BridgeControlAction, reason: Option<&str>) -> String {
    let summary = reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("no details provided");
    format!(
        "Bridge {} was not confirmed: {}.{}",
        action.label(),
        summary,
        action.continuation_suffix()
    )
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
        assert_eq!(
            summary,
            "multiple targets detected (2); use --device or --all"
        );
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

    #[test]
    fn post_flash_failure_prefers_runtime_reason() {
        let binding = BridgeInstanceBinding {
            instance_id: "bitwig-hardware-17076520".to_string(),
            display_name: None,
            app: ms_manager_core::BridgeApp::Bitwig,
            mode: ms_manager_core::BridgeMode::Hardware,
            controller_serial: "17076520".to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
            target: ms_manager_core::FirmwareTarget::Bitwig,
            artifact_source: ArtifactSource::Installed,
            installed_channel: Some(ms_manager_core::Channel::Stable),
            installed_pinned_tag: None,
            host_udp_port: 9000,
            control_port: 7999,
            log_broadcast_port: 9999,
            enabled: true,
        };

        let runtime = bridge_status::BridgeRuntimeState::from_value(serde_json::json!({
            "ok": true,
            "paused": false,
            "serial_open": false,
            "instance_id": "bitwig-hardware-17076520",
            "controller_serial": "17076520"
        }));

        assert_eq!(
            runtime.post_flash_message_for(&binding),
            Some("bridge has not reopened the serial port yet".to_string())
        );
    }

    #[test]
    fn bridge_pause_warning_includes_continuation() {
        assert_eq!(
            format_bridge_control_warning(
                BridgeControlAction::Pause,
                Some("timeout waiting for serial to close")
            ),
            "Bridge pause was not confirmed: timeout waiting for serial to close. Flash will continue."
        );
    }

    #[test]
    fn actionable_device_not_found_error_includes_guidance() {
        let binding = BridgeInstanceBinding {
            instance_id: "standalone-hardware-17076520".to_string(),
            display_name: None,
            app: ms_manager_core::BridgeApp::Bitwig,
            mode: ms_manager_core::BridgeMode::Hardware,
            controller_serial: "17076520".to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
            target: ms_manager_core::FirmwareTarget::Standalone,
            artifact_source: ArtifactSource::Workspace,
            installed_channel: None,
            installed_pinned_tag: None,
            host_udp_port: 9000,
            control_port: 7999,
            log_broadcast_port: 9999,
            enabled: true,
        };

        let err = make_actionable_flash_error(
            ApiError::new(
                "device_not_found",
                "no connected device matches serial 17076520",
            ),
            &binding,
        );

        assert_eq!(
            err.message,
            "Controller 17076520 was not found for flashing."
        );
        let details = err.details.expect("details");
        let actions = details
            .get("suggested_actions")
            .and_then(|value| value.as_array())
            .expect("suggested actions");
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn actionable_flash_failed_maps_bootloader_timeout() {
        let err = classify_loader_failure(ApiError::new(
            "flash_failed",
            "firmware flash failed: HalfKay did not appear after soft reboot",
        ));

        assert_eq!(
            err.message,
            "The controller did not enter bootloader mode for flashing."
        );
    }
}
