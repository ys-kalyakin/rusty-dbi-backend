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
