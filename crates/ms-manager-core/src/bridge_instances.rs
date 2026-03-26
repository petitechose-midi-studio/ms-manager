use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{ArtifactSource, Channel};

pub const BRIDGE_INSTANCES_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BridgeApp {
    Bitwig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BridgeMode {
    Hardware,
    NativeSim,
    WasmSim,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareTarget {
    Standalone,
    Bitwig,
}

impl FirmwareTarget {
    pub fn profile_id(self) -> &'static str {
        match self {
            Self::Standalone => "default",
            Self::Bitwig => "bitwig",
        }
    }

    pub fn from_profile_id(value: &str) -> Option<Self> {
        match value.trim() {
            "default" => Some(Self::Standalone),
            "bitwig" => Some(Self::Bitwig),
            _ => None,
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_target() -> FirmwareTarget {
    FirmwareTarget::Bitwig
}

fn default_artifact_source() -> ArtifactSource {
    ArtifactSource::Installed
}

fn default_installed_channel() -> Option<Channel> {
    Some(Channel::Stable)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeInstanceBinding {
    pub instance_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub app: BridgeApp,
    pub mode: BridgeMode,
    pub controller_serial: String,
    pub controller_vid: u32,
    pub controller_pid: u32,
    #[serde(default = "default_target")]
    pub target: FirmwareTarget,
    #[serde(default = "default_artifact_source")]
    pub artifact_source: ArtifactSource,
    #[serde(default = "default_installed_channel")]
    pub installed_channel: Option<Channel>,
    #[serde(default)]
    pub installed_pinned_tag: Option<String>,
    pub host_udp_port: u16,
    pub control_port: u16,
    pub log_broadcast_port: u16,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeInstancesState {
    pub schema: u32,
    #[serde(default)]
    pub instances: Vec<BridgeInstanceBinding>,
}

impl Default for BridgeInstancesState {
    fn default() -> Self {
        Self {
            schema: BRIDGE_INSTANCES_SCHEMA,
            instances: Vec::new(),
        }
    }
}

impl BridgeInstancesState {
    pub fn validate(&self) -> Result<(), String> {
        let mut instance_ids = HashSet::new();
        let mut host_udp_ports = HashSet::new();
        let mut control_ports = HashSet::new();
        let mut log_broadcast_ports = HashSet::new();

        for instance in &self.instances {
            if instance.instance_id.trim().is_empty() {
                return Err("instance_id cannot be empty".to_string());
            }
            if instance.controller_serial.trim().is_empty() {
                return Err(format!(
                    "controller_serial cannot be empty for {}",
                    instance.instance_id
                ));
            }
            if let Some(display_name) = &instance.display_name {
                if display_name.trim().is_empty() {
                    return Err(format!(
                        "display_name cannot be blank for {}",
                        instance.instance_id
                    ));
                }
            }
            if !instance_ids.insert(instance.instance_id.clone()) {
                return Err(format!("duplicate instance_id: {}", instance.instance_id));
            }
            if !host_udp_ports.insert(instance.host_udp_port) {
                return Err(format!(
                    "duplicate host_udp_port: {}",
                    instance.host_udp_port
                ));
            }
            if !control_ports.insert(instance.control_port) {
                return Err(format!("duplicate control_port: {}", instance.control_port));
            }
            if !log_broadcast_ports.insert(instance.log_broadcast_port) {
                return Err(format!(
                    "duplicate log_broadcast_port: {}",
                    instance.log_broadcast_port
                ));
            }
            match instance.artifact_source {
                ArtifactSource::Installed => {
                    if instance.installed_channel.is_none() {
                        return Err(format!(
                            "installed_channel is required for installed instance {}",
                            instance.instance_id
                        ));
                    }
                }
                ArtifactSource::Workspace => {
                    if instance.installed_channel.is_some() {
                        return Err(format!(
                            "installed_channel must be empty for workspace instance {}",
                            instance.instance_id
                        ));
                    }
                    if instance.installed_pinned_tag.is_some() {
                        return Err(format!(
                            "installed_pinned_tag must be empty for workspace instance {}",
                            instance.instance_id
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(instance_id: &str, serial: &str, offset: u16) -> BridgeInstanceBinding {
        BridgeInstanceBinding {
            instance_id: instance_id.to_string(),
            display_name: None,
            app: BridgeApp::Bitwig,
            mode: BridgeMode::Hardware,
            controller_serial: serial.to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
            target: FirmwareTarget::Bitwig,
            artifact_source: ArtifactSource::Installed,
            installed_channel: Some(Channel::Stable),
            installed_pinned_tag: None,
            host_udp_port: 9000 + offset,
            control_port: 7999 + offset,
            log_broadcast_port: 9999 + offset,
            enabled: true,
        }
    }

    #[test]
    fn bridge_instances_state_roundtrip() {
        let state = BridgeInstancesState {
            schema: BRIDGE_INSTANCES_SCHEMA,
            instances: vec![binding("bitwig-hardware-17081760", "17081760", 0)],
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: BridgeInstancesState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, state);
    }

    #[test]
    fn validate_rejects_duplicate_instance_id() {
        let state = BridgeInstancesState {
            schema: BRIDGE_INSTANCES_SCHEMA,
            instances: vec![
                binding("bitwig-hardware-17081760", "17081760", 0),
                binding("bitwig-hardware-17081760", "17076520", 1),
            ],
        };

        let err = state.validate().unwrap_err();
        assert!(err.contains("duplicate instance_id"));
    }

    #[test]
    fn validate_rejects_duplicate_port_assignments() {
        let mut second = binding("bitwig-hardware-17076520", "17076520", 1);
        second.control_port = 7999;
        let state = BridgeInstancesState {
            schema: BRIDGE_INSTANCES_SCHEMA,
            instances: vec![binding("bitwig-hardware-17081760", "17081760", 0), second],
        };

        let err = state.validate().unwrap_err();
        assert!(err.contains("duplicate control_port"));
    }

    #[test]
    fn validate_rejects_workspace_release_fields() {
        let mut instance = binding("bitwig-hardware-17081760", "17081760", 0);
        instance.artifact_source = ArtifactSource::Workspace;
        let state = BridgeInstancesState {
            schema: BRIDGE_INSTANCES_SCHEMA,
            instances: vec![instance],
        };

        let err = state.validate().unwrap_err();
        assert!(err.contains("installed_channel must be empty"));
    }
}
