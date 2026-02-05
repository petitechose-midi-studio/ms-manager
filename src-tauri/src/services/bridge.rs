use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use tauri::Manager;

use crate::layout::PayloadLayout;
use crate::services::{bridge_ctl, bridge_process, payload, process};
use crate::state::AppState;

/// Ensure oc-bridge is running for the current user session.
///
/// This is intentionally best-effort: if the host bundle is not installed, or if
/// the bridge cannot be started, the app should remain usable (firmware flashing,
/// installs, etc.).
pub fn spawn_bridge_supervisor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        supervisor_loop(app).await;
    });
}

async fn supervisor_loop(app: tauri::AppHandle) {
    // Give the app a moment to finish bootstrapping.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut cleaned_bridge_autostart = false;

    loop {
        // Don't hold any app state reference across await.
        let layout = app.state::<AppState>().layout_get();
        let exe = payload::oc_bridge_path(&layout);

        // If the host bundle is not installed, there is no bridge to manage.
        if !exe.exists() {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        }

        // Cleanup legacy oc-bridge autostarts to avoid collisions.
        if !cleaned_bridge_autostart {
            let _ = cleanup_legacy_bridge_autostart(&layout).await;
            cleaned_bridge_autostart = true;
        }

        // Health check: if the daemon doesn't respond, try to start it.
        if !bridge_ctl::ping(Duration::from_millis(150)).await {
            let _ = bridge_spawn_daemon(&layout).await;
            let ready = bridge_wait_ready(Duration::from_secs(4)).await;

            // If the daemon still doesn't come up, reconcile:
            // - oc-bridge might be stuck or running with an incompatible state
            // - kill only the daemon process for the current payload exe
            if !ready {
                let killed = bridge_process::kill_oc_bridge_daemons(&exe);
                if killed == 0 {
                    // If a legacy daemon (different payload path) is running, the instance lock
                    // can prevent startup. As a strict cleanup step, kill any oc-bridge daemon.
                    let _ = bridge_process::kill_all_oc_bridge_daemons();
                }

                tokio::time::sleep(Duration::from_millis(400)).await;
                let _ = bridge_spawn_daemon(&layout).await;
                let _ = bridge_wait_ready(Duration::from_secs(4)).await;
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn cleanup_legacy_bridge_autostart(layout: &PayloadLayout) -> Result<(), ()> {
    // Best effort: remove per-user autostart installed by oc-bridge (legacy).
    let _ = cleanup_oc_bridge_autostart_artifacts(layout).await;

    // Best effort: disable the legacy Task Scheduler entry (older builds).
    // This may require admin (Access denied). Ignore failures.
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

    // Linux: oc-bridge used systemd user unit "open-control-bridge".
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

    // macOS: oc-bridge used LaunchAgent "com.opencontrol.oc-bridge".
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
        if l.is_empty() {
            continue;
        }
        // Skip the key header line.
        if l.starts_with("HKEY_") {
            continue;
        }

        // Parse: <name> <type> <data...>
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

async fn bridge_spawn_daemon(layout: &PayloadLayout) -> Result<(), ()> {
    let exe = payload::oc_bridge_path(layout);
    if !exe.exists() {
        return Err(());
    }

    let mut cmd = Command::new(exe);
    process::no_console_window(&mut cmd);
    cmd.arg("--daemon")
        .arg("--daemon-control-port")
        .arg(bridge_ctl::DEFAULT_CONTROL_PORT.to_string())
        .arg("--daemon-log-broadcast-port")
        .arg("9999")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn().map(|_| ()).map_err(|_| ())
}

async fn bridge_wait_ready(timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if bridge_ctl::ping(Duration::from_millis(150)).await {
            return true;
        }

        if tokio::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(Duration::from_millis(140)).await;
    }
}
