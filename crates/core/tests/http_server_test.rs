//! Integration tests for the HTTP server: the listing it serves, downloads,
//! byte ranges, HEAD, and the hard stop.

use dbi_backend_core::filelist::FileList;
use dbi_backend_core::http::server::{serve, HttpUpdate};
use dbi_backend_core::source::local::LocalStore;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// A running server over one snapshot: its port, its stop switch, its update
/// stream, its task, and the directory holding the offered files (kept alive
/// so the files outlive the server).
struct Running {
    port: u16,
    stop: watch::Sender<bool>,
    updates: UnboundedReceiver<HttpUpdate>,
    task: JoinHandle<std::io::Result<()>>,
    _dir: tempfile::TempDir,
}

/// Start a server over `files`, keeping `dir` (where the files live) alive.
async fn server_over(files: FileList, dir: tempfile::TempDir) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, shutdown) = watch::channel(false);
    let (updates_tx, updates) = unbounded_channel();
    let task = tokio::spawn(serve(
        listener,
        files,
        Arc::new(LocalStore::new()),
        shutdown,
        updates_tx,
    ));
    Running {
        port,
        stop,
        updates,
        task,
        _dir: dir,
    }
}

/// One request over its own connection, read to its end.
async fn request(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

fn get(path: &str, extra_headers: &[&str]) -> String {
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n");
    for header in extra_headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    request.push_str("Connection: close\r\n\r\n");
    request
}

fn head(path: &str) -> String {
    format!("HEAD {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
}

fn body_of(response: &str) -> &str {
    response.split_once("\r\n\r\n").unwrap().1
}

/// Write the offered files into the tempdir and list them by name.
fn offered(dir: &tempfile::TempDir, names: &[(&str, usize)]) -> FileList {
    let mut files = FileList::new();
    for (name, len) in names {
        let path = dir.path().join(name);
        std::fs::write(&path, vec![0u8; *len]).unwrap();
        files.insert(name.to_string(), path);
    }
    files
}

/// A server offering one ten-byte file whose bytes count upward.
async fn ten_byte_server() -> Running {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ten.nsp"), (0..10u8).collect::<Vec<u8>>()).unwrap();
    let mut files = FileList::new();
    files.insert("ten.nsp", dir.path().join("ten.nsp"));
    server_over(files, dir).await
}

/// The updates sent so far, after giving the server a beat to finish them.
async fn drain(updates: &mut UnboundedReceiver<HttpUpdate>) -> Vec<HttpUpdate> {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let mut collected = Vec::new();
    while let Ok(update) = updates.try_recv() {
        collected.push(update);
    }
    collected
}

#[tokio::test]
async fn the_listing_lists_every_file_once() {
    let dir = tempfile::tempdir().unwrap();
    let files = offered(&dir, &[("a.nsp", 5), ("b c.nsp", 3)]);
    let mut running = server_over(files, dir).await;

    let response = request(running.port, &get("/", &[])).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(response.matches("<a href=").count(), 2);
    assert!(response.contains("<a href=\"a.nsp\">a.nsp</a>"));
    // A space survives the round trip: encoded in the href, readable in the text.
    assert!(response.contains("<a href=\"b%20c.nsp\">b c.nsp</a>"));

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
    assert!(
        drain(&mut running.updates).await.is_empty(),
        "browsing logs nothing"
    );
}

#[tokio::test]
async fn a_full_download_streams_the_whole_file() {
    let running = ten_byte_server().await;
    let response = request(running.port, &get("/ten.nsp", &[])).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.to_lowercase().contains("content-length: 10"));
    assert_eq!(
        body_of(&response).as_bytes(),
        (0..10u8).collect::<Vec<u8>>()
    );

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_bounded_range_serves_exact_bytes() {
    let running = ten_byte_server().await;
    let response = request(running.port, &get("/ten.nsp", &["Range: bytes=2-3"])).await;
    assert!(response.starts_with("HTTP/1.1 206"), "{response}");
    assert!(response
        .to_lowercase()
        .contains("content-range: bytes 2-3/10"));
    assert_eq!(body_of(&response).as_bytes(), &[2u8, 3]);

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn an_open_range_serves_the_tail() {
    let running = ten_byte_server().await;
    let response = request(running.port, &get("/ten.nsp", &["Range: bytes=4-"])).await;
    assert!(response.starts_with("HTTP/1.1 206"), "{response}");
    assert!(response
        .to_lowercase()
        .contains("content-range: bytes 4-9/10"));
    assert_eq!(body_of(&response).as_bytes(), &[4u8, 5, 6, 7, 8, 9]);

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn an_unsatisfiable_range_is_refused() {
    let running = ten_byte_server().await;
    let response = request(running.port, &get("/ten.nsp", &["Range: bytes=100-"])).await;
    assert!(response.starts_with("HTTP/1.1 416"), "{response}");
    assert!(response
        .to_lowercase()
        .contains("content-range: bytes */10"));
    assert!(body_of(&response).is_empty());

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn head_returns_the_headers_without_a_body() {
    let running = ten_byte_server().await;
    let response = request(running.port, &head("/ten.nsp")).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.to_lowercase().contains("content-length: 10"));
    assert!(body_of(&response).is_empty(), "no body follows: {response}");

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn an_unknown_name_is_not_found() {
    let mut running = ten_byte_server().await;
    let response = request(running.port, &get("/nope.nsp", &[])).await;
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    assert!(
        drain(&mut running.updates).await.is_empty(),
        "the miss logs nothing"
    );

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_download_reports_progress_and_completes_once() {
    let mut running = ten_byte_server().await;
    let response = request(running.port, &get("/ten.nsp", &[])).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    let updates = drain(&mut running.updates).await;
    assert_eq!(
        updates
            .iter()
            .filter(|update| matches!(update, HttpUpdate::Log(line) if line == "Sending: ten.nsp"))
            .count(),
        1,
        "the file is announced once: {updates:?}"
    );
    assert!(
        updates.iter().any(|update| matches!(
            update,
            HttpUpdate::Progress {
                name,
                sent: 10,
                total: 10
            } if name == "ten.nsp"
        )),
        "progress reaches the end: {updates:?}"
    );
    assert_eq!(
        updates
            .iter()
            .filter(|update| matches!(
                update,
                HttpUpdate::Completed { name, bytes: 10 } if name == "ten.nsp"
            ))
            .count(),
        1,
        "completion is reported once with the size: {updates:?}"
    );

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_partial_download_does_not_report_completion() {
    let mut running = ten_byte_server().await;
    let response = request(running.port, &get("/ten.nsp", &["Range: bytes=0-4"])).await;
    assert!(response.starts_with("HTTP/1.1 206"), "{response}");
    let updates = drain(&mut running.updates).await;
    assert!(
        !updates
            .iter()
            .any(|update| matches!(update, HttpUpdate::Completed { .. })),
        "a range short of the end is no completion: {updates:?}"
    );

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}

#[tokio::test]
async fn stopping_ends_the_server_promptly() {
    let running = ten_byte_server().await;
    running.stop.send(true).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), running.task)
        .await
        .expect("the stop ends the run at once")
        .unwrap();
    assert!(result.is_ok());

    // The port is gone: a connect after the stop refuses.
    assert!(TcpStream::connect(("127.0.0.1", running.port))
        .await
        .is_err());
}

#[tokio::test]
async fn the_snapshot_does_not_follow_later_edits() {
    let dir = tempfile::tempdir().unwrap();
    let files = offered(&dir, &[("a.nsp", 3)]);
    let running = server_over(files, dir).await;

    // A file appears after the server took its snapshot.
    std::fs::write(running._dir.path().join("late.nsp"), b"late").unwrap();

    let listing = request(running.port, &get("/", &[])).await;
    assert!(listing.contains("a.nsp"));
    assert!(
        !listing.contains("late.nsp"),
        "the snapshot is served: {listing}"
    );
    let late = request(running.port, &get("/late.nsp", &[])).await;
    assert!(late.starts_with("HTTP/1.1 404"), "{late}");

    running.stop.send(true).unwrap();
    running.task.await.unwrap().unwrap();
}
