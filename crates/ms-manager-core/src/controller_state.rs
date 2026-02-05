use serde::{Deserialize, Serialize};

use crate::Channel;

pub const CONTROLLER_STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LastFlashed {
    pub channel: Channel,
    pub tag: String,
    pub profile: String,
    /// Unix epoch milliseconds (best-effort).
    pub flashed_at_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ControllerState {
    pub schema: u32,
    #[serde(default)]
    pub last_flashed: Option<LastFlashed>,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            schema: CONTROLLER_STATE_SCHEMA,
            last_flashed: None,
        }
    }
}
