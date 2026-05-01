use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::state::AppState;

const BRIDGE_LOG_EVENT: &str = "ms-manager://bridge-log";

#[derive(Debug, Clone, Serialize)]
pub struct BridgeLogEvent {
    pub instance_id: Option<String>,
    pub port: u16,
    pub timestamp: String,
    pub kind: String,
    pub level: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct BridgeLogEntry {
    timestamp: String,
    kind: BridgeLogKind,
}

#[derive(Debug, Deserialize)]
enum BridgeLogKind {
    Protocol {
        direction: BridgeDirection,
        message_name: String,
        size: usize,
    },
    Debug {
        level: Option<BridgeLogLevel>,
        message: String,
    },
    System {
        message: String,
    },
}

#[derive(Debug, Deserialize)]
enum BridgeDirection {
    In,
    Out,
}

#[derive(Debug, Deserialize)]
enum BridgeLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub fn spawn_bridge_log_supervisor(app: tauri::AppHandle) {
    let started_ports = Arc::new(Mutex::new(HashSet::<u16>::new()));

    tauri::async_runtime::spawn(async move {
        loop {
            let bindings = app.state::<AppState>().bridge_instances_get();
            for binding in bindings.instances.iter().filter(|binding| binding.enabled) {
                let inserted = {
                    let mut ports = started_ports.lock().unwrap();
                    ports.insert(binding.log_broadcast_port)
                };

                if !inserted {
                    continue;
                }

                let app = app.clone();
                let port = binding.log_broadcast_port;
                let started_ports = started_ports.clone();
                tauri::async_runtime::spawn(async move {
                    run_bridge_log_listener(app, port, started_ports).await;
                });
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

async fn run_bridge_log_listener(
    app: tauri::AppHandle,
    port: u16,
    started_ports: Arc<Mutex<HashSet<u16>>>,
) {
    let Ok(socket) = tokio::net::UdpSocket::bind(("127.0.0.1", port)).await else {
        started_ports.lock().unwrap().remove(&port);
        return;
    };

    let mut buf = vec![0u8; 65535];
    loop {
        let Ok((len, _)) = socket.recv_from(&mut buf).await else {
            break;
        };

        let text = String::from_utf8_lossy(&buf[..len]);
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let Ok(entry) = serde_json::from_str::<BridgeLogEntry>(line) else {
                continue;
            };
            let payload = map_bridge_log_event(&app, port, entry);
            crate::services::ux_recorder::observe_bridge_log(&app, &payload);
            let _ = app.emit(BRIDGE_LOG_EVENT, payload);
        }
    }

    started_ports.lock().unwrap().remove(&port);
}

fn map_bridge_log_event(
    app: &tauri::AppHandle,
    port: u16,
    entry: BridgeLogEntry,
) -> BridgeLogEvent {
    let instance_id = app
        .state::<AppState>()
        .bridge_instances_get()
        .instances
        .into_iter()
        .find(|binding| binding.log_broadcast_port == port)
        .map(|binding| binding.instance_id);

    let (kind, level, message) = match entry.kind {
        BridgeLogKind::System { message } => {
            ("system".to_string(), Some("info".to_string()), message)
        }
        BridgeLogKind::Debug { level, message } => (
            "debug".to_string(),
            Some(
                match level.unwrap_or(BridgeLogLevel::Info) {
                    BridgeLogLevel::Debug => "debug",
                    BridgeLogLevel::Info => "info",
                    BridgeLogLevel::Warn => "warn",
                    BridgeLogLevel::Error => "error",
                }
                .to_string(),
            ),
            message,
        ),
        BridgeLogKind::Protocol {
            direction,
            message_name,
            size,
        } => (
            match direction {
                BridgeDirection::In => "protocol_in",
                BridgeDirection::Out => "protocol_out",
            }
            .to_string(),
            Some("info".to_string()),
            format!(
                "{} {} ({} B)",
                match direction {
                    BridgeDirection::In => "IN",
                    BridgeDirection::Out => "OUT",
                },
                message_name,
                size
            ),
        ),
    };

    BridgeLogEvent {
        instance_id,
        port,
        timestamp: entry.timestamp,
        kind,
        level,
        message,
    }
}
