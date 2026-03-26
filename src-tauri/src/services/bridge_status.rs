use ms_manager_core::{BridgeInstancesState, Settings};

use crate::layout::PayloadLayout;
use crate::models::{BridgeInstanceStatus, BridgeStatus};
use crate::services::{artifact_resolver, bridge_ctl};

pub async fn bridge_status(
    settings: &Settings,
    layout: &PayloadLayout,
    bindings: &BridgeInstancesState,
) -> BridgeStatus {
    let exe = match artifact_resolver::resolve_oc_bridge_exe(settings, layout) {
        Ok(path) => path,
        Err(err) => {
            return BridgeStatus {
                installed: false,
                running: false,
                paused: false,
                serial_open: false,
                version: None,
                message: Some(err.message),
                instances: Vec::new(),
            };
        }
    };

    if !exe.exists() {
        return BridgeStatus {
            installed: false,
            running: false,
            paused: false,
            serial_open: false,
            version: None,
            message: Some("oc-bridge missing".to_string()),
            instances: Vec::new(),
        };
    }

    if bindings.instances.is_empty() {
        return BridgeStatus {
            installed: true,
            running: false,
            paused: false,
            serial_open: false,
            version: None,
            message: Some("no bridge instances configured".to_string()),
            instances: Vec::new(),
        };
    }

    let mut instances = Vec::with_capacity(bindings.instances.len());

    for binding in &bindings.instances {
        let result = bridge_ctl::send_command(
            binding.control_port,
            "status",
            std::time::Duration::from_millis(180),
        )
        .await;

        match result {
            Ok(v) => {
                let ok = v.get("ok").and_then(|value| value.as_bool()).unwrap_or(false);
                let paused = v.get("paused").and_then(|value| value.as_bool()).unwrap_or(false);
                let serial_open = v
                    .get("serial_open")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let version = v
                    .get("version")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let resolved_serial_port = v
                    .get("resolved_serial_port")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let connected_serial = v
                    .get("controller_serial")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let reported_instance_id = v
                    .get("instance_id")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());

                let mut message = v
                    .get("message")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let running = if !ok {
                    false
                } else if reported_instance_id.as_deref() != Some(binding.instance_id.as_str()) {
                    message = Some("bridge responded with wrong instance_id".to_string());
                    false
                } else if connected_serial.as_deref() != Some(binding.controller_serial.as_str()) {
                    message = Some("bridge responded with wrong controller serial".to_string());
                    false
                } else {
                    true
                };

                instances.push(BridgeInstanceStatus {
                    instance_id: binding.instance_id.clone(),
                    configured_serial: binding.controller_serial.clone(),
                    enabled: binding.enabled,
                    running,
                    paused,
                    serial_open,
                    version,
                    resolved_serial_port,
                    connected_serial,
                    message,
                    host_udp_port: binding.host_udp_port,
                    control_port: binding.control_port,
                    log_broadcast_port: binding.log_broadcast_port,
                });
            }
            Err(err) => {
                instances.push(BridgeInstanceStatus {
                    instance_id: binding.instance_id.clone(),
                    configured_serial: binding.controller_serial.clone(),
                    enabled: binding.enabled,
                    running: false,
                    paused: false,
                    serial_open: false,
                    version: None,
                    resolved_serial_port: None,
                    connected_serial: None,
                    message: Some(err),
                    host_udp_port: binding.host_udp_port,
                    control_port: binding.control_port,
                    log_broadcast_port: binding.log_broadcast_port,
                });
            }
        }
    }

    let running = instances.iter().any(|instance| instance.running);
    let paused = instances.iter().any(|instance| instance.running && instance.paused);
    let serial_open = instances
        .iter()
        .any(|instance| instance.running && instance.serial_open);
    let version = instances
        .iter()
        .find_map(|instance| instance.version.clone());
    let message = instances
        .iter()
        .find(|instance| !instance.running)
        .and_then(|instance| instance.message.clone());

    BridgeStatus {
        installed: true,
        running,
        paused,
        serial_open,
        version,
        message,
        instances,
    }
}
