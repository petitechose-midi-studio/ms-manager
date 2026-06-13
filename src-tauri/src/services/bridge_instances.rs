use ms_manager_core::{
    ArtifactSource, BridgeApp, BridgeInstanceBinding, BridgeInstancesState, BridgeMode, Channel,
    FirmwareTarget,
};

pub const HARDWARE_HOST_UDP_PORT_START: u16 = 9000;
pub const NATIVE_HOST_UDP_PORT_START: u16 = 9100;
pub const WASM_HOST_UDP_PORT_START: u16 = 9200;
pub const HOST_UDP_PORT_RANGE: u16 = 8;
pub const CONTROL_PORT_START: u16 = 7999;
pub const LOG_BROADCAST_PORT_START: u16 = 9999;

pub fn derive_instance_id(app: &BridgeApp, mode: &BridgeMode, controller_serial: &str) -> String {
    format!(
        "{}-{}-{}",
        app_slug(app),
        mode_slug(mode),
        controller_serial.trim()
    )
}

pub fn allocate_ports(
    state: &BridgeInstancesState,
    mode: &BridgeMode,
) -> Result<(u16, u16, u16), String> {
    let host_udp_port = allocate_host_udp_port(state, mode)?;
    let (control_port, log_broadcast_port) = allocate_control_ports(state)?;

    Ok((host_udp_port, control_port, log_broadcast_port))
}

fn allocate_host_udp_port(state: &BridgeInstancesState, mode: &BridgeMode) -> Result<u16, String> {
    let base = match mode {
        BridgeMode::Hardware => HARDWARE_HOST_UDP_PORT_START,
        BridgeMode::NativeSim => NATIVE_HOST_UDP_PORT_START,
        BridgeMode::WasmSim => WASM_HOST_UDP_PORT_START,
    };

    for offset in 0..HOST_UDP_PORT_RANGE {
        let host_udp_port = base + offset;
        let conflict = state
            .instances
            .iter()
            .any(|instance| instance.host_udp_port == host_udp_port);
        if !conflict {
            return Ok(host_udp_port);
        }
    }

    Err(format!("no free host UDP port available for {:?}", mode))
}

fn allocate_control_ports(state: &BridgeInstancesState) -> Result<(u16, u16), String> {
    for offset in 0..=255u16 {
        let control_port = CONTROL_PORT_START + offset;
        let log_broadcast_port = LOG_BROADCAST_PORT_START + offset;

        let conflict = state.instances.iter().any(|instance| {
            instance.control_port == control_port
                || instance.log_broadcast_port == log_broadcast_port
        });
        if !conflict {
            return Ok((control_port, log_broadcast_port));
        }
    }

    Err("no free bridge control/log port pair available".to_string())
}

pub fn build_binding(
    state: &BridgeInstancesState,
    app: BridgeApp,
    mode: BridgeMode,
    controller_serial: &str,
    controller_vid: u32,
    controller_pid: u32,
    target: FirmwareTarget,
    artifact_source: ArtifactSource,
    installed_channel: Option<Channel>,
    installed_pinned_tag: Option<String>,
) -> Result<BridgeInstanceBinding, String> {
    let controller_serial = controller_serial.trim();
    if controller_serial.is_empty() {
        return Err("controller serial cannot be empty".to_string());
    }

    let instance_id = derive_instance_id(&app, &mode, controller_serial);
    if let Some(existing) = state
        .instances
        .iter()
        .find(|instance| instance.instance_id == instance_id)
    {
        return Ok(existing.clone());
    }

    let (host_udp_port, control_port, log_broadcast_port) = allocate_ports(state, &mode)?;
    Ok(BridgeInstanceBinding {
        instance_id,
        display_name: None,
        app,
        mode,
        controller_serial: controller_serial.to_string(),
        controller_vid,
        controller_pid,
        target,
        artifact_source,
        installed_channel,
        installed_pinned_tag,
        host_udp_port,
        control_port,
        log_broadcast_port,
        enabled: true,
    })
}

fn app_slug(app: &BridgeApp) -> &'static str {
    match app {
        BridgeApp::Bitwig => "bitwig",
    }
}

fn mode_slug(mode: &BridgeMode) -> &'static str {
    match mode {
        BridgeMode::Hardware => "hardware",
        BridgeMode::NativeSim => "native-sim",
        BridgeMode::WasmSim => "wasm-sim",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_binding_uses_first_port_triplet() {
        let binding = build_binding(
            &BridgeInstancesState::default(),
            BridgeApp::Bitwig,
            BridgeMode::Hardware,
            "17081760",
            0x16C0,
            0x0489,
            FirmwareTarget::Bitwig,
            ArtifactSource::Installed,
            Some(Channel::Stable),
            None,
        )
        .unwrap();

        assert_eq!(binding.instance_id, "bitwig-hardware-17081760");
        assert_eq!(binding.host_udp_port, 9000);
        assert_eq!(binding.control_port, 7999);
        assert_eq!(binding.log_broadcast_port, 9999);
    }

    #[test]
    fn allocate_ports_skips_used_hardware_host_and_control_ports() {
        let state = BridgeInstancesState {
            schema: 1,
            instances: vec![BridgeInstanceBinding {
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
            }],
        };

        let ports = allocate_ports(&state, &BridgeMode::Hardware).unwrap();
        assert_eq!(ports, (9001, 8000, 10000));
    }

    #[test]
    fn allocate_ports_uses_mode_specific_host_family() {
        let state = BridgeInstancesState {
            schema: 1,
            instances: vec![BridgeInstanceBinding {
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
            }],
        };

        let ports = allocate_ports(&state, &BridgeMode::NativeSim).unwrap();
        assert_eq!(ports, (9100, 8000, 10000));
    }
}
