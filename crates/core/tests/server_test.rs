use async_trait::async_trait;
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::protocol::*;
use dbi_backend_core::server::Server;
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
async fn file_range_with_unknown_file_is_logged_and_skipped() {
    let name = "missing.nsp";
    let mut range_header = Vec::new();
    range_header.extend_from_slice(&5u32.to_le_bytes());
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
    let files = FileList::new();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    server.run(|line| logs.push(line)).await.unwrap();
    let transport = server.into_transport();

    assert!(transport.writes.iter().any(|w| w.starts_with(b"DBI0")));
    assert!(logs.iter().any(|l| l.contains("missing.nsp")));
}
