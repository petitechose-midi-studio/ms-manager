use crate::models::{
    DeviceStatus, DeviceTarget, MidiInventoryProvider, MidiInventoryStatus, MidiMatchConfidence,
    MidiPortDirection, MidiPortInfo, MidiPortMatch,
};

#[cfg(windows)]
mod windows {
    use super::*;

    use std::collections::HashMap;
    use std::process::Command;

    use crate::services::process;

    #[derive(Debug, Clone, serde::Deserialize)]
    struct WindowsMidiInventorySnapshot {
        serial_ports: Vec<WindowsSerialPort>,
        midi_endpoints: Vec<WindowsMidiEndpoint>,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct WindowsSerialPort {
        device_id: String,
        pnp_device_id: String,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct WindowsMidiEndpoint {
        instance_id: String,
        friendly_name: Option<String>,
        parent: Option<String>,
        siblings: Vec<WindowsMidiSibling>,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct WindowsMidiSibling {
        instance_id: String,
        friendly_name: Option<String>,
    }

    const WINDOWS_MIDI_INVENTORY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'

$serialPorts = @(
  Get-CimInstance Win32_SerialPort |
    Where-Object { $_.PNPDeviceID } |
    ForEach-Object {
      [PSCustomObject]@{
        device_id = $_.DeviceID
        pnp_device_id = $_.PNPDeviceID
      }
    }
)

$midiEndpoints = @(
  Get-PnpDevice -PresentOnly |
    Where-Object {
      $_.InstanceId -like 'SWD\MIDISRV\MIDIU_*' -and
      $_.InstanceId -notlike 'SWD\MIDISRV\MIDIU_DIAG*' -and
      $_.InstanceId -notlike 'SWD\MIDISRV\MIDIU_LOOP*' -and
      $_.InstanceId -notlike 'SWD\MIDISRV\MIDIU_BLOOP*' -and
      $_.InstanceId -notlike 'SWD\MIDISRV\MIDIU_APP_TRANSPORT'
    } |
    ForEach-Object {
      $endpoint = $_
      $props = Get-PnpDeviceProperty -InstanceId $endpoint.InstanceId -KeyName 'DEVPKEY_Device_Parent','DEVPKEY_Device_Siblings'
      $parent = ($props | Where-Object KeyName -eq 'DEVPKEY_Device_Parent').Data
      $siblings = @(($props | Where-Object KeyName -eq 'DEVPKEY_Device_Siblings').Data)
      $siblingInfos = @(
        foreach ($sibling in $siblings) {
          $device = Get-PnpDevice -InstanceId $sibling -ErrorAction SilentlyContinue
          if ($null -ne $device) {
            [PSCustomObject]@{
              instance_id = $sibling
              friendly_name = $device.FriendlyName
            }
          }
        }
      )

      [PSCustomObject]@{
        instance_id = $endpoint.InstanceId
        friendly_name = $endpoint.FriendlyName
        parent = $parent
        siblings = $siblingInfos
      }
    }
)

[PSCustomObject]@{
  serial_ports = $serialPorts
  midi_endpoints = $midiEndpoints
} | ConvertTo-Json -Depth 6 -Compress
"#;

    pub fn inventory(device: &DeviceStatus) -> Option<MidiInventoryStatus> {
        let snapshot = collect_snapshot().ok()?;
        let ports = ports_from_snapshot(device, &snapshot);
        Some(MidiInventoryStatus {
            provider: MidiInventoryProvider::WindowsMidiServices,
            available: !ports.is_empty(),
            ports,
            notes: vec![
                "Windows MIDI Services is active. Ports are correlated with controllers using the shared Windows USB composite device identity when possible.".to_string(),
            ],
        })
    }

    fn collect_snapshot() -> Result<WindowsMidiInventorySnapshot, String> {
        let mut cmd = Command::new("powershell.exe");
        process::no_console_window_std(&mut cmd);
        let out = cmd
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                WINDOWS_MIDI_INVENTORY_SCRIPT,
            ])
            .output()
            .map_err(|e| format!("failed to run powershell MIDI inventory script: {e}"))?;

        if !out.status.success() {
            return Err(format!(
                "powershell MIDI inventory script failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }

        serde_json::from_slice::<WindowsMidiInventorySnapshot>(&out.stdout)
            .map_err(|e| format!("failed to parse powershell MIDI inventory JSON: {e}"))
    }

    fn ports_from_snapshot(
        device: &DeviceStatus,
        snapshot: &WindowsMidiInventorySnapshot,
    ) -> Vec<MidiPortInfo> {
        let controller_keys = controller_keys_by_composite(device, &snapshot.serial_ports);
        let mut ports = Vec::new();

        for endpoint in &snapshot.midi_endpoints {
            let parent = endpoint.parent.as_deref().unwrap_or_default();
            let Some(composite_key) = composite_usb_key(parent) else {
                continue;
            };

            let matched_target = controller_keys.get(&composite_key);

            for sibling in &endpoint.siblings {
                let Some(direction) = midi_direction_from_sibling_id(&sibling.instance_id) else {
                    continue;
                };

                let (vid, pid) = matched_target
                    .map(|target| (Some(target.vid), Some(target.pid)))
                    .unwrap_or_else(|| vid_pid_from_pnp_id(parent));
                let controller_serial = matched_target.and_then(|target| target.serial_number.clone());
                let manufacturer = matched_target
                    .and_then(|target| target.manufacturer.clone())
                    .or_else(|| endpoint.friendly_name.clone());
                let name = sibling
                    .friendly_name
                    .clone()
                    .or_else(|| endpoint.friendly_name.clone())
                    .unwrap_or_else(|| endpoint.instance_id.clone());

                ports.push(MidiPortInfo {
                    id: format!(
                        "windows_midi_services:{}:{}",
                        endpoint.instance_id, sibling.instance_id
                    ),
                    provider: MidiInventoryProvider::WindowsMidiServices,
                    name,
                    direction,
                    manufacturer,
                    serial_number: controller_serial.clone(),
                    vid,
                    pid,
                    system_device_id: Some(sibling.instance_id.clone()),
                    match_info: if controller_serial.is_some() {
                        MidiPortMatch {
                            controller_serial,
                            confidence: MidiMatchConfidence::Strong,
                            reason: Some(
                                "Matched via the shared Windows USB composite device identity between the serial interface and the MIDI endpoint."
                                    .to_string(),
                            ),
                        }
                    } else {
                        MidiPortMatch {
                            controller_serial: None,
                            confidence: MidiMatchConfidence::None,
                            reason: None,
                        }
                    },
                });
            }
        }

        ports
    }

    fn controller_keys_by_composite<'a>(
        device: &'a DeviceStatus,
        serial_ports: &'a [WindowsSerialPort],
    ) -> HashMap<String, &'a DeviceTarget> {
        let serial_ports_by_name = serial_ports
            .iter()
            .map(|port| (port.device_id.trim().to_ascii_uppercase(), port))
            .collect::<HashMap<_, _>>();

        device
            .targets
            .iter()
            .filter_map(|target| {
                let port_name = target.port_name.as_deref()?.trim();
                let serial_port = serial_ports_by_name.get(&port_name.to_ascii_uppercase())?;
                let composite_key = composite_usb_key(&serial_port.pnp_device_id)?;
                Some((composite_key, target))
            })
            .collect()
    }

    fn composite_usb_key(instance_id: &str) -> Option<String> {
        let raw = instance_id.trim();
        if raw.is_empty() {
            return None;
        }

        let (prefix, tail) = raw.split_once('\\')?;
        if !prefix.eq_ignore_ascii_case("USB") {
            return None;
        }

        let (device_id, instance_leaf) = tail.split_once('\\')?;
        let base_device_id = if let Some((left, _)) = device_id.split_once("&MI_") {
            left
        } else {
            device_id
        };
        let base_leaf = instance_leaf.rsplit_once('&').map(|(head, _)| head)?;

        Some(format!(
            "usb\\{}\\{}",
            base_device_id.to_ascii_lowercase(),
            base_leaf.to_ascii_lowercase()
        ))
    }

    fn vid_pid_from_pnp_id(instance_id: &str) -> (Option<u32>, Option<u32>) {
        let upper = instance_id.to_ascii_uppercase();
        let vid = extract_hex_segment(&upper, "VID_");
        let pid = extract_hex_segment(&upper, "PID_");
        (vid, pid)
    }

    fn extract_hex_segment(value: &str, marker: &str) -> Option<u32> {
        let start = value.find(marker)? + marker.len();
        let hex = value.get(start..start + 4)?;
        u32::from_str_radix(hex, 16).ok()
    }

    fn midi_direction_from_sibling_id(instance_id: &str) -> Option<MidiPortDirection> {
        let upper = instance_id.to_ascii_uppercase();
        if upper.ends_with("_0_0") {
            return Some(MidiPortDirection::Input);
        }
        if upper.ends_with("_1_0") {
            return Some(MidiPortDirection::Output);
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn composite_usb_key_normalizes_composite_interfaces() {
            let serial = composite_usb_key("USB\\VID_16C0&PID_0489&MI_00\\9&3032F450&0&0000");
            let midi = composite_usb_key("USB\\VID_16C0&PID_0489&MI_02\\9&3032f450&0&0002");

            assert_eq!(serial, midi);
        }

        #[test]
        fn midi_direction_maps_mmdevapi_suffixes() {
            assert!(matches!(
                midi_direction_from_sibling_id("SWD\\MMDEVAPI\\FOO_0_0"),
                Some(MidiPortDirection::Input)
            ));
            assert!(matches!(
                midi_direction_from_sibling_id("SWD\\MMDEVAPI\\FOO_1_0"),
                Some(MidiPortDirection::Output)
            ));
        }
    }
}

#[cfg(windows)]
mod windows_winmm {
    use super::*;

    use windows_sys::Win32::Media::Audio::{
        midiInGetDevCapsW, midiInGetNumDevs, midiOutGetDevCapsW, midiOutGetNumDevs, MIDIINCAPSW,
        MIDIOUTCAPSW,
    };

    pub fn inventory(device: &DeviceStatus) -> MidiInventoryStatus {
        let mut ports = Vec::new();

        for index in 0..unsafe { midiInGetNumDevs() } {
            if let Some(port) = input_port(index, device) {
                ports.push(port);
            }
        }

        for index in 0..unsafe { midiOutGetNumDevs() } {
            if let Some(port) = output_port(index, device) {
                ports.push(port);
            }
        }

        MidiInventoryStatus {
            provider: MidiInventoryProvider::Winmm,
            available: !ports.is_empty(),
            ports,
            notes: vec![
                "WinMM fallback enumerates MIDI port names, but it does not expose a reliable hardware serial for strong device matching.".to_string(),
            ],
        }
    }

    fn input_port(index: u32, device: &DeviceStatus) -> Option<MidiPortInfo> {
        let mut caps = unsafe { std::mem::zeroed::<MIDIINCAPSW>() };
        let result = unsafe {
            midiInGetDevCapsW(
                index as usize,
                &mut caps as *mut MIDIINCAPSW,
                std::mem::size_of::<MIDIINCAPSW>() as u32,
            )
        };
        if result != 0 {
            return None;
        }

        Some(port_info(
            format!("winmm:in:{index}"),
            midi_in_name(&caps),
            MidiPortDirection::Input,
            midi_in_manufacturer_id(&caps),
            midi_in_product_id(&caps),
            device,
        ))
    }

    fn output_port(index: u32, device: &DeviceStatus) -> Option<MidiPortInfo> {
        let mut caps = unsafe { std::mem::zeroed::<MIDIOUTCAPSW>() };
        let result = unsafe {
            midiOutGetDevCapsW(
                index as usize,
                &mut caps as *mut MIDIOUTCAPSW,
                std::mem::size_of::<MIDIOUTCAPSW>() as u32,
            )
        };
        if result != 0 {
            return None;
        }

        Some(port_info(
            format!("winmm:out:{index}"),
            midi_out_name(&caps),
            MidiPortDirection::Output,
            midi_out_manufacturer_id(&caps),
            midi_out_product_id(&caps),
            device,
        ))
    }

    fn port_info(
        id: String,
        name: String,
        direction: MidiPortDirection,
        manufacturer_id: u16,
        product_id: u16,
        device: &DeviceStatus,
    ) -> MidiPortInfo {
        let match_info = weak_name_match(&name, device);

        MidiPortInfo {
            id,
            provider: MidiInventoryProvider::Winmm,
            name,
            direction,
            manufacturer: None,
            serial_number: None,
            vid: None,
            pid: None,
            system_device_id: Some(format!("winmm:{manufacturer_id}:{product_id}")),
            match_info,
        }
    }

    fn weak_name_match(name: &str, device: &DeviceStatus) -> MidiPortMatch {
        let name = name.trim();
        if name.is_empty() {
            return MidiPortMatch {
                controller_serial: None,
                confidence: MidiMatchConfidence::None,
                reason: None,
            };
        }

        let lowered_name = name.to_ascii_lowercase();
        let matched = device.targets.iter().find(|target| {
            let product = target.product.as_deref().unwrap_or("").trim().to_ascii_lowercase();
            !product.is_empty() && product == lowered_name
        });

        if let Some(target) = matched {
            return MidiPortMatch {
                controller_serial: target.serial_number.clone(),
                confidence: MidiMatchConfidence::Weak,
                reason: Some("WinMM fallback matched this port by name only. This is helpful for diagnosis but not guaranteed to identify the physical controller.".to_string()),
            };
        }

        MidiPortMatch {
            controller_serial: None,
            confidence: MidiMatchConfidence::None,
            reason: None,
        }
    }

    fn wide_to_string(wide: &[u16]) -> String {
        let len = wide.iter().position(|value| *value == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..len]).trim().to_string()
    }

    fn midi_in_name(caps: &MIDIINCAPSW) -> String {
        let wide = unsafe { std::ptr::addr_of!(caps.szPname).read_unaligned() };
        wide_to_string(&wide)
    }

    fn midi_out_name(caps: &MIDIOUTCAPSW) -> String {
        let wide = unsafe { std::ptr::addr_of!(caps.szPname).read_unaligned() };
        wide_to_string(&wide)
    }

    fn midi_in_manufacturer_id(caps: &MIDIINCAPSW) -> u16 {
        unsafe { std::ptr::addr_of!(caps.wMid).read_unaligned() }
    }

    fn midi_in_product_id(caps: &MIDIINCAPSW) -> u16 {
        unsafe { std::ptr::addr_of!(caps.wPid).read_unaligned() }
    }

    fn midi_out_manufacturer_id(caps: &MIDIOUTCAPSW) -> u16 {
        unsafe { std::ptr::addr_of!(caps.wMid).read_unaligned() }
    }

    fn midi_out_product_id(caps: &MIDIOUTCAPSW) -> u16 {
        unsafe { std::ptr::addr_of!(caps.wPid).read_unaligned() }
    }
}

fn no_inventory(provider: MidiInventoryProvider, note: &str) -> MidiInventoryStatus {
    MidiInventoryStatus {
        provider,
        available: false,
        ports: Vec::new(),
        notes: vec![note.to_string()],
    }
}

pub fn midi_inventory(device: &DeviceStatus) -> MidiInventoryStatus {
    #[cfg(windows)]
    {
        if let Some(inventory) = windows::inventory(device) {
            return inventory;
        }
        return windows_winmm::inventory(device);
    }

    #[cfg(target_os = "linux")]
    {
        let _ = device;
        return no_inventory(
            MidiInventoryProvider::None,
            "Linux MIDI inventory is not implemented yet. ALSA is the planned provider.",
        );
    }

    #[cfg(target_os = "macos")]
    {
        let _ = device;
        return no_inventory(
            MidiInventoryProvider::None,
            "macOS MIDI inventory is not implemented yet. CoreMIDI is the planned provider.",
        );
    }

    #[allow(unreachable_code)]
    {
        let _ = device;
        no_inventory(
            MidiInventoryProvider::None,
            "MIDI inventory is not available on this platform.",
        )
    }
}
