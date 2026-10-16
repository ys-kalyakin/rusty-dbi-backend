//! DBI0 USB protocol framing.

/// Magic bytes at the start of every message.
pub const MAGIC: [u8; 4] = *b"DBI0";
/// Length of the fixed command header.
pub const HEADER_LEN: usize = 16;
/// Maximum bytes sent per bulk transfer when streaming a file range.
pub const BUFFER_SEGMENT_DATA_SIZE: usize = 0x100000;

pub const CMD_ID_EXIT: u32 = 0;
pub const CMD_ID_LIST_OLD: u32 = 1;
pub const CMD_ID_FILE_RANGE: u32 = 2;
pub const CMD_ID_LIST: u32 = 3;

pub const CMD_TYPE_REQUEST: u32 = 0;
pub const CMD_TYPE_RESPONSE: u32 = 1;
pub const CMD_TYPE_ACK: u32 = 2;

/// A 16-byte DBI0 command header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub cmd_type: u32,
    pub cmd_id: u32,
    pub data_size: u32,
}

impl Header {
    /// Build a header using the DBI0 magic.
    pub fn new(cmd_type: u32, cmd_id: u32, data_size: u32) -> Self {
        Self {
            magic: MAGIC,
            cmd_type,
            cmd_id,
            data_size,
        }
    }

    /// Serialize to the 16-byte little-endian wire form.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..4].copy_from_slice(&self.magic);
        out[4..8].copy_from_slice(&self.cmd_type.to_le_bytes());
        out[8..12].copy_from_slice(&self.cmd_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.data_size.to_le_bytes());
        out
    }

    /// Parse a header from at least 16 bytes, or `None` if the buffer is too short.
    pub fn parse(buf: &[u8]) -> Option<Header> {
        if buf.len() < HEADER_LEN {
            return None;
        }
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        let cmd_type = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        let cmd_id = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        let data_size = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        Some(Header {
            magic,
            cmd_type,
            cmd_id,
            data_size,
        })
    }

    /// Whether this header starts with the DBI0 magic.
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
    }
}

/// The payload describing which part of which file the Switch wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRangeHeader {
    pub range_size: u32,
    pub range_offset: u64,
    pub name_len: u32,
    pub name: String,
}

impl FileRangeHeader {
    /// Parse from a buffer holding at least 16 bytes followed by the UTF-8 name.
    pub fn parse(buf: &[u8]) -> Option<FileRangeHeader> {
        if buf.len() < HEADER_LEN {
            return None;
        }
        let range_size = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let range_offset = u64::from_le_bytes(buf[4..12].try_into().unwrap());
        let name_len = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        let name = String::from_utf8_lossy(&buf[16..]).into_owned();
        Some(FileRangeHeader {
            range_size,
            range_offset,
            name_len,
            name,
        })
    }
}

/// Build the LIST payload: each name followed by a newline, UTF-8 encoded.
pub fn build_list(names: impl IntoIterator<Item = String>) -> Vec<u8> {
    let mut out = String::new();
    for name in names {
        out.push_str(&name);
        out.push('\n');
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let h = Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, 123);
        let bytes = h.to_bytes();
        let parsed = Header::parse(&bytes).unwrap();
        assert_eq!(parsed, h);
        assert!(parsed.is_valid());
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = Header::new(CMD_TYPE_REQUEST, CMD_ID_EXIT, 0).to_bytes();
        bytes[0] = b'X';
        let parsed = Header::parse(&bytes).unwrap();
        assert!(!parsed.is_valid());
    }

    #[test]
    fn header_parse_rejects_short_buffer() {
        assert!(Header::parse(&[0u8; 15]).is_none());
    }

    #[test]
    fn file_range_header_parses_fields() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&0x1000u64.to_le_bytes());
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(b"a.ns");
        let fr = FileRangeHeader::parse(&buf).unwrap();
        assert_eq!(fr.range_size, 100);
        assert_eq!(fr.range_offset, 0x1000);
        assert_eq!(fr.name_len, 4);
        assert_eq!(fr.name, "a.ns");
    }

    #[test]
    fn file_range_header_rejects_short_buffer() {
        assert!(FileRangeHeader::parse(&[0u8; 15]).is_none());
    }

    #[test]
    fn list_payload_terminates_each_name() {
        let names = vec!["a.nsp".to_string(), "b.nsp".to_string()];
        let payload = build_list(names);
        assert_eq!(payload, b"a.nsp\nb.nsp\n");
    }
}
