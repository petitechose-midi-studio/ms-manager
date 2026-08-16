use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Channel;

pub const CONTROLLER_STATE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LastFlashed {
    pub channel: Channel,
    pub tag: String,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_profile: Option<String>,
    /// Unix epoch milliseconds (best-effort).
    pub flashed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ControllerState {
    pub schema: u32,
    #[serde(default)]
    pub last_flashed_by_instance: BTreeMap<String, LastFlashed>,
    #[serde(default, skip_serializing)]
    pub last_flashed: Option<LastFlashed>,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            schema: CONTROLLER_STATE_SCHEMA,
            last_flashed_by_instance: BTreeMap::new(),
            last_flashed: None,
        }
    }
}

impl ControllerState {
    pub fn last_flashed_for_instance(&self, instance_id: &str) -> Option<LastFlashed> {
        self.last_flashed_by_instance.get(instance_id).cloned()
    }

    pub fn set_last_flashed_for_instance(
        &mut self,
        instance_id: impl Into<String>,
        next: LastFlashed,
    ) {
        self.last_flashed_by_instance
            .insert(instance_id.into(), next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_last_flashed_per_instance() {
        let mut state = ControllerState::default();
        let flashed = LastFlashed {
            channel: crate::Channel::Stable,
            tag: "v0.1.0".to_string(),
            profile: "default".to_string(),
            build_profile: None,
            flashed_at_ms: 123,
        };

        state.set_last_flashed_for_instance("bitwig-hw-123", flashed.clone());

        assert_eq!(
            state
                .last_flashed_for_instance("bitwig-hw-123")
                .expect("instance flash state"),
            flashed
        );
        assert!(state.last_flashed_for_instance("missing").is_none());
    }

    #[test]
    fn reads_existing_flash_state_without_build_profile() {
        let flashed: LastFlashed = serde_json::from_str(
            r#"{"channel":"stable","tag":"workspace","profile":"default","flashed_at_ms":123}"#,
        )
        .unwrap();

        assert_eq!(flashed.build_profile, None);
    }
}
