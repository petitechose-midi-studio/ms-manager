use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeInstanceBinding {
    pub instance_id: String,
    pub app: BridgeApp,
    pub mode: BridgeMode,
    pub controller_serial: String,
    pub controller_vid: u32,
    pub controller_pid: u32,
    pub host_udp_port: u16,
    pub control_port: u16,
    pub log_broadcast_port: u16,
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
            app: BridgeApp::Bitwig,
            mode: BridgeMode::Hardware,
            controller_serial: serial.to_string(),
            controller_vid: 0x16C0,
            controller_pid: 0x0489,
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
}
