//! The DBI command loop.

use crate::filelist::FileList;
use crate::protocol::*;
use crate::transport::{Transport, TransportError};

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

/// Upper bound accepted for a FILE_RANGE request body (16-byte fixed part plus the file
/// name). Larger sizes are rejected instead of allocating an untrusted amount of memory.
const MAX_RANGE_HEADER_LEN: usize = 0x4000;

/// Serves DBI requests read from a [`Transport`].
pub struct Server<T: Transport> {
    transport: T,
    files: FileList,
}

impl<T: Transport> Server<T> {
    pub fn new(transport: T, files: FileList) -> Self {
        Self { transport, files }
    }

    /// Consume the server and return the transport (used by tests).
    #[doc(hidden)]
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Run the command loop until the Switch issues EXIT or the transport fails.
    pub async fn run<F: FnMut(String)>(&mut self, mut log: F) -> Result<(), ServerError> {
        log("Entering command loop".into());
        loop {
            let bytes = self.transport.read(HEADER_LEN).await?;
            let header = Header::parse(&bytes)
                .ok_or_else(|| ServerError::Protocol("short header".into()))?;
            if !header.is_valid() {
                log("Ignoring unknown framing".into());
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

    async fn process_exit<F: FnMut(String)>(&mut self, log: &mut F) -> Result<(), ServerError> {
        log("Exit".into());
        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes();
        self.transport.write(&resp).await?;
        Ok(())
    }

    async fn process_list<F: FnMut(String)>(&mut self, log: &mut F) -> Result<(), ServerError> {
        log("Get list".into());
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

    async fn process_file_range<F: FnMut(String)>(
        &mut self,
        data_size: u32,
        log: &mut F,
    ) -> Result<(), ServerError> {
        use std::io::{Read, Seek, SeekFrom};

        log("File range".into());

        if data_size as usize > MAX_RANGE_HEADER_LEN {
            log(format!("Range header too large: {data_size} bytes"));
            return Err(ServerError::Protocol("file range header too large".into()));
        }

        let ack = Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, data_size).to_bytes();
        self.transport.write(&ack).await?;

        let range_bytes = self.transport.read(data_size as usize).await?;
        let range = FileRangeHeader::parse(&range_bytes)
            .ok_or_else(|| ServerError::Protocol("short file range header".into()))?;

        log(format!(
            "Range Size: {}, Range Offset: {}, Name len: {}, Name: {}",
            range.range_size, range.range_offset, range.name_len, range.name
        ));

        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, range.range_size).to_bytes();
        self.transport.write(&resp).await?;

        self.transport.read(HEADER_LEN).await?;

        let Some(path) = self.files.get(&range.name).cloned() else {
            log(format!("Unknown file: {}", range.name));
            return Err(ServerError::Range(format!("unknown file: {}", range.name)));
        };

        let mut file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                log(format!("Failed to open {}: {e}", path.display()));
                return Err(ServerError::Range(format!(
                    "failed to open {}: {e}",
                    path.display()
                )));
            }
        };
        if let Err(e) = file.seek(SeekFrom::Start(range.range_offset)) {
            log(format!("Failed to seek {}: {e}", path.display()));
            return Err(ServerError::Range(format!(
                "failed to seek {}: {e}",
                path.display()
            )));
        }

        let mut remaining = range.range_size as usize;
        let mut buffer = vec![0u8; remaining.min(BUFFER_SEGMENT_DATA_SIZE)];
        while remaining > 0 {
            let to_read = remaining.min(BUFFER_SEGMENT_DATA_SIZE);
            let n = match file.read(&mut buffer[..to_read]) {
                Ok(n) => n,
                Err(e) => {
                    log(format!("Read failed for {}: {e}", path.display()));
                    return Err(ServerError::Range(format!(
                        "read failed for {}: {e}",
                        path.display()
                    )));
                }
            };
            if n == 0 {
                log(format!(
                    "Short transfer for {}: {} bytes remaining",
                    path.display(),
                    remaining
                ));
                return Err(ServerError::Range(format!(
                    "short transfer for {}: {remaining} bytes remaining",
                    path.display()
                )));
            }
            self.transport.write(&buffer[..n]).await?;
            remaining -= n;
        }
        Ok(())
    }
}
