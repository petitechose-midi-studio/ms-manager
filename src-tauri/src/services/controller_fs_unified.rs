//! First unified-protocol vertical slice. Not connected to the UI until migration completes.
use super::controller_fs::{BridgeBinaryClient, FsMessageId};
use filesystem_rpc::{self as wire, Error, Frame, Operation, State};
use std::time::Duration;

#[derive(Debug)]
pub enum Failure {
    Transport(String),
    Protocol,
    Remote(Error),
    Ambiguous,
}

pub struct Client {
    bridge: BridgeBinaryClient,
    sequence: u16,
}
impl Client {
    pub fn new(bridge: BridgeBinaryClient) -> Self {
        Self {
            bridge,
            sequence: 0,
        }
    }
    pub async fn close(&mut self) {
        self.bridge.close().await;
    }
    async fn abort(&mut self, session: u32) {
        let _ = self
            .request(
                Operation::UploadAbort,
                &session.to_le_bytes(),
                0,
                0,
                0,
                false,
            )
            .await;
    }

    async fn request(
        &mut self,
        operation: Operation,
        body: &[u8],
        nonce: u32,
        identity: u32,
        delay: u32,
        replay_on_loss: bool,
    ) -> Result<Vec<u8>, Failure> {
        self.sequence = self.sequence.wrapping_add(1).max(1);
        let mut frame = Frame {
            operation,
            state: State::Request,
            request_id: self.sequence,
            error: Error::None,
            nonce,
            operation_id: identity,
            delay_ms: delay,
            body,
            replayed: false,
        };
        let mut bytes = vec![0; wire::HEADER + body.len()];
        wire::encode(frame, &mut bytes).ok_or(Failure::Protocol)?;
        let mut replayed = false;
        let response = match self
            .bridge
            .controller_rpc(bytes, FsMessageId::JobResponse, 1000)
            .await
        {
            Ok(response) => response,
            Err(_) if replay_on_loss => {
                replayed = true;
                // Keep operation identity, but retire the timed-out transport waiter.
                self.sequence = self.sequence.wrapping_add(1).max(1);
                frame.request_id = self.sequence;
                let mut bytes = vec![0; wire::HEADER + body.len()];
                wire::encode(frame, &mut bytes).ok_or(Failure::Protocol)?;
                self.bridge
                    .controller_rpc(bytes, FsMessageId::JobResponse, 1000)
                    .await
                    .map_err(|_| Failure::Ambiguous)?
            }
            Err(error) => return Err(Failure::Transport(error.message)),
        };
        let decoded = wire::decode(&response).ok_or(Failure::Protocol)?;
        if decoded.state == State::Request
            || decoded.request_id != frame.request_id
            || decoded.operation != operation
            || decoded.nonce != nonce
            || (identity != 0 && decoded.operation_id != identity)
        {
            return Err(Failure::Protocol);
        }
        if decoded.replayed && !replayed {
            return Err(Failure::Remote(Error::Conflict));
        }
        if decoded.state == State::Failed || decoded.state == State::Cancelled {
            return Err(Failure::Remote(decoded.error));
        }
        Ok(response)
    }

    pub async fn capabilities(&mut self) -> Result<(), Failure> {
        let bytes = self
            .request(Operation::Capabilities, &[], 0, 0, 0, false)
            .await?;
        let frame = wire::decode(&bytes).ok_or(Failure::Protocol)?;
        if frame.state != State::Complete || frame.body.len() != 20 {
            return Err(Failure::Protocol);
        }
        let mask = u32::from_le_bytes(frame.body[..4].try_into().unwrap());
        let chunk = u32::from_le_bytes(frame.body[4..8].try_into().unwrap());
        let max_upload = u32::from_le_bytes(frame.body[8..12].try_into().unwrap());
        if mask & 0x60fb != 0x60fb || chunk < 30_720 || max_upload < 524_288 {
            return Err(Failure::Protocol);
        }
        Ok(())
    }

    /// Core assigns the upload identity; the caller retains the mutation nonce for retries.
    pub async fn upload(&mut self, path: &str, data: &[u8], nonce: u32) -> Result<(), Failure> {
        self.capabilities().await?;
        if nonce == 0 || data.len() > 524_288 {
            return Err(Failure::Protocol);
        }
        let mut begin = (data.len() as u32).to_le_bytes().to_vec();
        begin.extend_from_slice(&path_body(path)?);
        let admitted = self
            .request(Operation::UploadBegin, &begin, 0, 0, 0, false)
            .await?;
        let admitted = wire::decode(&admitted).ok_or(Failure::Protocol)?;
        if admitted.state != State::Complete || admitted.body.len() != 4 {
            return Err(Failure::Protocol);
        }
        let session = u32::from_le_bytes(admitted.body.try_into().unwrap());
        if session == 0 {
            return Err(Failure::Protocol);
        }
        // If Begin's reply is lost, its identity is unknown: let Core expire the
        // staging session. Guessing an Abort identity could cancel another upload.
        for (i, data) in data.chunks(30_720).enumerate() {
            let mut body = session.to_le_bytes().to_vec();
            body.extend_from_slice(&((i * 30_720) as u32).to_le_bytes());
            body.extend_from_slice(&(data.len() as u16).to_le_bytes());
            body.extend_from_slice(data);
            let response = match self
                .request(Operation::UploadChunk, &body, 0, 0, 0, false)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    self.abort(session).await;
                    return Err(error);
                }
            };
            let result = wire::decode(&response).ok_or(Failure::Protocol)?;
            if result.state != State::Complete
                || result.body != ((i * 30_720 + data.len()) as u32).to_le_bytes()
            {
                self.abort(session).await;
                return Err(Failure::Protocol);
            }
        }
        let started = tokio::time::Instant::now();
        let mut response = match self
            .request(
                Operation::UploadCommit,
                &session.to_le_bytes(),
                nonce,
                0,
                10_000,
                true,
            )
            .await
        {
            Ok(response) => response,
            Err(error) => {
                // Abort only addresses this upload and cannot cancel a pending
                // commit. Release staging after rejection without masking an
                // ambiguous result if the commit may already have executed.
                self.abort(session).await;
                return Err(error);
            }
        };
        loop {
            let frame = wire::decode(&response).ok_or(Failure::Protocol)?;
            if frame.state == State::Complete {
                return if frame.body.is_empty() {
                    Ok(())
                } else {
                    Err(Failure::Protocol)
                };
            }
            if frame.state != State::Pending {
                return Err(Failure::Protocol);
            }
            let identity = frame.operation_id;
            if started.elapsed() >= Duration::from_secs(12) {
                let _ = self
                    .request(Operation::Cancel, &[], nonce, identity, 0, false)
                    .await;
                return Err(Failure::Ambiguous);
            }
            tokio::time::sleep(Duration::from_millis(u64::from(frame.delay_ms))).await;
            response = self
                .request(Operation::Poll, &[], nonce, identity, 0, true)
                .await?;
        }
    }

    pub async fn read(&mut self, path: &str, offset: u32, size: u16) -> Result<Vec<u8>, Failure> {
        if size > 30_720 {
            return Err(Failure::Protocol);
        }
        let mut body = path_body(path)?;
        body.extend_from_slice(&offset.to_le_bytes());
        body.extend_from_slice(&size.to_le_bytes());
        let response = self.request(Operation::Read, &body, 0, 0, 0, false).await?;
        let frame = wire::decode(&response).ok_or(Failure::Protocol)?;
        if frame.state != State::Complete || frame.body.len() > size as usize {
            return Err(Failure::Protocol);
        }
        Ok(frame.body.to_vec())
    }
}

fn path_body(path: &str) -> Result<Vec<u8>, Failure> {
    if path.is_empty() || path.len() > 192 || path.as_bytes().contains(&0) {
        return Err(Failure::Protocol);
    }
    let mut body = vec![path.len() as u8];
    body.extend_from_slice(path.as_bytes());
    Ok(body)
}
