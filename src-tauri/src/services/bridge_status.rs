use ms_manager_core::{BridgeInstanceBinding, BridgeInstancesState, ControllerState, InstallState};

use crate::layout::PayloadLayout;
use crate::models::{BridgeInstanceStatus, BridgeStatus};
use crate::services::{artifact_resolver, bridge_ctl};

pub async fn bridge_status(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    bindings: &BridgeInstancesState,
    controller_state: &ControllerState,
) -> BridgeStatus {
    if bindings.instances.is_empty() {
        return BridgeStatus {
            installed: false,
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
        instances.push(bridge_instance_status(layout, installed, binding, controller_state).await);
    }

    let running = instances.iter().any(|instance| instance.running);
    let paused = instances
        .iter()
        .any(|instance| instance.running && instance.paused);
    let serial_open = instances
        .iter()
        .any(|instance| instance.running && instance.serial_open);
    let version = instances
        .iter()
        .find_map(|instance| instance.version.clone());
    let message = instances.iter().find_map(preferred_instance_message);

    BridgeStatus {
        installed: instances.iter().any(|instance| instance.artifacts_ready),
        running,
        paused,
        serial_open,
        version,
        message,
        instances,
    }
}

async fn bridge_instance_status(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
    controller_state: &ControllerState,
) -> BridgeInstanceStatus {
    let artifact_health =
        artifact_resolver::artifact_health_for_binding(layout, installed, binding);
    let mut status = base_instance_status(
        layout,
        installed,
        binding,
        controller_state,
        artifact_health,
    );

    match bridge_ctl::send_command(
        binding.control_port,
        "status",
        std::time::Duration::from_millis(180),
    )
    .await
    {
        Ok(value) => apply_runtime_status(&mut status, binding, value),
        Err(error) => {
            status.message = Some(error);
        }
    }

    status
}

fn base_instance_status(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
    controller_state: &ControllerState,
    artifact_health: artifact_resolver::ArtifactHealth,
) -> BridgeInstanceStatus {
    BridgeInstanceStatus {
        instance_id: binding.instance_id.clone(),
        display_name: binding.display_name.clone(),
        configured_serial: binding.controller_serial.clone(),
        target: binding.target,
        artifact_source: binding.artifact_source,
        installed_channel: binding.installed_channel,
        installed_pinned_tag: binding.installed_pinned_tag.clone(),
        artifacts_ready: artifact_health.ready,
        artifact_message: artifact_health.message,
        enabled: binding.enabled,
        running: false,
        paused: false,
        serial_open: false,
        version: None,
        resolved_serial_port: None,
        connected_serial: None,
        message: None,
        last_flashed: controller_state.last_flashed_for_instance(&binding.instance_id),
        artifact_location_path: Some(artifact_resolver::ui_path_string(
            &artifact_resolver::artifact_location_for_binding(layout, installed, binding),
        )),
        host_udp_port: binding.host_udp_port,
        control_port: binding.control_port,
        log_broadcast_port: binding.log_broadcast_port,
    }
}

fn apply_runtime_status(
    status: &mut BridgeInstanceStatus,
    binding: &BridgeInstanceBinding,
    value: serde_json::Value,
) {
    let ok = value
        .get("ok")
        .and_then(|field| field.as_bool())
        .unwrap_or(false);
    status.paused = value
        .get("paused")
        .and_then(|field| field.as_bool())
        .unwrap_or(false);
    status.serial_open = value
        .get("serial_open")
        .and_then(|field| field.as_bool())
        .unwrap_or(false);
    status.version = value
        .get("version")
        .and_then(|field| field.as_str())
        .map(ToOwned::to_owned);
    status.resolved_serial_port = value
        .get("resolved_serial_port")
        .and_then(|field| field.as_str())
        .map(ToOwned::to_owned);
    status.connected_serial = value
        .get("controller_serial")
        .and_then(|field| field.as_str())
        .map(ToOwned::to_owned);
    let reported_instance_id = value.get("instance_id").and_then(|field| field.as_str());

    status.message = value
        .get("message")
        .and_then(|field| field.as_str())
        .map(ToOwned::to_owned);

    status.running = if !ok {
        false
    } else if reported_instance_id != Some(binding.instance_id.as_str()) {
        status.message = Some("bridge responded with wrong instance_id".to_string());
        false
    } else if status.connected_serial.as_deref() != Some(binding.controller_serial.as_str()) {
        status.message = Some("bridge responded with wrong controller serial".to_string());
        false
    } else {
        true
    };
}

fn preferred_instance_message(instance: &BridgeInstanceStatus) -> Option<String> {
    if !instance.artifacts_ready {
        return instance.artifact_message.clone();
    }
    if !instance.running {
        return instance.message.clone();
    }
    None
}
