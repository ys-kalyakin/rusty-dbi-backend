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
}

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
            let _ack = self.transport.read(HEADER_LEN).await?;
            self.transport.write(&payload).await?;
        }
        Ok(())
    }

    // Implemented in Task 6.
    async fn process_file_range<F: FnMut(String)>(
        &mut self,
        _data_size: u32,
        _log: &mut F,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}
