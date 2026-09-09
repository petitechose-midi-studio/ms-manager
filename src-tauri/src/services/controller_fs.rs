//! Application filesystem operations over the single versioned RPC contract.
use super::controller_fs_unified::{Client, ConditionalResult, Failure};
pub use super::controller_transport::BridgeBinaryClient;
use filesystem_rpc::Error;
use serde::Serialize;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
pub const DEFAULT_BRIDGE_CONTROL_PORT: u16 = 7999;
pub const DEFAULT_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_RPC_TIMEOUT_MS: u32 = 2_000;
pub const DEFAULT_READ_PIPELINE_WINDOW: usize = 8;
pub const FS_RPC_MAX_CHUNK_SIZE: usize = 30_720;
pub const FS_RPC_SHA256_SIZE: usize = 32;
static CLIENT_NONCE_SEQUENCE: OnceLock<AtomicU32> = OnceLock::new();
static ACTIVE_MUTATION_PORTS: Mutex<[u16; 256]> = Mutex::new([0; 256]);
#[derive(Debug, Clone, Serialize)]
pub struct ControllerFsError {
    pub kind: String,
    pub message: String,
}

impl ControllerFsError {
    pub(crate) fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
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

#[derive(Debug, Clone, Serialize)]
pub struct FsCapabilities {
    pub protocol_version: u8,
    pub max_chunk_size: u32,
    pub max_upload_size: u32,
    pub max_list_entries: u8,
    pub max_path_length: u16,
    pub operations: u32,
}
impl FsCapabilities {
    pub fn require_conditional_mutations(&self) -> ControllerFsResult<()> {
        if self.protocol_version == filesystem_rpc::VERSION && self.operations & 0x1800 == 0x1800 {
            return Ok(());
        }
        Err(ControllerFsError::new("unsupported_feature", "Update Core, Manager and Bridge together: conditional filesystem operations are unavailable"))
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct FsStat {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FsConditionalMutationOutcome {
    Applied,
    AlreadyApplied,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsConditionalMutationResult {
    pub outcome: FsConditionalMutationOutcome,
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
    wire: Client,
    port: u16,
}
impl ControllerFsClient {
    pub fn new(bridge: BridgeBinaryClient) -> Self {
        let port = bridge.port();
        Self {
            wire: Client::new(bridge),
            port,
        }
    }
    pub async fn close(&mut self) {
        self.wire.close().await;
    }
    pub async fn capabilities(&mut self) -> ControllerFsResult<FsCapabilities> {
        let c = self.wire.capabilities().await.map_err(rpc_error)?;
        Ok(FsCapabilities {
            protocol_version: filesystem_rpc::VERSION,
            max_chunk_size: c.max_chunk,
            max_upload_size: c.max_upload,
            max_list_entries: 8,
            max_path_length: c.max_path,
            operations: c.operations,
        })
    }
    pub async fn stat(&mut self, path: &str) -> ControllerFsResult<FsStat> {
        match self.wire.stat(path).await {
            Ok((kind, size)) => Ok(FsStat {
                file_type: FsFileType::from_u8(kind)?,
                size_bytes: size,
            }),
            Err(Failure::Remote(Error::NotFound)) => Ok(FsStat {
                file_type: FsFileType::Missing,
                size_bytes: 0,
            }),
            Err(error) => Err(rpc_error(error)),
        }
    }
    pub async fn list(&mut self, path: &str) -> ControllerFsResult<Vec<FsListEntry>> {
        self.wire
            .list(path)
            .await
            .map_err(rpc_error)?
            .into_iter()
            .map(|e| {
                Ok(FsListEntry {
                    name: e.name,
                    file_type: FsFileType::from_u8(e.file_type)?,
                    size_bytes: e.size,
                    name_truncated: e.truncated,
                })
            })
            .collect()
    }
    pub async fn pull_file_to_path_with_progress<F: FnMut(usize, usize)>(
        &mut self,
        path: &str,
        destination: &Path,
        on_progress: F,
    ) -> ControllerFsResult<usize> {
        self.pull_file_to_path_with_progress_limit(path, destination, u32::MAX, on_progress)
            .await
    }
    pub async fn pull_file_to_path_with_progress_limit<F: FnMut(usize, usize)>(
        &mut self,
        path: &str,
        destination: &Path,
        max_bytes: u32,
        mut on_progress: F,
    ) -> ControllerFsResult<usize> {
        let stat = self.stat(path).await?;
        if stat.file_type != FsFileType::File {
            return Err(ControllerFsError::new(
                "not_file",
                format!("Remote path is not a file: {path}"),
            ));
        }
        if stat.size_bytes > max_bytes {
            return Err(ControllerFsError::new(
                "too_large",
                format!("Remote file exceeds {max_bytes} bytes: {path}"),
            ));
        }
        if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(local_error)?;
        }
        let mut file = tokio::fs::File::create(destination)
            .await
            .map_err(local_error)?;
        let mut offset = 0u32;
        while offset < stat.size_bytes {
            let count = (stat.size_bytes - offset)
                .min((DEFAULT_READ_PIPELINE_WINDOW * FS_RPC_MAX_CHUNK_SIZE) as u32);
            for bytes in self
                .wire
                .read_batch(path, offset, count)
                .await
                .map_err(rpc_error)?
            {
                file.write_all(&bytes).await.map_err(local_error)?;
                offset += bytes.len() as u32;
                on_progress(offset as usize, stat.size_bytes as usize);
            }
        }
        file.flush().await.map_err(local_error)?;
        Ok(offset as usize)
    }
    pub async fn push_file_from_path_with_progress<F: FnMut(usize, usize)>(
        &mut self,
        path: &str,
        source: &Path,
        on_progress: F,
    ) -> ControllerFsResult<usize> {
        let mut file = tokio::fs::File::open(source).await.map_err(local_error)?;
        let metadata = file.metadata().await.map_err(local_error)?;
        if !metadata.is_file() {
            return Err(ControllerFsError::new(
                "invalid_input",
                "Transfer source must be a file",
            ));
        }
        if metadata.len() > 524_288 {
            return Err(ControllerFsError::new(
                "too_large",
                "Transfer exceeds the controller's 524288 byte limit",
            ));
        }
        let _permit = acquire_mutation_permit(self.port)?;
        self.wire
            .upload_reader(
                path,
                &mut file,
                metadata.len() as usize,
                next_client_nonce(self.port)?,
                on_progress,
            )
            .await
            .map_err(rpc_error)?;
        Ok(metadata.len() as usize)
    }
    pub async fn mkdir(&mut self, path: &str) -> ControllerFsResult<()> {
        let _permit = acquire_mutation_permit(self.port)?;
        self.wire
            .mkdir(path, next_client_nonce(self.port)?)
            .await
            .map_err(rpc_error)
    }
    pub async fn delete(&mut self, path: &str, recursive: bool) -> ControllerFsResult<()> {
        let _permit = acquire_mutation_permit(self.port)?;
        self.wire
            .delete(path, recursive, next_client_nonce(self.port)?)
            .await
            .map_err(rpc_error)
    }
    pub async fn rename(&mut self, from: &str, to: &str) -> ControllerFsResult<()> {
        let _permit = acquire_mutation_permit(self.port)?;
        self.wire
            .rename(from, to, next_client_nonce(self.port)?)
            .await
            .map_err(rpc_error)
    }
    pub async fn conditional_replace(
        &mut self,
        nonce: u32,
        path: &str,
        staging: &str,
        expected: &[u8; 32],
        replacement: &[u8; 32],
    ) -> ControllerFsResult<FsConditionalMutationResult> {
        let _permit = acquire_mutation_permit(self.port)?;
        conditional(
            self.wire
                .conditional_replace(path, staging, expected, replacement, nonce)
                .await,
        )
    }
    pub async fn conditional_delete(
        &mut self,
        nonce: u32,
        path: &str,
        expected: &[u8; 32],
    ) -> ControllerFsResult<FsConditionalMutationResult> {
        let _permit = acquire_mutation_permit(self.port)?;
        conditional(self.wire.conditional_delete(path, expected, nonce).await)
    }
}
fn conditional(
    result: Result<ConditionalResult, Failure>,
) -> ControllerFsResult<FsConditionalMutationResult> {
    let result = result.map_err(rpc_error)?;
    let outcome = match result.outcome {
        1 => FsConditionalMutationOutcome::Applied,
        2 => FsConditionalMutationOutcome::AlreadyApplied,
        _ => {
            return Err(ControllerFsError::new(
                "protocol_error",
                "Conditional success has no outcome",
            ))
        }
    };
    Ok(FsConditionalMutationResult { outcome })
}
fn local_error(error: std::io::Error) -> ControllerFsError {
    ControllerFsError::new("local_io_failed", error.to_string())
}
fn rpc_error(error: Failure) -> ControllerFsError {
    match error {
        Failure::Local(message) => ControllerFsError::new("local_io_failed", message),
        Failure::Transport(message) => ControllerFsError::new("bridge_unavailable", message),
        Failure::Protocol => ControllerFsError::new(
            "protocol_error",
            "Invalid filesystem RPC response; update Core, Manager and Bridge together",
        ),
        Failure::Ambiguous => ControllerFsError::new(
            "mutation_ambiguous",
            "The mutation outcome is unknown; reconcile the file before retrying",
        ),
        Failure::Conditional(error, details) => {
            let hash = details
                .observed
                .map(|h| h.iter().map(|b| format!("{b:02x}")).collect::<String>())
                .unwrap_or_else(|| "unavailable".into());
            let kind = if error == Error::PreconditionFailed {
                "precondition_failed"
            } else {
                "conditional_storage_error"
            };
            ControllerFsError::new(
                kind,
                format!(
                    "Conditional mutation {error:?}; subject {}, observed SHA-256 {hash}",
                    details.subject
                ),
            )
        }
        Failure::Remote(error) => {
            let kind = match error {
                Error::Unsupported => "unsupported_feature",
                Error::NotFound => "not_found",
                Error::BusyPlaying => "busy_playing",
                Error::ResourceExhausted => "resource_exhausted",
                Error::Conflict => "mutation_conflict",
                Error::PreconditionFailed => "precondition_failed",
                Error::DeadlineExceeded => "deadline_exceeded",
                Error::MediaChanged => "media_changed",
                Error::StorageUnavailable => "storage_unavailable",
                Error::StorageReadFailed => "storage_read_failed",
                Error::StorageWriteFailed => "storage_write_failed",
                Error::StorageCorrupt => "storage_corrupt",
                Error::StorageFailure => "storage_failure",
                Error::TooLarge => "too_large",
                Error::ResultExpired => "result_expired",
                Error::CancelTooLate => "cancel_too_late",
                Error::Cancelled => "cancelled",
                Error::InvalidArgument => "invalid_input",
                _ => "protocol_error",
            };
            ControllerFsError::new(kind, format!("Controller filesystem: {error:?}"))
        }
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
                "mutation_nonce_exhausted",
                "mutation nonce sequence is exhausted; restart after retained operations expire",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn client_nonce_sequence_never_emits_zero_or_wraps() {
        assert_eq!(checked_nonce_successor(1).unwrap(), 2);
        assert_eq!(
            checked_nonce_successor(0).unwrap_err().kind,
            "mutation_nonce_exhausted"
        );
        assert_eq!(
            checked_nonce_successor(u32::MAX).unwrap_err().kind,
            "mutation_nonce_exhausted"
        );
    }
}
