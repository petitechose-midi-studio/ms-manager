use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;

use super::controller_fs_job::{
    self as job, JobCapabilities, JobCommand, JobError, JobRequest, JobResponse, JobState,
};

pub const DEFAULT_BRIDGE_CONTROL_PORT: u16 = 7999;
pub const DEFAULT_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_RPC_TIMEOUT_MS: u32 = 2_000;
pub const DEFAULT_READ_PIPELINE_WINDOW: usize = 8;

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
static CLIENT_NONCE_SEQUENCE: OnceLock<AtomicU32> = OnceLock::new();
static ACTIVE_MUTATION_PORTS: Mutex<[u16; 256]> = Mutex::new([0; 256]);

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
    JobRequest = 0xFC,
    JobResponse = 0xFD,
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
            0xFC => Ok(Self::JobRequest),
            0xFD => Ok(Self::JobResponse),
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

    fn supports_persistence_jobs(&self) -> bool {
        self.status == FsStatus::Ok
            && self.rpc_schema == FS_RPC_SCHEMA
            && (self.feature_flags & job::FILESYSTEM_FEATURE_PERSISTENCE_JOBS) != 0
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

    fn port(&self) -> u16 {
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

#[derive(Debug, Clone, Copy)]
enum PersistenceMode {
    Unknown,
    Legacy,
    Jobs(JobCapabilities),
}

#[derive(Debug)]
struct OwnedJobResponse {
    state: JobState,
    error: JobError,
    flags: u8,
    client_nonce: u32,
    job_id: u32,
    retry_after_ms: u32,
    body: Vec<u8>,
}

impl OwnedJobResponse {
    fn from_borrowed(response: JobResponse<'_>) -> Self {
        Self {
            state: response.state,
            error: response.error,
            flags: response.flags,
            client_nonce: response.client_nonce,
            job_id: response.job_id,
            retry_after_ms: response.retry_after_ms,
            body: response.body.to_vec(),
        }
    }
}

#[derive(Debug)]
struct MutationPermit {
    control_port: u16,
}

impl Drop for MutationPermit {
    fn drop(&mut self) {
        let mut ports = active_mutation_ports();
        if let Some(slot) = ports.iter_mut().find(|port| **port == self.control_port) {
            *slot = 0;
        }
    }
}

pub struct ControllerFsClient {
    bridge: BridgeBinaryClient,
    next_request_id: u16,
    next_write_session_id: u16,
    chunk_size: usize,
    read_pipeline_window: usize,
    conditional_mutations_supported: Option<bool>,
    persistence_mode: PersistenceMode,
}

impl ControllerFsClient {
    pub fn new(bridge: BridgeBinaryClient) -> Self {
        let next_write_session_id = initial_write_session_id(bridge.port);
        Self {
            bridge,
            next_request_id: 1,
            next_write_session_id,
            chunk_size: FS_RPC_MAX_CHUNK_SIZE,
            read_pipeline_window: DEFAULT_READ_PIPELINE_WINDOW,
            conditional_mutations_supported: None,
            persistence_mode: PersistenceMode::Unknown,
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

    pub fn with_read_pipeline_window(mut self, pipeline_window: usize) -> ControllerFsResult<Self> {
        if pipeline_window == 0 || pipeline_window > DEFAULT_READ_PIPELINE_WINDOW {
            return Err(ControllerFsError::new(
                "invalid_input",
                format!(
                    "read pipeline window must be between 1 and {DEFAULT_READ_PIPELINE_WINDOW}"
                ),
            ));
        }
        self.read_pipeline_window = pipeline_window;
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
        self.persistence_mode = self.negotiate_persistence_mode(&decoded).await?;
        Ok(decoded)
    }

    async fn negotiate_persistence_mode(
        &mut self,
        capabilities: &FsCapabilities,
    ) -> ControllerFsResult<PersistenceMode> {
        if !capabilities.supports_persistence_jobs() {
            return Ok(PersistenceMode::Legacy);
        }

        let supplier_version = bridge_job_protocol_version(self.bridge.port()).await;
        if supplier_version.is_none_or(|version| version < job::PROTOCOL_VERSION) {
            return Ok(PersistenceMode::Legacy);
        }

        let request_id = self.request_id();
        let payload = job::encode_request(JobRequest {
            request_id,
            command: JobCommand::Capabilities,
            client_nonce: 0,
            job_id: 0,
            total_deadline_ms: 0,
            inner_request: &[],
        })
        .map_err(job_codec_error)?;
        let response = self
            .bridge
            .controller_rpc(payload, FsMessageId::JobResponse, DEFAULT_RPC_TIMEOUT_MS)
            .await
            .map_err(|error| advertised_job_train_error("capabilities transport", error))?;
        let negotiated = match job::decode_capabilities_response(&response, request_id) {
            Ok(value) => value,
            Err(error) => {
                self.bridge.close().await;
                return Err(advertised_job_codec_error("capabilities response", error));
            }
        };
        Ok(PersistenceMode::Jobs(negotiated))
    }

    async fn ensure_persistence_mode(&mut self) -> ControllerFsResult<PersistenceMode> {
        if matches!(self.persistence_mode, PersistenceMode::Unknown) {
            self.capabilities().await?;
        }
        match self.persistence_mode {
            PersistenceMode::Unknown => Err(ControllerFsError::new(
                "invalid_state",
                "persistence mode remained unknown after capability negotiation",
            )),
            mode => Ok(mode),
        }
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

        let _mutation_permit = acquire_mutation_permit(self.bridge.port())?;
        let persistence_mode = self.ensure_persistence_mode().await?;

        // Session ids are client-owned by the firmware protocol. Do not reuse
        // the deterministic request-id sequence: after an ambiguous begin we
        // issue a best-effort abort, and a predictable id could otherwise
        // collide with (and abort) another local client's active upload.
        let session_id = self.write_session_id();
        let begin_id = self.request_id();
        let begin = encode_write_begin_request(begin_id, session_id, path, total_bytes as u32)?;
        let begin_response = match self
            .legacy_write_rpc(begin, FsMessageId::WriteBeginResponse, begin_id)
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
            let request = match self
                .build_write_request_from_reader(
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
            let WriteRequest {
                request_id,
                size,
                payload,
            } = request;
            let response = match self.rpc(payload, FsMessageId::WriteChunkResponse).await {
                Ok(value) => value,
                Err(err) => {
                    let _ = self.abort_write(session_id).await;
                    return Err(err);
                }
            };
            let decoded = match decode_write_response(&response, request_id) {
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
            if decoded.session_id != session_id || decoded.bytes_written as usize != size {
                let _ = self.abort_write(session_id).await;
                return Err(ControllerFsError::new(
                    "invalid_state",
                    "write chunk response mismatch",
                ));
            }
            offset += size;
            on_progress(offset, total_bytes as usize);
        }

        let commit_id = self.request_id();
        let commit = encode_write_commit_request(commit_id, session_id)?;
        let commit_response = match self
            .mutation_rpc(persistence_mode, commit, FsMessageId::WriteCommitResponse)
            .await
            .and_then(|response| decode_write_response(&response, commit_id))
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
        let _mutation_permit = acquire_mutation_permit(self.bridge.port())?;
        let persistence_mode = self.ensure_persistence_mode().await?;
        let request_id = self.request_id();
        let payload = encode_mkdir_request(request_id, path)?;
        self.status_mutation_rpc(
            persistence_mode,
            payload,
            FsMessageId::MkdirResponse,
            request_id,
            "mkdir",
            path,
        )
        .await
    }

    pub async fn delete(&mut self, path: &str, recursive: bool) -> ControllerFsResult<()> {
        let _mutation_permit = acquire_mutation_permit(self.bridge.port())?;
        let persistence_mode = self.ensure_persistence_mode().await?;
        let request_id = self.request_id();
        let payload = encode_delete_request(request_id, path, recursive)?;
        self.status_mutation_rpc(
            persistence_mode,
            payload,
            FsMessageId::DeleteResponse,
            request_id,
            "delete",
            path,
        )
        .await
    }

    pub async fn rename(&mut self, from_path: &str, to_path: &str) -> ControllerFsResult<()> {
        let _mutation_permit = acquire_mutation_permit(self.bridge.port())?;
        let persistence_mode = self.ensure_persistence_mode().await?;
        let request_id = self.request_id();
        let payload = encode_rename_request(request_id, from_path, to_path)?;
        self.status_mutation_rpc(
            persistence_mode,
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
        let _mutation_permit = acquire_mutation_permit(self.bridge.port())?;
        let persistence_mode = self.ensure_persistence_mode().await?;
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
            .mutation_rpc(
                persistence_mode,
                payload,
                FsMessageId::ConditionalReplaceResponse,
            )
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
        let _mutation_permit = acquire_mutation_permit(self.bridge.port())?;
        let persistence_mode = self.ensure_persistence_mode().await?;
        let request_id = self.request_id();
        let payload = encode_conditional_delete_request(
            request_id,
            operation_id,
            path,
            expected_source_sha256,
        )?;
        let response = self
            .mutation_rpc(
                persistence_mode,
                payload,
                FsMessageId::ConditionalDeleteResponse,
            )
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
            .legacy_write_rpc(payload, FsMessageId::WriteAbortResponse, request_id)
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
        let request_id = decode_frame(&payload)?.request_id;
        let response = self
            .bridge
            .controller_rpc(payload, expected, DEFAULT_RPC_TIMEOUT_MS)
            .await?;
        checked_rpc_terminal_response(response, request_id)
    }

    async fn rpc_many(
        &mut self,
        requests: &[(Vec<u8>, FsMessageId)],
    ) -> ControllerFsResult<Vec<Vec<u8>>> {
        if self.read_pipeline_window <= 1 || requests.len() <= 1 {
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
        let request_ids = requests
            .iter()
            .map(|(payload, _)| decode_frame(payload).map(|frame| frame.request_id))
            .collect::<ControllerFsResult<Vec<_>>>()?;
        self.bridge
            .controller_rpc_batch(&batch)
            .await?
            .into_iter()
            .zip(request_ids)
            .map(|(response, request_id)| checked_rpc_terminal_response(response, request_id))
            .collect()
    }

    async fn legacy_write_rpc(
        &mut self,
        payload: Vec<u8>,
        expected: FsMessageId,
        request_id: u16,
    ) -> ControllerFsResult<FsWriteResponse> {
        let response = self.rpc(payload, expected).await?;
        decode_write_response(&response, request_id)
    }

    async fn mutation_rpc(
        &mut self,
        mode: PersistenceMode,
        payload: Vec<u8>,
        expected: FsMessageId,
    ) -> ControllerFsResult<Vec<u8>> {
        match mode {
            PersistenceMode::Unknown => Err(ControllerFsError::new(
                "invalid_state",
                "cannot mutate before persistence capability negotiation",
            )),
            PersistenceMode::Legacy => self.rpc(payload, expected).await,
            PersistenceMode::Jobs(capabilities) => {
                self.run_persistence_job(capabilities, payload).await
            }
        }
    }

    async fn run_persistence_job(
        &mut self,
        capabilities: JobCapabilities,
        inner_request: Vec<u8>,
    ) -> ControllerFsResult<Vec<u8>> {
        let total_deadline_ms = job::checked_job_deadline(capabilities, inner_request.len())
            .map_err(job_codec_error)?;
        let supervision_started = Instant::now();
        let mut collision_rekeyed = false;

        loop {
            let client_nonce = next_client_nonce(self.bridge.port())?;
            let (start, replayed_after_ambiguity) = self
                .start_persistence_job(client_nonce, total_deadline_ms, &inner_request)
                .await?;
            let duplicate = start.flags & job::FLAG_DUPLICATE_START != 0;
            let conflict = start.state == JobState::Rejected && start.error == JobError::Conflict;

            if (duplicate || conflict) && !replayed_after_ambiguity {
                if collision_rekeyed {
                    return Err(ControllerFsError::new(
                        "persistence_job_nonce_collision",
                        "persistence job nonce collided twice; operation was not adopted",
                    ));
                }
                collision_rekeyed = true;
                continue;
            }
            if conflict {
                return Err(ControllerFsError::new(
                    "persistence_job_start_ambiguous",
                    "persistence job replay conflicted after an ambiguous START",
                ));
            }

            return self.poll_persistence_job(start, supervision_started).await;
        }
    }

    async fn start_persistence_job(
        &mut self,
        client_nonce: u32,
        total_deadline_ms: u32,
        inner_request: &[u8],
    ) -> ControllerFsResult<(OwnedJobResponse, bool)> {
        let request_id = self.request_id();
        let payload = job::encode_request(JobRequest {
            request_id,
            command: JobCommand::Start,
            client_nonce,
            job_id: 0,
            total_deadline_ms,
            inner_request,
        })
        .map_err(job_codec_error)?;

        match self
            .send_job_payload(&payload, request_id, JobCommand::Start, client_nonce, None)
            .await
        {
            Ok(response) => Ok((response, false)),
            Err(first_error) => match self
                .send_job_payload(&payload, request_id, JobCommand::Start, client_nonce, None)
                .await
            {
                Ok(response) => Ok((response, true)),
                Err(second_error) => Err(ControllerFsError::new(
                    "persistence_job_start_ambiguous",
                    format!(
                        "persistence START remained ambiguous after one identical replay: {}; {}",
                        first_error.message, second_error.message
                    ),
                )),
            },
        }
    }

    async fn poll_persistence_job(
        &mut self,
        mut response: OwnedJobResponse,
        supervision_started: Instant,
    ) -> ControllerFsResult<Vec<u8>> {
        let client_nonce = response.client_nonce;
        let job_id = response.job_id;
        let mut poll_count = 0u16;
        let mut poll_delay_ms = 0u32;
        let mut cancel_sent = false;

        loop {
            if response.state.is_terminal() {
                return terminal_job_result(response);
            }
            if !matches!(
                response.state,
                JobState::Accepted | JobState::Pending | JobState::CancelPending
            ) || job_id == 0
            {
                return Err(ControllerFsError::new(
                    "persistence_job_protocol_error",
                    "persistence job entered an invalid non-terminal state",
                ));
            }

            let elapsed = supervision_started.elapsed();
            if elapsed >= Duration::from_millis(job::MANAGER_SUPERVISION_MS)
                || poll_count >= job::MAX_POLL_COUNT
            {
                if !cancel_sent {
                    match self.cancel_persistence_job(client_nonce, job_id).await {
                        Ok(cancel_response) if cancel_response.state.is_terminal() => {
                            return terminal_job_result(cancel_response);
                        }
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
                return Err(ControllerFsError::new(
                    "persistence_job_supervision_ambiguous",
                    format!(
                        "persistence job {job_id} exceeded Manager supervision after {poll_count} polls; cancellation outcome is not terminal"
                    ),
                ));
            }

            poll_delay_ms = job::next_poll_delay_ms(poll_delay_ms, response.retry_after_ms);
            let supervision_remaining =
                Duration::from_millis(job::MANAGER_SUPERVISION_MS).saturating_sub(elapsed);
            tokio::time::sleep(
                Duration::from_millis(u64::from(poll_delay_ms)).min(supervision_remaining),
            )
            .await;

            // Do not launch another potentially multi-second RPC once the
            // Manager supervision window has closed. The next loop turn owns
            // the single bounded cancellation attempt.
            if supervision_started.elapsed() >= Duration::from_millis(job::MANAGER_SUPERVISION_MS) {
                continue;
            }

            poll_count += 1;
            let request_id = self.request_id();
            let payload = job::encode_request(JobRequest {
                request_id,
                command: JobCommand::Poll,
                client_nonce,
                job_id,
                total_deadline_ms: 0,
                inner_request: &[],
            })
            .map_err(job_codec_error)?;
            match self
                .send_job_payload(
                    &payload,
                    request_id,
                    JobCommand::Poll,
                    client_nonce,
                    Some(job_id),
                )
                .await
            {
                Ok(poll_response) => response = poll_response,
                Err(poll_error) => {
                    if cancel_sent {
                        return Err(job_after_admission_ambiguous(
                            job_id,
                            "poll failed after cancellation",
                            poll_error,
                        ));
                    }
                    cancel_sent = true;
                    response = self
                        .cancel_persistence_job(client_nonce, job_id)
                        .await
                        .map_err(|cancel_error| {
                            ControllerFsError::new(
                                "persistence_job_ambiguous",
                                format!(
                                    "persistence job {job_id} poll failed after admission ({}); its single cancellation attempt also failed ({})",
                                    poll_error.message, cancel_error.message
                                ),
                            )
                        })?;
                }
            }
        }
    }

    async fn cancel_persistence_job(
        &mut self,
        client_nonce: u32,
        job_id: u32,
    ) -> ControllerFsResult<OwnedJobResponse> {
        let request_id = self.request_id();
        let payload = job::encode_request(JobRequest {
            request_id,
            command: JobCommand::Cancel,
            client_nonce,
            job_id,
            total_deadline_ms: 0,
            inner_request: &[],
        })
        .map_err(job_codec_error)?;
        self.send_job_payload(
            &payload,
            request_id,
            JobCommand::Cancel,
            client_nonce,
            Some(job_id),
        )
        .await
    }

    async fn send_job_payload(
        &mut self,
        payload: &[u8],
        expected_request_id: u16,
        expected_command: JobCommand,
        expected_nonce: u32,
        expected_job_id: Option<u32>,
    ) -> ControllerFsResult<OwnedJobResponse> {
        let encoded = self
            .bridge
            .controller_rpc(
                payload.to_vec(),
                FsMessageId::JobResponse,
                DEFAULT_RPC_TIMEOUT_MS,
            )
            .await?;
        let decoded = match job::decode_response(&encoded) {
            Ok(value) => value,
            Err(error) => {
                self.bridge.close().await;
                return Err(job_codec_error(error));
            }
        };
        if decoded.request_id != expected_request_id
            || decoded.command != expected_command
            || decoded.client_nonce != expected_nonce
            || expected_job_id.is_some_and(|job_id| decoded.job_id != job_id)
        {
            self.bridge.close().await;
            return Err(ControllerFsError::new(
                "persistence_job_protocol_error",
                "persistence job response identity does not match its exact request",
            ));
        }
        if expected_command != JobCommand::Start && decoded.state == JobState::Accepted {
            self.bridge.close().await;
            return Err(ControllerFsError::new(
                "persistence_job_protocol_error",
                "only START may return the accepted state",
            ));
        }
        Ok(OwnedJobResponse::from_borrowed(decoded))
    }

    async fn status_mutation_rpc(
        &mut self,
        mode: PersistenceMode,
        payload: Vec<u8>,
        expected: FsMessageId,
        request_id: u16,
        action: &str,
        path: &str,
    ) -> ControllerFsResult<()> {
        let response = self.mutation_rpc(mode, payload, expected).await?;
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
        while cursor < size_bytes && batch.len() < self.read_pipeline_window {
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

    async fn build_write_request_from_reader(
        &mut self,
        session_id: u16,
        source: &mut tokio::fs::File,
        offset: usize,
        total_size: usize,
    ) -> ControllerFsResult<WriteRequest> {
        if offset >= total_size {
            return Err(ControllerFsError::new(
                "invalid_state",
                "cannot build a write request at or beyond end of file",
            ));
        }
        let size = self.chunk_size.min(total_size - offset);
        let mut chunk = vec![0u8; size];
        source.read_exact(&mut chunk).await.map_err(|err| {
            ControllerFsError::new(
                "local_io_failed",
                format!("read local transfer file: {err}"),
            )
        })?;
        let request_id = self.request_id();
        let payload = encode_write_chunk_request(request_id, session_id, offset as u32, &chunk)?;
        Ok(WriteRequest {
            request_id,
            size,
            payload,
        })
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

fn active_mutation_ports() -> MutexGuard<'static, [u16; 256]> {
    ACTIVE_MUTATION_PORTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn acquire_mutation_permit(control_port: u16) -> ControllerFsResult<MutationPermit> {
    if control_port == 0 {
        return Err(ControllerFsError::new(
            "invalid_input",
            "bridge control port cannot be zero",
        ));
    }
    let mut ports = active_mutation_ports();
    if ports.contains(&control_port) {
        return Err(ControllerFsError::new(
            "mutation_busy",
            format!(
                "another controller filesystem mutation is already active on control port {control_port}"
            ),
        ));
    }
    let Some(slot) = ports.iter_mut().find(|port| **port == 0) else {
        return Err(ControllerFsError::new(
            "mutation_registry_full",
            "all 256 bounded controller mutation slots are active",
        ));
    };
    *slot = control_port;
    Ok(MutationPermit { control_port })
}

async fn bridge_job_protocol_version(control_port: u16) -> Option<u8> {
    let value = super::bridge_ctl::send_command(control_port, "status", DEFAULT_CONTROL_TIMEOUT)
        .await
        .ok()?;
    let schema = value.get("schema")?.as_u64()?;
    let ok = value.get("ok")?.as_bool()?;
    let version = value.get("persistence_job_protocol_version")?.as_u64()?;
    if schema != 1 || !ok {
        return None;
    }
    u8::try_from(version).ok().filter(|version| *version != 0)
}

fn initial_client_nonce(control_port: u16) -> u32 {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mixed = stamp
        ^ stamp.rotate_right(19)
        ^ u64::from(std::process::id()).rotate_left(17)
        ^ u64::from(control_port).rotate_left(41);
    let folded = (mixed ^ (mixed >> 32)) as u32;
    match folded {
        0 | u32::MAX => 1,
        value => value,
    }
}

fn next_client_nonce(control_port: u16) -> ControllerFsResult<u32> {
    let sequence =
        CLIENT_NONCE_SEQUENCE.get_or_init(|| AtomicU32::new(initial_client_nonce(control_port)));
    loop {
        let current = sequence.load(Ordering::Relaxed);
        let successor = checked_nonce_successor(current)?;
        if sequence
            .compare_exchange_weak(current, successor, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Ok(current);
        }
    }
}

fn checked_nonce_successor(current: u32) -> ControllerFsResult<u32> {
    current
        .checked_add(1)
        .filter(|value| *value != 1)
        .ok_or_else(|| {
            ControllerFsError::new(
                "persistence_job_nonce_exhausted",
                "persistence job nonce sequence is exhausted; restart after retained jobs expire",
            )
        })
}

fn job_codec_error(error: job::JobCodecError) -> ControllerFsError {
    ControllerFsError::new(
        "persistence_job_codec_error",
        format!("persistence job codec rejected data: {error}"),
    )
}

fn advertised_job_codec_error(context: &str, error: job::JobCodecError) -> ControllerFsError {
    ControllerFsError::new(
        "persistence_job_negotiation_failed",
        format!("advertised persistence job {context} is incompatible: {error}"),
    )
}

fn advertised_job_train_error(context: &str, error: ControllerFsError) -> ControllerFsError {
    ControllerFsError::new(
        "persistence_job_negotiation_failed",
        format!(
            "advertised persistence job {context} failed without legacy fallback: {}",
            error.message
        ),
    )
}

fn terminal_job_result(response: OwnedJobResponse) -> ControllerFsResult<Vec<u8>> {
    match response.state {
        JobState::Completed => Ok(response.body),
        JobState::Cancelled | JobState::Failed | JobState::Rejected => {
            Err(terminal_job_error(&response))
        }
        _ => Err(ControllerFsError::new(
            "persistence_job_protocol_error",
            "non-terminal persistence job was passed to terminal handling",
        )),
    }
}

fn terminal_job_error(response: &OwnedJobResponse) -> ControllerFsError {
    let kind = match response.error {
        JobError::None => "persistence_job_protocol_error",
        JobError::InvalidMessage => "persistence_invalid_message",
        JobError::InvalidArgument => "persistence_invalid_argument",
        JobError::Unsupported => "persistence_unsupported",
        JobError::NotFound => "persistence_job_not_found",
        JobError::BusyPlaying => "persistence_busy_playing",
        JobError::ResourceExhausted => "persistence_resource_exhausted",
        JobError::Conflict => "persistence_job_conflict",
        JobError::PreconditionFailed => "precondition_failed",
        JobError::DeadlineExceeded => "persistence_deadline_exceeded",
        JobError::MediaChanged => "persistence_media_changed",
        JobError::StorageUnavailable => "persistence_storage_unavailable",
        JobError::StorageReadFailed => "persistence_storage_read_failed",
        JobError::StorageWriteFailed => "persistence_storage_write_failed",
        JobError::StorageCorrupt => "persistence_storage_corrupt",
        JobError::Cancelled => "persistence_job_cancelled",
        JobError::Internal => "persistence_internal",
        JobError::LegacyBusy => "persistence_legacy_busy",
        JobError::LegacyStorageError => "persistence_legacy_storage_error",
    };
    ControllerFsError::new(
        kind,
        format!(
            "persistence job {} ended as {:?} with {:?} (flags=0x{:02x})",
            response.job_id, response.state, response.error, response.flags
        ),
    )
}

fn job_after_admission_ambiguous(
    job_id: u32,
    context: &str,
    error: ControllerFsError,
) -> ControllerFsError {
    ControllerFsError::new(
        "persistence_job_ambiguous",
        format!("persistence job {job_id} {context}: {}", error.message),
    )
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

fn checked_rpc_terminal_response(
    data: Vec<u8>,
    expected_request_id: u16,
) -> ControllerFsResult<Vec<u8>> {
    let frame = decode_frame(&data)?;
    if frame.request_id != expected_request_id {
        return Err(ControllerFsError::new(
            "invalid_state",
            format!(
                "request id mismatch: expected {}, got {}",
                expected_request_id, frame.request_id
            ),
        ));
    }
    if frame.message_id != FsMessageId::ErrorResponse {
        return Ok(data);
    }
    if frame.schema != FS_RPC_SCHEMA {
        return Err(ControllerFsError::new(
            "codec_error",
            format!("unsupported filesystem rpc schema: {}", frame.schema),
        ));
    }

    let mut reader = Reader::new(&frame.payload);
    let status = FsStatus::from_u8(reader.u8()?)?;
    reader.expect_empty()?;
    if status == FsStatus::Ok {
        return Err(ControllerFsError::new(
            "protocol_error",
            "filesystem error response carried an OK status",
        ));
    }
    Err(ControllerFsError::new(
        "remote_status",
        format!("controller filesystem rpc failed: {}", status.label()),
    ))
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
        FsMessageId::JobRequest => "FsJobRequest",
        FsMessageId::JobResponse => "FsJobResponse",
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
    fn client_surfaces_filesystem_error_without_rpc_timeout() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_binary_request(&mut stream).await;
                let request_id = decode_frame(&request.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    request.token,
                    &error_response(request_id, FsStatus::Busy),
                )
                .await;
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            let error = client.capabilities().await.unwrap_err();
            assert_eq!(error.kind, "remote_status");
            assert_eq!(error.message, "controller filesystem rpc failed: busy");
            server.await.unwrap();
        });
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
                .with_read_pipeline_window(3)
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
    fn client_writes_file_one_acknowledged_chunk_at_a_time() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let begin = read_binary_request(&mut stream).await;
                let session_id = write_begin_session_id(&begin.payload);
                let begin_id = decode_frame(&begin.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    begin.token,
                    &write_response(FsMessageId::WriteBeginResponse, begin_id, session_id, 0),
                )
                .await;
                let first = read_binary_request(&mut stream).await;
                let first_id = decode_frame(&first.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    first.token,
                    &write_response(FsMessageId::WriteChunkResponse, first_id, session_id, 4),
                )
                .await;
                let second = read_binary_request(&mut stream).await;
                let second_id = decode_frame(&second.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    second.token,
                    &write_response(FsMessageId::WriteChunkResponse, second_id, session_id, 4),
                )
                .await;
                let third = read_binary_request(&mut stream).await;
                let third_id = decode_frame(&third.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    third.token,
                    &write_response(FsMessageId::WriteChunkResponse, third_id, session_id, 1),
                )
                .await;
                let commit = read_binary_request(&mut stream).await;
                let commit_id = decode_frame(&commit.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    commit.token,
                    &write_response(FsMessageId::WriteCommitResponse, commit_id, session_id, 0),
                )
                .await;
                (first.payload, second.payload, third.payload, commit.payload)
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge)
                .with_chunk_size(4)
                .unwrap()
                .with_read_pipeline_window(3)
                .unwrap();
            client.persistence_mode = PersistenceMode::Legacy;
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
                let begin_id = decode_frame(&begin.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    begin.token,
                    &write_response(FsMessageId::WriteBeginResponse, begin_id, session_id, 0),
                )
                .await;
                let commit = read_binary_request(&mut stream).await;
                let commit_id = decode_frame(&commit.payload).unwrap().request_id;
                write_binary_response(
                    &mut stream,
                    commit.token,
                    &write_response(FsMessageId::WriteCommitResponse, commit_id, session_id, 0),
                )
                .await;
                (begin.payload, commit.payload)
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge)
                .with_chunk_size(4)
                .unwrap()
                .with_read_pipeline_window(3)
                .unwrap();
            client.persistence_mode = PersistenceMode::Legacy;
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
    fn new_upload_streams_legacy_chunks_then_commits_as_one_job() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let begin = read_binary_request(&mut stream).await;
                let begin_frame = decode_frame(&begin.payload).unwrap();
                let session_id = write_begin_session_id(&begin.payload);
                write_binary_response(
                    &mut stream,
                    begin.token,
                    &write_response(
                        FsMessageId::WriteBeginResponse,
                        begin_frame.request_id,
                        session_id,
                        0,
                    ),
                )
                .await;

                for expected_size in [4, 1] {
                    let chunk = read_binary_request(&mut stream).await;
                    let chunk_frame = decode_frame(&chunk.payload).unwrap();
                    assert_eq!(chunk_frame.message_id, FsMessageId::WriteChunkRequest);
                    write_binary_response(
                        &mut stream,
                        chunk.token,
                        &write_response(
                            FsMessageId::WriteChunkResponse,
                            chunk_frame.request_id,
                            session_id,
                            expected_size,
                        ),
                    )
                    .await;
                }

                let start = read_binary_request(&mut stream).await;
                let job_request = job::decode_request(&start.payload).unwrap();
                assert_eq!(job_request.command, JobCommand::Start);
                let commit = decode_frame(job_request.inner_request).unwrap();
                assert_eq!(commit.message_id, FsMessageId::WriteCommitRequest);
                let terminal = write_response(
                    FsMessageId::WriteCommitResponse,
                    commit.request_id,
                    session_id,
                    0,
                );
                write_binary_response(
                    &mut stream,
                    start.token,
                    &encoded_job_response(JobResponse {
                        request_id: job_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Completed,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: job_request.client_nonce,
                        job_id: 73,
                        retry_after_ms: 0,
                        progress_per_mille: 1_000,
                        body: &terminal,
                    }),
                )
                .await;
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge)
                .with_chunk_size(4)
                .unwrap()
                .with_read_pipeline_window(3)
                .unwrap();
            client.persistence_mode = PersistenceMode::Jobs(JobCapabilities::V1);
            let source = temp_test_path("controller-fs-job-push.bin");
            std::fs::write(&source, b"abcde").unwrap();
            let bytes = client
                .push_file_from_path_with_progress("projects/job.bin", &source, |_, _| {})
                .await
                .unwrap();
            assert_eq!(bytes, 5);
            let _ = std::fs::remove_file(&source);
            client.close().await;
            server.await.unwrap();
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

    #[test]
    fn mutation_permit_is_exactly_one_per_control_port() {
        let new_static_bytes =
            std::mem::size_of::<Mutex<[u16; 256]>>() + std::mem::size_of::<OnceLock<AtomicU32>>();
        assert!(new_static_bytes <= 544, "static bytes: {new_static_bytes}");

        let first = acquire_mutation_permit(65_534).unwrap();
        let error = acquire_mutation_permit(65_534).unwrap_err();
        assert_eq!(error.kind, "mutation_busy");

        let independent = acquire_mutation_permit(65_533).unwrap();
        drop(first);
        let reacquired = acquire_mutation_permit(65_534).unwrap();
        drop(reacquired);
        drop(independent);
    }

    #[test]
    fn bridge_job_supplier_discovery_is_additive_and_optional() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_json_request(&mut stream).await;
                assert_eq!(
                    request.get("cmd").and_then(|value| value.as_str()),
                    Some("status")
                );
                stream
                    .write_all(
                        br#"{"schema":1,"ok":true,"paused":false,"serial_open":true,"message":null,"persistence_job_protocol_version":1}"#,
                    )
                    .await
                    .unwrap();
                stream.write_all(b"\n").await.unwrap();
                stream.shutdown().await.unwrap();
            });
            assert_eq!(bridge_job_protocol_version(port).await, Some(1));
            server.await.unwrap();

            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let _ = read_json_request(&mut stream).await;
                stream
                    .write_all(
                        br#"{"schema":1,"ok":true,"paused":false,"serial_open":true,"message":null}"#,
                    )
                    .await
                    .unwrap();
                stream.write_all(b"\n").await.unwrap();
                stream.shutdown().await.unwrap();
            });
            assert_eq!(bridge_job_protocol_version(port).await, None);
            server.await.unwrap();
        });
    }

    #[test]
    fn new_manager_uses_jobs_only_after_both_discovery_signals() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut binary, _) = listener.accept().await.unwrap();
                let capabilities = read_binary_request(&mut binary).await;
                let capabilities_id = decode_frame(&capabilities.payload).unwrap().request_id;
                write_binary_response(
                    &mut binary,
                    capabilities.token,
                    &capabilities_response_with_features(
                        capabilities_id,
                        7 | FS_RPC_FEATURE_CONDITIONAL_MUTATIONS
                            | job::FILESYSTEM_FEATURE_PERSISTENCE_JOBS,
                    ),
                )
                .await;

                let (mut json, _) = listener.accept().await.unwrap();
                let _ = read_json_request(&mut json).await;
                json.write_all(
                    br#"{"schema":1,"ok":true,"paused":false,"serial_open":true,"message":null,"persistence_job_protocol_version":1}"#,
                )
                .await
                .unwrap();
                json.write_all(b"\n").await.unwrap();
                json.shutdown().await.unwrap();

                let job_capabilities = read_binary_request(&mut binary).await;
                let job_capabilities_request =
                    job::decode_request(&job_capabilities.payload).unwrap();
                assert_eq!(job_capabilities_request.command, JobCommand::Capabilities);
                write_binary_response(
                    &mut binary,
                    job_capabilities.token,
                    &job_capabilities_response(job_capabilities_request.request_id),
                )
                .await;

                let start = read_binary_request(&mut binary).await;
                let start_request = job::decode_request(&start.payload).unwrap();
                assert_eq!(start_request.command, JobCommand::Start);
                let inner = decode_frame(start_request.inner_request).unwrap();
                assert_eq!(inner.message_id, FsMessageId::MkdirRequest);
                write_binary_response(
                    &mut binary,
                    start.token,
                    &encoded_job_response(JobResponse {
                        request_id: start_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Accepted,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: start_request.client_nonce,
                        job_id: 77,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;

                let poll = read_binary_request(&mut binary).await;
                let poll_request = job::decode_request(&poll.payload).unwrap();
                assert_eq!(poll_request.command, JobCommand::Poll);
                assert_eq!(poll_request.client_nonce, start_request.client_nonce);
                assert_eq!(poll_request.job_id, 77);
                let terminal = status_response(FsMessageId::MkdirResponse, inner.request_id);
                write_binary_response(
                    &mut binary,
                    poll.token,
                    &encoded_job_response(JobResponse {
                        request_id: poll_request.request_id,
                        command: JobCommand::Poll,
                        state: JobState::Completed,
                        error: JobError::None,
                        flags: job::FLAG_TERMINAL_RETAINED,
                        client_nonce: poll_request.client_nonce,
                        job_id: poll_request.job_id,
                        retry_after_ms: 0,
                        progress_per_mille: 1_000,
                        body: &terminal,
                    }),
                )
                .await;
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            client.mkdir("projects/new").await.unwrap();
            assert!(matches!(client.persistence_mode, PersistenceMode::Jobs(_)));
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn missing_bridge_supplier_field_selects_bounded_legacy() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut binary, _) = listener.accept().await.unwrap();
                let capabilities = read_binary_request(&mut binary).await;
                let capabilities_id = decode_frame(&capabilities.payload).unwrap().request_id;
                write_binary_response(
                    &mut binary,
                    capabilities.token,
                    &capabilities_response_with_features(
                        capabilities_id,
                        7 | job::FILESYSTEM_FEATURE_PERSISTENCE_JOBS,
                    ),
                )
                .await;

                let (mut json, _) = listener.accept().await.unwrap();
                let _ = read_json_request(&mut json).await;
                json.write_all(
                    br#"{"schema":1,"ok":true,"paused":false,"serial_open":true,"message":null}"#,
                )
                .await
                .unwrap();
                json.write_all(b"\n").await.unwrap();
                json.shutdown().await.unwrap();

                let mkdir = read_binary_request(&mut binary).await;
                let inner = decode_frame(&mkdir.payload).unwrap();
                assert_eq!(inner.message_id, FsMessageId::MkdirRequest);
                write_binary_response(
                    &mut binary,
                    mkdir.token,
                    &status_response(FsMessageId::MkdirResponse, inner.request_id),
                )
                .await;
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            client.mkdir("projects/legacy").await.unwrap();
            assert!(matches!(client.persistence_mode, PersistenceMode::Legacy));
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn advertised_job_train_with_invalid_capabilities_fails_closed() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut binary, _) = listener.accept().await.unwrap();
                let capabilities = read_binary_request(&mut binary).await;
                let capabilities_id = decode_frame(&capabilities.payload).unwrap().request_id;
                write_binary_response(
                    &mut binary,
                    capabilities.token,
                    &capabilities_response_with_features(
                        capabilities_id,
                        7 | job::FILESYSTEM_FEATURE_PERSISTENCE_JOBS,
                    ),
                )
                .await;

                let (mut json, _) = listener.accept().await.unwrap();
                let _ = read_json_request(&mut json).await;
                json.write_all(
                    br#"{"schema":1,"ok":true,"paused":false,"serial_open":true,"message":null,"persistence_job_protocol_version":1}"#,
                )
                .await
                .unwrap();
                json.write_all(b"\n").await.unwrap();
                json.shutdown().await.unwrap();

                let job_capabilities = read_binary_request(&mut binary).await;
                let request = job::decode_request(&job_capabilities.payload).unwrap();
                let mut invalid = job_capabilities_response(request.request_id);
                let body_offset = invalid.len() - 24;
                invalid[body_offset] = 2;
                write_binary_response(&mut binary, job_capabilities.token, &invalid).await;
            });

            let bridge = BridgeBinaryClient::new(port);
            let mut client = ControllerFsClient::new(bridge);
            let error = client.mkdir("projects/reject").await.unwrap_err();
            assert_eq!(error.kind, "persistence_job_negotiation_failed");
            assert!(matches!(client.persistence_mode, PersistenceMode::Unknown));
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn ambiguous_start_replays_the_exact_frame_once() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut first_stream, _) = listener.accept().await.unwrap();
                let first = read_binary_request(&mut first_stream).await;
                write_binary_error_response(&mut first_stream, first.token, 4, "timeout").await;

                let (mut second_stream, _) = listener.accept().await.unwrap();
                let second = read_binary_request(&mut second_stream).await;
                assert_eq!(second.payload, first.payload);
                let request = job::decode_request(&second.payload).unwrap();
                write_binary_response(
                    &mut second_stream,
                    second.token,
                    &encoded_job_response(JobResponse {
                        request_id: request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Accepted,
                        error: JobError::None,
                        flags: job::FLAG_DUPLICATE_START,
                        client_nonce: request.client_nonce,
                        job_id: 19,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let (response, replayed) = client
                .start_persistence_job(0x1020_3040, job::MAX_TOTAL_DEADLINE_MS, &inner)
                .await
                .unwrap();
            assert!(replayed);
            assert_eq!(response.job_id, 19);
            assert_ne!(response.flags & job::FLAG_DUPLICATE_START, 0);
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn a_second_ambiguous_start_fails_without_unbounded_retry() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut first_stream, _) = listener.accept().await.unwrap();
                let first = read_binary_request(&mut first_stream).await;
                write_binary_error_response(&mut first_stream, first.token, 4, "timeout one").await;

                let (mut second_stream, _) = listener.accept().await.unwrap();
                let second = read_binary_request(&mut second_stream).await;
                assert_eq!(second.payload, first.payload);
                write_binary_error_response(&mut second_stream, second.token, 4, "timeout two")
                    .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let error = client
                .start_persistence_job(0x1020_3040, job::MAX_TOTAL_DEADLINE_MS, &inner)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "persistence_job_start_ambiguous");
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn unexpected_duplicate_start_rekeys_once_without_adopting_old_job() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let terminal = status_response(FsMessageId::MkdirResponse, 7);
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let first = read_binary_request(&mut stream).await;
                let first_request = job::decode_request(&first.payload).unwrap();
                write_binary_response(
                    &mut stream,
                    first.token,
                    &encoded_job_response(JobResponse {
                        request_id: first_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Accepted,
                        error: JobError::None,
                        flags: job::FLAG_DUPLICATE_START,
                        client_nonce: first_request.client_nonce,
                        job_id: 41,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;

                let second = read_binary_request(&mut stream).await;
                let second_request = job::decode_request(&second.payload).unwrap();
                assert_ne!(second_request.client_nonce, first_request.client_nonce);
                assert_eq!(second_request.inner_request, first_request.inner_request);
                write_binary_response(
                    &mut stream,
                    second.token,
                    &encoded_job_response(JobResponse {
                        request_id: second_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Completed,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: second_request.client_nonce,
                        job_id: 42,
                        retry_after_ms: 0,
                        progress_per_mille: 1_000,
                        body: &terminal,
                    }),
                )
                .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let response = client
                .run_persistence_job(JobCapabilities::V1, inner)
                .await
                .unwrap();
            assert_eq!(
                decode_status_response(&response, 7).unwrap().status,
                FsStatus::Ok
            );
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn a_second_nonce_collision_fails_without_a_third_start() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let first = read_binary_request(&mut stream).await;
                let first_request = job::decode_request(&first.payload).unwrap();
                write_binary_response(
                    &mut stream,
                    first.token,
                    &encoded_job_response(JobResponse {
                        request_id: first_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Pending,
                        error: JobError::None,
                        flags: job::FLAG_DUPLICATE_START,
                        client_nonce: first_request.client_nonce,
                        job_id: 43,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;

                let second = read_binary_request(&mut stream).await;
                let second_request = job::decode_request(&second.payload).unwrap();
                assert_ne!(second_request.client_nonce, first_request.client_nonce);
                write_binary_response(
                    &mut stream,
                    second.token,
                    &encoded_job_response(JobResponse {
                        request_id: second_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Rejected,
                        error: JobError::Conflict,
                        flags: 0,
                        client_nonce: second_request.client_nonce,
                        job_id: 44,
                        retry_after_ms: 0,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let error = client
                .run_persistence_job(JobCapabilities::V1, inner)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "persistence_job_nonce_collision");
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn admitted_poll_failure_sends_one_cancel_and_surfaces_terminal_cancel() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut first_stream, _) = listener.accept().await.unwrap();
                let start = read_binary_request(&mut first_stream).await;
                let start_request = job::decode_request(&start.payload).unwrap();
                write_binary_response(
                    &mut first_stream,
                    start.token,
                    &encoded_job_response(JobResponse {
                        request_id: start_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Accepted,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: start_request.client_nonce,
                        job_id: 51,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;
                let poll = read_binary_request(&mut first_stream).await;
                write_binary_error_response(&mut first_stream, poll.token, 4, "poll timeout").await;

                let (mut second_stream, _) = listener.accept().await.unwrap();
                let cancel = read_binary_request(&mut second_stream).await;
                let cancel_request = job::decode_request(&cancel.payload).unwrap();
                assert_eq!(cancel_request.command, JobCommand::Cancel);
                write_binary_response(
                    &mut second_stream,
                    cancel.token,
                    &encoded_job_response(JobResponse {
                        request_id: cancel_request.request_id,
                        command: JobCommand::Cancel,
                        state: JobState::Cancelled,
                        error: JobError::Cancelled,
                        flags: 0,
                        client_nonce: cancel_request.client_nonce,
                        job_id: cancel_request.job_id,
                        retry_after_ms: 0,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let error = client
                .run_persistence_job(JobCapabilities::V1, inner)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "persistence_job_cancelled");
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn mismatched_poll_identity_drops_the_stream_before_one_cancel() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut first_stream, _) = listener.accept().await.unwrap();
                let start = read_binary_request(&mut first_stream).await;
                let start_request = job::decode_request(&start.payload).unwrap();
                write_binary_response(
                    &mut first_stream,
                    start.token,
                    &encoded_job_response(JobResponse {
                        request_id: start_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Accepted,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: start_request.client_nonce,
                        job_id: 52,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;

                let poll = read_binary_request(&mut first_stream).await;
                let poll_request = job::decode_request(&poll.payload).unwrap();
                write_binary_response(
                    &mut first_stream,
                    poll.token,
                    &encoded_job_response(JobResponse {
                        request_id: poll_request.request_id,
                        command: JobCommand::Poll,
                        state: JobState::Pending,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: poll_request.client_nonce,
                        job_id: poll_request.job_id + 1,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;

                let (mut second_stream, _) = listener.accept().await.unwrap();
                let cancel = read_binary_request(&mut second_stream).await;
                let cancel_request = job::decode_request(&cancel.payload).unwrap();
                assert_eq!(cancel_request.command, JobCommand::Cancel);
                assert_eq!(cancel_request.client_nonce, start_request.client_nonce);
                assert_eq!(cancel_request.job_id, 52);
                write_binary_response(
                    &mut second_stream,
                    cancel.token,
                    &encoded_job_response(JobResponse {
                        request_id: cancel_request.request_id,
                        command: JobCommand::Cancel,
                        state: JobState::Cancelled,
                        error: JobError::Cancelled,
                        flags: 0,
                        client_nonce: cancel_request.client_nonce,
                        job_id: cancel_request.job_id,
                        retry_after_ms: 0,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let error = client
                .run_persistence_job(JobCapabilities::V1, inner)
                .await
                .unwrap_err();
            assert_eq!(error.kind, "persistence_job_cancelled");
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn cancel_too_late_continues_bounded_polling_to_exact_completion() {
        run_async(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let terminal = status_response(FsMessageId::MkdirResponse, 7);
            let server = tokio::spawn(async move {
                let (mut first_stream, _) = listener.accept().await.unwrap();
                let start = read_binary_request(&mut first_stream).await;
                let start_request = job::decode_request(&start.payload).unwrap();
                write_binary_response(
                    &mut first_stream,
                    start.token,
                    &encoded_job_response(JobResponse {
                        request_id: start_request.request_id,
                        command: JobCommand::Start,
                        state: JobState::Accepted,
                        error: JobError::None,
                        flags: 0,
                        client_nonce: start_request.client_nonce,
                        job_id: 61,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;
                let poll = read_binary_request(&mut first_stream).await;
                write_binary_error_response(&mut first_stream, poll.token, 4, "poll timeout").await;

                let (mut second_stream, _) = listener.accept().await.unwrap();
                let cancel = read_binary_request(&mut second_stream).await;
                let cancel_request = job::decode_request(&cancel.payload).unwrap();
                write_binary_response(
                    &mut second_stream,
                    cancel.token,
                    &encoded_job_response(JobResponse {
                        request_id: cancel_request.request_id,
                        command: JobCommand::Cancel,
                        state: JobState::Pending,
                        error: JobError::None,
                        flags: job::FLAG_CANCEL_TOO_LATE,
                        client_nonce: cancel_request.client_nonce,
                        job_id: cancel_request.job_id,
                        retry_after_ms: 5,
                        progress_per_mille: 0,
                        body: &[],
                    }),
                )
                .await;

                let final_poll = read_binary_request(&mut second_stream).await;
                let final_request = job::decode_request(&final_poll.payload).unwrap();
                assert_eq!(final_request.command, JobCommand::Poll);
                write_binary_response(
                    &mut second_stream,
                    final_poll.token,
                    &encoded_job_response(JobResponse {
                        request_id: final_request.request_id,
                        command: JobCommand::Poll,
                        state: JobState::Completed,
                        error: JobError::None,
                        flags: job::FLAG_TERMINAL_RETAINED,
                        client_nonce: final_request.client_nonce,
                        job_id: final_request.job_id,
                        retry_after_ms: 0,
                        progress_per_mille: 1_000,
                        body: &terminal,
                    }),
                )
                .await;
            });

            let inner = encode_mkdir_request(7, "projects/a").unwrap();
            let mut client = ControllerFsClient::new(BridgeBinaryClient::new(port));
            let response = client
                .run_persistence_job(JobCapabilities::V1, inner)
                .await
                .unwrap();
            assert_eq!(
                decode_status_response(&response, 7).unwrap().status,
                FsStatus::Ok
            );
            client.close().await;
            server.await.unwrap();
        });
    }

    #[test]
    fn every_job_error_has_a_stable_distinct_manager_kind() {
        let errors = [
            JobError::InvalidMessage,
            JobError::InvalidArgument,
            JobError::Unsupported,
            JobError::NotFound,
            JobError::BusyPlaying,
            JobError::ResourceExhausted,
            JobError::Conflict,
            JobError::PreconditionFailed,
            JobError::DeadlineExceeded,
            JobError::MediaChanged,
            JobError::StorageUnavailable,
            JobError::StorageReadFailed,
            JobError::StorageWriteFailed,
            JobError::StorageCorrupt,
            JobError::Cancelled,
            JobError::Internal,
            JobError::LegacyBusy,
            JobError::LegacyStorageError,
        ];
        let mut kinds = std::collections::HashSet::new();
        for error in errors {
            let response = OwnedJobResponse {
                state: if error == JobError::Cancelled {
                    JobState::Cancelled
                } else {
                    JobState::Failed
                },
                error,
                flags: if matches!(error, JobError::LegacyBusy | JobError::LegacyStorageError) {
                    job::FLAG_LEGACY_MAPPED
                } else {
                    0
                },
                client_nonce: 1,
                job_id: 2,
                retry_after_ms: 0,
                body: Vec::new(),
            };
            assert!(kinds.insert(terminal_job_error(&response).kind));
        }
        assert_eq!(kinds.len(), errors.len());
        assert!(kinds.contains("precondition_failed"));
        assert!(kinds.contains("persistence_legacy_busy"));
        assert!(kinds.contains("persistence_legacy_storage_error"));
    }

    #[test]
    fn client_nonce_sequence_never_emits_zero_or_wraps() {
        assert_eq!(checked_nonce_successor(1).unwrap(), 2);
        assert_eq!(
            checked_nonce_successor(0).unwrap_err().kind,
            "persistence_job_nonce_exhausted"
        );
        assert_eq!(
            checked_nonce_successor(u32::MAX).unwrap_err().kind,
            "persistence_job_nonce_exhausted"
        );
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

    async fn read_json_request(stream: &mut TcpStream) -> serde_json::Value {
        let mut bytes = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            if byte[0] == b'\n' {
                break;
            }
            bytes.push(byte[0]);
        }
        serde_json::from_slice(&bytes).unwrap()
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

    async fn write_binary_error_response(
        stream: &mut TcpStream,
        token: u16,
        status: u8,
        message: &str,
    ) {
        assert_ne!(status, BINARY_STATUS_OK);
        let message = message.as_bytes();
        let mut response = Vec::new();
        response.extend_from_slice(BINARY_RESPONSE_MAGIC);
        response.push(BINARY_CONTROL_VERSION);
        response.push(status);
        response.extend_from_slice(&token.to_le_bytes());
        response.extend_from_slice(&0u32.to_le_bytes());
        response.extend_from_slice(&(message.len() as u16).to_le_bytes());
        response.extend_from_slice(&0u16.to_le_bytes());
        response.extend_from_slice(message);
        stream.write_all(&response).await.unwrap();
    }

    fn capabilities_response(request_id: u16) -> Vec<u8> {
        capabilities_response_with_features(request_id, 7 | FS_RPC_FEATURE_CONDITIONAL_MUTATIONS)
    }

    fn error_response(request_id: u16, status: FsStatus) -> Vec<u8> {
        frame(FsMessageId::ErrorResponse, request_id, &[status as u8]).unwrap()
    }

    fn capabilities_response_with_features(request_id: u16, feature_flags: u32) -> Vec<u8> {
        let mut payload = vec![FsStatus::Ok as u8, FS_RPC_SCHEMA];
        payload.extend_from_slice(&(FS_RPC_MAX_CHUNK_SIZE as u16).to_le_bytes());
        payload.extend_from_slice(&32_512u16.to_le_bytes());
        payload.push(FS_RPC_MAX_LIST_ENTRIES);
        payload.extend_from_slice(&192u16.to_le_bytes());
        payload.extend_from_slice(&feature_flags.to_le_bytes());
        frame(FsMessageId::CapabilitiesResponse, request_id, &payload).unwrap()
    }

    fn job_capabilities_response(request_id: u16) -> Vec<u8> {
        let body = JobCapabilities::V1.encode();
        encoded_job_response(JobResponse {
            request_id,
            command: JobCommand::Capabilities,
            state: JobState::None,
            error: JobError::None,
            flags: 0,
            client_nonce: 0,
            job_id: 0,
            retry_after_ms: 0,
            progress_per_mille: 0,
            body: &body,
        })
    }

    fn encoded_job_response(response: JobResponse<'_>) -> Vec<u8> {
        job::encode_response(response).unwrap()
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
