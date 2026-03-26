use std::process::Stdio;
use std::time::Duration;

use ms_manager_core::{
    ArtifactSource, BridgeApp, BridgeInstanceBinding, BridgeInstancesState, BridgeMode,
    FirmwareTarget,
};
use tokio::process::Command;

use tauri::Manager;

use crate::layout::PayloadLayout;
use crate::models::DeviceTargetKind;
use crate::services::{artifact_resolver, bridge_ctl, bridge_instances, device, process};
use crate::state::AppState;

const SUPERVISOR_START_DELAY: Duration = Duration::from_millis(300);
const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_secs(2);
const STATUS_TIMEOUT: Duration = Duration::from_millis(180);
const WAIT_STATUS_TIMEOUT: Duration = Duration::from_millis(150);
const WAIT_READY_TIMEOUT: Duration = Duration::from_secs(4);
const WAIT_READY_POLL_INTERVAL: Duration = Duration::from_millis(140);

/// Ensure oc-bridge instances are running for the current user session.
pub fn spawn_bridge_supervisor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        supervisor_loop(app).await;
    });
}

async fn supervisor_loop(app: tauri::AppHandle) {
    tokio::time::sleep(SUPERVISOR_START_DELAY).await;

    let mut cleaned_bridge_autostart = false;

    loop {
        let layout = app.state::<AppState>().layout_get();

        if !cleaned_bridge_autostart {
            let _ = cleanup_legacy_bridge_autostart(&layout).await;
            cleaned_bridge_autostart = true;
        }

        let bindings = bridge_instances_for_cycle(&app, &layout).await;
        ensure_enabled_instances_running(&layout, &bindings).await;

        tokio::time::sleep(SUPERVISOR_POLL_INTERVAL).await;
    }
}

async fn bridge_instances_for_cycle(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
) -> BridgeInstancesState {
    let bindings = app.state::<AppState>().bridge_instances_get();
    if !bindings.instances.is_empty() {
        return bindings;
    }

    auto_bind_single_serial_target(app, layout, &bindings).await
}

async fn auto_bind_single_serial_target(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    bindings: &BridgeInstancesState,
) -> BridgeInstancesState {
    let device_status = device::device_status(layout).await;
    let Some((controller_serial, controller_vid, controller_pid)) =
        single_serial_target(&device_status.targets)
    else {
        return bindings.clone();
    };

    let Ok(binding) = bridge_instances::build_binding(
        bindings,
        BridgeApp::Bitwig,
        BridgeMode::Hardware,
        &controller_serial,
        controller_vid,
        controller_pid,
        FirmwareTarget::Bitwig,
        ArtifactSource::Installed,
        Some(ms_manager_core::Channel::Stable),
        None,
    ) else {
        return bindings.clone();
    };

    app.state::<AppState>()
        .bridge_instance_upsert(binding)
        .unwrap_or_else(|_| bindings.clone())
}

fn single_serial_target(
    targets: &[crate::models::DeviceTarget],
) -> Option<(String, u32, u32)> {
    let mut serial_targets = targets
        .iter()
        .filter(|target| matches!(target.kind, DeviceTargetKind::Serial))
        .filter_map(|target| {
            let serial = target.serial_number.as_deref()?.trim();
            if serial.is_empty() {
                return None;
            }
            Some((serial.to_string(), target.vid, target.pid))
        });

    let target = serial_targets.next()?;
    if serial_targets.next().is_some() {
        return None;
    }

    Some(target)
}

async fn ensure_enabled_instances_running(layout: &PayloadLayout, bindings: &BridgeInstancesState) {
    for binding in bindings.instances.iter().filter(|binding| binding.enabled) {
        ensure_bridge_instance_running(layout, binding).await;
    }
}

async fn ensure_bridge_instance_running(layout: &PayloadLayout, binding: &BridgeInstanceBinding) {
    if bridge_instance_ready(binding, STATUS_TIMEOUT).await {
        return;
    }

    let _ = bridge_spawn_daemon(layout, binding).await;
    let _ = bridge_wait_ready(binding, WAIT_READY_TIMEOUT).await;
}

async fn bridge_instance_ready(binding: &BridgeInstanceBinding, timeout: Duration) -> bool {
    let Ok(v) = bridge_ctl::send_command(binding.control_port, "status", timeout).await else {
        return false;
    };

    let ok = v.get("ok").and_then(|value| value.as_bool()).unwrap_or(false);
    let instance_id = v
        .get("instance_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let controller_serial = v
        .get("controller_serial")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    ok && instance_id == binding.instance_id && controller_serial == binding.controller_serial
}

async fn cleanup_legacy_bridge_autostart(layout: &PayloadLayout) -> Result<(), ()> {
    let _ = cleanup_oc_bridge_autostart_artifacts(layout).await;

    #[cfg(windows)]
    {
        let mut cmd = Command::new("schtasks");
        process::no_console_window(&mut cmd);
        let _ = cmd
            .args(["/Delete", "/TN", "\\OpenControlBridge", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        let mut cmd = Command::new("schtasks");
        process::no_console_window(&mut cmd);
        let _ = cmd
            .args(["/Change", "/TN", "\\OpenControlBridge", "/Disable"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }

    Ok(())
}

async fn cleanup_oc_bridge_autostart_artifacts(_layout: &PayloadLayout) -> Result<(), ()> {
    #[cfg(windows)]
    {
        cleanup_windows_run_key_values().await;
        cleanup_windows_wrapper_files().await;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").unwrap_or_default();
        let unit = std::path::PathBuf::from(home)
            .join(".config")
            .join("systemd")
            .join("user")
            .join("open-control-bridge.service");

        let mut cmd = Command::new("systemctl");
        let _ = cmd
            .args(["--user", "stop", "open-control-bridge"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        let mut cmd = Command::new("systemctl");
        let _ = cmd
            .args(["--user", "disable", "open-control-bridge"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        let _ = tokio::fs::remove_file(&unit).await;
        let mut cmd = Command::new("systemctl");
        let _ = cmd
            .args(["--user", "daemon-reload"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").unwrap_or_default();
        let plist = std::path::PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join("com.opencontrol.oc-bridge.plist");

        let mut cmd = Command::new("launchctl");
        let _ = cmd
            .args(["unload"])
            .arg(&plist)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        let _ = tokio::fs::remove_file(&plist).await;
        return Ok(());
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = _layout;
        Ok(())
    }
}

#[cfg(windows)]
async fn cleanup_windows_run_key_values() {
    const RUN_KEY: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    let mut cmd = Command::new("reg");
    process::no_console_window(&mut cmd);
    let out = cmd.args(["query", RUN_KEY]).output().await;
    let Ok(out) = out else {
        return;
    };

    if !out.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let l = line.trim_start();
        if l.is_empty() || l.starts_with("HKEY_") {
            continue;
        }

        let mut it = l.split_whitespace();
        let Some(name) = it.next() else {
            continue;
        };
        let data = it.collect::<Vec<_>>().join(" ").to_ascii_lowercase();

        let matches_name = name.starts_with("OpenControlBridge");
        let matches_data = data.contains("oc-bridge") || data.contains("start-bridge.bat");
        if !matches_name && !matches_data {
            continue;
        }

        let mut del = Command::new("reg");
        process::no_console_window(&mut del);
        let _ = del
            .args(["delete", RUN_KEY, "/v", name, "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

#[cfg(windows)]
async fn cleanup_windows_wrapper_files() {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };

    let root = std::path::PathBuf::from(local)
        .join("open-control")
        .join("bridge");

    let bat = root.join("start-bridge.bat");
    let _ = tokio::fs::remove_file(bat).await;
}

async fn bridge_spawn_daemon(layout: &PayloadLayout, binding: &BridgeInstanceBinding) -> Result<(), ()> {
    let exe = artifact_resolver::resolve_oc_bridge_exe_for_binding(layout, binding).map_err(|_| ())?;

    let mut cmd = Command::new(exe);
    process::no_console_window(&mut cmd);
    cmd.args(daemon_args(binding))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn().map(|_| ()).map_err(|_| ())
}

fn daemon_args(binding: &BridgeInstanceBinding) -> Vec<String> {
    vec![
        "--daemon".to_string(),
        "--instance-id".to_string(),
        binding.instance_id.clone(),
        "--serial-number".to_string(),
        binding.controller_serial.clone(),
        "--udp-port".to_string(),
        binding.host_udp_port.to_string(),
        "--daemon-control-port".to_string(),
        binding.control_port.to_string(),
        "--daemon-log-broadcast-port".to_string(),
        binding.log_broadcast_port.to_string(),
    ]
}

async fn bridge_wait_ready(binding: &BridgeInstanceBinding, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if bridge_instance_ready(binding, WAIT_STATUS_TIMEOUT).await {
            return true;
        }

        if tokio::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(WAIT_READY_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceTarget;
    use ms_manager_core::Channel;

    fn binding() -> BridgeInstanceBinding {
        BridgeInstanceBinding {
            instance_id: "bitwig-hardware-17081760".to_string(),
            display_name: None,
            app: BridgeApp::Bitwig,
            mode: BridgeMode::Hardware,
            controller_serial: "17081760".to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
            target: FirmwareTarget::Bitwig,
            artifact_source: ArtifactSource::Installed,
            installed_channel: Some(Channel::Stable),
            installed_pinned_tag: None,
            host_udp_port: 9000,
            control_port: 7999,
            log_broadcast_port: 9999,
            enabled: true,
        }
    }

    #[test]
    fn daemon_args_include_identity_and_ports() {
        let args = daemon_args(&binding());
        assert_eq!(
            args,
            vec![
                "--daemon",
                "--instance-id",
                "bitwig-hardware-17081760",
                "--serial-number",
                "17081760",
                "--udp-port",
                "9000",
                "--daemon-control-port",
                "7999",
                "--daemon-log-broadcast-port",
                "9999",
            ]
        );
    }

    #[test]
    fn single_serial_target_requires_exactly_one_serial_device() {
        let target = DeviceTarget {
            index: 0,
            target_id: "serial:COM6".to_string(),
            kind: DeviceTargetKind::Serial,
            port_name: Some("COM6".to_string()),
            path: Some("COM6".to_string()),
            serial_number: Some("17081760".to_string()),
            manufacturer: Some("MIDI Studio".to_string()),
            product: Some("MIDI Studio".to_string()),
            vid: 0x16C0,
            pid: 0x0489,
        };

        assert_eq!(
            single_serial_target(&[target.clone()]),
            Some(("17081760".to_string(), 0x16C0, 0x0489))
        );
        assert_eq!(single_serial_target(&[target.clone(), target]), None);
    }
}
