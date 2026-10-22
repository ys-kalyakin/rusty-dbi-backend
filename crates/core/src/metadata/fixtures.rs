//! Fixtures the metadata readers' tests share: an in-memory file handle, a
//! content meta header, and a container holding whatever members a test
//! needs.

use crate::source::{FileHandle, SourceError};
use async_trait::async_trait;

use super::container::{ENTRY_LEN, HEADER_LEN, MAGIC};

/// A handle over a fixed buffer: the whole file is in memory.
pub struct Memory {
    bytes: Vec<u8>,
}

impl Memory {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

#[async_trait]
impl FileHandle for Memory {
    async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError> {
        let start = offset as usize;
        if start >= self.bytes.len() {
            return Ok(0);
        }
        let count = buf.len().min(self.bytes.len() - start);
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }

    async fn size(&self) -> Option<u64> {
        Some(self.bytes.len() as u64)
    }
}

/// A content meta header: title id, version and type.
pub fn content_meta(title_id: u64, version: u32, content_type: u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&title_id.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.push(content_type);
    out.push(0);
    out.extend_from_slice(&0u16.to_le_bytes()); // extended header size
    out.extend_from_slice(&0u16.to_le_bytes()); // content count
    out.extend_from_slice(&0u16.to_le_bytes()); // content meta count
    out.extend_from_slice(&[0u8; 0x10]);
    out
}

/// A container with the given members, each holding its bytes.
///
/// The name table is UTF-8 unless `utf16` is set, the way the console spells
/// it, and is padded to a 0x20 boundary when `padded` is set, the way the
/// tools that write containers spell it. A member's offset counts from the
/// end of the header, and its name index is the byte offset of its name in
/// the name table.
pub fn container(members: &[(&str, Vec<u8>)], utf16: bool, padded: bool) -> Vec<u8> {
    let mut name_table: Vec<u8> = Vec::new();
    let mut offsets: Vec<u64> = Vec::new();
    let mut name_offsets: Vec<u32> = Vec::new();
    let mut body: Vec<u8> = Vec::new();
    for (name, bytes) in members {
        offsets.push(body.len() as u64);
        body.extend_from_slice(bytes);
        name_offsets.push(name_table.len() as u32);
        if utf16 {
            for unit in name.encode_utf16() {
                name_table.extend_from_slice(&unit.to_le_bytes());
            }
            name_table.extend_from_slice(&[0, 0]);
        } else {
            name_table.extend_from_slice(name.as_bytes());
            name_table.push(0);
        }
    }
    if padded {
        // The table is padded so the header ends on a 0x20 boundary.
        let header_size = HEADER_LEN + members.len() * ENTRY_LEN + name_table.len();
        let padding = 0x20 - header_size % 0x20;
        name_table.extend(std::iter::repeat_n(0u8, padding));
    }

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(members.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name_table.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    for (index, offset) in offsets.iter().enumerate() {
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&members[index].1.len().to_le_bytes());
        out.extend_from_slice(&name_offsets[index].to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
    }
    out.extend_from_slice(&name_table);
    debug_assert_eq!(
        out.len(),
        HEADER_LEN + members.len() * ENTRY_LEN + name_table.len()
    );
    out.extend_from_slice(&body);
    out
}
