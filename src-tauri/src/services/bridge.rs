use std::process::Stdio;
use std::time::Duration;

use ms_manager_core::{BridgeApp, BridgeInstanceBinding, BridgeInstancesState, BridgeMode};
use tokio::process::Command;

use tauri::Manager;

use crate::layout::PayloadLayout;
use crate::models::DeviceTargetKind;
use crate::services::{
    artifact_resolver, bridge_ctl, bridge_instances, device, payload, process,
};
use crate::state::AppState;

/// Ensure oc-bridge instances are running for the current user session.
pub fn spawn_bridge_supervisor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        supervisor_loop(app).await;
    });
}

async fn supervisor_loop(app: tauri::AppHandle) {
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut cleaned_bridge_autostart = false;

    loop {
        let layout = app.state::<AppState>().layout_get();
        let settings = app.state::<AppState>().settings_get();
        let exe = artifact_resolver::resolve_oc_bridge_exe(&settings, &layout)
            .unwrap_or_else(|_| payload::oc_bridge_path(&layout));

        if !exe.exists() {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        }

        if !cleaned_bridge_autostart {
            let _ = cleanup_legacy_bridge_autostart(&layout).await;
            cleaned_bridge_autostart = true;
        }

        let bindings = ensure_bridge_instances(&app, &layout, &settings).await;
        for binding in bindings.instances.iter().filter(|binding| binding.enabled) {
            reconcile_bridge_instance(&settings, &layout, binding).await;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn ensure_bridge_instances(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    settings: &ms_manager_core::Settings,
) -> BridgeInstancesState {
    let bindings = app.state::<AppState>().bridge_instances_get();
    if !bindings.instances.is_empty() {
        return bindings;
    }

    let device_status = device::device_status(settings, layout).await;
    let serial_targets = device_status
        .targets
        .iter()
        .filter(|target| matches!(target.kind, DeviceTargetKind::Serial))
        .filter_map(|target| {
            let serial = target.serial_number.as_deref()?.trim();
            if serial.is_empty() {
                return None;
            }
            Some((serial.to_string(), target.vid, target.pid))
        })
        .collect::<Vec<_>>();

    if serial_targets.len() != 1 {
        return bindings;
    }

    let (controller_serial, controller_vid, controller_pid) = &serial_targets[0];
    let Ok(binding) = bridge_instances::build_binding(
        &bindings,
        BridgeApp::Bitwig,
        BridgeMode::Hardware,
        controller_serial,
        *controller_vid,
        *controller_pid,
    ) else {
        return bindings;
    };

    app.state::<AppState>()
        .bridge_instance_upsert(binding)
        .unwrap_or_else(|_| bindings)
}

async fn reconcile_bridge_instance(
    settings: &ms_manager_core::Settings,
    layout: &PayloadLayout,
    binding: &BridgeInstanceBinding,
) {
    if bridge_instance_ready(binding, Duration::from_millis(180)).await {
        return;
    }

    let _ = bridge_spawn_daemon(settings, layout, binding).await;
    let _ = bridge_wait_ready(binding, Duration::from_secs(4)).await;
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

async fn bridge_spawn_daemon(
    settings: &ms_manager_core::Settings,
    layout: &PayloadLayout,
    binding: &BridgeInstanceBinding,
) -> Result<(), ()> {
    let exe = artifact_resolver::resolve_oc_bridge_exe(settings, layout).map_err(|_| ())?;

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
        if bridge_instance_ready(binding, Duration::from_millis(150)).await {
            return true;
        }

        if tokio::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(Duration::from_millis(140)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> BridgeInstanceBinding {
        BridgeInstanceBinding {
            instance_id: "bitwig-hardware-17081760".to_string(),
            app: BridgeApp::Bitwig,
            mode: BridgeMode::Hardware,
            controller_serial: "17081760".to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
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
}
