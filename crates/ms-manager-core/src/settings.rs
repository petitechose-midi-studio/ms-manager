use serde::{Deserialize, Serialize};

use crate::Channel;

pub const SETTINGS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub schema: u32,
    pub channel: Channel,
    #[serde(default)]
    pub pinned_tag: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SETTINGS_SCHEMA,
            channel: Channel::Stable,
            pinned_tag: None,
        }
    }
}
