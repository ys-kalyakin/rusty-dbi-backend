//! The DBI command loop.

use crate::filelist::FileList;
use crate::protocol::*;
use crate::source::Sources;
use crate::transport::{Transport, TransportError};
use std::collections::HashMap;
use std::sync::Arc;

/// Fatal errors from the command loop. Transport errors signal a lost connection.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("cannot serve range: {0}")]
    Range(String),
}

/// A message from the command loop to its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerUpdate {
    /// A human-readable log line.
    Log(String),
    /// Transfer progress for a file range being served.
    Progress { name: String, sent: u64, total: u64 },
    /// A file range was streamed to the peer in full.
    Completed { name: String, bytes: u64 },
}

/// Upper bound accepted for a FILE_RANGE request body (16-byte fixed part plus the file
/// name). Larger sizes are rejected instead of allocating an untrusted amount of memory.
const MAX_RANGE_HEADER_LEN: usize = 0x4000;

/// The streamed sequence of one file on one connection.
///
/// `position` records the byte offset the file's streamed ranges have reached;
/// `None` means the file's streaming has not started (or was dropped after a
/// completion). `announced` says whether the current sequence already emitted
/// its one start announcement.
#[derive(Debug, Default)]
struct StreamState {
    position: Option<u64>,
    announced: bool,
}

/// Serves DBI requests read from a [`Transport`].
pub struct Server<T: Transport> {
    transport: T,
    files: FileList,
    /// Opens files through their entries' sources: the local filesystem or a
    /// store the caller wires in (a hub over connected shares later).
    store: Arc<dyn Sources>,
    /// Streamed-sequence state per file, cleared when that file's streaming
    /// completes so a later re-read announces a fresh start.
    streams: HashMap<String, StreamState>,
}

impl<T: Transport> Server<T> {
    pub fn new(transport: T, files: FileList, store: Arc<dyn Sources>) -> Self {
        Self {
            transport,
            files,
            store,
            streams: HashMap::new(),
        }
    }

    /// Consume the server and return the transport (used by tests).
    #[doc(hidden)]
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Run the command loop until the Switch issues EXIT or the transport fails.
    pub async fn run<F: FnMut(ServerUpdate)>(&mut self, mut log: F) -> Result<(), ServerError> {
        log(ServerUpdate::Log("Entering command loop".into()));
        loop {
            let bytes = self.transport.read(HEADER_LEN).await?;
            let header = Header::parse(&bytes)
                .ok_or_else(|| ServerError::Protocol("short header".into()))?;
            if !header.is_valid() {
                log(ServerUpdate::Log("Ignoring unknown framing".into()));
                continue;
            }
            match header.cmd_id {
                CMD_ID_EXIT => {
                    self.process_exit(&mut log).await?;
                    return Ok(());
                }
                CMD_ID_FILE_RANGE => {
                    self.process_file_range(header.data_size, &mut log).await?;
                }
                CMD_ID_LIST => {
                    self.process_list(&mut log).await?;
                }
                _ => {}
            }
        }
    }

    async fn process_exit<F: FnMut(ServerUpdate)>(
        &mut self,
        log: &mut F,
    ) -> Result<(), ServerError> {
        log(ServerUpdate::Log("Exit".into()));
        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes();
        self.transport.write(&resp).await?;
        Ok(())
    }

    async fn process_list<F: FnMut(ServerUpdate)>(
        &mut self,
        log: &mut F,
    ) -> Result<(), ServerError> {
        log(ServerUpdate::Log("Get list".into()));
        let payload = build_list(self.files.names().cloned());
        let len = payload.len() as u32;
        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, len).to_bytes();
        self.transport.write(&resp).await?;
        if len > 0 {
            self.transport.read(HEADER_LEN).await?;
            self.transport.write(&payload).await?;
        }
        Ok(())
    }

    async fn process_file_range<F: FnMut(ServerUpdate)>(
        &mut self,
        data_size: u32,
        log: &mut F,
    ) -> Result<(), ServerError> {
        if data_size as usize > MAX_RANGE_HEADER_LEN {
            log(ServerUpdate::Log(format!(
                "Range header too large: {data_size} bytes"
            )));
            return Err(ServerError::Protocol("file range header too large".into()));
        }

        let ack = Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, data_size).to_bytes();
        self.transport.write(&ack).await?;

        let range_bytes = self.transport.read(data_size as usize).await?;
        let range = FileRangeHeader::parse(&range_bytes)
            .ok_or_else(|| ServerError::Protocol("short file range header".into()))?;

        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, range.range_size).to_bytes();
        self.transport.write(&resp).await?;

        self.transport.read(HEADER_LEN).await?;

        let Some(entry) = self.files.get(&range.name).cloned() else {
            log(ServerUpdate::Log(format!("Unknown file: {}", range.name)));
            return Err(ServerError::Range(format!("unknown file: {}", range.name)));
        };
        let source = entry.source.clone();
        let recorded_size = entry.size;

        let mut handle = match self.store.open(&source).await {
            Ok(handle) => handle,
            Err(e) => {
                log(ServerUpdate::Log(format!("Failed to open {source}: {e}")));
                return Err(ServerError::Range(format!("failed to open {source}: {e}")));
            }
        };

        // Decide whether this range continues the file's streamed sequence: it
        // either starts the file at its first byte — opening a fresh sequence or
        // restarting one that streamed before — or continues exactly where the
        // file's streamed position left off. Anything else is an auxiliary read:
        // served byte for byte as requested, but silent about progress and
        // completion, and it leaves the streamed position untouched.
        let (track, announce_start) = {
            let state = self.streams.entry(range.name.clone()).or_default();
            let restart =
                state.position.is_some_and(|position| position > 0) && range.range_offset == 0;
            if restart {
                state.position = None;
                state.announced = false;
            }
            let track = match state.position {
                None => range.range_offset == 0,
                Some(position) => range.range_offset == position,
            };
            let announce_start = track && !state.announced;
            if announce_start {
                state.announced = true;
            }
            (track, announce_start)
        };

        // One significant event per served sequence: announce the file's start
        // only when this range opens or restarts the file's streamed sequence
        // (auxiliary reads stay silent).
        if announce_start {
            log(ServerUpdate::Log(format!("Sending: {}", range.name)));
        }

        let total = handle.size().await.unwrap_or(recorded_size);
        // `sent` doubles as the stream position: it starts at the requested
        // offset and advances by the bytes actually served, exactly as before.
        let mut sent = range.range_offset;
        let mut last_percent = None;

        let mut remaining = range.range_size as usize;
        let mut buffer = vec![0u8; remaining.min(BUFFER_SEGMENT_DATA_SIZE)];
        while remaining > 0 {
            let to_read = remaining.min(BUFFER_SEGMENT_DATA_SIZE);
            let n = match handle.read_at(&mut buffer[..to_read], sent).await {
                Ok(n) => n,
                Err(e) => {
                    log(ServerUpdate::Log(format!("Read failed for {source}: {e}")));
                    return Err(ServerError::Range(format!("read failed for {source}: {e}")));
                }
            };
            if n == 0 {
                log(ServerUpdate::Log(format!(
                    "Short transfer for {source}: {} bytes remaining",
                    remaining
                )));
                return Err(ServerError::Range(format!(
                    "short transfer for {source}: {remaining} bytes remaining"
                )));
            }
            self.transport.write(&buffer[..n]).await?;
            remaining -= n;
            sent += n as u64;

            if track {
                if let Some(percent) = sent.saturating_mul(100).checked_div(total) {
                    let percent = percent.min(100);
                    if last_percent != Some(percent) {
                        log(ServerUpdate::Progress {
                            name: range.name.clone(),
                            sent,
                            total,
                        });
                        last_percent = Some(percent);
                    }
                }
            }
        }
        // One significant event per streamed sequence: a participated range
        // completes the file when it reaches the file's last byte; auxiliary
        // reads and mid-file ranges stay silent.
        if track {
            if sent >= total {
                log(ServerUpdate::Completed {
                    name: range.name.clone(),
                    bytes: total,
                });
                self.streams.remove(&range.name);
            } else {
                let state = self
                    .streams
                    .get_mut(&range.name)
                    .expect("a tracked range has stream state");
                state.position = Some(sent);
            }
        }
        Ok(())
    }
}
