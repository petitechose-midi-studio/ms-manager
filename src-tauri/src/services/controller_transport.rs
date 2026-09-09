//! Bounded binary control transport; independent of filesystem wire operations.
use super::controller_fs::{
    ControllerFsError, ControllerFsResult, DEFAULT_CONTROL_TIMEOUT, DEFAULT_RPC_TIMEOUT_MS,
};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
const BINARY_REQUEST_MAGIC: &[u8; 4] = b"OCRQ";
const BINARY_RESPONSE_MAGIC: &[u8; 4] = b"OCRS";
const BINARY_CONTROL_VERSION: u8 = 1;
const BINARY_HEADER_BYTES: usize = 16;
const BINARY_STATUS_OK: u8 = 0;
// The bridge is local, but it is still an external process. Bound lengths from
// its response header before allocating so a stale or spoofed listener cannot
// make the manager reserve attacker-controlled amounts of memory.
const BINARY_MAX_RESPONSE_PAYLOAD_BYTES: usize = 1024 * 1024;
const BINARY_MAX_RESPONSE_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct ControllerRpcBatchItem {
    pub payload: Vec<u8>,
    pub expected_response_id: u8,
    pub timeout_ms: u32,
}

#[derive(Debug)]
struct BinaryControlResponse {
    token: u16,
    status: u8,
    payload: Vec<u8>,
    message: String,
}

pub struct BridgeBinaryClient {
    port: u16,
    timeout: Duration,
    stream: Option<TcpStream>,
    next_token: u16,
}

impl BridgeBinaryClient {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            timeout: DEFAULT_CONTROL_TIMEOUT,
            stream: None,
            next_token: 1,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn close(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
    }

    pub async fn controller_rpc(
        &mut self,
        payload: Vec<u8>,
        expected_response_id: u8,
        timeout_ms: u32,
    ) -> ControllerFsResult<Vec<u8>> {
        let mut responses = self
            .controller_rpc_batch(&[ControllerRpcBatchItem {
                payload,
                expected_response_id,
                timeout_ms,
            }])
            .await?;
        responses.pop().ok_or_else(|| {
            ControllerFsError::new("invalid_state", "missing binary control response")
        })
    }

    pub async fn controller_rpc_batch(
        &mut self,
        requests: &[ControllerRpcBatchItem],
    ) -> ControllerFsResult<Vec<Vec<u8>>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let mut packet = Vec::new();
        let mut token_to_index = HashMap::new();
        for (index, request) in requests.iter().enumerate() {
            let token = self.next_request_token();
            token_to_index.insert(token, index);
            packet.extend_from_slice(BINARY_REQUEST_MAGIC);
            packet.push(BINARY_CONTROL_VERSION);
            packet.push(request.expected_response_id as u8);
            packet.extend_from_slice(&token.to_le_bytes());
            packet.extend_from_slice(&request.timeout_ms.to_le_bytes());
            packet.extend_from_slice(&(request.payload.len() as u32).to_le_bytes());
            packet.extend_from_slice(&request.payload);
        }

        let max_timeout_ms = requests
            .iter()
            .map(|item| item.timeout_ms)
            .max()
            .unwrap_or(DEFAULT_RPC_TIMEOUT_MS);
        let timeout = self.timeout + Duration::from_millis(u64::from(max_timeout_ms));

        let write_result = {
            let stream = self.connect().await?;
            tokio::time::timeout(timeout, stream.write_all(&packet)).await
        };
        match write_result {
            Err(_) => {
                // Never reuse a stream after an ambiguous timeout: the late
                // response would otherwise be consumed by the retry and its
                // token would no longer match.
                self.stream = None;
                return Err(ControllerFsError::new(
                    "bridge_timeout",
                    "binary write timeout",
                ));
            }
            Ok(Err(err)) => {
                self.stream = None;
                return Err(bridge_io_error(err));
            }
            Ok(Ok(())) => {}
        }

        let mut responses: Vec<Option<Vec<u8>>> = vec![None; requests.len()];
        while !token_to_index.is_empty() {
            let read_result = {
                let stream = self.connect().await?;
                tokio::time::timeout(timeout, read_binary_response(stream)).await
            };
            let response = match read_result {
                Err(_) => {
                    self.stream = None;
                    return Err(ControllerFsError::new(
                        "bridge_timeout",
                        "binary read timeout",
                    ));
                }
                Ok(Ok(value)) => value,
                Ok(Err(err)) => {
                    self.stream = None;
                    return Err(bridge_io_error(err));
                }
            };
            let Some(index) = token_to_index.remove(&response.token) else {
                self.stream = None;
                return Err(ControllerFsError::new(
                    "protocol_error",
                    format!("unexpected binary response token: {}", response.token),
                ));
            };
            if response.status != BINARY_STATUS_OK {
                self.stream = None;
                return Err(ControllerFsError::new(
                    "controller_rpc_failed",
                    if response.message.is_empty() {
                        format!("controller rpc failed: status {}", response.status)
                    } else {
                        response.message
                    },
                ));
            }
            responses[index] = Some(response.payload);
        }

        responses
            .into_iter()
            .map(|item| {
                item.ok_or_else(|| {
                    ControllerFsError::new("invalid_state", "missing binary control response")
                })
            })
            .collect()
    }

    async fn connect(&mut self) -> ControllerFsResult<&mut TcpStream> {
        if self.stream.is_none() {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port);
            let stream = tokio::time::timeout(self.timeout, TcpStream::connect(addr))
                .await
                .map_err(|_| ControllerFsError::new("bridge_timeout", "connect timeout"))?
                .map_err(|err| {
                    ControllerFsError::new(
                        "bridge_unavailable",
                        format!(
                            "cannot connect to oc-bridge control port {}: {err}",
                            self.port
                        ),
                    )
                })?;
            self.stream = Some(stream);
        }
        self.stream.as_mut().ok_or_else(|| {
            ControllerFsError::new("invalid_state", "bridge stream was not initialized")
        })
    }

    fn next_request_token(&mut self) -> u16 {
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1);
        if self.next_token == 0 {
            self.next_token = 1;
        }
        token
    }
}

fn bridge_io_error(err: std::io::Error) -> ControllerFsError {
    ControllerFsError::new(
        "bridge_unavailable",
        format!("oc-bridge binary control IO failed: {err}"),
    )
}

async fn read_binary_response(stream: &mut TcpStream) -> std::io::Result<BinaryControlResponse> {
    let mut header = [0u8; BINARY_HEADER_BYTES];
    stream.read_exact(&mut header).await?;
    if &header[0..4] != BINARY_RESPONSE_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid binary control response magic",
        ));
    }
    if header[4] != BINARY_CONTROL_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported binary control response version: {}", header[4]),
        ));
    }

    let status = header[5];
    let token = u16::from_le_bytes([header[6], header[7]]);
    let payload_len = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
    let message_len = u16::from_le_bytes([header[12], header[13]]) as usize;
    if payload_len > BINARY_MAX_RESPONSE_PAYLOAD_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("binary response payload is too large: {payload_len} bytes"),
        ));
    }
    if message_len > BINARY_MAX_RESPONSE_MESSAGE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("binary response message is too large: {message_len} bytes"),
        ));
    }
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload).await?;
    }
    let mut message_bytes = vec![0u8; message_len];
    if message_len > 0 {
        stream.read_exact(&mut message_bytes).await?;
    }
    let message = String::from_utf8_lossy(&message_bytes).to_string();
    Ok(BinaryControlResponse {
        token,
        status,
        payload,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    #[test]
    fn binary_timeout_drops_stream_before_retry() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut first_stream, _) = listener.accept().await.unwrap();
                let first = read_binary_request(&mut first_stream).await;
                // Keep the first connection open without replying. The client
                // must discard it on timeout and establish a fresh connection.
                let (mut second_stream, _) = listener.accept().await.unwrap();
                let second = read_binary_request(&mut second_stream).await;
                write_binary_response(&mut second_stream, second.token, &[0xab]).await;
                (first.token, second.token)
            });

            let mut client = BridgeBinaryClient::new(port).with_timeout(Duration::from_millis(5));
            let first_error = client
                .controller_rpc(vec![0x01], 0xfd, 1)
                .await
                .unwrap_err();
            assert_eq!(first_error.kind, "bridge_timeout");

            let response = client.controller_rpc(vec![0x02], 0xfd, 50).await.unwrap();
            assert_eq!(response, vec![0xab]);
            let (first_token, second_token) = server.await.unwrap();
            assert_ne!(first_token, second_token);
        });
    }

    #[test]
    fn binary_response_lengths_are_bounded_before_allocation() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_binary_request(&mut stream).await;
                let mut response = Vec::new();
                response.extend_from_slice(BINARY_RESPONSE_MAGIC);
                response.push(BINARY_CONTROL_VERSION);
                response.push(BINARY_STATUS_OK);
                response.extend_from_slice(&request.token.to_le_bytes());
                response.extend_from_slice(
                    &((BINARY_MAX_RESPONSE_PAYLOAD_BYTES + 1) as u32).to_le_bytes(),
                );
                response.extend_from_slice(&0u16.to_le_bytes());
                response.extend_from_slice(&0u16.to_le_bytes());
                stream.write_all(&response).await.unwrap();
            });

            let mut client = BridgeBinaryClient::new(port);
            let error = client
                .controller_rpc(vec![0x01], 0xfd, 50)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "bridge_unavailable");
            assert!(error.message.contains("too large"));
            server.await.unwrap();
        });
    }

    fn run_async(future: impl std::future::Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future);
    }

    struct CapturedBinaryRequest {
        token: u16,
    }

    async fn read_binary_request(stream: &mut TcpStream) -> CapturedBinaryRequest {
        let mut header = [0u8; BINARY_HEADER_BYTES];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(&header[0..4], BINARY_REQUEST_MAGIC);
        assert_eq!(header[4], BINARY_CONTROL_VERSION);
        let token = u16::from_le_bytes([header[6], header[7]]);
        let payload_len =
            u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload).await.unwrap();
        CapturedBinaryRequest { token }
    }

    async fn write_binary_response(stream: &mut TcpStream, token: u16, payload: &[u8]) {
        let mut response = Vec::new();
        response.extend_from_slice(BINARY_RESPONSE_MAGIC);
        response.push(BINARY_CONTROL_VERSION);
        response.push(BINARY_STATUS_OK);
        response.extend_from_slice(&token.to_le_bytes());
        response.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        response.extend_from_slice(&0u16.to_le_bytes());
        response.extend_from_slice(&0u16.to_le_bytes());
        response.extend_from_slice(payload);
        stream.write_all(&response).await.unwrap();
    }
}
