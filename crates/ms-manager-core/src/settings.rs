use serde::{Deserialize, Serialize};

use crate::Channel;

pub const SETTINGS_SCHEMA: u32 = 1;

fn default_profile() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub schema: u32,
    pub channel: Channel,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub pinned_tag: Option<String>,
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
            payload_root_override: None,
        }
    }
}
