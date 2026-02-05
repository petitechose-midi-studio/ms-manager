use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const DEFAULT_CONTROL_PORT: u16 = 7999;
const SCHEMA: u32 = 1;

#[derive(Debug, serde::Serialize)]
struct Request<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<u32>,
    cmd: &'a str,
}

pub async fn send_command(
    port: u16,
    cmd: &str,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| "connect timeout".to_string())?
        .map_err(|e| e.to_string())?;

    let req = serde_json::to_vec(&Request {
        schema: Some(SCHEMA),
        cmd,
    })
    .map_err(|e| e.to_string())?;

    tokio::time::timeout(timeout, stream.write_all(&req))
        .await
        .map_err(|_| "write timeout".to_string())
        ?
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(timeout, stream.write_all(b"\n"))
        .await
        .map_err(|_| "write timeout".to_string())
        ?
        .map_err(|e| e.to_string())?;

    let mut buf = Vec::new();
    tokio::time::timeout(timeout, stream.read_to_end(&mut buf))
        .await
        .map_err(|_| "read timeout".to_string())
        ?
        .map_err(|e| e.to_string())?;

    let text = String::from_utf8_lossy(&buf);
    let text = text.trim();
    if text.is_empty() {
        return Err("empty response".to_string());
    }

    serde_json::from_str::<serde_json::Value>(text).map_err(|e| e.to_string())
}

pub async fn ping(timeout: Duration) -> bool {
    let Ok(v) = send_command(DEFAULT_CONTROL_PORT, "ping", timeout).await else {
        return false;
    };

    v.get("ok").and_then(|v| v.as_bool()).unwrap_or(false)
}
