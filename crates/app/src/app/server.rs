//! The background server task: it waits for the Switch, answers it, and
//! reports what it did.

use std::sync::Arc;
use std::time::Duration;

use dbi_backend_core::filelist::FileList;
use dbi_backend_core::server::{Server, ServerUpdate};
use dbi_backend_core::source::Sources;
use dbi_backend_core::transport::usb::UsbTransport;
use tokio::sync::mpsc::UnboundedSender;

use super::state::ServerEvent;

/// Decides which connection attempts reach the log.
///
/// The waiting state is logged once per waiting period, and a failing connection is
/// logged once while the same error persists.
#[derive(Debug, Default)]
pub(crate) struct ConnectLog {
    waiting_logged: bool,
    last_error: Option<String>,
}

impl ConnectLog {
    /// The message to log before a connection attempt, if the waiting state is new.
    pub(crate) fn waiting(&mut self) -> Option<&'static str> {
        if self.waiting_logged {
            None
        } else {
            self.waiting_logged = true;
            Some("Waiting for Switch...")
        }
    }

    /// The message to log for a failed attempt, if the error differs from the last one.
    pub(crate) fn failed(&mut self, error: &str) -> Option<String> {
        let message = format!("USB error: {error}");
        if self.last_error.as_deref() == Some(message.as_str()) {
            None
        } else {
            self.last_error = Some(message.clone());
            Some(message)
        }
    }

    /// Forget the waiting state and the last error once a connection is established.
    pub(crate) fn connected(&mut self) {
        self.waiting_logged = false;
        self.last_error = None;
    }
}

/// Spawn the background server loop. Logs are forwarded to `log_tx`, lifecycle
/// transitions and progress to `event_tx`.
pub fn spawn_server(
    files: FileList,
    store: Arc<dyn Sources>,
    log_tx: UnboundedSender<String>,
    event_tx: UnboundedSender<ServerEvent>,
) {
    tokio::spawn(async move {
        let mut connect_log = ConnectLog::default();
        loop {
            if let Some(message) = connect_log.waiting() {
                let _ = log_tx.send(message.into());
            }
            let transport = match UsbTransport::connect().await {
                Ok(t) => t,
                Err(e) => {
                    if let Some(message) = connect_log.failed(&e.to_string()) {
                        let _ = log_tx.send(message);
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            connect_log.connected();
            let _ = log_tx.send("Switch connected".into());
            let _ = event_tx.send(ServerEvent::Started);

            let mut server = Server::new(transport, files.clone(), store.clone());
            let log = log_tx.clone();
            let events = event_tx.clone();
            let mut on_update = move |update: ServerUpdate| match update {
                ServerUpdate::Log(line) => {
                    let _ = log.send(line);
                }
                ServerUpdate::Progress { name, sent, total } => {
                    let _ = events.send(ServerEvent::Progress { name, sent, total });
                }
                ServerUpdate::Completed { name, bytes } => {
                    let _ = events.send(ServerEvent::Completed { name, bytes });
                }
            };
            match server.run(&mut on_update).await {
                Ok(()) => {
                    let _ = log_tx.send("Server stopped".into());
                    let _ = event_tx.send(ServerEvent::Ended);
                    break;
                }
                Err(e) => {
                    let _ = log_tx.send(format!("Connection lost: {e}"));
                    let _ = event_tx.send(ServerEvent::Disconnected);
                    continue;
                }
            }
        }
    });
}
