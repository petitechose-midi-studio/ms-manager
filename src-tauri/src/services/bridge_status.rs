use crate::layout::PayloadLayout;
use crate::models::BridgeStatus;
use crate::services::{bridge_ctl, payload};

pub async fn bridge_status(layout: &PayloadLayout) -> BridgeStatus {
    let exe = payload::oc_bridge_path(layout);
    if !exe.exists() {
        return BridgeStatus {
            installed: false,
            running: false,
            paused: false,
            serial_open: false,
            version: None,
            message: Some("oc-bridge missing".to_string()),
        };
    }

    let res = bridge_ctl::send_command(
        bridge_ctl::DEFAULT_CONTROL_PORT,
        "status",
        std::time::Duration::from_millis(180),
    )
    .await;

    match res {
        Ok(v) => {
            let ok = v.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            let paused = v.get("paused").and_then(|v| v.as_bool()).unwrap_or(false);
            let serial_open = v
                .get("serial_open")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let version = v
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let message = v
                .get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            BridgeStatus {
                installed: true,
                running: ok,
                paused,
                serial_open,
                version,
                message,
            }
        }
        Err(e) => BridgeStatus {
            installed: true,
            running: false,
            paused: false,
            serial_open: false,
            version: None,
            message: Some(e),
        },
    }
}
