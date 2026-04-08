use serde::{Deserialize, Serialize};

use ms_manager_core::{
    ArtifactSource, BridgeInstanceBinding, BridgeInstancesState, Channel, FirmwareTarget,
    InstallState, LastFlashed, Platform,
};

#[derive(Debug, Clone, Serialize)]
pub struct BridgeInstanceStatus {
    pub instance_id: String,
    pub display_name: Option<String>,
    pub configured_serial: String,
    pub target: FirmwareTarget,
    pub artifact_source: ArtifactSource,
    pub installed_channel: Option<Channel>,
    pub installed_pinned_tag: Option<String>,
    pub artifacts_ready: bool,
    pub artifact_message: Option<String>,
    pub enabled: bool,
    pub running: bool,
    pub paused: bool,
    pub serial_open: bool,
    pub version: Option<String>,
    pub resolved_serial_port: Option<String>,
    pub connected_serial: Option<String>,
    pub message: Option<String>,
    pub last_flashed: Option<LastFlashed>,
    pub artifact_location_path: Option<String>,
    pub host_udp_port: u16,
    pub control_port: u16,
    pub log_broadcast_port: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    pub installed: bool,
    pub running: bool,
    pub paused: bool,
    pub serial_open: bool,
    pub version: Option<String>,
    pub message: Option<String>,
    pub instances: Vec<BridgeInstanceStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetPlan {
    pub id: String,
    pub kind: String,
    pub filename: String,
    pub sha256: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub channel: Channel,
    pub tag: String,
    pub profile: String,
    pub platform: Platform,
    pub assets: Vec<AssetPlan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub installed: Option<InstallState>,
    pub host_installed: bool,
    pub artifact_source: ArtifactSource,
    pub artifact_config_path: Option<String>,
    pub artifact_message: Option<String>,
    pub tab_order: Vec<String>,
    pub platform: Platform,
    pub payload_root: String,
    pub device: DeviceStatus,
    pub bridge: BridgeStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeInstancesResponse {
    pub state: BridgeInstancesState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInstanceBindRequest {
    pub app: ms_manager_core::BridgeApp,
    pub mode: ms_manager_core::BridgeMode,
    pub controller_serial: String,
    pub controller_vid: u32,
    pub controller_pid: u32,
    pub target: ms_manager_core::FirmwareTarget,
    pub artifact_source: ms_manager_core::ArtifactSource,
    pub installed_channel: Option<Channel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInstanceTargetSetRequest {
    pub instance_id: String,
    pub target: ms_manager_core::FirmwareTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInstanceArtifactSourceSetRequest {
    pub instance_id: String,
    pub artifact_source: ms_manager_core::ArtifactSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInstanceInstalledReleaseSetRequest {
    pub instance_id: String,
    pub channel: Channel,
    pub pinned_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInstanceNameSetRequest {
    pub instance_id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeInstanceBindingResponse {
    pub binding: BridgeInstanceBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabOrderSetRequest {
    pub instance_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TabOrderResponse {
    pub tab_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub pub_date: Option<String>,
    pub notes: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateStatus {
    pub current_version: String,
    pub available: bool,
    pub update: Option<AppUpdateInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStatus {
    pub connected: bool,
    pub count: u32,
    pub targets: Vec<DeviceTarget>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiInventoryProvider {
    WindowsMidiServices,
    Winmm,
    Alsa,
    CoreMidi,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiPortDirection {
    Input,
    Output,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiMatchConfidence {
    Strong,
    Weak,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiPortMatch {
    pub controller_serial: Option<String>,
    pub confidence: MidiMatchConfidence,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiPortInfo {
    pub id: String,
    pub provider: MidiInventoryProvider,
    pub name: String,
    pub direction: MidiPortDirection,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub vid: Option<u32>,
    pub pid: Option<u32>,
    pub system_device_id: Option<String>,
    pub match_info: MidiPortMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiInventoryStatus {
    pub provider: MidiInventoryProvider,
    pub available: bool,
    pub ports: Vec<MidiPortInfo>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTargetKind {
    #[serde(rename = "serial")]
    Serial,
    #[serde(rename = "halfkay")]
    HalfKay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTarget {
    pub index: u32,
    pub target_id: String,
    pub kind: DeviceTargetKind,

    #[serde(default)]
    pub port_name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,

    #[serde(default)]
    pub serial_number: Option<String>,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub product: Option<String>,

    pub vid: u32,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallEvent {
    Begin {
        channel: Channel,
        tag: String,
        profile: String,
        assets_total: usize,
    },
    Downloading {
        index: usize,
        total: usize,
        asset_id: String,
        filename: String,
    },
    Applying {
        step: String,
    },
    Done {
        tag: String,
        profile: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlashEvent {
    Begin {
        channel: Channel,
        tag: String,
        profile: String,
    },
    Message {
        level: FlashMessageLevel,
        message: String,
    },
    Output {
        line: String,
    },
    Done {
        ok: bool,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashMessageLevel {
    Info,
    Warn,
}
