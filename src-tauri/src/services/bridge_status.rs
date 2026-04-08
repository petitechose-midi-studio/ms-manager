use ms_manager_core::{BridgeInstanceBinding, BridgeInstancesState, ControllerState, InstallState};

use crate::layout::PayloadLayout;
use crate::models::{BridgeInstanceStatus, BridgeStatus};
use crate::services::{artifact_resolver, bridge_ctl};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeRuntimeState {
    pub ok: bool,
    pub paused: bool,
    pub serial_open: bool,
    pub version: Option<String>,
    pub resolved_serial_port: Option<String>,
    pub reported_instance_id: Option<String>,
    pub connected_serial: Option<String>,
    pub message: Option<String>,
}

impl BridgeRuntimeState {
    pub fn from_value(value: serde_json::Value) -> Self {
        Self {
            ok: value
                .get("ok")
                .and_then(|field| field.as_bool())
                .unwrap_or(false),
            paused: value
                .get("paused")
                .and_then(|field| field.as_bool())
                .unwrap_or(false),
            serial_open: value
                .get("serial_open")
                .and_then(|field| field.as_bool())
                .unwrap_or(false),
            version: value
                .get("version")
                .and_then(|field| field.as_str())
                .map(ToOwned::to_owned),
            resolved_serial_port: value
                .get("resolved_serial_port")
                .and_then(|field| field.as_str())
                .map(ToOwned::to_owned),
            reported_instance_id: value
                .get("instance_id")
                .and_then(|field| field.as_str())
                .map(ToOwned::to_owned),
            connected_serial: value
                .get("controller_serial")
                .and_then(|field| field.as_str())
                .map(ToOwned::to_owned),
            message: value
                .get("message")
                .and_then(|field| field.as_str())
                .map(ToOwned::to_owned),
        }
    }

    pub fn running_message_for(&self, binding: &BridgeInstanceBinding) -> Option<String> {
        if !self.ok {
            return self.message.clone();
        }
        if self.reported_instance_id.as_deref() != Some(binding.instance_id.as_str()) {
            return Some("bridge responded with wrong instance_id".to_string());
        }
        if self.connected_serial.as_deref() != Some(binding.controller_serial.as_str()) {
            return Some("bridge responded with wrong controller serial".to_string());
        }
        self.message.clone()
    }

    pub fn is_running_for(&self, binding: &BridgeInstanceBinding) -> bool {
        self.ok
            && self.reported_instance_id.as_deref() == Some(binding.instance_id.as_str())
            && self.connected_serial.as_deref() == Some(binding.controller_serial.as_str())
    }

    pub fn post_flash_message_for(&self, binding: &BridgeInstanceBinding) -> Option<String> {
        if !self.is_running_for(binding) {
            return self.running_message_for(binding);
        }
        if self.paused {
            return Some("bridge is still paused after flash".to_string());
        }
        if !self.serial_open {
            return Some("bridge has not reopened the serial port yet".to_string());
        }
        None
    }

    pub fn is_ready_after_flash_for(&self, binding: &BridgeInstanceBinding) -> bool {
        self.is_running_for(binding) && !self.paused && self.serial_open
    }
}

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
    let runtime = BridgeRuntimeState::from_value(value);
    status.paused = runtime.paused;
    status.serial_open = runtime.serial_open;
    status.version = runtime.version.clone();
    status.resolved_serial_port = runtime.resolved_serial_port.clone();
    status.connected_serial = runtime.connected_serial.clone();
    status.message = runtime.running_message_for(binding);
    status.running = runtime.is_running_for(binding);
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

#[cfg(test)]
mod tests {
    use super::*;
    use ms_manager_core::{ArtifactSource, BridgeApp, BridgeMode, Channel, FirmwareTarget};

    fn binding() -> BridgeInstanceBinding {
        BridgeInstanceBinding {
            instance_id: "bitwig-hardware-17076520".to_string(),
            display_name: None,
            app: BridgeApp::Bitwig,
            mode: BridgeMode::Hardware,
            controller_serial: "17076520".to_string(),
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
    fn runtime_state_detects_ready_after_flash() {
        let runtime = BridgeRuntimeState::from_value(serde_json::json!({
            "ok": true,
            "paused": false,
            "serial_open": true,
            "instance_id": "bitwig-hardware-17076520",
            "controller_serial": "17076520",
            "resolved_serial_port": "COM6"
        }));

        assert!(runtime.is_running_for(&binding()));
        assert!(runtime.is_ready_after_flash_for(&binding()));
        assert_eq!(runtime.post_flash_message_for(&binding()), None);
    }

    #[test]
    fn runtime_state_reports_specific_post_flash_failure() {
        let runtime = BridgeRuntimeState::from_value(serde_json::json!({
            "ok": true,
            "paused": true,
            "serial_open": false,
            "instance_id": "bitwig-hardware-17076520",
            "controller_serial": "17076520"
        }));

        assert!(runtime.is_running_for(&binding()));
        assert!(!runtime.is_ready_after_flash_for(&binding()));
        assert_eq!(
            runtime.post_flash_message_for(&binding()),
            Some("bridge is still paused after flash".to_string())
        );
    }
}
