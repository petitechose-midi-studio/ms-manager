use serde::{Deserialize, Serialize};

use crate::Channel;

pub const INSTALL_STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstallState {
    pub schema: u32,
    pub channel: Channel,
    pub profile: String,
    pub tag: String,
}
