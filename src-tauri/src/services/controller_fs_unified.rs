//! First unified-protocol vertical slice. Not connected to the UI until migration completes.
use super::controller_fs::{BridgeBinaryClient, ControllerRpcBatchItem, FsMessageId};
use filesystem_rpc::{self as wire, Error, Frame, Operation, State};
use std::time::Duration;

#[derive(Debug)]
pub enum Failure {
    Transport(String),
    Protocol,
    Remote(Error),
    Ambiguous,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub file_type: u8,
    pub size: u32,
    pub truncated: bool,
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
        validate_response(&response, frame, replayed)?;
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
        if mask & 0x67ff != 0x67ff || chunk < 30_720 || max_upload < 524_288 {
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
        let result = self
            .mutation(Operation::UploadCommit, &session.to_le_bytes(), nonce)
            .await;
        // Abort only addresses this upload and cannot cancel an admitted commit.
        if result.is_err() {
            self.abort(session).await;
        }
        result
    }

    pub async fn mkdir(&mut self, path: &str, nonce: u32) -> Result<(), Failure> {
        self.capabilities().await?;
        self.mutation(Operation::Mkdir, &path_body(path)?, nonce)
            .await
    }

    pub async fn rename(&mut self, from: &str, to: &str, nonce: u32) -> Result<(), Failure> {
        self.capabilities().await?;
        let mut body = path_body(from)?;
        body.extend(path_body(to)?);
        self.mutation(Operation::Rename, &body, nonce).await
    }

    pub async fn delete(&mut self, path: &str, recursive: bool, nonce: u32) -> Result<(), Failure> {
        self.capabilities().await?;
        let mut body = path_body(path)?;
        body.push(u8::from(recursive));
        self.mutation(Operation::Delete, &body, nonce).await
    }

    async fn mutation(
        &mut self,
        operation: Operation,
        body: &[u8],
        nonce: u32,
    ) -> Result<(), Failure> {
        let started = tokio::time::Instant::now();
        let mut response = self
            .request(operation, body, nonce, 0, 10_000, true)
            .await?;
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

    pub async fn stat(&mut self, path: &str) -> Result<(u8, u32), Failure> {
        let bytes = self
            .request(Operation::Stat, &path_body(path)?, 0, 0, 0, false)
            .await?;
        let frame = wire::decode(&bytes).ok_or(Failure::Protocol)?;
        let body = frame.body;
        if frame.state != State::Complete || body.len() != 5 || body[0] > 3 {
            return Err(Failure::Protocol);
        }
        Ok((body[0], u32::from_le_bytes(body[1..].try_into().unwrap())))
    }

    pub async fn list(&mut self, path: &str) -> Result<Vec<Entry>, Failure> {
        let prefix = path_body(path)?;
        let mut entries = Vec::new();
        let mut snapshot = 0u32;
        loop {
            let mut body = prefix.clone();
            body.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            body.push(8);
            body.extend_from_slice(&snapshot.to_le_bytes());
            let bytes = self.request(Operation::List, &body, 0, 0, 0, false).await?;
            let frame = wire::decode(&bytes).ok_or(Failure::Protocol)?;
            if frame.state != State::Complete {
                return Err(Failure::Protocol);
            }
            let (identity, more, page) = list_page(frame.body, entries.len(), snapshot)?;
            snapshot = identity;
            entries.extend(page);
            if !more {
                return Ok(entries);
            }
        }
    }

    /// Up to eight reads in flight, with exact lengths; callers stream successive batches.
    pub async fn read_batch(
        &mut self,
        path: &str,
        offset: u32,
        size: u32,
    ) -> Result<Vec<Vec<u8>>, Failure> {
        let prefix = path_body(path)?;
        if size > 8 * 30_720 || offset.checked_add(size).is_none() {
            return Err(Failure::Protocol);
        }
        let mut requests = Vec::new();
        let mut expected = Vec::new();
        let mut done = 0;
        while done < size {
            let count = (size - done).min(30_720) as u16;
            let mut body = prefix.clone();
            body.extend_from_slice(&(offset + done).to_le_bytes());
            body.extend_from_slice(&count.to_le_bytes());
            self.sequence = self.sequence.wrapping_add(1).max(1);
            let frame = Frame {
                operation: Operation::Read,
                state: State::Request,
                request_id: self.sequence,
                error: Error::None,
                nonce: 0,
                operation_id: 0,
                delay_ms: 0,
                body: &body,
                replayed: false,
            };
            let mut payload = vec![0; wire::HEADER + body.len()];
            wire::encode(frame, &mut payload).ok_or(Failure::Protocol)?;
            expected.push((self.sequence, count as usize));
            requests.push(ControllerRpcBatchItem {
                payload,
                expected_response_id: FsMessageId::JobResponse,
                timeout_ms: 1000,
            });
            done += u32::from(count);
        }
        let mut responses = self
            .bridge
            .controller_rpc_batch(&requests)
            .await
            .map_err(|e| Failure::Transport(e.message))?;
        if responses.len() != expected.len() {
            return Err(Failure::Protocol);
        }
        for (bytes, (id, count)) in responses.iter_mut().zip(expected) {
            let request = Frame {
                operation: Operation::Read,
                state: State::Request,
                request_id: id,
                error: Error::None,
                nonce: 0,
                operation_id: 0,
                delay_ms: 0,
                body: &[],
                replayed: false,
            };
            let response = validate_response(bytes, request, false)?;
            if response.state != State::Complete || response.body.len() != count {
                return Err(Failure::Protocol);
            }
            bytes.drain(..wire::HEADER);
        }
        Ok(responses)
    }
}

fn validate_response<'a>(
    bytes: &'a [u8],
    request: Frame<'_>,
    replayed: bool,
) -> Result<Frame<'a>, Failure> {
    let decoded = wire::decode(bytes).ok_or(Failure::Protocol)?;
    if decoded.state == State::Request
        || decoded.request_id != request.request_id
        || decoded.operation != request.operation
        || decoded.nonce != request.nonce
        || (request.operation_id != 0 && decoded.operation_id != request.operation_id)
    {
        return Err(Failure::Protocol);
    }
    if decoded.replayed && !replayed {
        return Err(Failure::Remote(Error::Conflict));
    }
    if decoded.state == State::Failed || decoded.state == State::Cancelled {
        return Err(Failure::Remote(decoded.error));
    }
    Ok(decoded)
}

fn take<'a>(body: &mut &'a [u8], count: usize) -> Result<&'a [u8], Failure> {
    let bytes = body.get(..count).ok_or(Failure::Protocol)?;
    *body = &body[count..];
    Ok(bytes)
}

fn list_page(
    mut body: &[u8],
    start: usize,
    snapshot: u32,
) -> Result<(u32, bool, Vec<Entry>), Failure> {
    let id = u32::from_le_bytes(take(&mut body, 4)?.try_into().unwrap());
    let index = u16::from_le_bytes(take(&mut body, 2)?.try_into().unwrap());
    let count = take(&mut body, 1)?[0] as usize;
    let more = take(&mut body, 1)?[0];
    if !matches!(more, 0 | 1)
        || id == 0
        || (snapshot != 0 && snapshot != id)
        || index as usize != start
        || count > 8
        || start + count > 256
        || (more == 1 && (count == 0 || start + count >= 256))
    {
        return Err(Failure::Protocol);
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let len = take(&mut body, 1)?[0] as usize;
        if len == 0 || len >= 64 {
            return Err(Failure::Protocol);
        }
        let name = std::str::from_utf8(take(&mut body, len)?).map_err(|_| Failure::Protocol)?;
        if name.contains(['\0', '/', '\\']) || matches!(name, "." | "..") {
            return Err(Failure::Protocol);
        }
        let file_type = take(&mut body, 1)?[0];
        let size = u32::from_le_bytes(take(&mut body, 4)?.try_into().unwrap());
        let truncated = take(&mut body, 1)?[0];
        if file_type > 3 || truncated > 1 {
            return Err(Failure::Protocol);
        }
        entries.push(Entry {
            name: name.to_owned(),
            file_type,
            size,
            truncated: truncated != 0,
        });
    }
    if !body.is_empty() {
        return Err(Failure::Protocol);
    }
    Ok((id, more != 0, entries))
}

fn path_body(path: &str) -> Result<Vec<u8>, Failure> {
    if path.is_empty() || path.len() > 192 || path.as_bytes().contains(&0) {
        return Err(Failure::Protocol);
    }
    let mut body = vec![path.len() as u8];
    body.extend_from_slice(path.as_bytes());
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_rejects_truncation_stale_identity_and_noncanonical_fields() {
        let mut page = vec![4, 0, 0, 0, 0, 0, 1, 0, 7];
        page.extend_from_slice(b"one.bin");
        page.extend_from_slice(&[1, 9, 0, 0, 0, 0]);
        let (id, more, entries) = list_page(&page, 0, 0).unwrap();
        assert_eq!(
            (id, more, entries[0].name.as_str(), entries[0].size),
            (4, false, "one.bin", 9)
        );
        for end in 0..page.len() {
            assert!(list_page(&page[..end], 0, 0).is_err());
        }
        assert!(list_page(&page, 1, 0).is_err());
        assert!(list_page(&page, 0, 5).is_err());
        for (index, value) in [
            (0, 0),
            (6, 9),
            (7, 2),
            (8, 64),
            (9, 0),
            (9, b'/'),
            (16, 4),
            (21, 2),
        ] {
            let mut bad = page.clone();
            bad[index] = value;
            assert!(
                list_page(&bad, 0, 0).is_err(),
                "byte {index} accepted {value}"
            );
        }
        page.push(0);
        assert!(list_page(&page, 0, 0).is_err());
        assert!(list_page(&[4, 0, 0, 0, 0, 0, 0, 1], 0, 0).is_err());
        assert!(list_page(&[4, 0, 0, 0, 0, 1, 0, 1], 256, 4).is_err());
        assert_eq!(
            list_page(&[4, 0, 0, 0, 0, 0, 0, 0], 0, 0).unwrap(),
            (4, false, vec![])
        );
    }

    #[tokio::test]
    async fn read_batch_bounds_are_checked_before_transport() {
        let mut client = Client::new(BridgeBinaryClient::new(0));
        assert!(matches!(
            client.read_batch("projects/a", 0, 8 * 30_720 + 1).await,
            Err(Failure::Protocol)
        ));
        assert!(matches!(
            client.read_batch("projects/a", u32::MAX, 1).await,
            Err(Failure::Protocol)
        ));
        assert!(matches!(
            client.read_batch("", 0, 1).await,
            Err(Failure::Protocol)
        ));
        assert!(client
            .read_batch("projects/a", 0, 0)
            .await
            .unwrap()
            .is_empty());
    }
}
