use async_trait::async_trait;
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::protocol::*;
use dbi_backend_core::server::{Server, ServerError, ServerUpdate};
use dbi_backend_core::transport::{Transport, TransportError};
use std::collections::VecDeque;

pub struct MockTransport {
    reads: VecDeque<Vec<u8>>,
    writes: Vec<Vec<u8>>,
}

impl MockTransport {
    pub fn new(reads: Vec<Vec<u8>>) -> Self {
        Self {
            reads: reads.into(),
            writes: Vec::new(),
        }
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn read(&mut self, _len: usize) -> Result<Vec<u8>, TransportError> {
        self.reads
            .pop_front()
            .ok_or_else(|| TransportError::Usb("no more reads".into()))
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.writes.push(data.to_vec());
        Ok(())
    }
}

fn req(cmd_id: u32, data_size: u32) -> Vec<u8> {
    Header::new(CMD_TYPE_REQUEST, cmd_id, data_size)
        .to_bytes()
        .to_vec()
}

fn ack() -> Vec<u8> {
    Header::new(CMD_TYPE_ACK, 0, 0).to_bytes().to_vec()
}

#[tokio::test]
async fn list_command_sends_file_names_then_exits() {
    let mut files = FileList::new();
    files.insert("test.nsp", "/tmp/test.nsp");

    let reads = vec![req(CMD_ID_LIST, 0), ack(), req(CMD_ID_EXIT, 0)];
    let mut server = Server::new(MockTransport::new(reads), files);

    server.run(|_line| {}).await.unwrap();
    let transport = server.into_transport();

    assert_eq!(
        transport.writes[0],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, 9).to_bytes()
    );
    assert_eq!(transport.writes[1], b"test.nsp\n");
    assert_eq!(
        transport.writes[2],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes()
    );
}

#[tokio::test]
async fn empty_list_command_writes_zero_length_and_does_not_read_ack() {
    let files = FileList::new();

    let reads = vec![req(CMD_ID_LIST, 0), req(CMD_ID_EXIT, 0)];
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    server
        .run(|update| {
            if let ServerUpdate::Log(line) = update {
                logs.push(line);
            }
        })
        .await
        .unwrap();
    let transport = server.into_transport();

    assert_eq!(
        transport.writes[0],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, 0).to_bytes()
    );
    assert_eq!(
        transport.writes[1],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes()
    );
    assert!(logs.iter().any(|l| l.contains("Entering command loop")));
    assert!(logs.iter().any(|l| l.contains("Get list")));
    assert!(logs.iter().any(|l| l.contains("Exit")));
}

#[tokio::test]
async fn file_range_command_streams_requested_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    let mut range_header = Vec::new();
    range_header.extend_from_slice(&5u32.to_le_bytes()); // range_size
    range_header.extend_from_slice(&2u64.to_le_bytes()); // range_offset
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![
        req(CMD_ID_FILE_RANGE, data_size),
        range_header,
        ack(),
        req(CMD_ID_EXIT, 0),
    ];
    let mut server = Server::new(MockTransport::new(reads), files);

    server.run(|_line| {}).await.unwrap();
    let transport = server.into_transport();

    assert_eq!(
        transport.writes[0],
        Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, data_size).to_bytes()
    );
    assert_eq!(
        transport.writes[1],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, 5).to_bytes()
    );
    assert_eq!(transport.writes[2], vec![2u8, 3, 4, 5, 6]);
    assert_eq!(
        transport.writes[3],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes()
    );
}

/// The reads a single `FILE_RANGE` request consumes on the transport: the request
/// header, the range header, and the acknowledgment before the payload.
fn range_read_parts(name: &str, range_size: u32, offset: u64) -> Vec<Vec<u8>> {
    let mut range_header = Vec::new();
    range_header.extend_from_slice(&range_size.to_le_bytes());
    range_header.extend_from_slice(&offset.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;
    vec![req(CMD_ID_FILE_RANGE, data_size), range_header, ack()]
}

#[tokio::test]
async fn file_range_aux_mid_file_read_is_silent_and_streams_requested_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    // A mid-file range opens neither at offset zero nor at a streamed position:
    // an auxiliary read, which is served as requested but stays silent.
    let ranges = range_read_parts(name, 5, 2);
    let reads = [ranges, vec![req(CMD_ID_EXIT, 0)]].concat();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let mut completions = Vec::new();
    let mut progress = Vec::new();
    server
        .run(|update| match update {
            ServerUpdate::Log(line) => logs.push(line),
            ServerUpdate::Completed { name, bytes } => completions.push((name, bytes)),
            ServerUpdate::Progress { name, sent, total } => progress.push((name, sent, total)),
        })
        .await
        .unwrap();

    assert!(
        completions.is_empty(),
        "an auxiliary read must not complete"
    );
    assert!(
        progress.is_empty(),
        "an auxiliary read must not report progress: {progress:?}"
    );
    assert_eq!(
        logs.iter().filter(|l| l.starts_with("Sending: ")).count(),
        0,
        "an auxiliary read is not announced: {logs:?}"
    );
    assert!(
        !logs
            .iter()
            .any(|l| l.contains("File range") || l.contains("Range Size")),
        "the request must not be logged: {logs:?}"
    );

    // The stream itself is unchanged: exactly the requested bytes are served.
    let transport = server.into_transport();
    assert_eq!(
        transport.writes[2],
        vec![2u8, 3, 4, 5, 6],
        "the auxiliary read still serves the requested bytes"
    );
}

#[tokio::test]
async fn file_range_announces_a_file_once_and_completes_at_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8; 10]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    // Three consecutive ranges of the same file; only the third reaches the end.
    let reads = [
        range_read_parts(name, 4, 0),
        range_read_parts(name, 5, 4),
        range_read_parts(name, 1, 9),
        vec![req(CMD_ID_EXIT, 0)],
    ]
    .concat();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let mut completions = Vec::new();
    server
        .run(|update| match update {
            ServerUpdate::Log(line) => logs.push(line),
            ServerUpdate::Completed { name, bytes } => completions.push((name, bytes)),
            ServerUpdate::Progress { .. } => {}
        })
        .await
        .unwrap();

    assert_eq!(
        completions,
        vec![("game.nsp".to_string(), 10)],
        "the file completes exactly once, at its end, with its size"
    );
    assert_eq!(
        logs.iter().filter(|l| l.starts_with("Sending: ")).count(),
        1,
        "one start announcement spans all ranges of the file: {logs:?}"
    );
}

#[tokio::test]
async fn file_range_reannounces_a_re_read_file_and_reports_each_completion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8; 10]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    // The same file is streamed through twice; each pass is a distinct transfer.
    let reads = [
        range_read_parts(name, 10, 0),
        range_read_parts(name, 10, 0),
        vec![req(CMD_ID_EXIT, 0)],
    ]
    .concat();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let mut completions = Vec::new();
    server
        .run(|update| match update {
            ServerUpdate::Log(line) => logs.push(line),
            ServerUpdate::Completed { name, bytes } => completions.push((name, bytes)),
            ServerUpdate::Progress { .. } => {}
        })
        .await
        .unwrap();

    assert_eq!(
        logs.iter().filter(|l| l.starts_with("Sending: ")).count(),
        2,
        "each re-read announces a fresh start: {logs:?}"
    );
    assert_eq!(
        completions,
        vec![("game.nsp".to_string(), 10), ("game.nsp".to_string(), 10)]
    );
}

#[tokio::test]
async fn streamed_phase_reports_progress_across_ranges_and_completes_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8; 10]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    // A streamed phase opens at offset zero and advances through contiguous
    // ranges to the file's end.
    let reads = [
        range_read_parts(name, 4, 0),
        range_read_parts(name, 4, 4),
        range_read_parts(name, 2, 8),
        vec![req(CMD_ID_EXIT, 0)],
    ]
    .concat();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let mut completions = Vec::new();
    let mut progress = Vec::new();
    server
        .run(|update| match update {
            ServerUpdate::Log(line) => logs.push(line),
            ServerUpdate::Completed { name, bytes } => completions.push((name, bytes)),
            ServerUpdate::Progress { name, sent, total } => progress.push((name, sent, total)),
        })
        .await
        .unwrap();

    // Progress samples track the streamed position across the ranges.
    let positions: Vec<u64> = progress
        .iter()
        .filter(|(progress_name, ..)| progress_name == name)
        .map(|(_, sent, _)| *sent)
        .collect();
    assert_eq!(positions, vec![4, 8, 10], "progress: {progress:?}");
    assert!(
        positions.windows(2).all(|w| w[0] <= w[1]),
        "the bar must never jump backward: {progress:?}"
    );
    assert_eq!(
        completions,
        vec![("game.nsp".to_string(), 10)],
        "the streamed phase completes exactly once"
    );
    assert_eq!(
        logs.iter().filter(|l| l.starts_with("Sending: ")).count(),
        1,
        "one announcement spans the streamed phase: {logs:?}"
    );
}

#[tokio::test]
async fn dbi_preparation_sequence_keeps_aux_reads_silent_and_restarts_cleanly() {
    let total: u64 = 100;
    let content: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, &content).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    // The DBI install-preparation flow: small reads at the file's start, then an
    // auxiliary read of the trailing ticket/certificate region, then the content
    // stream restarting at offset zero and running to the file's end.
    let reads = [
        range_read_parts(name, 10, 0),  // streamed start: header region
        range_read_parts(name, 10, 10), // streamed continuation
        range_read_parts(name, 2, 98),  // trailing auxiliary read at the file's end
        range_read_parts(name, 5, 20),  // streamed continuation from the paused position
        range_read_parts(name, 10, 0),  // restart: the real install streams from zero
        range_read_parts(name, 90, 10), // streamed continuation to the file's end
        vec![req(CMD_ID_EXIT, 0)],
    ]
    .concat();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let mut completions = Vec::new();
    let mut progress = Vec::new();
    server
        .run(|update| match update {
            ServerUpdate::Log(line) => logs.push(line),
            ServerUpdate::Completed { name, bytes } => completions.push((name, bytes)),
            ServerUpdate::Progress { name, sent, total } => progress.push((name, sent, total)),
        })
        .await
        .unwrap();
    let transport = server.into_transport();

    // The trailing auxiliary read ends at the file's last byte (98+2 == 100), the
    // exact shape behind the reported "fully green bar" defect: aux reads must
    // not complete the file, and no progress sample may reach the file's size
    // before the streamed restart runs to the end.
    let pre_restart = &progress[..3];
    assert!(
        pre_restart.iter().all(|(_, sent, _)| *sent < total),
        "no sample before the streamed restart reaches EOF: {progress:?}"
    );
    assert_eq!(
        progress,
        vec![
            (name.to_string(), 10, total),
            (name.to_string(), 20, total),
            (name.to_string(), 25, total),
            (name.to_string(), 10, total),
            (name.to_string(), 100, total),
        ],
        "progress follows the streamed phases only"
    );
    assert_eq!(
        completions,
        vec![(name.to_string(), total)],
        "exactly one completion, at the streamed phase's end"
    );
    assert_eq!(
        logs.iter().filter(|l| l.starts_with("Sending: ")).count(),
        2,
        "one announcement per streamed sequence: {logs:?}"
    );

    // Every request is still served in full: range i writes ACK, RESPONSE, payload.
    for (index, (offset, range_size)) in [
        (0usize, 10usize),
        (10, 10),
        (98, 2),
        (20, 5),
        (0, 10),
        (10, 90),
    ]
    .into_iter()
    .enumerate()
    {
        let range_size = range_size as u32;
        let base = index * 3;
        assert_eq!(
            transport.writes[base],
            Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, (16 + name.len()) as u32).to_bytes()
        );
        assert_eq!(
            transport.writes[base + 1],
            Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, range_size).to_bytes()
        );
        assert_eq!(
            transport.writes[base + 2],
            content[offset..offset + range_size as usize],
            "range {index} ({offset}, {range_size} bytes) served unchanged"
        );
    }
}

#[tokio::test]
async fn file_range_with_unknown_file_ends_the_run() {
    let name = "missing.nsp";
    let mut range_header = Vec::new();
    range_header.extend_from_slice(&5u32.to_le_bytes());
    range_header.extend_from_slice(&0u64.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![req(CMD_ID_FILE_RANGE, data_size), range_header, ack()];
    let files = FileList::new();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let mut completions = Vec::new();
    let err = server
        .run(|update| match update {
            ServerUpdate::Log(line) => logs.push(line),
            ServerUpdate::Completed { name, bytes } => completions.push((name, bytes)),
            ServerUpdate::Progress { .. } => {}
        })
        .await
        .unwrap_err();
    let transport = server.into_transport();

    assert!(matches!(err, ServerError::Range(_)), "{err}");
    assert_eq!(transport.writes.len(), 2);
    assert_eq!(
        transport.writes[0],
        Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, data_size).to_bytes()
    );
    assert_eq!(
        transport.writes[1],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, 5).to_bytes()
    );
    assert!(logs.iter().any(|l| l.contains("missing.nsp")));
    assert!(completions.is_empty(), "a failed range must not complete");
}

#[tokio::test]
async fn unopenable_file_ends_the_run() {
    let name = "broken.nsp";
    let mut files = FileList::new();
    files.insert(name, "/nonexistent/dbi-backend/definitely-not-here.nsp");

    let mut range_header = Vec::new();
    range_header.extend_from_slice(&5u32.to_le_bytes());
    range_header.extend_from_slice(&0u64.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![req(CMD_ID_FILE_RANGE, data_size), range_header, ack()];
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let err = server
        .run(|update| {
            if let ServerUpdate::Log(line) = update {
                logs.push(line);
            }
        })
        .await
        .unwrap_err();

    assert!(matches!(err, ServerError::Range(_)), "{err}");
    assert!(logs.iter().any(|l| l.contains("Failed to open")));
}

#[tokio::test]
async fn short_read_ends_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8, 1, 2, 3, 4]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    // Request more bytes than the file holds.
    let mut range_header = Vec::new();
    range_header.extend_from_slice(&10u32.to_le_bytes());
    range_header.extend_from_slice(&0u64.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![req(CMD_ID_FILE_RANGE, data_size), range_header, ack()];
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let err = server
        .run(|update| {
            if let ServerUpdate::Log(line) = update {
                logs.push(line);
            }
        })
        .await
        .unwrap_err();
    let transport = server.into_transport();

    assert!(matches!(err, ServerError::Range(_)), "{err}");
    // ACK, RESPONSE, then the five bytes the file could supply.
    assert_eq!(transport.writes.len(), 3);
    assert_eq!(transport.writes[2].len(), 5);
    assert!(logs.iter().any(|l| l.contains("Short transfer")));
}

#[tokio::test]
async fn oversized_file_range_header_is_rejected() {
    let files = FileList::new();
    let reads = vec![req(CMD_ID_FILE_RANGE, u32::MAX)];
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    let err = server
        .run(|update| {
            if let ServerUpdate::Log(line) = update {
                logs.push(line);
            }
        })
        .await
        .unwrap_err();
    let transport = server.into_transport();

    assert!(err.to_string().contains("too large"), "{err}");
    assert!(transport.writes.is_empty());
    assert!(logs.iter().any(|l| l.contains("too large")));
}

#[tokio::test]
async fn file_range_larger_than_one_chunk_is_streamed_in_multiple_writes() {
    let total = BUFFER_SEGMENT_DATA_SIZE + 10;
    let content: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.nsp");
    std::fs::write(&path, &content).unwrap();

    let name = "big.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    let mut range_header = Vec::new();
    range_header.extend_from_slice(&(total as u32).to_le_bytes());
    range_header.extend_from_slice(&0u64.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![
        req(CMD_ID_FILE_RANGE, data_size),
        range_header,
        ack(),
        req(CMD_ID_EXIT, 0),
    ];
    let mut server = Server::new(MockTransport::new(reads), files);
    server.run(|_line| {}).await.unwrap();
    let transport = server.into_transport();

    // writes[0] = ACK, writes[1] = RESPONSE, writes[2..4] = two data chunks, writes[4] = EXIT.
    assert_eq!(transport.writes.len(), 5);
    assert_eq!(transport.writes[2].len(), BUFFER_SEGMENT_DATA_SIZE);
    assert_eq!(transport.writes[3].len(), 10);
    assert_eq!(transport.writes[2], &content[..BUFFER_SEGMENT_DATA_SIZE]);
    assert_eq!(transport.writes[3], &content[BUFFER_SEGMENT_DATA_SIZE..]);
}

#[tokio::test]
async fn file_range_reports_throttled_progress() {
    let size = BUFFER_SEGMENT_DATA_SIZE + 10;
    let content: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.nsp");
    std::fs::write(&path, &content).unwrap();

    let name = "big.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    let mut range_header = Vec::new();
    range_header.extend_from_slice(&(size as u32).to_le_bytes());
    range_header.extend_from_slice(&0u64.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![
        req(CMD_ID_FILE_RANGE, data_size),
        range_header,
        ack(),
        req(CMD_ID_EXIT, 0),
    ];
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut progress = Vec::new();
    server
        .run(|update| {
            if let ServerUpdate::Progress { name, sent, total } = update {
                progress.push((name, sent, total));
            }
        })
        .await
        .unwrap();

    // Throttled to percent changes: two chunks yield at most two reports.
    assert!(!progress.is_empty());
    assert!(
        progress.len() <= 2,
        "too many progress reports: {progress:?}"
    );
    assert!(progress.windows(2).all(|w| w[0].1 <= w[1].1));
    let last = progress.last().unwrap();
    assert_eq!(last.0, "big.nsp");
    assert_eq!(last.1, size as u64);
    assert_eq!(last.2, size as u64);
}
