use async_trait::async_trait;
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::protocol::*;
use dbi_backend_core::server::{Server, ServerError};
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
    server.run(|line| logs.push(line)).await.unwrap();
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
    let err = server.run(|line| logs.push(line)).await.unwrap_err();
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
    let err = server.run(|line| logs.push(line)).await.unwrap_err();

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
    let err = server.run(|line| logs.push(line)).await.unwrap_err();
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
    let err = server.run(|line| logs.push(line)).await.unwrap_err();
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
