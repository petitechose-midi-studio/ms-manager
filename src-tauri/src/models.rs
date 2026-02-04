use serde::Serialize;

use ms_manager_core::{Channel, InstallState, Manifest, Platform, Settings};

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
    pub platform: Platform,
    pub payload_root: String,
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
