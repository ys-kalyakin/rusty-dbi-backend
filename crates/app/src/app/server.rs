//! The background server tasks: the USB task waits for the Switch and answers
//! it, the HTTP task serves DBI's network installs, and both report what they
//! did.

use std::sync::Arc;
use std::time::Duration;

use dbi_backend_core::filelist::FileList;
use dbi_backend_core::http::server::{self as http_server, HttpUpdate};
use dbi_backend_core::server::{Server, ServerUpdate};
use dbi_backend_core::source::Sources;
use dbi_backend_core::transport::usb::UsbTransport;
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;

use super::state::ServerEvent;
use crate::config::HttpSettings;

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

/// Spawn the background HTTP server loop. Returns the switch that stops it.
/// Logs are forwarded to `log_tx`, lifecycle transitions and progress to
/// `event_tx`; a bind failure is logged once and the run ends at once.
pub fn spawn_http_server(
    files: FileList,
    store: Arc<dyn Sources>,
    settings: HttpSettings,
    log_tx: UnboundedSender<String>,
    event_tx: UnboundedSender<ServerEvent>,
) -> watch::Sender<bool> {
    let (stop, shutdown) = watch::channel(false);
    tokio::spawn(async move {
        let listener = match TcpListener::bind((settings.host.as_str(), settings.port)).await {
            Ok(listener) => listener,
            Err(e) => {
                let _ = log_tx.send(format!(
                    "HTTP error: cannot bind {}:{}: {e}",
                    settings.host, settings.port
                ));
                let _ = event_tx.send(ServerEvent::HttpEnded);
                return;
            }
        };
        let port = listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or(settings.port);
        let url = format!("http://{}:{port}/", display_host(&settings.host));
        let _ = log_tx.send(format!("HTTP server listening on {url}"));
        let _ = event_tx.send(ServerEvent::HttpStarted { url });

        let (updates_tx, mut updates_rx) = http_server::updates_channel();
        let serve = http_server::serve(listener, files, store, shutdown, updates_tx);
        tokio::pin!(serve);
        loop {
            tokio::select! {
                update = updates_rx.recv() => {
                    match update {
                        Some(HttpUpdate::Log(line)) => {
                            let _ = log_tx.send(line);
                        }
                        Some(HttpUpdate::Progress { name, sent, total }) => {
                            let _ = event_tx.send(ServerEvent::Progress { name, sent, total });
                        }
                        Some(HttpUpdate::Completed { name, bytes }) => {
                            let _ = event_tx.send(ServerEvent::Completed { name, bytes });
                        }
                        None => break,
                    }
                }
                result = &mut serve => {
                    if let Err(e) = result {
                        let _ = log_tx.send(format!("HTTP error: {e}"));
                    }
                    break;
                }
            }
        }
        let _ = log_tx.send("HTTP server stopped".into());
        let _ = event_tx.send(ServerEvent::HttpEnded);
    });
    stop
}

/// The host to show in the served URL: a specific bind host as configured,
/// or — for a wildcard bind — this machine's address on the route toward an
/// external host, found by connecting an unconnected (UDP) socket, which
/// sends nothing. Falls back to the wildcard itself when nothing is found.
pub(crate) fn display_host(bind_host: &str) -> String {
    if bind_host != "0.0.0.0" && bind_host != "::" && !bind_host.is_empty() {
        return bind_host.to_string();
    }
    let any = if bind_host == "::" {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    std::net::UdpSocket::bind(any)
        .and_then(|socket| {
            socket.connect("192.0.2.1:80")?;
            socket.local_addr()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|_| bind_host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_specific_bind_host_is_shown_as_configured() {
        assert_eq!(display_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(display_host("192.168.1.5"), "192.168.1.5");
    }

    #[test]
    fn a_wildcard_bind_resolves_without_hanging() {
        // The lookup must answer, not hang; on a machine with any route it
        // names that route's local address, and with none it falls back.
        let found = display_host("0.0.0.0");
        assert!(!found.is_empty());
    }
}
