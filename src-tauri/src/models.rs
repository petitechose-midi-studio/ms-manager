use serde::{Deserialize, Serialize};

use ms_manager_core::{Channel, InstallState, LastFlashed, Manifest, Platform, Settings};

#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    pub installed: bool,
    pub running: bool,
    pub paused: bool,
    pub serial_open: bool,
    pub version: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestManifestResponse {
    pub channel: Channel,
    pub available: bool,
    pub tag: Option<String>,
    pub manifest: Option<Manifest>,
    pub message: Option<String>,
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
    pub settings: Settings,
    pub installed: Option<InstallState>,
    pub host_installed: bool,
    pub platform: Platform,
    pub payload_root: String,
    pub device: DeviceStatus,
    pub last_flashed: Option<LastFlashed>,
    pub bridge: BridgeStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub pub_date: Option<String>,
    pub notes: Option<String>,
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
    Output {
        line: String,
    },
    Done {
        ok: bool,
    },
}
