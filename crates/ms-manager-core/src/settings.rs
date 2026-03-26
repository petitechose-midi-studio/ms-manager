use serde::{Deserialize, Serialize};

pub const SETTINGS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSource {
    Installed,
    Workspace,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub schema: u32,
    #[serde(default)]
    pub payload_root_override: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SETTINGS_SCHEMA,
            payload_root_override: None,
        }
    }
}
