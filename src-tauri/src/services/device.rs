use std::path::Path;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::models::{DeviceStatus, DeviceTarget};
use crate::services::{artifact_resolver, process};

pub async fn device_status(layout: &PayloadLayout) -> DeviceStatus {
    let loader = match artifact_resolver::resolve_management_loader_exe(layout) {
        Ok(path) => path,
        Err(_) => {
            return DeviceStatus {
                connected: false,
                count: 0,
                targets: Vec::new(),
            }
        }
    };

    let targets = match list_targets_with_loader(&loader).await {
        Ok(targets) => targets,
        Err(_) => Vec::new(),
    };

    DeviceStatus {
        connected: !targets.is_empty(),
        count: targets.len() as u32,
        targets,
    }
}

pub async fn list_targets_with_loader(loader: &Path) -> ApiResult<Vec<DeviceTarget>> {
    if !loader.exists() {
        return Ok(Vec::new());
    }

    let mut cmd = tokio::process::Command::new(loader);
    process::no_console_window(&mut cmd);
    let out = cmd
        .args(["list", "--json"])
        .output()
        .await
        .map_err(|e| ApiError::new("io_exec_failed", format!("run loader list: {e}")))?;

    if !out.status.success() {
        return Err(
            ApiError::new("device_list_failed", "loader list failed").with_details(
                serde_json::json!({
                    "exit_code": out.status.code(),
                    "stdout": String::from_utf8_lossy(&out.stdout),
                    "stderr": String::from_utf8_lossy(&out.stderr),
                    "loader": loader.display().to_string(),
                }),
            ),
        );
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| !l.trim().is_empty()).unwrap_or("");

    let (_, targets) = parse_list_event(line);
    Ok(targets)
}

pub fn select_serial_target(targets: &[DeviceTarget], serial: &str) -> ApiResult<DeviceTarget> {
    let serial = serial.trim();
    let mut matches = targets
        .iter()
        .filter(|target| target.serial_number.as_deref().map(str::trim) == Some(serial))
        .cloned()
        .collect::<Vec<_>>();

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(ApiError::new(
            "device_not_found",
            format!("no connected device matches serial {serial}"),
        )
        .with_details(serde_json::json!({
            "controller_serial": serial,
            "available_targets": summarize_targets(targets),
        }))),
        count => Err(ApiError::new(
            "device_ambiguous",
            format!("{count} connected devices match serial {serial}"),
        )
        .with_details(serde_json::json!({
            "controller_serial": serial,
            "match_count": count,
            "matching_targets": summarize_targets(&matches),
            "available_targets": summarize_targets(targets),
        }))),
    }
}

fn summarize_targets(targets: &[DeviceTarget]) -> Vec<serde_json::Value> {
    targets
        .iter()
        .map(|target| {
            serde_json::json!({
                "target_id": target.target_id,
                "kind": target.kind,
                "port_name": target.port_name,
                "path": target.path,
                "serial_number": target.serial_number,
                "product": target.product,
            })
        })
        .collect()
}

fn parse_list_event(json_line: &str) -> (u32, Vec<DeviceTarget>) {
    #[derive(serde::Deserialize)]
    struct ListEvent {
        event: String,
        #[serde(default)]
        count: u32,
        #[serde(default)]
        targets: Vec<DeviceTarget>,
    }

    let Ok(ev) = serde_json::from_str::<ListEvent>(json_line) else {
        return (0, Vec::new());
    };
    if ev.event != "list" {
        return (0, Vec::new());
    }
    let count = if ev.count > 0 {
        ev.count
    } else {
        ev.targets.len() as u32
    };
    (count, ev.targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceTargetKind;

    fn serial_target(serial: &str, port_name: &str) -> DeviceTarget {
        DeviceTarget {
            index: 0,
            target_id: format!("serial:{port_name}"),
            kind: DeviceTargetKind::Serial,
            port_name: Some(port_name.to_string()),
            path: None,
            serial_number: Some(serial.to_string()),
            manufacturer: Some("Microsoft".to_string()),
            product: Some("USB Serial Device".to_string()),
            vid: 0x16C0,
            pid: 0x0489,
        }
    }

    #[test]
    fn select_serial_target_returns_exact_match() {
        let target = select_serial_target(
            &[
                serial_target("17081760", "COM3"),
                serial_target("17076520", "COM6"),
            ],
            "17076520",
        )
        .unwrap();

        assert_eq!(target.target_id, "serial:COM6");
    }

    #[test]
    fn select_serial_target_rejects_missing_serial() {
        let err =
            select_serial_target(&[serial_target("17081760", "COM3")], "17076520").unwrap_err();

        assert_eq!(err.code, "device_not_found");
    }
}
