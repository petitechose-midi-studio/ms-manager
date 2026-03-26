use serde::{Deserialize, Serialize};

use crate::Channel;

pub const SETTINGS_SCHEMA: u32 = 1;

fn default_profile() -> String {
    "default".to_string()
}

fn default_artifact_source() -> ArtifactSource {
    ArtifactSource::Installed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSource {
    Installed,
    Workspace,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub schema: u32,
    pub channel: Channel,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub pinned_tag: Option<String>,
    #[serde(default = "default_artifact_source")]
    pub artifact_source: ArtifactSource,
    #[serde(default)]
    pub payload_root_override: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SETTINGS_SCHEMA,
            channel: Channel::Stable,
            profile: default_profile(),
            pinned_tag: None,
            artifact_source: default_artifact_source(),
            payload_root_override: None,
        }
    }
}
