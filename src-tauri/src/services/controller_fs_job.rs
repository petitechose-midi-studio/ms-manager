//! Byte-exact persistence-job v1 client contract.
//!
//! Core owns durable jobs and Bridge owns response matching/quarantine. This
//! module owns no job queue or payload: it validates negotiated limits and
//! encodes/decodes one caller-owned request or response at a time.

use std::fmt;

pub(super) const FILESYSTEM_FEATURE_PERSISTENCE_JOBS: u32 = 1 << 4;
pub(super) const REQUEST_MESSAGE_ID: u8 = 0xFC;
pub(super) const RESPONSE_MESSAGE_ID: u8 = 0xFD;
pub(super) const PROTOCOL_VERSION: u8 = 1;
pub(super) const MAX_INNER_REQUEST_BYTES: usize = 32_512;
pub(super) const MAX_INNER_RESPONSE_BYTES: usize = 32_512;
pub(super) const MAX_TOTAL_DEADLINE_MS: u32 = 10_000;
pub(super) const TERMINAL_RETENTION_MS: u32 = 30_000;
pub(super) const MANAGER_SUPERVISION_MS: u64 = 30_000;
pub(super) const MAX_POLL_COUNT: u16 = 1_024;
pub(super) const MIN_POLL_DELAY_MS: u32 = 5;
pub(super) const MAX_POLL_DELAY_MS: u32 = 50;

const SCHEMA: u8 = 1;
const REQUEST_NAME: &[u8] = b"FsJobRequest";
const RESPONSE_NAME: &[u8] = b"FsJobResponse";
const REQUEST_APPLICATION_HEADER_BYTES: usize = 16;
#[cfg(test)]
const RESPONSE_APPLICATION_HEADER_BYTES: usize = 20;
const CAPABILITIES_BODY_BYTES: usize = 24;
const MAX_PROGRESS_PER_MILLE: u32 = 1_000;
const MAX_CONCURRENT_JOBS: u8 = 2;

pub(super) const FEATURE_START: u32 = 1 << 0;
pub(super) const FEATURE_POLL: u32 = 1 << 1;
pub(super) const FEATURE_CANCEL: u32 = 1 << 2;
pub(super) const FEATURE_TERMINAL_RETENTION: u32 = 1 << 3;
pub(super) const FEATURE_TYPED_ERRORS: u32 = 1 << 4;
pub(super) const FEATURE_LEGACY_MAPPING: u32 = 1 << 5;
pub(super) const ALL_FEATURES: u32 = FEATURE_START
    | FEATURE_POLL
    | FEATURE_CANCEL
    | FEATURE_TERMINAL_RETENTION
    | FEATURE_TYPED_ERRORS
    | FEATURE_LEGACY_MAPPING;

pub(super) const FLAG_DUPLICATE_START: u8 = 1 << 0;
pub(super) const FLAG_LEGACY_MAPPED: u8 = 1 << 1;
pub(super) const FLAG_TERMINAL_RETAINED: u8 = 1 << 2;
pub(super) const FLAG_CANCEL_TOO_LATE: u8 = 1 << 3;
const ALL_RESPONSE_FLAGS: u8 =
    FLAG_DUPLICATE_START | FLAG_LEGACY_MAPPED | FLAG_TERMINAL_RETAINED | FLAG_CANCEL_TOO_LATE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum JobCommand {
    Capabilities = 0,
    Start = 1,
    Poll = 2,
    Cancel = 3,
}

impl JobCommand {
    fn decode(value: u8) -> Result<Self, JobCodecError> {
        match value {
            0 => Ok(Self::Capabilities),
            1 => Ok(Self::Start),
            2 => Ok(Self::Poll),
            3 => Ok(Self::Cancel),
            _ => Err(JobCodecError::UnknownCommand),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum JobState {
    None = 0,
    Accepted = 1,
    Pending = 2,
    Completed = 3,
    CancelPending = 4,
    Cancelled = 5,
    Failed = 6,
    Rejected = 7,
}

impl JobState {
    fn decode(value: u8) -> Result<Self, JobCodecError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Accepted),
            2 => Ok(Self::Pending),
            3 => Ok(Self::Completed),
            4 => Ok(Self::CancelPending),
            5 => Ok(Self::Cancelled),
            6 => Ok(Self::Failed),
            7 => Ok(Self::Rejected),
            _ => Err(JobCodecError::UnknownState),
        }
    }

    pub(super) const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Rejected
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum JobError {
    None = 0,
    InvalidMessage = 1,
    InvalidArgument = 2,
    Unsupported = 3,
    NotFound = 4,
    BusyPlaying = 5,
    ResourceExhausted = 6,
    Conflict = 7,
    PreconditionFailed = 8,
    DeadlineExceeded = 9,
    MediaChanged = 10,
    StorageUnavailable = 11,
    StorageReadFailed = 12,
    StorageWriteFailed = 13,
    StorageCorrupt = 14,
    Cancelled = 15,
    Internal = 16,
    LegacyBusy = 17,
    LegacyStorageError = 18,
}

impl JobError {
    fn decode(value: u8) -> Result<Self, JobCodecError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::InvalidMessage),
            2 => Ok(Self::InvalidArgument),
            3 => Ok(Self::Unsupported),
            4 => Ok(Self::NotFound),
            5 => Ok(Self::BusyPlaying),
            6 => Ok(Self::ResourceExhausted),
            7 => Ok(Self::Conflict),
            8 => Ok(Self::PreconditionFailed),
            9 => Ok(Self::DeadlineExceeded),
            10 => Ok(Self::MediaChanged),
            11 => Ok(Self::StorageUnavailable),
            12 => Ok(Self::StorageReadFailed),
            13 => Ok(Self::StorageWriteFailed),
            14 => Ok(Self::StorageCorrupt),
            15 => Ok(Self::Cancelled),
            16 => Ok(Self::Internal),
            17 => Ok(Self::LegacyBusy),
            18 => Ok(Self::LegacyStorageError),
            _ => Err(JobCodecError::UnknownError),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JobCodecError {
    Truncated,
    InvalidMessageId,
    InvalidMessageName,
    UnsupportedSchema,
    UnknownCommand,
    UnknownState,
    UnknownError,
    InvalidFlags,
    InvalidReserved,
    InvalidIdentity,
    InvalidDeadline,
    InvalidBody,
    InvalidProgress,
    LimitExceeded,
}

impl fmt::Display for JobCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Truncated => "truncated frame",
            Self::InvalidMessageId => "invalid message id",
            Self::InvalidMessageName => "invalid message name",
            Self::UnsupportedSchema => "unsupported schema",
            Self::UnknownCommand => "unknown command",
            Self::UnknownState => "unknown state",
            Self::UnknownError => "unknown typed error",
            Self::InvalidFlags => "invalid flags",
            Self::InvalidReserved => "invalid reserved field",
            Self::InvalidIdentity => "invalid job identity",
            Self::InvalidDeadline => "invalid deadline",
            Self::InvalidBody => "invalid body",
            Self::InvalidProgress => "invalid progress",
            Self::LimitExceeded => "protocol limit exceeded",
        };
        f.write_str(label)
    }
}

impl std::error::Error for JobCodecError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JobRequest<'a> {
    pub request_id: u16,
    pub command: JobCommand,
    pub client_nonce: u32,
    pub job_id: u32,
    pub total_deadline_ms: u32,
    pub inner_request: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JobResponse<'a> {
    pub request_id: u16,
    pub command: JobCommand,
    pub state: JobState,
    pub error: JobError,
    pub flags: u8,
    pub client_nonce: u32,
    pub job_id: u32,
    pub retry_after_ms: u32,
    pub progress_per_mille: u32,
    pub body: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JobCapabilities {
    pub protocol_version: u8,
    pub max_concurrent_jobs: u8,
    pub feature_flags: u32,
    pub max_inner_request_bytes: u32,
    pub max_inner_response_bytes: u32,
    pub max_total_deadline_ms: u32,
    pub terminal_retention_ms: u32,
}

impl JobCapabilities {
    fn decode(data: &[u8]) -> Result<Self, JobCodecError> {
        if data.len() != CAPABILITIES_BODY_BYTES {
            return Err(JobCodecError::InvalidBody);
        }
        let mut reader = Reader::new(data);
        let capabilities = Self {
            protocol_version: reader.u8()?,
            max_concurrent_jobs: reader.u8()?,
            feature_flags: {
                if reader.u16()? != 0 {
                    return Err(JobCodecError::InvalidReserved);
                }
                reader.u32()?
            },
            max_inner_request_bytes: reader.u32()?,
            max_inner_response_bytes: reader.u32()?,
            max_total_deadline_ms: reader.u32()?,
            terminal_retention_ms: reader.u32()?,
        };
        validate_capabilities(capabilities)?;
        Ok(capabilities)
    }

    #[cfg(test)]
    pub(super) fn encode(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(CAPABILITIES_BODY_BYTES);
        out.push(self.protocol_version);
        out.push(self.max_concurrent_jobs);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&self.feature_flags.to_le_bytes());
        out.extend_from_slice(&self.max_inner_request_bytes.to_le_bytes());
        out.extend_from_slice(&self.max_inner_response_bytes.to_le_bytes());
        out.extend_from_slice(&self.max_total_deadline_ms.to_le_bytes());
        out.extend_from_slice(&self.terminal_retention_ms.to_le_bytes());
        out
    }

    #[cfg(test)]
    pub(super) const V1: Self = Self {
        protocol_version: PROTOCOL_VERSION,
        max_concurrent_jobs: MAX_CONCURRENT_JOBS,
        feature_flags: ALL_FEATURES,
        max_inner_request_bytes: MAX_INNER_REQUEST_BYTES as u32,
        max_inner_response_bytes: MAX_INNER_RESPONSE_BYTES as u32,
        max_total_deadline_ms: MAX_TOTAL_DEADLINE_MS,
        terminal_retention_ms: TERMINAL_RETENTION_MS,
    };
}

pub(super) fn encode_request(request: JobRequest<'_>) -> Result<Vec<u8>, JobCodecError> {
    validate_request(request)?;
    let mut out = Vec::with_capacity(
        envelope_bytes(REQUEST_NAME)
            + REQUEST_APPLICATION_HEADER_BYTES
            + request.inner_request.len(),
    );
    write_envelope(
        &mut out,
        REQUEST_MESSAGE_ID,
        REQUEST_NAME,
        request.request_id,
    );
    out.push(request.command as u8);
    out.push(0);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&request.client_nonce.to_le_bytes());
    out.extend_from_slice(&request.job_id.to_le_bytes());
    out.extend_from_slice(&request.total_deadline_ms.to_le_bytes());
    out.extend_from_slice(request.inner_request);
    Ok(out)
}

pub(super) fn decode_response(data: &[u8]) -> Result<JobResponse<'_>, JobCodecError> {
    let (request_id, mut reader) = read_envelope(data, RESPONSE_MESSAGE_ID, RESPONSE_NAME)?;
    let response = JobResponse {
        request_id,
        command: JobCommand::decode(reader.u8()?)?,
        state: JobState::decode(reader.u8()?)?,
        error: JobError::decode(reader.u8()?)?,
        flags: reader.u8()?,
        client_nonce: reader.u32()?,
        job_id: reader.u32()?,
        retry_after_ms: reader.u32()?,
        progress_per_mille: reader.u32()?,
        body: reader.rest(),
    };
    validate_response(response)?;
    Ok(response)
}

pub(super) fn decode_capabilities_response(
    data: &[u8],
    expected_request_id: u16,
) -> Result<JobCapabilities, JobCodecError> {
    let response = decode_response(data)?;
    if response.request_id != expected_request_id || response.command != JobCommand::Capabilities {
        return Err(JobCodecError::InvalidIdentity);
    }
    JobCapabilities::decode(response.body)
}

pub(super) fn checked_job_deadline(
    capabilities: JobCapabilities,
    inner_request_bytes: usize,
) -> Result<u32, JobCodecError> {
    if inner_request_bytes == 0
        || inner_request_bytes > MAX_INNER_REQUEST_BYTES
        || inner_request_bytes > capabilities.max_inner_request_bytes as usize
    {
        return Err(JobCodecError::LimitExceeded);
    }
    let deadline = MAX_TOTAL_DEADLINE_MS.min(capabilities.max_total_deadline_ms);
    if deadline == 0 {
        return Err(JobCodecError::InvalidDeadline);
    }
    Ok(deadline)
}

pub(super) fn next_poll_delay_ms(previous_ms: u32, retry_after_ms: u32) -> u32 {
    let hinted = retry_after_ms.clamp(MIN_POLL_DELAY_MS, MAX_POLL_DELAY_MS);
    if previous_ms == 0 {
        return hinted;
    }
    previous_ms
        .saturating_mul(2)
        .max(hinted)
        .min(MAX_POLL_DELAY_MS)
}

#[cfg(test)]
pub(super) fn encode_response(response: JobResponse<'_>) -> Result<Vec<u8>, JobCodecError> {
    validate_response(response)?;
    let mut out = Vec::with_capacity(
        envelope_bytes(RESPONSE_NAME) + RESPONSE_APPLICATION_HEADER_BYTES + response.body.len(),
    );
    write_envelope(
        &mut out,
        RESPONSE_MESSAGE_ID,
        RESPONSE_NAME,
        response.request_id,
    );
    out.push(response.command as u8);
    out.push(response.state as u8);
    out.push(response.error as u8);
    out.push(response.flags);
    out.extend_from_slice(&response.client_nonce.to_le_bytes());
    out.extend_from_slice(&response.job_id.to_le_bytes());
    out.extend_from_slice(&response.retry_after_ms.to_le_bytes());
    out.extend_from_slice(&response.progress_per_mille.to_le_bytes());
    out.extend_from_slice(response.body);
    Ok(out)
}

#[cfg(test)]
pub(super) fn decode_request(data: &[u8]) -> Result<JobRequest<'_>, JobCodecError> {
    let (request_id, mut reader) = read_envelope(data, REQUEST_MESSAGE_ID, REQUEST_NAME)?;
    let command = JobCommand::decode(reader.u8()?)?;
    if reader.u8()? != 0 {
        return Err(JobCodecError::InvalidFlags);
    }
    if reader.u16()? != 0 {
        return Err(JobCodecError::InvalidReserved);
    }
    let request = JobRequest {
        request_id,
        command,
        client_nonce: reader.u32()?,
        job_id: reader.u32()?,
        total_deadline_ms: reader.u32()?,
        inner_request: reader.rest(),
    };
    validate_request(request)?;
    Ok(request)
}

fn validate_request(request: JobRequest<'_>) -> Result<(), JobCodecError> {
    match request.command {
        JobCommand::Capabilities => {
            if request.client_nonce != 0
                || request.job_id != 0
                || request.total_deadline_ms != 0
                || !request.inner_request.is_empty()
            {
                return Err(JobCodecError::InvalidBody);
            }
        }
        JobCommand::Start => {
            if request.client_nonce == 0 || request.job_id != 0 {
                return Err(JobCodecError::InvalidIdentity);
            }
            if request.total_deadline_ms == 0 || request.total_deadline_ms > MAX_TOTAL_DEADLINE_MS {
                return Err(JobCodecError::InvalidDeadline);
            }
            if request.inner_request.len() > MAX_INNER_REQUEST_BYTES {
                return Err(JobCodecError::LimitExceeded);
            }
            if !is_legacy_frame(request.inner_request, is_legacy_request_id) {
                return Err(JobCodecError::InvalidBody);
            }
        }
        JobCommand::Poll | JobCommand::Cancel => {
            if request.client_nonce == 0 || request.job_id == 0 {
                return Err(JobCodecError::InvalidIdentity);
            }
            if request.total_deadline_ms != 0 {
                return Err(JobCodecError::InvalidDeadline);
            }
            if !request.inner_request.is_empty() {
                return Err(JobCodecError::InvalidBody);
            }
        }
    }
    Ok(())
}

fn validate_response(response: JobResponse<'_>) -> Result<(), JobCodecError> {
    if response.flags & !ALL_RESPONSE_FLAGS != 0 {
        return Err(JobCodecError::InvalidFlags);
    }
    if response.progress_per_mille > MAX_PROGRESS_PER_MILLE {
        return Err(JobCodecError::InvalidProgress);
    }

    if response.command == JobCommand::Capabilities {
        if response.state != JobState::None
            || response.error != JobError::None
            || response.flags != 0
            || response.client_nonce != 0
            || response.job_id != 0
            || response.retry_after_ms != 0
            || response.progress_per_mille != 0
        {
            return Err(JobCodecError::InvalidBody);
        }
        JobCapabilities::decode(response.body)?;
        return Ok(());
    }

    if response.client_nonce == 0 {
        return Err(JobCodecError::InvalidIdentity);
    }
    if response.command != JobCommand::Start && response.job_id == 0 {
        return Err(JobCodecError::InvalidIdentity);
    }
    if response.command == JobCommand::Start
        && response.job_id == 0
        && (response.state != JobState::Rejected || response.error == JobError::Conflict)
    {
        return Err(JobCodecError::InvalidIdentity);
    }

    match response.state {
        JobState::None => return Err(JobCodecError::InvalidBody),
        JobState::Accepted | JobState::Pending | JobState::CancelPending | JobState::Completed => {
            if response.error != JobError::None {
                return Err(JobCodecError::InvalidBody);
            }
        }
        JobState::Cancelled => {
            if response.error != JobError::Cancelled {
                return Err(JobCodecError::InvalidBody);
            }
        }
        JobState::Failed | JobState::Rejected => {
            if response.error == JobError::None {
                return Err(JobCodecError::InvalidBody);
            }
        }
    }

    if matches!(
        response.state,
        JobState::Accepted | JobState::Pending | JobState::CancelPending
    ) {
        if response.retry_after_ms > MAX_TOTAL_DEADLINE_MS {
            return Err(JobCodecError::InvalidDeadline);
        }
    } else if response.retry_after_ms != 0 {
        return Err(JobCodecError::InvalidDeadline);
    }

    if response.state == JobState::Completed {
        if response.body.len() > MAX_INNER_RESPONSE_BYTES {
            return Err(JobCodecError::LimitExceeded);
        }
        if !is_legacy_frame(response.body, is_legacy_response_id) {
            return Err(JobCodecError::InvalidBody);
        }
    } else if !response.body.is_empty() {
        return Err(JobCodecError::InvalidBody);
    }

    if response.flags & FLAG_DUPLICATE_START != 0
        && (response.command != JobCommand::Start
            || response.job_id == 0
            || response.state == JobState::Rejected)
    {
        return Err(JobCodecError::InvalidFlags);
    }
    if response.flags & FLAG_TERMINAL_RETAINED != 0
        && (response.command != JobCommand::Poll || !response.state.is_terminal())
    {
        return Err(JobCodecError::InvalidFlags);
    }
    if response.flags & FLAG_CANCEL_TOO_LATE != 0
        && (response.command != JobCommand::Cancel
            || !matches!(
                response.state,
                JobState::Pending | JobState::Completed | JobState::Failed
            ))
    {
        return Err(JobCodecError::InvalidFlags);
    }
    if matches!(
        response.error,
        JobError::LegacyBusy | JobError::LegacyStorageError
    ) && response.flags & FLAG_LEGACY_MAPPED == 0
    {
        return Err(JobCodecError::InvalidFlags);
    }
    if matches!(response.state, JobState::None | JobState::Rejected)
        && response.progress_per_mille != 0
    {
        return Err(JobCodecError::InvalidProgress);
    }
    Ok(())
}

fn validate_capabilities(capabilities: JobCapabilities) -> Result<(), JobCodecError> {
    if capabilities.protocol_version != PROTOCOL_VERSION
        || capabilities.max_concurrent_jobs != MAX_CONCURRENT_JOBS
    {
        return Err(JobCodecError::UnsupportedSchema);
    }
    if capabilities.feature_flags != ALL_FEATURES {
        return Err(JobCodecError::InvalidFlags);
    }
    if capabilities.max_inner_request_bytes != MAX_INNER_REQUEST_BYTES as u32
        || capabilities.max_inner_response_bytes != MAX_INNER_RESPONSE_BYTES as u32
        || capabilities.max_total_deadline_ms != MAX_TOTAL_DEADLINE_MS
        || capabilities.terminal_retention_ms != TERMINAL_RETENTION_MS
    {
        return Err(JobCodecError::InvalidBody);
    }
    Ok(())
}

fn envelope_bytes(name: &[u8]) -> usize {
    1 + 1 + name.len() + 1 + 2
}

fn write_envelope(out: &mut Vec<u8>, message_id: u8, name: &[u8], request_id: u16) {
    out.push(message_id);
    out.push(name.len() as u8);
    out.extend_from_slice(name);
    out.push(SCHEMA);
    out.extend_from_slice(&request_id.to_le_bytes());
}

fn read_envelope<'a>(
    data: &'a [u8],
    expected_message_id: u8,
    expected_name: &[u8],
) -> Result<(u16, Reader<'a>), JobCodecError> {
    let mut reader = Reader::new(data);
    if reader.u8()? != expected_message_id {
        return Err(JobCodecError::InvalidMessageId);
    }
    let name_length = reader.u8()? as usize;
    if reader.bytes(name_length)? != expected_name {
        return Err(JobCodecError::InvalidMessageName);
    }
    if reader.u8()? != SCHEMA {
        return Err(JobCodecError::UnsupportedSchema);
    }
    let request_id = reader.u16()?;
    Ok((request_id, reader))
}

fn is_legacy_frame(data: &[u8], valid_id: fn(u8) -> bool) -> bool {
    if data.len() < 5 || !valid_id(data[0]) {
        return false;
    }
    let name_length = data[1] as usize;
    let schema_offset = 2usize.saturating_add(name_length);
    let header_bytes = schema_offset.saturating_add(3);
    header_bytes <= data.len() && data[schema_offset] == SCHEMA
}

fn is_legacy_request_id(value: u8) -> bool {
    matches!(
        value,
        0xE0 | 0xE2 | 0xE4 | 0xE6 | 0xE8 | 0xEA | 0xEC | 0xF0 | 0xF2 | 0xF4 | 0xF6 | 0xF8 | 0xFA
    )
}

fn is_legacy_response_id(value: u8) -> bool {
    matches!(
        value,
        0xE1 | 0xE3
            | 0xE5
            | 0xE7
            | 0xE9
            | 0xEB
            | 0xED
            | 0xEF
            | 0xF1
            | 0xF3
            | 0xF5
            | 0xF7
            | 0xF9
            | 0xFB
    )
}

#[derive(Clone, Copy)]
struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn u8(&mut self) -> Result<u8, JobCodecError> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, JobCodecError> {
        let data = self.bytes(2)?;
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }

    fn u32(&mut self) -> Result<u32, JobCodecError> {
        let data = self.bytes(4)?;
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    fn bytes(&mut self, size: usize) -> Result<&'a [u8], JobCodecError> {
        let end = self
            .position
            .checked_add(size)
            .ok_or(JobCodecError::Truncated)?;
        if end > self.data.len() {
            return Err(JobCodecError::Truncated);
        }
        let out = &self.data[self.position..end];
        self.position = end;
        Ok(out)
    }

    fn rest(&mut self) -> &'a [u8] {
        let out = &self.data[self.position..];
        self.position = self.data.len();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_request_matches_bridge_golden_bytes() {
        let encoded = encode_request(JobRequest {
            request_id: 0x1234,
            command: JobCommand::Capabilities,
            client_nonce: 0,
            job_id: 0,
            total_deadline_ms: 0,
            inner_request: &[],
        })
        .unwrap();
        let mut expected = vec![REQUEST_MESSAGE_ID, REQUEST_NAME.len() as u8];
        expected.extend_from_slice(REQUEST_NAME);
        expected.extend_from_slice(&[SCHEMA, 0x34, 0x12]);
        expected.extend_from_slice(&[0; REQUEST_APPLICATION_HEADER_BYTES]);
        assert_eq!(encoded, expected);
        assert_eq!(
            decode_request(&encoded).unwrap().command,
            JobCommand::Capabilities
        );
    }

    #[test]
    fn start_request_roundtrips_nonce_deadline_and_inner_frame() {
        let inner = legacy_frame(0xF0, b"FsMkdirRequest", 7, &[0]);
        let request = JobRequest {
            request_id: 8,
            command: JobCommand::Start,
            client_nonce: 0x1020_3040,
            job_id: 0,
            total_deadline_ms: MAX_TOTAL_DEADLINE_MS,
            inner_request: &inner,
        };
        let encoded = encode_request(request).unwrap();
        assert_eq!(decode_request(&encoded).unwrap(), request);
        assert_eq!(encoded[0], REQUEST_MESSAGE_ID);
        assert_eq!(&encoded[encoded.len() - inner.len()..], inner);
    }

    #[test]
    fn request_validation_rejects_ambiguous_identity_and_recursive_body() {
        let inner = legacy_frame(0xF0, b"FsMkdirRequest", 7, &[0]);
        let start = JobRequest {
            request_id: 8,
            command: JobCommand::Start,
            client_nonce: 1,
            job_id: 0,
            total_deadline_ms: 1,
            inner_request: &inner,
        };
        assert_eq!(
            encode_request(JobRequest {
                client_nonce: 0,
                ..start
            }),
            Err(JobCodecError::InvalidIdentity)
        );
        assert_eq!(
            encode_request(JobRequest {
                total_deadline_ms: 0,
                ..start
            }),
            Err(JobCodecError::InvalidDeadline)
        );

        let recursive = legacy_frame(REQUEST_MESSAGE_ID, REQUEST_NAME, 9, &[]);
        assert_eq!(
            encode_request(JobRequest {
                inner_request: &recursive,
                ..start
            }),
            Err(JobCodecError::InvalidBody)
        );
    }

    #[test]
    fn capabilities_response_matches_exact_v1_body() {
        let body = JobCapabilities::V1.encode();
        assert_eq!(
            body,
            [
                1, 2, 0, 0, 0x3F, 0, 0, 0, 0, 0x7F, 0, 0, 0, 0x7F, 0, 0, 0x10, 0x27, 0, 0, 0x30,
                0x75, 0, 0
            ]
        );
        let encoded = encode_response(JobResponse {
            request_id: 9,
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
        .unwrap();
        assert_eq!(
            decode_capabilities_response(&encoded, 9).unwrap(),
            JobCapabilities::V1
        );

        let body_offset = envelope_bytes(RESPONSE_NAME) + RESPONSE_APPLICATION_HEADER_BYTES;
        let mut incompatible = encoded;
        incompatible[body_offset] = 2;
        assert_eq!(
            decode_capabilities_response(&incompatible, 9),
            Err(JobCodecError::UnsupportedSchema)
        );
    }

    #[test]
    fn completed_response_preserves_exact_inner_frame_and_identity() {
        let inner = legacy_frame(0xF1, b"FsMkdirResponse", 7, &[0]);
        let expected = JobResponse {
            request_id: 10,
            command: JobCommand::Poll,
            state: JobState::Completed,
            error: JobError::None,
            flags: FLAG_TERMINAL_RETAINED,
            client_nonce: 0x1122_3344,
            job_id: 17,
            retry_after_ms: 0,
            progress_per_mille: 1_000,
            body: &inner,
        };
        let encoded = encode_response(expected).unwrap();
        assert_eq!(decode_response(&encoded).unwrap(), expected);
    }

    #[test]
    fn response_validation_rejects_wrong_flags_errors_and_body() {
        let pending = JobResponse {
            request_id: 10,
            command: JobCommand::Poll,
            state: JobState::Pending,
            error: JobError::None,
            flags: 0,
            client_nonce: 1,
            job_id: 2,
            retry_after_ms: 5,
            progress_per_mille: 0,
            body: &[],
        };
        assert_eq!(
            encode_response(JobResponse {
                flags: FLAG_TERMINAL_RETAINED,
                ..pending
            }),
            Err(JobCodecError::InvalidFlags)
        );
        assert_eq!(
            encode_response(JobResponse {
                error: JobError::StorageWriteFailed,
                ..pending
            }),
            Err(JobCodecError::InvalidBody)
        );
        assert_eq!(
            encode_response(JobResponse {
                body: &[1],
                ..pending
            }),
            Err(JobCodecError::InvalidBody)
        );
        assert_eq!(
            encode_response(JobResponse {
                state: JobState::Rejected,
                error: JobError::LegacyBusy,
                retry_after_ms: 0,
                ..pending
            }),
            Err(JobCodecError::InvalidFlags)
        );
    }

    #[test]
    fn response_decoder_rejects_every_truncation() {
        let encoded = encode_response(JobResponse {
            request_id: 10,
            command: JobCommand::Poll,
            state: JobState::Pending,
            error: JobError::None,
            flags: 0,
            client_nonce: 1,
            job_id: 2,
            retry_after_ms: 5,
            progress_per_mille: 0,
            body: &[],
        })
        .unwrap();
        for length in 0..encoded.len() {
            assert!(
                decode_response(&encoded[..length]).is_err(),
                "length {length}"
            );
        }
    }

    #[test]
    fn typed_error_values_are_total_and_unknown_is_rejected() {
        for value in 0..=18 {
            assert_eq!(JobError::decode(value).unwrap() as u8, value);
        }
        assert_eq!(JobError::decode(19), Err(JobCodecError::UnknownError));
    }

    #[test]
    fn negotiated_request_limit_and_poll_backoff_are_bounded() {
        assert_eq!(
            checked_job_deadline(JobCapabilities::V1, MAX_INNER_REQUEST_BYTES).unwrap(),
            MAX_TOTAL_DEADLINE_MS
        );
        assert_eq!(
            checked_job_deadline(JobCapabilities::V1, MAX_INNER_REQUEST_BYTES + 1),
            Err(JobCodecError::LimitExceeded)
        );

        let mut delay = 0;
        let mut observed = Vec::new();
        for _ in 0..6 {
            delay = next_poll_delay_ms(delay, 5);
            observed.push(delay);
        }
        assert_eq!(observed, [5, 10, 20, 40, 50, 50]);
        assert_eq!(next_poll_delay_ms(0, 0), MIN_POLL_DELAY_MS);
        assert_eq!(next_poll_delay_ms(0, u32::MAX), MAX_POLL_DELAY_MS);
        assert_eq!(MAX_POLL_COUNT, 1_024);
        assert_eq!(MANAGER_SUPERVISION_MS, 30_000);
    }

    fn legacy_frame(message_id: u8, name: &[u8], request_id: u16, body: &[u8]) -> Vec<u8> {
        let mut out = vec![message_id, name.len() as u8];
        out.extend_from_slice(name);
        out.push(SCHEMA);
        out.extend_from_slice(&request_id.to_le_bytes());
        out.extend_from_slice(body);
        out
    }
}
