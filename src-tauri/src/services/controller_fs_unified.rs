//! Single wire client shared by the application and the integrated interoperability tests.
use super::controller_transport::{BridgeBinaryClient, ControllerRpcBatchItem};
use filesystem_rpc::{self as wire, Error, Frame, Operation, State};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug)]
pub enum Failure {
    Transport(String),
    Local(String),
    Protocol,
    Remote(Error),
    Conditional(Error, ConditionalResult),
    Ambiguous,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub file_type: u8,
    pub size: u32,
    pub truncated: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ConditionalResult {
    pub outcome: u8,
    pub subject: u8,
    pub observed: Option<[u8; 32]>,
}

#[derive(Debug)]
pub struct Capabilities {
    pub operations: u32,
    pub max_chunk: u32,
    pub max_upload: u32,
    pub max_path: u16,
}

pub struct Client {
    bridge: BridgeBinaryClient,
    sequence: u64,
    lifetime: u64,
}
impl Client {
    pub fn new(bridge: BridgeBinaryClient) -> Self {
        Self {
            bridge,
            sequence: 0,
            lifetime: 0,
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
        if self.lifetime == 0 {
            self.capabilities().await?;
        }
        self.exchange(operation, body, nonce, identity, delay, replay_on_loss)
            .await
    }

    async fn exchange(
        &mut self,
        operation: Operation,
        body: &[u8],
        nonce: u32,
        identity: u32,
        delay: u32,
        replay_on_loss: bool,
    ) -> Result<Vec<u8>, Failure> {
        self.sequence = self.sequence.checked_add(1).ok_or(Failure::Protocol)?;
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
            lifetime: if operation == Operation::Capabilities {
                0
            } else {
                self.lifetime
            },
        };
        let mut bytes = vec![0; wire::HEADER + body.len()];
        wire::encode(frame, &mut bytes).ok_or(Failure::Protocol)?;
        let mut replayed = false;
        let response = match self
            .bridge
            .controller_rpc(bytes, wire::RESPONSE, 1000)
            .await
        {
            Ok(response) => response,
            Err(_) if replay_on_loss => {
                replayed = true;
                // Keep operation identity, but retire the timed-out transport waiter.
                self.sequence = self.sequence.checked_add(1).ok_or(Failure::Protocol)?;
                frame.request_id = self.sequence;
                let mut bytes = vec![0; wire::HEADER + body.len()];
                wire::encode(frame, &mut bytes).ok_or(Failure::Protocol)?;
                self.bridge
                    .controller_rpc(bytes, wire::RESPONSE, 1000)
                    .await
                    .map_err(|_| Failure::Ambiguous)?
            }
            Err(error) => return Err(Failure::Transport(error.message)),
        };
        validate_response(&response, frame, replayed)?;
        Ok(response)
    }

    pub async fn capabilities(&mut self) -> Result<Capabilities, Failure> {
        let bytes = self
            .exchange(Operation::Capabilities, &[], 0, 0, 0, false)
            .await?;
        let frame = wire::decode(&bytes).ok_or(Failure::Protocol)?;
        if frame.state != State::Complete || frame.body.len() != 20 {
            return Err(Failure::Protocol);
        }
        let mask = u32::from_le_bytes(frame.body[..4].try_into().unwrap());
        let chunk = u32::from_le_bytes(frame.body[4..8].try_into().unwrap());
        let max_upload = u32::from_le_bytes(frame.body[8..12].try_into().unwrap());
        if mask & 0x7fff != 0x7fff || chunk < 30_720 || max_upload < 524_288 {
            return Err(Failure::Protocol);
        }
        let max_path = u16::from_le_bytes(frame.body[16..18].try_into().unwrap());
        if max_path == 0 || max_path > 192 || frame.body[18] != 1 || frame.body[19] != 32 {
            return Err(Failure::Protocol);
        }
        if frame.lifetime == 0 {
            return Err(Failure::Protocol);
        }
        if self.lifetime != 0 && self.lifetime != frame.lifetime {
            return Err(Failure::Remote(Error::LifetimeChanged));
        }
        // Pin for the entire client lifetime, including TCP reconnect and abort.
        // Adopting a new epoch here could make an old upload ticket target new work.
        self.lifetime = frame.lifetime;
        Ok(Capabilities {
            operations: mask,
            max_chunk: chunk,
            max_upload,
            max_path,
        })
    }

    /// Core assigns the upload identity; the caller retains the mutation nonce for retries.
    #[cfg(test)]
    pub async fn upload(&mut self, path: &str, data: &[u8], nonce: u32) -> Result<(), Failure> {
        self.upload_reader(path, &mut &data[..], data.len(), nonce, |_, _| {})
            .await
    }

    pub async fn upload_reader<R: AsyncRead + Unpin, F: FnMut(usize, usize)>(
        &mut self,
        path: &str,
        reader: &mut R,
        size: usize,
        nonce: u32,
        mut on_progress: F,
    ) -> Result<(), Failure> {
        self.capabilities().await?;
        if nonce == 0 || size > 524_288 {
            return Err(Failure::Protocol);
        }
        let mut begin = (size as u32).to_le_bytes().to_vec();
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
        let mut offset = 0usize;
        let mut body = vec![0; 10 + 30_720.min(size)];
        while offset < size {
            let count = (size - offset).min(30_720);
            body.resize(10 + count, 0);
            body[..4].copy_from_slice(&session.to_le_bytes());
            body[4..8].copy_from_slice(&(offset as u32).to_le_bytes());
            body[8..10].copy_from_slice(&(count as u16).to_le_bytes());
            if let Err(error) = reader.read_exact(&mut body[10..]).await {
                self.abort(session).await;
                return Err(Failure::Local(error.to_string()));
            }
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
                || result.body != ((offset + count) as u32).to_le_bytes()
            {
                self.abort(session).await;
                return Err(Failure::Protocol);
            }
            offset += count;
            on_progress(offset, size);
        }
        // Detect a source that grew after metadata was read, before committing.
        let mut extra = [0];
        match reader.read(&mut extra).await {
            Ok(0) => {}
            Ok(_) => {
                self.abort(session).await;
                return Err(Failure::Local("Source changed size during upload".into()));
            }
            Err(error) => {
                self.abort(session).await;
                return Err(Failure::Local(error.to_string()));
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
        let result = self.mutation_result(operation, body, nonce).await?;
        if result.is_empty() {
            Ok(())
        } else {
            Err(Failure::Protocol)
        }
    }

    pub async fn conditional_replace(
        &mut self,
        path: &str,
        staging: &str,
        expected: &[u8; 32],
        replacement: &[u8; 32],
        nonce: u32,
    ) -> Result<ConditionalResult, Failure> {
        self.capabilities().await?;
        let mut body = expected.to_vec();
        body.extend_from_slice(replacement);
        body.extend(path_body(path)?);
        body.extend(path_body(staging)?);
        conditional_result(
            &self
                .mutation_result(Operation::ConditionalReplace, &body, nonce)
                .await?,
        )
    }

    pub async fn conditional_delete(
        &mut self,
        path: &str,
        expected: &[u8; 32],
        nonce: u32,
    ) -> Result<ConditionalResult, Failure> {
        self.capabilities().await?;
        let mut body = expected.to_vec();
        body.extend(path_body(path)?);
        conditional_result(
            &self
                .mutation_result(Operation::ConditionalDelete, &body, nonce)
                .await?,
        )
    }

    async fn mutation_result(
        &mut self,
        operation: Operation,
        body: &[u8],
        nonce: u32,
    ) -> Result<Vec<u8>, Failure> {
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_secs(12);
        let mut response = self
            .request(operation, body, nonce, 0, 10_000, true)
            .await?;
        loop {
            let frame = wire::decode(&response).ok_or(Failure::Protocol)?;
            if frame.state == State::Complete {
                return Ok(frame.body.to_vec());
            }
            if frame.state != State::Pending {
                return Err(Failure::Protocol);
            }
            let identity = frame.operation_id;
            let next_poll =
                tokio::time::Instant::now() + poll_interval(started.elapsed(), frame.delay_ms);
            tokio::time::sleep_until(next_poll.min(deadline)).await;
            if tokio::time::Instant::now() >= deadline {
                let _ = self
                    .request(Operation::Cancel, &[], nonce, identity, 0, false)
                    .await;
                return Err(Failure::Ambiguous);
            }
            response = self
                .request(Operation::Poll, &[], nonce, identity, 0, true)
                .await?;
        }
    }

    #[cfg(test)]
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
        if size != 0 && self.lifetime == 0 {
            self.capabilities().await?;
        }
        let mut requests = Vec::new();
        let mut expected = Vec::new();
        let mut done = 0;
        while done < size {
            let count = (size - done).min(30_720) as u16;
            let mut body = prefix.clone();
            body.extend_from_slice(&(offset + done).to_le_bytes());
            body.extend_from_slice(&count.to_le_bytes());
            self.sequence = self.sequence.checked_add(1).ok_or(Failure::Protocol)?;
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
                lifetime: self.lifetime,
            };
            let mut payload = vec![0; wire::HEADER + body.len()];
            wire::encode(frame, &mut payload).ok_or(Failure::Protocol)?;
            expected.push((self.sequence, count as usize));
            requests.push(ControllerRpcBatchItem {
                payload,
                expected_response_id: wire::RESPONSE,
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
                lifetime: self.lifetime,
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

fn poll_interval(elapsed: Duration, server_delay_ms: u32) -> Duration {
    // Preserve quick mutations; reduce status traffic for work lasting hundreds of ms.
    // The server hint remains a minimum, including across a transport retry.
    let local_ms = match elapsed.as_millis() {
        0..=99 => 5,
        100..=199 => 10,
        200..=399 => 20,
        _ => 40,
    };
    Duration::from_millis(u64::from(server_delay_ms.max(local_ms)))
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
        || (request.operation != Operation::Capabilities && decoded.lifetime != request.lifetime)
        || (request.operation_id != 0 && decoded.operation_id != request.operation_id)
    {
        return Err(Failure::Protocol);
    }
    if decoded.replayed
        && !replayed
        && !matches!(
            request.operation,
            Operation::ConditionalReplace | Operation::ConditionalDelete
        )
    {
        return Err(Failure::Remote(Error::Conflict));
    }
    if decoded.state == State::Failed || decoded.state == State::Cancelled {
        if decoded.error == Error::LifetimeChanged
            && (request.operation.retained() || request.operation.control())
        {
            return Err(Failure::Ambiguous);
        }
        if !decoded.body.is_empty() {
            return Err(Failure::Conditional(
                decoded.error,
                conditional_result(decoded.body)?,
            ));
        }
        return Err(Failure::Remote(decoded.error));
    }
    Ok(decoded)
}

fn conditional_result(body: &[u8]) -> Result<ConditionalResult, Failure> {
    if body.len() != 35
        || body[0] > 2
        || body[1] > 2
        || body[2] > 1
        || (body[2] == 0 && body[3..].iter().any(|&b| b != 0))
    {
        return Err(Failure::Protocol);
    }
    Ok(ConditionalResult {
        outcome: body[0],
        subject: body[1],
        observed: if body[2] != 0 {
            Some(body[3..].try_into().unwrap())
        } else {
            None
        },
    })
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
    fn polling_preserves_short_operations_and_server_minimum() {
        for elapsed in [0, 50, 99, 100, 199, 200, 399, 400, 10_000] {
            let delay = poll_interval(Duration::from_millis(elapsed), 5);
            assert!(delay >= Duration::from_millis(5));
            assert!(delay <= Duration::from_millis(40));
            if elapsed < 100 {
                assert_eq!(delay, Duration::from_millis(5));
            }
            assert_eq!(
                poll_interval(Duration::from_millis(elapsed), 250),
                Duration::from_millis(250)
            );
        }
        // A two-second operation must require far fewer polls than a fixed 5 ms loop.
        let mut elapsed = Duration::ZERO;
        let mut polls = 0;
        while elapsed < Duration::from_secs(2) {
            elapsed += poll_interval(elapsed, 5);
            polls += 1;
        }
        assert!(polls < 100);
        assert!(elapsed < Duration::from_millis(2040));
    }

    async fn test_request(stream: &mut tokio::net::TcpStream) -> ([u8; 16], Vec<u8>) {
        let mut header = [0; 16];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(&header[..4], b"OCRQ");
        let length = u32::from_le_bytes(header[12..].try_into().unwrap()) as usize;
        assert!(length <= wire::HEADER + wire::MAX_BODY);
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await.unwrap();
        (header, bytes)
    }

    async fn test_reply(
        stream: &mut tokio::net::TcpStream,
        header: [u8; 16],
        mut frame: Frame<'_>,
        state: State,
        error: Error,
        delay_ms: u32,
    ) {
        use tokio::io::AsyncWriteExt;
        frame.state = state;
        frame.error = error;
        frame.operation_id = 7;
        frame.delay_ms = delay_ms;
        frame.body = &[];
        let mut payload = vec![0; wire::HEADER];
        wire::encode(frame, &mut payload).unwrap();
        let mut reply = b"OCRS".to_vec();
        reply.extend([1, 0]);
        reply.extend(&header[6..8]);
        reply.extend((payload.len() as u32).to_le_bytes());
        reply.extend([0; 4]);
        reply.extend(payload);
        stream.write_all(&reply).await.unwrap();
    }

    #[test]
    fn progressive_polling_survives_reconnect_and_preserves_terminal_outcomes() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                for (state, error) in [
                    (State::Complete, Error::None),
                    (State::Failed, Error::DeadlineExceeded),
                    (State::Cancelled, Error::Cancelled),
                    (State::Failed, Error::ResultExpired),
                    (State::Failed, Error::LifetimeChanged),
                ] {
                    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                        .await
                        .unwrap();
                    let port = listener.local_addr().unwrap().port();
                    let server = tokio::spawn(async move {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let start = tokio::time::Instant::now();
                        let mut polls = 0;
                        let mut sequence = 0;
                        loop {
                            let (header, bytes) = test_request(&mut stream).await;
                            let frame = wire::decode(&bytes).unwrap();
                            assert_eq!((frame.nonce, frame.lifetime), (99, 11));
                            assert!(frame.request_id > sequence);
                            if sequence == 0 {
                                assert_eq!(
                                    (frame.operation, frame.operation_id),
                                    (Operation::Mkdir, 0)
                                );
                            } else {
                                assert_eq!(
                                    (frame.operation, frame.operation_id),
                                    (Operation::Poll, 7)
                                );
                                polls += 1;
                            }
                            sequence = frame.request_id;
                            if polls == 2 {
                                // Lose a pending result and force a new TCP connection.
                                drop(stream);
                                stream = listener.accept().await.unwrap().0;
                                continue;
                            }
                            if start.elapsed() >= Duration::from_millis(450) {
                                test_reply(&mut stream, header, frame, state, error, 0).await;
                                assert!(polls < 50, "{polls} polls");
                                break;
                            }
                            test_reply(&mut stream, header, frame, State::Pending, Error::None, 5)
                                .await;
                        }
                    });
                    let mut client = Client::new(BridgeBinaryClient::new(port));
                    client.lifetime = 11;
                    let result = tokio::time::timeout(
                        Duration::from_secs(5),
                        client.mutation(Operation::Mkdir, &path_body("projects/poll").unwrap(), 99),
                    )
                    .await
                    .unwrap();
                    match error {
                        Error::None => assert!(result.is_ok()),
                        Error::LifetimeChanged => {
                            assert!(matches!(result, Err(Failure::Ambiguous)))
                        }
                        expected => assert!(
                            matches!(result, Err(Failure::Remote(actual)) if actual == expected)
                        ),
                    }
                    server.await.unwrap();
                }
            });
    }

    #[test]
    fn server_wait_cannot_postpone_cancellation_past_local_deadline() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                    .await
                    .unwrap();
                let port = listener.local_addr().unwrap().port();
                let server = tokio::spawn(async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let start = tokio::time::Instant::now();
                    for operation in [Operation::Mkdir, Operation::Poll, Operation::Cancel] {
                        let (header, bytes) = test_request(&mut stream).await;
                        let frame = wire::decode(&bytes).unwrap();
                        assert_eq!(
                            (frame.operation, frame.nonce, frame.lifetime),
                            (operation, 99, 11)
                        );
                        assert_eq!(
                            frame.operation_id,
                            if operation == Operation::Mkdir { 0 } else { 7 }
                        );
                        if operation == Operation::Cancel {
                            assert!(start.elapsed() >= Duration::from_millis(11_900));
                            test_reply(
                                &mut stream,
                                header,
                                frame,
                                State::Failed,
                                Error::CancelTooLate,
                                0,
                            )
                            .await;
                        } else {
                            test_reply(
                                &mut stream,
                                header,
                                frame,
                                State::Pending,
                                Error::None,
                                10_000,
                            )
                            .await;
                        }
                    }
                });
                let mut client = Client::new(BridgeBinaryClient::new(port));
                client.lifetime = 11;
                let result = tokio::time::timeout(
                    Duration::from_secs(15),
                    client.mutation(
                        Operation::Mkdir,
                        &path_body("projects/deadline").unwrap(),
                        99,
                    ),
                )
                .await
                .unwrap();
                assert!(matches!(result, Err(Failure::Ambiguous)));
                server.await.unwrap();
            });
    }

    #[test]
    fn reboot_during_mutation_pins_retry_abort_and_renegotiation_to_old_lifetime() {
        use tokio::{io::AsyncWriteExt, net::TcpListener};
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                for lose_admission in [false, true] {
                    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
                    let port = listener.local_addr().unwrap().port();
                    let server = tokio::spawn(async move {
                        let (mut stream, _) = listener.accept().await.unwrap();
                        let mut capabilities = Vec::new();
                        for n in [0x7fffu32, 30720, 524288, 30000] {
                            capabilities.extend(n.to_le_bytes());
                        }
                        capabilities.extend([192, 0, 1, 32]);
                        let stages = [
                            (
                                Operation::Capabilities,
                                0,
                                State::Complete,
                                Error::None,
                                0,
                                11,
                            ),
                            (Operation::Mkdir, 11, State::Pending, Error::None, 7, 11),
                            (
                                if lose_admission {
                                    Operation::Mkdir
                                } else {
                                    Operation::Poll
                                },
                                11,
                                State::Failed,
                                Error::LifetimeChanged,
                                if lose_admission { 0 } else { 7 },
                                11,
                            ),
                            (
                                Operation::Capabilities,
                                0,
                                State::Complete,
                                Error::None,
                                0,
                                22,
                            ),
                            (
                                Operation::UploadAbort,
                                11,
                                State::Failed,
                                Error::LifetimeChanged,
                                0,
                                11,
                            ),
                            (
                                Operation::Capabilities,
                                0,
                                State::Complete,
                                Error::None,
                                0,
                                22,
                            ),
                            (Operation::Stat, 22, State::Complete, Error::None, 0, 22),
                        ];
                        for (
                            index,
                            (operation, lifetime, state, error, identity, response_lifetime),
                        ) in stages.into_iter().enumerate()
                        {
                            if index == 5 || (index == 2 && lose_admission) {
                                stream = listener.accept().await.unwrap().0;
                            }
                            let mut header = [0; 16];
                            stream.read_exact(&mut header).await.unwrap();
                            assert_eq!(&header[..4], b"OCRQ");
                            let length =
                                u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
                            assert!(length <= wire::HEADER + wire::MAX_BODY);
                            let mut payload = vec![0; length];
                            stream.read_exact(&mut payload).await.unwrap();
                            let mut frame = wire::decode(&payload).unwrap();
                            assert_eq!((frame.operation, frame.lifetime), (operation, lifetime));
                            frame.state = state;
                            frame.error = error;
                            frame.operation_id = identity;
                            frame.delay_ms = if state == State::Pending { 1 } else { 0 };
                            frame.lifetime = response_lifetime;
                            frame.body = if operation == Operation::Capabilities {
                                &capabilities
                            } else if operation == Operation::Stat {
                                &[1, 42, 0, 0, 0]
                            } else {
                                &[]
                            };
                            let lost = index == 1 && lose_admission;
                            let mut response = vec![0; wire::HEADER + frame.body.len()];
                            wire::encode(frame, &mut response).unwrap();
                            if lost {
                                response.clear();
                            }
                            let mut reply = b"OCRS".to_vec();
                            reply.extend([1, if lost { 4 } else { 0 }]);
                            reply.extend(&header[6..8]);
                            reply.extend((response.len() as u32).to_le_bytes());
                            reply.extend([0; 4]);
                            reply.extend(response);
                            stream.write_all(&reply).await.unwrap();
                        }
                    });
                    let mut client = Client::new(BridgeBinaryClient::new(port));
                    assert!(matches!(
                        client.mkdir("projects/reboot", 99).await,
                        Err(Failure::Ambiguous)
                    ));
                    assert!(matches!(
                        client.capabilities().await,
                        Err(Failure::Remote(Error::LifetimeChanged))
                    ));
                    client.abort(7).await;
                    assert_eq!(client.lifetime, 11);
                    client.close().await;
                    let mut fresh = Client::new(BridgeBinaryClient::new(port));
                    assert_eq!(fresh.stat("projects/new").await.unwrap(), (1, 42));
                    server.await.unwrap();
                }
            });
    }

    #[test]
    fn conditional_details_are_exact_and_canonical() {
        let mut body = [0; 35];
        body[0] = 1;
        assert_eq!(
            conditional_result(&body).unwrap(),
            ConditionalResult {
                outcome: 1,
                subject: 0,
                observed: None
            }
        );
        for size in 0..35 {
            assert!(conditional_result(&body[..size]).is_err());
        }
        for index in 0..3 {
            let mut invalid = body;
            invalid[index] = 3;
            assert!(conditional_result(&invalid).is_err());
        }
        body[3] = 7;
        assert!(conditional_result(&body).is_err());
        body[2] = 1;
        assert_eq!(conditional_result(&body).unwrap().observed.unwrap()[0], 7);
        let mut trailing = body.to_vec();
        trailing.push(0);
        assert!(conditional_result(&trailing).is_err());
    }

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

    #[test]
    fn read_batch_bounds_are_checked_before_transport() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
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
            });
    }
}
