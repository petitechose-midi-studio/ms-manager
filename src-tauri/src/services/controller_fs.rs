use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const DEFAULT_BRIDGE_CONTROL_PORT: u16 = 7999;
pub const DEFAULT_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_RPC_TIMEOUT_MS: u32 = 2_000;
pub const DEFAULT_PIPELINE_WINDOW: usize = 8;

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

const FS_RPC_SCHEMA: u8 = 1;
pub const FS_RPC_MAX_CHUNK_SIZE: usize = 30_720;
pub const FS_RPC_MAX_LIST_ENTRIES: u8 = 8;
pub const FS_RPC_SHA256_SIZE: usize = 32;
pub const FS_RPC_FEATURE_CONDITIONAL_MUTATIONS: u32 = 1 << 3;

static WRITE_SESSION_SEQUENCE: AtomicU16 = AtomicU16::new(1);

#[derive(Debug, Clone, Serialize)]
pub struct ControllerFsError {
    pub kind: String,
    pub message: String,
}

impl ControllerFsError {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ControllerFsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ControllerFsError {}

pub type ControllerFsResult<T> = Result<T, ControllerFsError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FsMessageId {
    StatRequest = 0xE0,
    StatResponse = 0xE1,
    ListRequest = 0xE2,
    ListResponse = 0xE3,
    ReadRequest = 0xE4,
    ReadResponse = 0xE5,
    WriteBeginRequest = 0xE6,
    WriteBeginResponse = 0xE7,
    WriteChunkRequest = 0xE8,
    WriteChunkResponse = 0xE9,
    WriteCommitRequest = 0xEA,
    WriteCommitResponse = 0xEB,
    WriteAbortRequest = 0xEC,
    WriteAbortResponse = 0xED,
    ErrorResponse = 0xEF,
    MkdirRequest = 0xF0,
    MkdirResponse = 0xF1,
    DeleteRequest = 0xF2,
    DeleteResponse = 0xF3,
    RenameRequest = 0xF4,
    RenameResponse = 0xF5,
    CapabilitiesRequest = 0xF6,
    CapabilitiesResponse = 0xF7,
    ConditionalReplaceRequest = 0xF8,
    ConditionalReplaceResponse = 0xF9,
    ConditionalDeleteRequest = 0xFA,
    ConditionalDeleteResponse = 0xFB,
}

impl FsMessageId {
    fn from_u8(value: u8) -> ControllerFsResult<Self> {
        match value {
            0xE0 => Ok(Self::StatRequest),
            0xE1 => Ok(Self::StatResponse),
            0xE2 => Ok(Self::ListRequest),
            0xE3 => Ok(Self::ListResponse),
            0xE4 => Ok(Self::ReadRequest),
            0xE5 => Ok(Self::ReadResponse),
            0xE6 => Ok(Self::WriteBeginRequest),
            0xE7 => Ok(Self::WriteBeginResponse),
            0xE8 => Ok(Self::WriteChunkRequest),
            0xE9 => Ok(Self::WriteChunkResponse),
            0xEA => Ok(Self::WriteCommitRequest),
            0xEB => Ok(Self::WriteCommitResponse),
            0xEC => Ok(Self::WriteAbortRequest),
            0xED => Ok(Self::WriteAbortResponse),
            0xEF => Ok(Self::ErrorResponse),
            0xF0 => Ok(Self::MkdirRequest),
            0xF1 => Ok(Self::MkdirResponse),
            0xF2 => Ok(Self::DeleteRequest),
            0xF3 => Ok(Self::DeleteResponse),
            0xF4 => Ok(Self::RenameRequest),
            0xF5 => Ok(Self::RenameResponse),
            0xF6 => Ok(Self::CapabilitiesRequest),
            0xF7 => Ok(Self::CapabilitiesResponse),
            0xF8 => Ok(Self::ConditionalReplaceRequest),
            0xF9 => Ok(Self::ConditionalReplaceResponse),
            0xFA => Ok(Self::ConditionalDeleteRequest),
            0xFB => Ok(Self::ConditionalDeleteResponse),
            _ => Err(ControllerFsError::new(
                "codec_error",
                format!("unknown filesystem rpc message id: 0x{value:02x}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum FsStatus {
    Ok,
    InvalidMessage,
    InvalidArgument,
    NotFound,
    Busy,
    TooLarge,
    StorageError,
    InvalidState,
    Unsupported,
    PreconditionFailed,
}

impl FsStatus {
    fn from_u8(value: u8) -> ControllerFsResult<Self> {
        match value {
            0 => Ok(Self::Ok),
            1 => Ok(Self::InvalidMessage),
            2 => Ok(Self::InvalidArgument),
            3 => Ok(Self::NotFound),
            4 => Ok(Self::Busy),
            5 => Ok(Self::TooLarge),
            6 => Ok(Self::StorageError),
            7 => Ok(Self::InvalidState),
            8 => Ok(Self::Unsupported),
            9 => Ok(Self::PreconditionFailed),
            _ => Err(ControllerFsError::new(
                "codec_error",
                format!("unknown filesystem rpc status: {value}"),
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::InvalidMessage => "invalid-message",
            Self::InvalidArgument => "invalid-argument",
            Self::NotFound => "not-found",
            Self::Busy => "busy",
            Self::TooLarge => "too-large",
            Self::StorageError => "storage-error",
            Self::InvalidState => "invalid-state",
            Self::Unsupported => "unsupported",
            Self::PreconditionFailed => "precondition-failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum FsConditionalMutationOutcome {
    None,
    Applied,
    AlreadyApplied,
}

impl FsConditionalMutationOutcome {
    fn from_u8(value: u8) -> ControllerFsResult<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Applied),
            2 => Ok(Self::AlreadyApplied),
            _ => Err(ControllerFsError::new(
                "codec_error",
                format!("unknown conditional mutation outcome: {value}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum FsConditionalMutationSubject {
    None,
    Source,
    Staging,
}

impl FsConditionalMutationSubject {
    fn from_u8(value: u8) -> ControllerFsResult<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Source),
            2 => Ok(Self::Staging),
            _ => Err(ControllerFsError::new(
                "codec_error",
                format!("unknown conditional mutation subject: {value}"),
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "transaction",
            Self::Source => "source",
            Self::Staging => "staging file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum FsFileType {
    Missing,
    File,
    Directory,
    Other,
}

impl FsFileType {
    fn from_u8(value: u8) -> ControllerFsResult<Self> {
        match value {
            0 => Ok(Self::Missing),
            1 => Ok(Self::File),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Other),
            _ => Err(ControllerFsError::new(
                "codec_error",
                format!("unknown filesystem rpc file type: {value}"),
            )),
        }
    }
}

#[derive(Debug, Clone)]
struct FsFrame {
    message_id: FsMessageId,
    schema: u8,
    request_id: u16,
    payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FsCapabilities {
    pub status: FsStatus,
    pub rpc_schema: u8,
    pub max_chunk_size: u16,
    pub response_buffer_size: u16,
    pub max_list_entries: u8,
    pub max_path_length: u16,
    pub feature_flags: u32,
}

impl FsCapabilities {
    pub fn supports_conditional_mutations(&self) -> bool {
        self.status == FsStatus::Ok
            && self.rpc_schema == FS_RPC_SCHEMA
            && (self.feature_flags & FS_RPC_FEATURE_CONDITIONAL_MUTATIONS) != 0
    }

    pub fn require_conditional_mutations(&self) -> ControllerFsResult<()> {
        if self.supports_conditional_mutations() {
            return Ok(());
        }
        Err(ControllerFsError::new(
            "unsupported_feature",
            "controller firmware does not advertise conditional filesystem mutations",
        ))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FsStat {
    pub status: FsStatus,
    pub file_type: FsFileType,
    pub size_bytes: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FsListEntry {
    pub name: String,
    pub file_type: FsFileType,
    pub size_bytes: u32,
    pub name_truncated: bool,
}

#[derive(Debug, Clone)]
struct FsListPage {
    status: FsStatus,
    start_index: u16,
    has_more: bool,
    entries: Vec<FsListEntry>,
}

#[derive(Debug, Clone)]
struct FsReadResponse {
    status: FsStatus,
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct FsWriteResponse {
    status: FsStatus,
    session_id: u16,
    bytes_written: u16,
}

#[derive(Debug, Clone)]
struct FsStatusResponse {
    status: FsStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsConditionalMutationResult {
    pub outcome: FsConditionalMutationOutcome,
    pub operation_id: u32,
}

#[derive(Debug, Clone)]
struct FsConditionalMutationResponse {
    status: FsStatus,
    outcome: FsConditionalMutationOutcome,
    subject: FsConditionalMutationSubject,
    operation_id: u32,
    observed_sha256: [u8; FS_RPC_SHA256_SIZE],
}

#[derive(Debug, Clone)]
pub struct ControllerRpcBatchItem {
    pub payload: Vec<u8>,
    pub expected_response_id: FsMessageId,
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

    pub async fn close(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
    }

    pub async fn controller_rpc(
        &mut self,
        payload: Vec<u8>,
        expected_response_id: FsMessageId,
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

pub struct ControllerFsClient {
    bridge: BridgeBinaryClient,
    next_request_id: u16,
    next_write_session_id: u16,
    chunk_size: usize,
    pipeline_window: usize,
    conditional_mutations_supported: Option<bool>,
}

impl ControllerFsClient {
    pub fn new(bridge: BridgeBinaryClient) -> Self {
        let next_write_session_id = initial_write_session_id(bridge.port);
        Self {
            bridge,
            next_request_id: 1,
            next_write_session_id,
            chunk_size: FS_RPC_MAX_CHUNK_SIZE,
            pipeline_window: DEFAULT_PIPELINE_WINDOW,
            conditional_mutations_supported: None,
        }
    }

    pub fn with_chunk_size(mut self, chunk_size: usize) -> ControllerFsResult<Self> {
        if chunk_size == 0 || chunk_size > FS_RPC_MAX_CHUNK_SIZE {
            return Err(ControllerFsError::new(
                "invalid_input",
                format!("chunk size must be between 1 and {FS_RPC_MAX_CHUNK_SIZE}"),
            ));
        }
        self.chunk_size = chunk_size;
        Ok(self)
    }

    pub fn with_pipeline_window(mut self, pipeline_window: usize) -> ControllerFsResult<Self> {
        if pipeline_window == 0 || pipeline_window > DEFAULT_PIPELINE_WINDOW {
            return Err(ControllerFsError::new(
                "invalid_input",
                format!("pipeline window must be between 1 and {DEFAULT_PIPELINE_WINDOW}"),
            ));
        }
        self.pipeline_window = pipeline_window;
        Ok(self)
    }

    pub async fn close(&mut self) {
        self.bridge.close().await;
    }

    pub async fn capabilities(&mut self) -> ControllerFsResult<FsCapabilities> {
        let request_id = self.request_id();
        let response = self
            .rpc(
                encode_capabilities_request(request_id)?,
                FsMessageId::CapabilitiesResponse,
            )
            .await?;
        let decoded = decode_capabilities_response(&response, request_id)?;
        self.conditional_mutations_supported = Some(decoded.supports_conditional_mutations());
        Ok(decoded)
    }

    pub async fn stat(&mut self, path: &str) -> ControllerFsResult<FsStat> {
        let request_id = self.request_id();
        let response = self
            .rpc(
                encode_stat_request(request_id, path)?,
                FsMessageId::StatResponse,
            )
            .await?;
        let decoded = decode_stat_response(&response, request_id)?;
        Ok(decoded)
    }

    pub async fn list(&mut self, path: &str) -> ControllerFsResult<Vec<FsListEntry>> {
        let mut start_index = 0u16;
        let mut entries = Vec::new();
        loop {
            let request_id = self.request_id();
            let request =
                encode_list_request(request_id, path, start_index, FS_RPC_MAX_LIST_ENTRIES)?;
            let response = self.rpc(request, FsMessageId::ListResponse).await?;
            let decoded = decode_list_response(&response, request_id)?;
            if decoded.status != FsStatus::Ok {
                return Err(remote_status_error("list", path, decoded.status));
            }
            if decoded.start_index != start_index {
                return Err(ControllerFsError::new(
                    "invalid_state",
                    format!(
                        "list response index mismatch: expected {start_index}, got {}",
                        decoded.start_index
                    ),
                ));
            }
            let has_more = decoded.has_more;
            if has_more && decoded.entries.is_empty() {
                return Err(ControllerFsError::new(
                    "protocol_error",
                    "list response requested another page without making progress",
                ));
            }
            let page_len = decoded.entries.len() as u16;
            entries.extend(decoded.entries);
            if !has_more {
                return Ok(entries);
            }
            start_index = start_index.checked_add(page_len).ok_or_else(|| {
                ControllerFsError::new("protocol_error", "list response index overflow")
            })?;
        }
    }

    pub async fn pull_file_to_path_with_progress<F>(
        &mut self,
        path: &str,
        destination: &Path,
        on_progress: F,
    ) -> ControllerFsResult<usize>
    where
        F: FnMut(usize, usize),
    {
        self.pull_file_to_path_with_progress_limit(path, destination, u32::MAX, on_progress)
            .await
    }

    pub async fn pull_file_to_path_with_progress_limit<F>(
        &mut self,
        path: &str,
        destination: &Path,
        max_bytes: u32,
        mut on_progress: F,
    ) -> ControllerFsResult<usize>
    where
        F: FnMut(usize, usize),
    {
        let stat = self.stat(path).await?;
        if stat.status != FsStatus::Ok {
            return Err(remote_status_error("stat", path, stat.status));
        }
        if stat.file_type != FsFileType::File {
            return Err(ControllerFsError::new(
                "not_file",
                format!("remote path is not a file: {path}"),
            ));
        }
        if stat.size_bytes > max_bytes {
            return Err(ControllerFsError::new(
                "too_large",
                format!("remote file exceeds the allowed size ({max_bytes} bytes): {path}"),
            ));
        }

        if let Some(parent) = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await.map_err(|err| {
                ControllerFsError::new(
                    "local_io_failed",
                    format!(
                        "create local transfer directory {}: {err}",
                        parent.display()
                    ),
                )
            })?;
        }
        let mut destination_file = tokio::fs::File::create(destination).await.map_err(|err| {
            ControllerFsError::new(
                "local_io_failed",
                format!(
                    "create local transfer file {}: {err}",
                    destination.display()
                ),
            )
        })?;

        let mut offset = 0u32;
        while offset < stat.size_bytes {
            let batch = self.build_read_batch(path, stat.size_bytes, offset)?;
            let responses = self
                .rpc_many(
                    &batch
                        .iter()
                        .map(|item| (item.payload.clone(), FsMessageId::ReadResponse))
                        .collect::<Vec<_>>(),
                )
                .await?;
            for (item, response) in batch.iter().zip(responses.iter()) {
                let decoded = decode_read_response(response, item.request_id, item.offset)?;
                if decoded.status != FsStatus::Ok {
                    return Err(remote_status_error("read", path, decoded.status));
                }
                if decoded.data.is_empty() && offset < stat.size_bytes {
                    return Err(ControllerFsError::new(
                        "invalid_state",
                        "read returned no data before EOF",
                    ));
                }
                if decoded.data.len() != item.size {
                    return Err(ControllerFsError::new(
                        "protocol_error",
                        format!(
                            "read response size mismatch at offset {}: expected {}, got {}",
                            item.offset,
                            item.size,
                            decoded.data.len()
                        ),
                    ));
                }
                offset = offset.saturating_add(decoded.data.len() as u32);
                destination_file
                    .write_all(&decoded.data)
                    .await
                    .map_err(|err| {
                        ControllerFsError::new(
                            "local_io_failed",
                            format!("write local transfer file {}: {err}", destination.display()),
                        )
                    })?;
                on_progress(offset as usize, stat.size_bytes as usize);
            }
        }
        destination_file.flush().await.map_err(|err| {
            ControllerFsError::new(
                "local_io_failed",
                format!("flush local transfer file {}: {err}", destination.display()),
            )
        })?;
        Ok(offset as usize)
    }

    pub async fn push_file_from_path_with_progress<F>(
        &mut self,
        path: &str,
        source: &Path,
        mut on_progress: F,
    ) -> ControllerFsResult<usize>
    where
        F: FnMut(usize, usize),
    {
        let metadata = tokio::fs::metadata(source).await.map_err(|err| {
            ControllerFsError::new(
                "local_io_failed",
                format!("read local transfer metadata {}: {err}", source.display()),
            )
        })?;
        if !metadata.is_file() {
            return Err(ControllerFsError::new(
                "invalid_input",
                format!("local transfer source is not a file: {}", source.display()),
            ));
        }
        let total_bytes = metadata.len();
        if total_bytes > u64::from(u32::MAX) {
            return Err(ControllerFsError::new(
                "invalid_input",
                format!(
                    "local transfer file exceeds controller limit: {}",
                    source.display()
                ),
            ));
        }

        let mut source_file = tokio::fs::File::open(source).await.map_err(|err| {
            ControllerFsError::new(
                "local_io_failed",
                format!("open local transfer file {}: {err}", source.display()),
            )
        })?;

        // Session ids are client-owned by the firmware protocol. Do not reuse
        // the deterministic request-id sequence: after an ambiguous begin we
        // issue a best-effort abort, and a predictable id could otherwise
        // collide with (and abort) another local client's active upload.
        let session_id = self.write_session_id();
        let begin_id = self.request_id();
        let begin = encode_write_begin_request(begin_id, session_id, path, total_bytes as u32)?;
        let begin_response = match self
            .write_rpc(begin, FsMessageId::WriteBeginResponse, begin_id)
            .await
        {
            Ok(value) => value,
            Err(err) => {
                // The request may have reached the controller even when its
                // response was lost or malformed. Abort by the known session
                // id so the next transaction is not left permanently busy.
                let _ = self.abort_write(session_id).await;
                return Err(err);
            }
        };
        if begin_response.status != FsStatus::Ok {
            return Err(remote_status_error(
                "write-begin",
                path,
                begin_response.status,
            ));
        }
        if begin_response.session_id != session_id || begin_response.bytes_written != 0 {
            let _ = self.abort_write(session_id).await;
            return Err(ControllerFsError::new(
                "invalid_state",
                "write begin response mismatch",
            ));
        }

        let mut offset = 0usize;
        while offset < total_bytes as usize {
            let batch = match self
                .build_write_batch_from_reader(
                    session_id,
                    &mut source_file,
                    offset,
                    total_bytes as usize,
                )
                .await
            {
                Ok(value) => value,
                Err(err) => {
                    let _ = self.abort_write(session_id).await;
                    return Err(err);
                }
            };
            let responses = self
                .rpc_many(
                    &batch
                        .iter()
                        .map(|item| (item.payload.clone(), FsMessageId::WriteChunkResponse))
                        .collect::<Vec<_>>(),
                )
                .await;
            let responses = match responses {
                Ok(value) => value,
                Err(err) => {
                    let _ = self.abort_write(session_id).await;
                    return Err(err);
                }
            };
            for (item, response) in batch.iter().zip(responses.iter()) {
                let decoded = match decode_write_response(response, item.request_id) {
                    Ok(value) => value,
                    Err(err) => {
                        let _ = self.abort_write(session_id).await;
                        return Err(err);
                    }
                };
                if decoded.status != FsStatus::Ok {
                    let _ = self.abort_write(session_id).await;
                    return Err(remote_status_error("write-chunk", path, decoded.status));
                }
                if decoded.session_id != session_id || decoded.bytes_written as usize != item.size {
                    let _ = self.abort_write(session_id).await;
                    return Err(ControllerFsError::new(
                        "invalid_state",
                        "write chunk response mismatch",
                    ));
                }
                offset += item.size;
                on_progress(offset, total_bytes as usize);
            }
        }

        let commit_id = self.request_id();
        let commit = encode_write_commit_request(commit_id, session_id)?;
        let commit_response = match self
            .write_rpc(commit, FsMessageId::WriteCommitResponse, commit_id)
            .await
        {
            Ok(value) => value,
            Err(err) => {
                let _ = self.abort_write(session_id).await;
                return Err(err);
            }
        };
        if commit_response.status != FsStatus::Ok {
            let _ = self.abort_write(session_id).await;
            return Err(remote_status_error(
                "write-commit",
                path,
                commit_response.status,
            ));
        }
        if commit_response.session_id != session_id || commit_response.bytes_written != 0 {
            // The commit may already be durable. Abort is therefore only a
            // best-effort session cleanup; callers still receive an explicit
            // protocol mismatch instead of accepting an unrelated response.
            let _ = self.abort_write(session_id).await;
            return Err(ControllerFsError::new(
                "invalid_state",
                "write commit response mismatch",
            ));
        }
        Ok(total_bytes as usize)
    }

    pub async fn mkdir(&mut self, path: &str) -> ControllerFsResult<()> {
        let request_id = self.request_id();
        let payload = encode_mkdir_request(request_id, path)?;
        self.status_rpc(
            payload,
            FsMessageId::MkdirResponse,
            request_id,
            "mkdir",
            path,
        )
        .await
    }

    pub async fn delete(&mut self, path: &str, recursive: bool) -> ControllerFsResult<()> {
        let request_id = self.request_id();
        let payload = encode_delete_request(request_id, path, recursive)?;
        self.status_rpc(
            payload,
            FsMessageId::DeleteResponse,
            request_id,
            "delete",
            path,
        )
        .await
    }

    pub async fn rename(&mut self, from_path: &str, to_path: &str) -> ControllerFsResult<()> {
        let request_id = self.request_id();
        let payload = encode_rename_request(request_id, from_path, to_path)?;
        self.status_rpc(
            payload,
            FsMessageId::RenameResponse,
            request_id,
            "rename",
            from_path,
        )
        .await
    }

    /// Replaces `current_path` with a previously uploaded staging file in one
    /// firmware transaction. Callers must fetch capabilities and require
    /// `conditional-mutations` before uploading so older schema-1 firmware is
    /// rejected without sending an unknown message id.
    pub async fn conditional_replace(
        &mut self,
        operation_id: u32,
        current_path: &str,
        staging_path: &str,
        expected_source_sha256: &[u8; FS_RPC_SHA256_SIZE],
        replacement_sha256: &[u8; FS_RPC_SHA256_SIZE],
    ) -> ControllerFsResult<FsConditionalMutationResult> {
        self.require_negotiated_conditional_mutations()?;
        let request_id = self.request_id();
        let payload = encode_conditional_replace_request(
            request_id,
            operation_id,
            current_path,
            staging_path,
            expected_source_sha256,
            replacement_sha256,
        )?;
        let response = self
            .rpc(payload, FsMessageId::ConditionalReplaceResponse)
            .await?;
        let decoded = decode_conditional_mutation_response(
            &response,
            FsMessageId::ConditionalReplaceResponse,
            request_id,
            operation_id,
        )?;
        checked_conditional_result("replace", current_path, decoded)
    }

    /// Deletes `path` only if its SHA-256 still matches the inspected source.
    /// A missing path is an idempotent success (`already-applied`).
    pub async fn conditional_delete(
        &mut self,
        operation_id: u32,
        path: &str,
        expected_source_sha256: &[u8; FS_RPC_SHA256_SIZE],
    ) -> ControllerFsResult<FsConditionalMutationResult> {
        self.require_negotiated_conditional_mutations()?;
        let request_id = self.request_id();
        let payload = encode_conditional_delete_request(
            request_id,
            operation_id,
            path,
            expected_source_sha256,
        )?;
        let response = self
            .rpc(payload, FsMessageId::ConditionalDeleteResponse)
            .await?;
        let decoded = decode_conditional_mutation_response(
            &response,
            FsMessageId::ConditionalDeleteResponse,
            request_id,
            operation_id,
        )?;
        checked_conditional_result("delete", path, decoded)
    }

    fn require_negotiated_conditional_mutations(&self) -> ControllerFsResult<()> {
        match self.conditional_mutations_supported {
            Some(true) => Ok(()),
            Some(false) => Err(ControllerFsError::new(
                "unsupported_feature",
                "controller firmware does not advertise conditional filesystem mutations",
            )),
            None => Err(ControllerFsError::new(
                "capability_required",
                "filesystem capabilities must be negotiated before a conditional mutation",
            )),
        }
    }

    async fn abort_write(&mut self, session_id: u16) -> ControllerFsResult<()> {
        let request_id = self.request_id();
        let payload = encode_write_abort_request(request_id, session_id)?;
        let response = self
            .write_rpc(payload, FsMessageId::WriteAbortResponse, request_id)
            .await?;
        if response.session_id != session_id || response.bytes_written != 0 {
            return Err(ControllerFsError::new(
                "invalid_state",
                "write abort response mismatch",
            ));
        }
        if matches!(response.status, FsStatus::Ok | FsStatus::InvalidState) {
            return Ok(());
        }
        Err(remote_status_error(
            "write-abort",
            "active write session",
            response.status,
        ))
    }

    async fn rpc(
        &mut self,
        payload: Vec<u8>,
        expected: FsMessageId,
    ) -> ControllerFsResult<Vec<u8>> {
        self.bridge
            .controller_rpc(payload, expected, DEFAULT_RPC_TIMEOUT_MS)
            .await
    }

    async fn rpc_many(
        &mut self,
        requests: &[(Vec<u8>, FsMessageId)],
    ) -> ControllerFsResult<Vec<Vec<u8>>> {
        if self.pipeline_window <= 1 || requests.len() <= 1 {
            let mut responses = Vec::with_capacity(requests.len());
            for (payload, expected) in requests {
                responses.push(self.rpc(payload.clone(), *expected).await?);
            }
            return Ok(responses);
        }

        let batch = requests
            .iter()
            .map(|(payload, expected)| ControllerRpcBatchItem {
                payload: payload.clone(),
                expected_response_id: *expected,
                timeout_ms: DEFAULT_RPC_TIMEOUT_MS,
            })
            .collect::<Vec<_>>();
        self.bridge.controller_rpc_batch(&batch).await
    }

    async fn write_rpc(
        &mut self,
        payload: Vec<u8>,
        expected: FsMessageId,
        request_id: u16,
    ) -> ControllerFsResult<FsWriteResponse> {
        let response = self.rpc(payload, expected).await?;
        decode_write_response(&response, request_id)
    }

    async fn status_rpc(
        &mut self,
        payload: Vec<u8>,
        expected: FsMessageId,
        request_id: u16,
        action: &str,
        path: &str,
    ) -> ControllerFsResult<()> {
        let response = self.rpc(payload, expected).await?;
        let decoded = decode_status_response(&response, request_id)?;
        if decoded.status != FsStatus::Ok {
            return Err(remote_status_error(action, path, decoded.status));
        }
        Ok(())
    }

    fn build_read_batch(
        &mut self,
        path: &str,
        size_bytes: u32,
        offset: u32,
    ) -> ControllerFsResult<Vec<ReadRequest>> {
        let mut batch = Vec::new();
        let mut cursor = offset;
        while cursor < size_bytes && batch.len() < self.pipeline_window {
            let request_id = self.request_id();
            let size = self.chunk_size.min((size_bytes - cursor) as usize);
            let payload = encode_read_request(request_id, path, cursor, size as u16)?;
            batch.push(ReadRequest {
                request_id,
                offset: cursor,
                size,
                payload,
            });
            cursor += size as u32;
        }
        Ok(batch)
    }

    async fn build_write_batch_from_reader(
        &mut self,
        session_id: u16,
        source: &mut tokio::fs::File,
        offset: usize,
        total_size: usize,
    ) -> ControllerFsResult<Vec<WriteRequest>> {
        let mut batch = Vec::new();
        let mut cursor = offset;
        while cursor < total_size && batch.len() < self.pipeline_window {
            let size = self.chunk_size.min(total_size - cursor);
            let mut chunk = vec![0u8; size];
            source.read_exact(&mut chunk).await.map_err(|err| {
                ControllerFsError::new(
                    "local_io_failed",
                    format!("read local transfer file: {err}"),
                )
            })?;
            let request_id = self.request_id();
            let payload =
                encode_write_chunk_request(request_id, session_id, cursor as u32, &chunk)?;
            batch.push(WriteRequest {
                request_id,
                size,
                payload,
            });
            cursor += size;
        }
        Ok(batch)
    }

    fn request_id(&mut self) -> u16 {
        let value = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        if self.next_request_id == 0 {
            self.next_request_id = 1;
        }
        value
    }

    fn write_session_id(&mut self) -> u16 {
        let value = self.next_write_session_id;
        self.next_write_session_id = self.next_write_session_id.wrapping_add(1);
        if self.next_write_session_id == 0 {
            self.next_write_session_id = 1;
        }
        value
    }
}

fn initial_write_session_id(port: u16) -> u16 {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let sequence = u64::from(WRITE_SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed));
    let mixed = stamp
        ^ stamp.rotate_right(17)
        ^ u64::from(std::process::id()).rotate_left(11)
        ^ u64::from(port).rotate_left(29)
        ^ sequence.rotate_left(43);
    let folded = (mixed ^ (mixed >> 16) ^ (mixed >> 32) ^ (mixed >> 48)) as u16;
    folded.max(1)
}

struct ReadRequest {
    request_id: u16,
    offset: u32,
    size: usize,
    payload: Vec<u8>,
}

struct WriteRequest {
    request_id: u16,
    size: usize,
    payload: Vec<u8>,
}

fn encode_stat_request(request_id: u16, path: &str) -> ControllerFsResult<Vec<u8>> {
    frame(FsMessageId::StatRequest, request_id, &encoded_string(path)?)
}

fn encode_capabilities_request(request_id: u16) -> ControllerFsResult<Vec<u8>> {
    frame(FsMessageId::CapabilitiesRequest, request_id, &[])
}

fn encode_list_request(
    request_id: u16,
    path: &str,
    start_index: u16,
    max_entries: u8,
) -> ControllerFsResult<Vec<u8>> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&start_index.to_le_bytes());
    payload.push(max_entries);
    payload.extend_from_slice(&encoded_string(path)?);
    frame(FsMessageId::ListRequest, request_id, &payload)
}

fn encode_read_request(
    request_id: u16,
    path: &str,
    offset: u32,
    size: u16,
) -> ControllerFsResult<Vec<u8>> {
    if usize::from(size) > FS_RPC_MAX_CHUNK_SIZE {
        return Err(ControllerFsError::new(
            "codec_error",
            "read size exceeds filesystem rpc chunk limit",
        ));
    }
    let mut payload = Vec::new();
    payload.extend_from_slice(&offset.to_le_bytes());
    payload.extend_from_slice(&size.to_le_bytes());
    payload.extend_from_slice(&encoded_string(path)?);
    frame(FsMessageId::ReadRequest, request_id, &payload)
}

fn encode_write_begin_request(
    request_id: u16,
    session_id: u16,
    path: &str,
    expected_size: u32,
) -> ControllerFsResult<Vec<u8>> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&session_id.to_le_bytes());
    payload.extend_from_slice(&expected_size.to_le_bytes());
    payload.extend_from_slice(&encoded_string(path)?);
    frame(FsMessageId::WriteBeginRequest, request_id, &payload)
}

fn encode_write_chunk_request(
    request_id: u16,
    session_id: u16,
    offset: u32,
    data: &[u8],
) -> ControllerFsResult<Vec<u8>> {
    if data.len() > FS_RPC_MAX_CHUNK_SIZE {
        return Err(ControllerFsError::new(
            "codec_error",
            "write chunk exceeds filesystem rpc chunk limit",
        ));
    }
    let mut payload = Vec::new();
    payload.extend_from_slice(&session_id.to_le_bytes());
    payload.extend_from_slice(&offset.to_le_bytes());
    payload.extend_from_slice(&(data.len() as u16).to_le_bytes());
    payload.extend_from_slice(data);
    frame(FsMessageId::WriteChunkRequest, request_id, &payload)
}

fn encode_write_commit_request(request_id: u16, session_id: u16) -> ControllerFsResult<Vec<u8>> {
    frame(
        FsMessageId::WriteCommitRequest,
        request_id,
        &session_id.to_le_bytes(),
    )
}

fn encode_write_abort_request(request_id: u16, session_id: u16) -> ControllerFsResult<Vec<u8>> {
    frame(
        FsMessageId::WriteAbortRequest,
        request_id,
        &session_id.to_le_bytes(),
    )
}

fn encode_mkdir_request(request_id: u16, path: &str) -> ControllerFsResult<Vec<u8>> {
    frame(
        FsMessageId::MkdirRequest,
        request_id,
        &encoded_string(path)?,
    )
}

fn encode_delete_request(
    request_id: u16,
    path: &str,
    recursive: bool,
) -> ControllerFsResult<Vec<u8>> {
    let mut payload = vec![if recursive { 1 } else { 0 }];
    payload.extend_from_slice(&encoded_string(path)?);
    frame(FsMessageId::DeleteRequest, request_id, &payload)
}

fn encode_rename_request(
    request_id: u16,
    from_path: &str,
    to_path: &str,
) -> ControllerFsResult<Vec<u8>> {
    let mut payload = encoded_string(from_path)?;
    payload.extend_from_slice(&encoded_string(to_path)?);
    frame(FsMessageId::RenameRequest, request_id, &payload)
}

fn encode_conditional_replace_request(
    request_id: u16,
    operation_id: u32,
    current_path: &str,
    staging_path: &str,
    expected_source_sha256: &[u8; FS_RPC_SHA256_SIZE],
    replacement_sha256: &[u8; FS_RPC_SHA256_SIZE],
) -> ControllerFsResult<Vec<u8>> {
    let mut payload = Vec::with_capacity(
        4 + (2 * FS_RPC_SHA256_SIZE) + current_path.len() + staging_path.len() + 2,
    );
    payload.extend_from_slice(&operation_id.to_le_bytes());
    payload.extend_from_slice(expected_source_sha256);
    payload.extend_from_slice(replacement_sha256);
    payload.extend_from_slice(&encoded_string(current_path)?);
    payload.extend_from_slice(&encoded_string(staging_path)?);
    frame(FsMessageId::ConditionalReplaceRequest, request_id, &payload)
}

fn encode_conditional_delete_request(
    request_id: u16,
    operation_id: u32,
    path: &str,
    expected_source_sha256: &[u8; FS_RPC_SHA256_SIZE],
) -> ControllerFsResult<Vec<u8>> {
    let mut payload = Vec::with_capacity(4 + FS_RPC_SHA256_SIZE + path.len() + 1);
    payload.extend_from_slice(&operation_id.to_le_bytes());
    payload.extend_from_slice(expected_source_sha256);
    payload.extend_from_slice(&encoded_string(path)?);
    frame(FsMessageId::ConditionalDeleteRequest, request_id, &payload)
}

fn frame(message_id: FsMessageId, request_id: u16, payload: &[u8]) -> ControllerFsResult<Vec<u8>> {
    let mut out = Vec::new();
    out.push(message_id as u8);
    out.extend_from_slice(&encoded_string(message_name(message_id))?);
    out.push(FS_RPC_SCHEMA);
    out.extend_from_slice(&request_id.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

fn decode_frame(data: &[u8]) -> ControllerFsResult<FsFrame> {
    let mut reader = Reader::new(data);
    let message_id = FsMessageId::from_u8(reader.u8()?)?;
    let name_len = reader.u8()? as usize;
    let name = reader.bytes(name_len)?;
    if name != message_name(message_id).as_bytes() {
        return Err(ControllerFsError::new(
            "codec_error",
            format!(
                "filesystem rpc message name does not match id 0x{:02x}",
                message_id as u8
            ),
        ));
    }
    let schema = reader.u8()?;
    let request_id = reader.u16()?;
    Ok(FsFrame {
        message_id,
        schema,
        request_id,
        payload: reader.remaining_bytes(),
    })
}

fn decode_capabilities_response(
    data: &[u8],
    expected_request_id: u16,
) -> ControllerFsResult<FsCapabilities> {
    let frame =
        checked_response_frame(data, FsMessageId::CapabilitiesResponse, expected_request_id)?;
    let mut reader = Reader::new(&frame.payload);
    let status = FsStatus::from_u8(reader.u8()?)?;
    if status != FsStatus::Ok {
        let response = FsCapabilities {
            status,
            rpc_schema: 0,
            max_chunk_size: 0,
            response_buffer_size: 0,
            max_list_entries: 0,
            max_path_length: 0,
            feature_flags: 0,
        };
        reader.expect_empty()?;
        return Ok(response);
    }
    let response = FsCapabilities {
        status,
        rpc_schema: reader.u8()?,
        max_chunk_size: reader.u16()?,
        response_buffer_size: reader.u16()?,
        max_list_entries: reader.u8()?,
        max_path_length: reader.u16()?,
        feature_flags: reader.u32()?,
    };
    reader.expect_empty()?;
    Ok(response)
}

fn decode_stat_response(data: &[u8], expected_request_id: u16) -> ControllerFsResult<FsStat> {
    let frame = checked_response_frame(data, FsMessageId::StatResponse, expected_request_id)?;
    let mut reader = Reader::new(&frame.payload);
    let status = FsStatus::from_u8(reader.u8()?)?;
    if status != FsStatus::Ok {
        reader.expect_empty()?;
        return Ok(FsStat {
            status,
            file_type: FsFileType::Missing,
            size_bytes: 0,
        });
    }
    let response = FsStat {
        status,
        file_type: FsFileType::from_u8(reader.u8()?)?,
        size_bytes: reader.u32()?,
    };
    reader.expect_empty()?;
    Ok(response)
}

fn decode_list_response(data: &[u8], expected_request_id: u16) -> ControllerFsResult<FsListPage> {
    let frame = checked_response_frame(data, FsMessageId::ListResponse, expected_request_id)?;
    let mut reader = Reader::new(&frame.payload);
    let status = FsStatus::from_u8(reader.u8()?)?;
    if status != FsStatus::Ok {
        reader.expect_empty()?;
        return Ok(FsListPage {
            status,
            start_index: 0,
            has_more: false,
            entries: Vec::new(),
        });
    }
    let start_index = reader.u16()?;
    let entry_count = reader.u8()?;
    let has_more = reader.bool()?;
    if entry_count > FS_RPC_MAX_LIST_ENTRIES {
        return Err(ControllerFsError::new(
            "codec_error",
            "filesystem rpc list response entry count exceeds limit",
        ));
    }
    let mut entries = Vec::new();
    for _ in 0..entry_count {
        entries.push(FsListEntry {
            name: reader.string()?,
            file_type: FsFileType::from_u8(reader.u8()?)?,
            size_bytes: reader.u32()?,
            name_truncated: reader.bool()?,
        });
    }
    reader.expect_empty()?;
    Ok(FsListPage {
        status,
        start_index,
        has_more,
        entries,
    })
}

fn decode_read_response(
    data: &[u8],
    expected_request_id: u16,
    expected_offset: u32,
) -> ControllerFsResult<FsReadResponse> {
    let frame = checked_response_frame(data, FsMessageId::ReadResponse, expected_request_id)?;
    let mut reader = Reader::new(&frame.payload);
    let status = FsStatus::from_u8(reader.u8()?)?;
    if status != FsStatus::Ok {
        reader.expect_empty()?;
        return Ok(FsReadResponse {
            status,
            data: Vec::new(),
        });
    }
    let offset = reader.u32()?;
    if offset != expected_offset {
        return Err(ControllerFsError::new(
            "invalid_state",
            format!("read response offset mismatch: expected {expected_offset}, got {offset}"),
        ));
    }
    let size = reader.u16()? as usize;
    let data = reader.bytes(size)?.to_vec();
    reader.expect_empty()?;
    Ok(FsReadResponse { status, data })
}

fn decode_write_response(
    data: &[u8],
    expected_request_id: u16,
) -> ControllerFsResult<FsWriteResponse> {
    let frame = decode_frame(data)?;
    if !matches!(
        frame.message_id,
        FsMessageId::WriteBeginResponse
            | FsMessageId::WriteChunkResponse
            | FsMessageId::WriteCommitResponse
            | FsMessageId::WriteAbortResponse
    ) {
        return Err(ControllerFsError::new(
            "codec_error",
            "not a write response",
        ));
    }
    if frame.schema != FS_RPC_SCHEMA {
        return Err(ControllerFsError::new(
            "codec_error",
            format!("unsupported filesystem rpc schema: {}", frame.schema),
        ));
    }
    if frame.request_id != expected_request_id {
        return Err(ControllerFsError::new(
            "invalid_state",
            format!(
                "request id mismatch: expected {}, got {}",
                expected_request_id, frame.request_id
            ),
        ));
    }
    let mut reader = Reader::new(&frame.payload);
    let response = FsWriteResponse {
        status: FsStatus::from_u8(reader.u8()?)?,
        session_id: reader.u16()?,
        bytes_written: reader.u16()?,
    };
    reader.expect_empty()?;
    Ok(response)
}

fn decode_status_response(
    data: &[u8],
    expected_request_id: u16,
) -> ControllerFsResult<FsStatusResponse> {
    let frame = decode_frame(data)?;
    if !matches!(
        frame.message_id,
        FsMessageId::MkdirResponse | FsMessageId::DeleteResponse | FsMessageId::RenameResponse
    ) {
        return Err(ControllerFsError::new(
            "codec_error",
            "not a status response",
        ));
    }
    if frame.schema != FS_RPC_SCHEMA {
        return Err(ControllerFsError::new(
            "codec_error",
            format!("unsupported filesystem rpc schema: {}", frame.schema),
        ));
    }
    if frame.request_id != expected_request_id {
        return Err(ControllerFsError::new(
            "invalid_state",
            format!(
                "request id mismatch: expected {}, got {}",
                expected_request_id, frame.request_id
            ),
        ));
    }
    let mut reader = Reader::new(&frame.payload);
    let response = FsStatusResponse {
        status: FsStatus::from_u8(reader.u8()?)?,
    };
    reader.expect_empty()?;
    Ok(response)
}

fn decode_conditional_mutation_response(
    data: &[u8],
    expected_message_id: FsMessageId,
    expected_request_id: u16,
    expected_operation_id: u32,
) -> ControllerFsResult<FsConditionalMutationResponse> {
    if !matches!(
        expected_message_id,
        FsMessageId::ConditionalReplaceResponse | FsMessageId::ConditionalDeleteResponse
    ) {
        return Err(ControllerFsError::new(
            "codec_error",
            "invalid expected conditional mutation response id",
        ));
    }
    let frame = checked_response_frame(data, expected_message_id, expected_request_id)?;
    let mut reader = Reader::new(&frame.payload);
    let status = FsStatus::from_u8(reader.u8()?)?;
    let outcome = FsConditionalMutationOutcome::from_u8(reader.u8()?)?;
    let subject = FsConditionalMutationSubject::from_u8(reader.u8()?)?;
    let operation_id = reader.u32()?;
    let mut observed_sha256 = [0u8; FS_RPC_SHA256_SIZE];
    observed_sha256.copy_from_slice(reader.bytes(FS_RPC_SHA256_SIZE)?);
    reader.expect_empty()?;
    if operation_id != expected_operation_id {
        return Err(ControllerFsError::new(
            "invalid_state",
            format!(
                "conditional mutation operation id mismatch: expected {expected_operation_id}, got {operation_id}"
            ),
        ));
    }
    Ok(FsConditionalMutationResponse {
        status,
        outcome,
        subject,
        operation_id,
        observed_sha256,
    })
}

fn checked_response_frame(
    data: &[u8],
    expected: FsMessageId,
    expected_request_id: u16,
) -> ControllerFsResult<FsFrame> {
    let frame = response_frame(data, expected)?;
    if frame.request_id != expected_request_id {
        return Err(ControllerFsError::new(
            "invalid_state",
            format!(
                "request id mismatch: expected {}, got {}",
                expected_request_id, frame.request_id
            ),
        ));
    }
    Ok(frame)
}

fn response_frame(data: &[u8], expected: FsMessageId) -> ControllerFsResult<FsFrame> {
    let frame = decode_frame(data)?;
    if frame.message_id != expected {
        return Err(ControllerFsError::new(
            "codec_error",
            format!(
                "expected filesystem response 0x{:02x}, got 0x{:02x}",
                expected as u8, frame.message_id as u8
            ),
        ));
    }
    if frame.schema != FS_RPC_SCHEMA {
        return Err(ControllerFsError::new(
            "codec_error",
            format!("unsupported filesystem rpc schema: {}", frame.schema),
        ));
    }
    Ok(frame)
}

fn encoded_string(value: &str) -> ControllerFsResult<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() > u8::MAX as usize {
        return Err(ControllerFsError::new(
            "codec_error",
            "filesystem rpc string exceeds 255 bytes",
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    Ok(out)
}

fn remote_status_error(action: &str, path: &str, status: FsStatus) -> ControllerFsError {
    ControllerFsError::new(
        "remote_status",
        format!("{action} failed for {path}: {}", status.label()),
    )
}

fn checked_conditional_result(
    action: &str,
    path: &str,
    response: FsConditionalMutationResponse,
) -> ControllerFsResult<FsConditionalMutationResult> {
    if response.status == FsStatus::PreconditionFailed {
        if response.outcome != FsConditionalMutationOutcome::None
            || response.subject == FsConditionalMutationSubject::None
        {
            return Err(ControllerFsError::new(
                "protocol_error",
                format!(
                    "conditional {action} returned an invalid precondition response for {path}"
                ),
            ));
        }
        return Err(ControllerFsError::new(
            "precondition_failed",
            format!(
                "conditional {action} rejected for {path}: {} SHA-256 changed to {}",
                response.subject.label(),
                sha256_hex(&response.observed_sha256)
            ),
        ));
    }
    if response.status == FsStatus::Unsupported {
        return Err(ControllerFsError::new(
            "unsupported_feature",
            "controller firmware does not support conditional filesystem mutations",
        ));
    }
    // A journaled mutation can have reached its canonical state and still
    // report a storage/cleanup failure. Preserve that uncertainty explicitly
    // so callers replay the same operation id and let firmware recovery finish
    // before reconciling the canonical path.
    if response.status == FsStatus::StorageError {
        return Err(ControllerFsError::new(
            "conditional_storage_error",
            format!("conditional {action} reported a storage error for {path}"),
        ));
    }
    if response.status == FsStatus::InvalidState {
        return Err(ControllerFsError::new(
            "conditional_invalid_state",
            format!("conditional {action} reported incomplete transaction state for {path}"),
        ));
    }
    if response.status != FsStatus::Ok {
        return Err(remote_status_error(action, path, response.status));
    }
    if response.outcome == FsConditionalMutationOutcome::None
        || response.subject != FsConditionalMutationSubject::None
    {
        return Err(ControllerFsError::new(
            "protocol_error",
            format!("conditional {action} returned an invalid success response for {path}"),
        ));
    }
    Ok(FsConditionalMutationResult {
        outcome: response.outcome,
        operation_id: response.operation_id,
    })
}

fn sha256_hex(digest: &[u8; FS_RPC_SHA256_SIZE]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(FS_RPC_SHA256_SIZE * 2);
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn message_name(message_id: FsMessageId) -> &'static str {
    match message_id {
        FsMessageId::StatRequest => "FsStatRequest",
        FsMessageId::StatResponse => "FsStatResponse",
        FsMessageId::ListRequest => "FsListRequest",
        FsMessageId::ListResponse => "FsListResponse",
        FsMessageId::ReadRequest => "FsReadRequest",
        FsMessageId::ReadResponse => "FsReadResponse",
        FsMessageId::WriteBeginRequest => "FsWriteBeginRequest",
        FsMessageId::WriteBeginResponse => "FsWriteBeginResponse",
        FsMessageId::WriteChunkRequest => "FsWriteChunkRequest",
        FsMessageId::WriteChunkResponse => "FsWriteChunkResponse",
        FsMessageId::WriteCommitRequest => "FsWriteCommitRequest",
        FsMessageId::WriteCommitResponse => "FsWriteCommitResponse",
        FsMessageId::WriteAbortRequest => "FsWriteAbortRequest",
        FsMessageId::WriteAbortResponse => "FsWriteAbortResponse",
        FsMessageId::ErrorResponse => "FsErrorResponse",
        FsMessageId::MkdirRequest => "FsMkdirRequest",
        FsMessageId::MkdirResponse => "FsMkdirResponse",
        FsMessageId::DeleteRequest => "FsDeleteRequest",
        FsMessageId::DeleteResponse => "FsDeleteResponse",
        FsMessageId::RenameRequest => "FsRenameRequest",
        FsMessageId::RenameResponse => "FsRenameResponse",
        FsMessageId::CapabilitiesRequest => "FsCapabilitiesRequest",
        FsMessageId::CapabilitiesResponse => "FsCapabilitiesResponse",
        FsMessageId::ConditionalReplaceRequest => "FsConditionalReplaceRequest",
        FsMessageId::ConditionalReplaceResponse => "FsConditionalReplaceResponse",
        FsMessageId::ConditionalDeleteRequest => "FsConditionalDeleteRequest",
        FsMessageId::ConditionalDeleteResponse => "FsConditionalDeleteResponse",
    }
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn u8(&mut self) -> ControllerFsResult<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn bool(&mut self) -> ControllerFsResult<bool> {
        Ok(self.u8()? != 0)
    }

    fn u16(&mut self) -> ControllerFsResult<u16> {
        let data = self.bytes(2)?;
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }

    fn u32(&mut self) -> ControllerFsResult<u32> {
        let data = self.bytes(4)?;
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    fn string(&mut self) -> ControllerFsResult<String> {
        let len = self.u8()? as usize;
        let data = self.bytes(len)?;
        String::from_utf8(data.to_vec()).map_err(|err| {
            ControllerFsError::new(
                "codec_error",
                format!("filesystem rpc string is not valid utf-8: {err}"),
            )
        })
    }

    fn bytes(&mut self, size: usize) -> ControllerFsResult<&'a [u8]> {
        let end = self.offset.checked_add(size).ok_or_else(|| {
            ControllerFsError::new("codec_error", "filesystem rpc payload offset overflow")
        })?;
        if end > self.data.len() {
            return Err(ControllerFsError::new(
                "codec_error",
                "truncated filesystem rpc payload",
            ));
        }
        let out = &self.data[self.offset..end];
        self.offset = end;
        Ok(out)
    }

    fn remaining_bytes(&mut self) -> Vec<u8> {
        let out = self.data[self.offset..].to_vec();
        self.offset = self.data.len();
        out
    }

    fn expect_empty(&self) -> ControllerFsResult<()> {
        if self.offset != self.data.len() {
            return Err(ControllerFsError::new(
                "codec_error",
                "trailing filesystem rpc payload bytes",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn encodes_stat_request_wire_format() {
        let encoded = encode_stat_request(7, "/midi-studio/tmp").unwrap();
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.message_id, FsMessageId::StatRequest);
        assert_eq!(frame.schema, FS_RPC_SCHEMA);
        assert_eq!(frame.request_id, 7);
        assert_eq!(frame.payload[0], 16);
        assert_eq!(&frame.payload[1..], b"/midi-studio/tmp");
    }

    #[test]
    fn decodes_capabilities_response() {
        let payload = capabilities_response(3);
        let decoded = decode_capabilities_response(&payload, 3).unwrap();
        assert_eq!(decoded.status, FsStatus::Ok);
        assert_eq!(decoded.rpc_schema, 1);
        assert_eq!(decoded.max_chunk_size, FS_RPC_MAX_CHUNK_SIZE as u16);
        assert_eq!(decoded.max_list_entries, FS_RPC_MAX_LIST_ENTRIES);
        assert!(decoded.supports_conditional_mutations());
        decoded.require_conditional_mutations().unwrap();

        let mut legacy = decoded.clone();
        legacy.feature_flags &= !FS_RPC_FEATURE_CONDITIONAL_MUTATIONS;
        let error = legacy.require_conditional_mutations().unwrap_err();
        assert_eq!(error.kind, "unsupported_feature");

        let mut wrong_schema = decoded;
        wrong_schema.rpc_schema = FS_RPC_SCHEMA + 1;
        assert!(!wrong_schema.supports_conditional_mutations());
    }

    #[test]
    fn capabilities_response_must_match_request_id() {
        let payload = capabilities_response(3);
        let error = decode_capabilities_response(&payload, 4).unwrap_err();
        assert_eq!(error.kind, "invalid_state");
    }

    #[test]
    fn frame_rejects_a_message_name_that_disagrees_with_its_id() {
        let mut payload = status_response(FsMessageId::DeleteResponse, 3);
        payload[2] ^= 1;
        let error = decode_frame(&payload).unwrap_err();
        assert_eq!(error.kind, "codec_error");
    }

    #[test]
    fn non_ok_responses_reject_trailing_payload() {
        let payload = frame(
            FsMessageId::StatResponse,
            3,
            &[FsStatus::NotFound as u8, 0xff],
        )
        .unwrap();
        let error = decode_stat_response(&payload, 3).unwrap_err();
        assert_eq!(error.kind, "codec_error");
    }

    #[test]
    fn list_response_preserves_the_echoed_page_index() {
        let mut payload = vec![FsStatus::Ok as u8];
        payload.extend_from_slice(&17u16.to_le_bytes());
        payload.extend_from_slice(&[0, 0]);
        let encoded = frame(FsMessageId::ListResponse, 3, &payload).unwrap();
        let decoded = decode_list_response(&encoded, 3).unwrap();
        assert_eq!(decoded.start_index, 17);
    }

    #[test]
    fn conditional_response_rejects_inconsistent_success_metadata() {
        let response = FsConditionalMutationResponse {
            status: FsStatus::Ok,
            outcome: FsConditionalMutationOutcome::Applied,
            subject: FsConditionalMutationSubject::Source,
            operation_id: 7,
            observed_sha256: [0; FS_RPC_SHA256_SIZE],
        };
        let error = checked_conditional_result("delete", "/a.mssp", response).unwrap_err();
        assert_eq!(error.kind, "protocol_error");
    }

    #[test]
    fn conditional_storage_failure_is_exposed_as_retryable_uncertainty() {
        let response = FsConditionalMutationResponse {
            status: FsStatus::StorageError,
            outcome: FsConditionalMutationOutcome::None,
            subject: FsConditionalMutationSubject::None,
            operation_id: 7,
            observed_sha256: [0; FS_RPC_SHA256_SIZE],
        };
        let error = checked_conditional_result("replace", "/a.mssp", response).unwrap_err();
        assert_eq!(error.kind, "conditional_storage_error");
    }

    #[test]
    fn encodes_conditional_mutation_wire_formats() {
        let expected = [0x11; FS_RPC_SHA256_SIZE];
        let replacement = [0x22; FS_RPC_SHA256_SIZE];
        let encoded = encode_conditional_replace_request(
            8,
            0x1234_5678,
            "library/step-presets/a.mssp",
            "tmp/a.stage",
            &expected,
            &replacement,
        )
        .unwrap();
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.message_id, FsMessageId::ConditionalReplaceRequest);
        assert_eq!(frame.request_id, 8);
        let mut reader = Reader::new(&frame.payload);
        assert_eq!(reader.u32().unwrap(), 0x1234_5678);
        assert_eq!(reader.bytes(FS_RPC_SHA256_SIZE).unwrap(), expected);
        assert_eq!(reader.bytes(FS_RPC_SHA256_SIZE).unwrap(), replacement);
        assert_eq!(reader.string().unwrap(), "library/step-presets/a.mssp");
        assert_eq!(reader.string().unwrap(), "tmp/a.stage");
        reader.expect_empty().unwrap();

        let encoded = encode_conditional_delete_request(
            9,
            0x8765_4321,
            "library/step-presets/a.mssp",
            &expected,
        )
        .unwrap();
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.message_id, FsMessageId::ConditionalDeleteRequest);
        let mut reader = Reader::new(&frame.payload);
        assert_eq!(reader.u32().unwrap(), 0x8765_4321);
        assert_eq!(reader.bytes(FS_RPC_SHA256_SIZE).unwrap(), expected);
        assert_eq!(reader.string().unwrap(), "library/step-presets/a.mssp");
        reader.expect_empty().unwrap();
    }

    #[test]
    fn binary_batch_maps_out_of_order_tokens() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let first = read_binary_request(&mut stream).await;
                let second = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    second.token,
                    &status_response(FsMessageId::DeleteResponse, 2),
                )
                .await;
                write_binary_response(
                    &mut stream,
                    first.token,
                    &status_response(FsMessageId::MkdirResponse, 1),
                )
                .await;
                (first, second)
            });

            let mut client = BridgeBinaryClient::new(port);
            let responses = client
                .controller_rpc_batch(&[
                    ControllerRpcBatchItem {
                        payload: encode_mkdir_request(1, "tmp/a").unwrap(),
                        expected_response_id: FsMessageId::MkdirResponse,
                        timeout_ms: DEFAULT_RPC_TIMEOUT_MS,
                    },
                    ControllerRpcBatchItem {
                        payload: encode_delete_request(2, "tmp/b", false).unwrap(),
                        expected_response_id: FsMessageId::DeleteResponse,
                        timeout_ms: DEFAULT_RPC_TIMEOUT_MS,
                    },
                ])
                .await
                .unwrap();

            assert_eq!(
                decode_status_response(&responses[0], 1).unwrap().status,
                FsStatus::Ok
            );
            assert_eq!(
                decode_status_response(&responses[1], 2).unwrap().status,
                FsStatus::Ok
            );
            let (first, second) = server.await.unwrap();
            assert_eq!(first.token, 1);
            assert_eq!(second.token, 2);
        });
    }

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
                .controller_rpc(vec![0x01], FsMessageId::StatResponse, 1)
                .await
                .unwrap_err();
            assert_eq!(first_error.kind, "bridge_timeout");

            let response = client
                .controller_rpc(vec![0x02], FsMessageId::StatResponse, 50)
                .await
                .unwrap();
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
                .controller_rpc(vec![0x01], FsMessageId::StatResponse, 50)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "bridge_unavailable");
            assert!(error.message.contains("too large"));
            server.await.unwrap();
        });
    }

    #[test]
    fn client_reads_file_with_pipeline() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let stat = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    stat.token,
                    &stat_response(1, FsFileType::File, 10),
                )
                .await;
                let first = read_binary_request(&mut stream).await;
                let second = read_binary_request(&mut stream).await;
                let third = read_binary_request(&mut stream).await;
                write_binary_response(&mut stream, first.token, &read_response(2, 0, b"abcd"))
                    .await;
                write_binary_response(&mut stream, second.token, &read_response(3, 4, b"efgh"))
                    .await;
                write_binary_response(&mut stream, third.token, &read_response(4, 8, b"ij")).await;
                (first.payload, second.payload, third.payload)
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge)
                .with_chunk_size(4)
                .unwrap()
                .with_pipeline_window(3)
                .unwrap();
            let destination = temp_test_path("controller-fs-pull.bin");
            let _ = std::fs::remove_file(&destination);
            let bytes = client
                .pull_file_to_path_with_progress("projects/a.bin", &destination, |_, _| {})
                .await
                .unwrap();

            assert_eq!(bytes, 10);
            assert_eq!(std::fs::read(&destination).unwrap(), b"abcdefghij");
            let _ = std::fs::remove_file(&destination);
            let (first, second, third) = server.await.unwrap();
            assert_eq!(decode_frame(&first).unwrap().request_id, 2);
            assert_eq!(decode_frame(&second).unwrap().request_id, 3);
            assert_eq!(decode_frame(&third).unwrap().request_id, 4);
        });
    }

    #[test]
    fn client_rejects_oversized_pull_before_creating_destination() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let stat = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    stat.token,
                    &stat_response(1, FsFileType::File, 11),
                )
                .await;
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            let destination = temp_test_path("controller-fs-pull-limit.bin");
            let _ = std::fs::remove_file(&destination);
            let error = client
                .pull_file_to_path_with_progress_limit(
                    "projects/a.bin",
                    &destination,
                    10,
                    |_, _| {},
                )
                .await
                .unwrap_err();
            assert_eq!(error.kind, "too_large");
            assert!(!destination.exists());
            server.await.unwrap();
        });
    }

    #[test]
    fn client_writes_file_with_pipeline() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let begin = read_binary_request(&mut stream).await;
                let session_id = write_begin_session_id(&begin.payload);
                write_binary_response(
                    &mut stream,
                    begin.token,
                    &write_response(FsMessageId::WriteBeginResponse, 2, session_id, 0),
                )
                .await;
                let first = read_binary_request(&mut stream).await;
                let second = read_binary_request(&mut stream).await;
                let third = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    first.token,
                    &write_response(FsMessageId::WriteChunkResponse, 3, session_id, 4),
                )
                .await;
                write_binary_response(
                    &mut stream,
                    second.token,
                    &write_response(FsMessageId::WriteChunkResponse, 4, session_id, 4),
                )
                .await;
                write_binary_response(
                    &mut stream,
                    third.token,
                    &write_response(FsMessageId::WriteChunkResponse, 5, session_id, 1),
                )
                .await;
                let commit = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    commit.token,
                    &write_response(FsMessageId::WriteCommitResponse, 6, session_id, 0),
                )
                .await;
                (first.payload, second.payload, third.payload, commit.payload)
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge)
                .with_chunk_size(4)
                .unwrap()
                .with_pipeline_window(3)
                .unwrap();
            let source = temp_test_path("controller-fs-push.bin");
            std::fs::write(&source, b"abcdefghi").unwrap();
            client
                .push_file_from_path_with_progress("projects/a.bin", &source, |_, _| {})
                .await
                .unwrap();
            let _ = std::fs::remove_file(&source);

            let (first, second, third, commit) = server.await.unwrap();
            assert_eq!(write_chunk_offset_and_size(&first), (0, 4));
            assert_eq!(write_chunk_offset_and_size(&second), (4, 4));
            assert_eq!(write_chunk_offset_and_size(&third), (8, 1));
            assert_eq!(
                decode_frame(&commit).unwrap().message_id,
                FsMessageId::WriteCommitRequest
            );
        });
    }

    #[test]
    fn client_writes_empty_file_without_chunks() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let begin = read_binary_request(&mut stream).await;
                let session_id = write_begin_session_id(&begin.payload);
                write_binary_response(
                    &mut stream,
                    begin.token,
                    &write_response(FsMessageId::WriteBeginResponse, 2, session_id, 0),
                )
                .await;
                let commit = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    commit.token,
                    &write_response(FsMessageId::WriteCommitResponse, 3, session_id, 0),
                )
                .await;
                (begin.payload, commit.payload)
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge)
                .with_chunk_size(4)
                .unwrap()
                .with_pipeline_window(3)
                .unwrap();
            let source = temp_test_path("controller-fs-empty-push.bin");
            std::fs::write(&source, b"").unwrap();
            client
                .push_file_from_path_with_progress("projects/empty.bin", &source, |_, _| {})
                .await
                .unwrap();
            let _ = std::fs::remove_file(&source);

            let (begin, commit) = server.await.unwrap();
            assert_eq!(
                decode_frame(&begin).unwrap().message_id,
                FsMessageId::WriteBeginRequest
            );
            assert_eq!(
                decode_frame(&commit).unwrap().message_id,
                FsMessageId::WriteCommitRequest
            );
        });
    }

    #[test]
    fn client_conditional_replace_uses_one_post_negotiation_rpc_and_checks_echoes() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let capabilities = read_binary_request(&mut stream).await;
                write_binary_response(&mut stream, capabilities.token, &capabilities_response(1))
                    .await;
                let request = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    request.token,
                    &conditional_response(
                        FsMessageId::ConditionalReplaceResponse,
                        2,
                        FsStatus::Ok,
                        FsConditionalMutationOutcome::Applied,
                        FsConditionalMutationSubject::None,
                        0x1234_5678,
                        &[0; FS_RPC_SHA256_SIZE],
                    ),
                )
                .await;
                request.payload
            });

            let expected = [0x11; FS_RPC_SHA256_SIZE];
            let replacement = [0x22; FS_RPC_SHA256_SIZE];
            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            client.capabilities().await.unwrap();
            let result = client
                .conditional_replace(
                    0x1234_5678,
                    "library/step-presets/a.mssp",
                    "tmp/a.stage",
                    &expected,
                    &replacement,
                )
                .await
                .unwrap();
            assert_eq!(result.outcome, FsConditionalMutationOutcome::Applied);
            assert_eq!(result.operation_id, 0x1234_5678);

            let payload = server.await.unwrap();
            let frame = decode_frame(&payload).unwrap();
            assert_eq!(frame.message_id, FsMessageId::ConditionalReplaceRequest);
            assert_eq!(frame.request_id, 2);
        });
    }

    #[test]
    fn client_conditional_delete_surfaces_precondition_digest() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let observed = [0xab; FS_RPC_SHA256_SIZE];
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let capabilities = read_binary_request(&mut stream).await;
                write_binary_response(&mut stream, capabilities.token, &capabilities_response(1))
                    .await;
                let request = read_binary_request(&mut stream).await;
                write_binary_response(
                    &mut stream,
                    request.token,
                    &conditional_response(
                        FsMessageId::ConditionalDeleteResponse,
                        2,
                        FsStatus::PreconditionFailed,
                        FsConditionalMutationOutcome::None,
                        FsConditionalMutationSubject::Source,
                        42,
                        &observed,
                    ),
                )
                .await;
            });

            let expected = [0x11; FS_RPC_SHA256_SIZE];
            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            client.capabilities().await.unwrap();
            let error = client
                .conditional_delete(42, "library/step-presets/a.mssp", &expected)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "precondition_failed");
            assert!(error.message.contains(&"ab".repeat(FS_RPC_SHA256_SIZE)));
            server.await.unwrap();
        });
    }

    #[test]
    fn client_refuses_conditional_mutation_without_capability_negotiation() {
        run_async(async {
            let expected = [0x11; FS_RPC_SHA256_SIZE];
            let bridge = BridgeBinaryClient::new(1);
            let mut client = ControllerFsClient::new(bridge);
            let error = client
                .conditional_delete(1, "library/step-presets/a.mssp", &expected)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "capability_required");
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

    fn temp_test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ms-manager-{name}-{}", std::process::id()))
    }

    struct CapturedBinaryRequest {
        token: u16,
        payload: Vec<u8>,
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
        CapturedBinaryRequest { token, payload }
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

    fn capabilities_response(request_id: u16) -> Vec<u8> {
        let mut payload = vec![FsStatus::Ok as u8, FS_RPC_SCHEMA];
        payload.extend_from_slice(&(FS_RPC_MAX_CHUNK_SIZE as u16).to_le_bytes());
        payload.extend_from_slice(&32_512u16.to_le_bytes());
        payload.push(FS_RPC_MAX_LIST_ENTRIES);
        payload.extend_from_slice(&192u16.to_le_bytes());
        payload.extend_from_slice(&(7u32 | FS_RPC_FEATURE_CONDITIONAL_MUTATIONS).to_le_bytes());
        frame(FsMessageId::CapabilitiesResponse, request_id, &payload).unwrap()
    }

    fn conditional_response(
        message_id: FsMessageId,
        request_id: u16,
        status: FsStatus,
        outcome: FsConditionalMutationOutcome,
        subject: FsConditionalMutationSubject,
        operation_id: u32,
        observed_sha256: &[u8; FS_RPC_SHA256_SIZE],
    ) -> Vec<u8> {
        let mut payload = vec![status as u8, outcome as u8, subject as u8];
        payload.extend_from_slice(&operation_id.to_le_bytes());
        payload.extend_from_slice(observed_sha256);
        frame(message_id, request_id, &payload).unwrap()
    }

    fn stat_response(request_id: u16, file_type: FsFileType, size: u32) -> Vec<u8> {
        let mut payload = vec![FsStatus::Ok as u8, file_type as u8];
        payload.extend_from_slice(&size.to_le_bytes());
        frame(FsMessageId::StatResponse, request_id, &payload).unwrap()
    }

    fn read_response(request_id: u16, offset: u32, data: &[u8]) -> Vec<u8> {
        let mut payload = vec![FsStatus::Ok as u8];
        payload.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(&(data.len() as u16).to_le_bytes());
        payload.extend_from_slice(data);
        frame(FsMessageId::ReadResponse, request_id, &payload).unwrap()
    }

    fn write_response(
        message_id: FsMessageId,
        request_id: u16,
        session_id: u16,
        written: u16,
    ) -> Vec<u8> {
        let mut payload = vec![FsStatus::Ok as u8];
        payload.extend_from_slice(&session_id.to_le_bytes());
        payload.extend_from_slice(&written.to_le_bytes());
        frame(message_id, request_id, &payload).unwrap()
    }

    fn status_response(message_id: FsMessageId, request_id: u16) -> Vec<u8> {
        frame(message_id, request_id, &[FsStatus::Ok as u8]).unwrap()
    }

    fn write_chunk_offset_and_size(payload: &[u8]) -> (u32, u16) {
        let frame = decode_frame(payload).unwrap();
        assert_eq!(frame.message_id, FsMessageId::WriteChunkRequest);
        let mut reader = Reader::new(&frame.payload);
        let _session_id = reader.u16().unwrap();
        let offset = reader.u32().unwrap();
        let size = reader.u16().unwrap();
        (offset, size)
    }

    fn write_begin_session_id(payload: &[u8]) -> u16 {
        let frame = decode_frame(payload).unwrap();
        assert_eq!(frame.message_id, FsMessageId::WriteBeginRequest);
        let mut reader = Reader::new(&frame.payload);
        reader.u16().unwrap()
    }
}
