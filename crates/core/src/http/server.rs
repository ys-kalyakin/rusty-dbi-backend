//! The HTTP server: answers DBI's "Home server" browsing and downloads from
//! the same file list the USB server serves.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::watch;
use tokio_stream::wrappers::ReceiverStream;

use crate::filelist::FileList;
use crate::http::listing;
use crate::http::ranges::{self as range_spec, ByteRange};
use crate::source::Sources;

/// The bytes one streaming read moves at a time, matching the USB server.
const CHUNK: usize = 1024 * 1024;

/// How many read chunks may sit queued toward the client before reading
/// pauses: enough to keep the pipe full, few enough to stop early when the
/// client goes away.
const BODY_BACKLOG: usize = 4;

/// A message from the HTTP server to its caller, shaped like the USB
/// server's updates so both feed the same progress and log plumbing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpUpdate {
    Log(String),
    Progress { name: String, sent: u64, total: u64 },
    Completed { name: String, bytes: u64 },
}

/// Everything the request handlers share: the snapshot, the store it streams
/// through, the fixed date column, and where updates go.
struct Shared {
    files: FileList,
    store: Arc<dyn Sources>,
    date: String,
    updates: UnboundedSender<HttpUpdate>,
}

/// Serve the file list over HTTP until `shutdown` fires. The stop is a hard
/// one: the serve future is dropped, aborting open downloads — the user asked
/// for the server to stop, not to wait it out.
pub async fn serve(
    listener: TcpListener,
    files: FileList,
    store: Arc<dyn Sources>,
    mut shutdown: watch::Receiver<bool>,
    updates: UnboundedSender<HttpUpdate>,
) -> std::io::Result<()> {
    let shared = Arc::new(Shared {
        files,
        store,
        date: listing::list_date_now(),
        updates,
    });
    let app = Router::new()
        .route("/", get(root))
        .route("/{*name}", get(download))
        .with_state(shared);
    let server = axum::serve(listener, app);
    tokio::select! {
        result = server => result,
        _ = shutdown.changed() => Ok(()),
    }
}

/// The root listing: nginx's autoindex shape over the offered files.
async fn root(State(shared): State<Arc<Shared>>) -> Response {
    let page = listing::listing_page(&shared.files, &shared.date);
    ([(header::CONTENT_TYPE, "text/html")], page).into_response()
}

/// One file download: a percent-encoded name in the path, the entry's bytes
/// streamed through its source, byte ranges honored. `GET` and `HEAD` share
/// this handler; the body is elided for `HEAD` by the HTTP layer.
async fn download(State(shared): State<Arc<Shared>>, uri: Uri, headers: HeaderMap) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let Some(name) = listing::decode_href(raw) else {
        return (StatusCode::NOT_FOUND).into_response();
    };
    let Some(entry) = shared.files.get(&name) else {
        return (StatusCode::NOT_FOUND).into_response();
    };
    let source = entry.source.clone();
    let mut handle = match shared.store.open(&source).await {
        Ok(handle) => handle,
        Err(e) => {
            let _ = shared
                .updates
                .send(HttpUpdate::Log(format!("HTTP: cannot open {name}: {e}")));
            return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };
    let size = handle.size().await.unwrap_or(entry.size);
    let range_header = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let range = range_spec::parse(range_header, size);

    let (status, start, end) = match range {
        ByteRange::Full => (StatusCode::OK, 0, size),
        ByteRange::Slice { start, end } => (StatusCode::PARTIAL_CONTENT, start, end + 1),
        ByteRange::Unsatisfiable => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{size}"))],
            )
                .into_response();
        }
    };

    // A response that opens the file at its first byte starts its streamed
    // sequence, which is the one that announces the file.
    if start == 0 && end > 0 {
        let _ = shared
            .updates
            .send(HttpUpdate::Log(format!("Sending: {name}")));
    }

    let (body_tx, body_rx) =
        tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(BODY_BACKLOG);
    let updates = shared.updates.clone();
    tokio::spawn(async move {
        let mut pos = start;
        let mut last_percent = None;
        while pos < end {
            let to_read = (end - pos).min(CHUNK as u64) as usize;
            let mut buf = vec![0u8; to_read];
            let n = match handle.read_at(&mut buf, pos).await {
                Ok(n) => n,
                Err(e) => {
                    let _ = updates.send(HttpUpdate::Log(format!(
                        "HTTP: failed streaming {name}: {e}"
                    )));
                    return;
                }
            };
            if n == 0 {
                let _ = updates.send(HttpUpdate::Log(format!(
                    "HTTP: failed streaming {name}: file ended early"
                )));
                return;
            }
            pos += n as u64;
            // A client that walked away ends the stream quietly; the next
            // download says its own lines.
            if body_tx
                .send(Ok(Bytes::copy_from_slice(&buf[..n])))
                .await
                .is_err()
            {
                return;
            }
            if let Some(percent) = pos.saturating_mul(100).checked_div(size) {
                let percent = percent.min(100);
                if last_percent != Some(percent) {
                    let _ = updates.send(HttpUpdate::Progress {
                        name: name.clone(),
                        sent: pos,
                        total: size,
                    });
                    last_percent = Some(percent);
                }
            }
        }
        if end == size {
            let _ = updates.send(HttpUpdate::Completed { name, bytes: size });
        }
    });

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, end - start);
    if let ByteRange::Slice { start, end } = range {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }
    builder
        .body(Body::from_stream(ReceiverStream::new(body_rx)))
        .expect("the response headers are constant strings")
}

/// A channel pair for the updates a [`serve`] run reports.
pub fn updates_channel() -> (
    UnboundedSender<HttpUpdate>,
    tokio::sync::mpsc::UnboundedReceiver<HttpUpdate>,
) {
    unbounded_channel()
}
