//! The container's table of contents: the members a file holds and where
//! each of them lives.

use crate::source::FileHandle;

use super::cnmt;

/// The container's marker: the four bytes every member table starts with.
pub(super) const MAGIC: &[u8; 4] = b"PFS0";

/// The fixed part of the header: the marker, the member count, the name-table
/// size and four reserved bytes.
pub(super) const HEADER_LEN: usize = 0x10;

/// The size of one member's entry in the table.
pub(super) const ENTRY_LEN: usize = 0x18;

/// The largest table of contents the reader accepts: a name table and the
/// member entries must fit in the file, and a file that claims more is
/// refused rather than trusted.
const MAX_TABLE_LEN: u64 = 16 * 1024 * 1024;

/// One member of a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The member's name, as the container spells it.
    pub name: String,
    /// Where the member's bytes start, counted from the end of the header.
    pub offset: u64,
    /// How many bytes the member holds.
    pub size: u64,
}

/// A container's table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    members: Vec<Member>,
    /// The size of the header, so a member's offset can be turned into a
    /// position in the file.
    header_len: u64,
}

impl Container {
    /// The member with that name.
    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.name == name)
    }

    /// The container's members.
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// Where a member's bytes start in the file.
    pub fn member_offset(&self, member: &Member) -> u64 {
        self.header_len + member.offset
    }
}

/// Read a container's table of contents, or `None` when the file is not a
/// container this reader understands.
pub async fn read_toc(handle: &mut dyn FileHandle) -> Option<Container> {
    let mut header = [0u8; HEADER_LEN];
    read_exact(handle, 0, &mut header).await?;
    if &header[0..4] != MAGIC {
        return None;
    }
    let count = u32::from_le_bytes(header[4..8].try_into().ok()?) as u64;
    let name_table_len = u32::from_le_bytes(header[8..12].try_into().ok()?) as u64;
    let table_len = count
        .checked_mul(ENTRY_LEN as u64)?
        .checked_add(name_table_len)?;
    if table_len > MAX_TABLE_LEN {
        return None;
    }
    let header_len = HEADER_LEN as u64 + table_len;
    let file_len = handle.size().await?;
    if header_len > file_len {
        return None;
    }

    let mut table = vec![0u8; table_len as usize];
    read_exact(handle, HEADER_LEN as u64, &mut table).await?;
    let entries = &table[..count as usize * ENTRY_LEN];
    let names = &table[count as usize * ENTRY_LEN..];

    let mut members = Vec::with_capacity(count as usize);
    let utf16 = is_utf16(names);
    for index in 0..count as usize {
        let entry = &entries[index * ENTRY_LEN..(index + 1) * ENTRY_LEN];
        let offset = u64::from_le_bytes(entry[0..8].try_into().ok()?);
        let size = u64::from_le_bytes(entry[8..16].try_into().ok()?);
        let name_index = u32::from_le_bytes(entry[16..20].try_into().ok()?) as usize;
        let name = name_at(names, name_index, utf16)?;
        members.push(Member { name, offset, size });
    }
    Some(Container {
        members,
        header_len,
    })
}

/// The suffix the plain content meta member's name ends in.
const CONTENT_META_SUFFIX: &str = ".npdm";

/// The most the reader reads of a content meta member: its header sits at the
/// front, and the rest of it is not needed.
const META_READ_LEN: usize = 0x100;

/// Read the content meta a container exposes in the clear, if it exposes one.
///
/// A dumped title keeps its content meta inside an encrypted archive, so this
/// finds nothing for the containers a dump tool writes; it is the path for a
/// container that carries the meta plainly.
pub async fn read_metadata(handle: &mut dyn FileHandle) -> Option<cnmt::ContentMeta> {
    let toc = read_toc(handle).await?;
    let member = toc
        .members()
        .iter()
        .find(|m| m.name.to_ascii_lowercase().ends_with(CONTENT_META_SUFFIX))?;
    let len = member.size.min(META_READ_LEN as u64) as usize;
    let mut buf = vec![0u8; len];
    read_exact(handle, toc.member_offset(member), &mut buf).await?;
    cnmt::parse(&buf)
}

/// Whether the name table holds UTF-16. The containers the console writes
/// spell their names in UTF-16, the ones some tools write in UTF-8; a UTF-16
/// table of ASCII names carries a zero byte every other position.
fn is_utf16(names: &[u8]) -> bool {
    names.len() >= 2 && names[1] == 0
}

/// The name at `index` in the container's name table: the bytes from there to
/// the name's terminator.
fn name_at(names: &[u8], index: usize, utf16: bool) -> Option<String> {
    let tail = names.get(index..)?;
    if utf16 {
        if !index.is_multiple_of(2) {
            return None;
        }
        let mut units = Vec::new();
        for pair in tail.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let unit = u16::from_le_bytes([pair[0], pair[1]]);
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        String::from_utf16(&units).ok()
    } else {
        let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
        String::from_utf8(tail[..end].to_vec()).ok()
    }
}

/// Read exactly `buf.len()` bytes at `offset`, or `None` when the file ends
/// first.
async fn read_exact(handle: &mut dyn FileHandle, offset: u64, buf: &mut [u8]) -> Option<()> {
    let mut read = 0;
    while read < buf.len() {
        let count = handle
            .read_at(&mut buf[read..], offset + read as u64)
            .await
            .ok()?;
        if count == 0 {
            return None;
        }
        read += count;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::fixtures::{container as build_container, Memory};

    /// A container with the given members, each holding `body`. The name
    /// table is UTF-8, the way the tools that write containers spell it.
    fn container(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
        build_container(members, false, false)
    }

    /// A container whose name table is UTF-16, the way the console spells it.
    fn container_utf16(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
        build_container(members, true, false)
    }

    /// A container built the way the tools that write containers build it:
    /// the name table is UTF-8 and padded to a 0x20 boundary.
    fn container_as_the_tools_write_it(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
        build_container(members, false, true)
    }

    #[tokio::test]
    async fn a_container_built_the_way_the_tools_build_it_is_read() {
        // The layout of a dumped title: the content NCAs, the content meta NCA
        // and the ticket.
        let bytes = container_as_the_tools_write_it(&[
            ("0123456789abcdef0123456789abcdef.nca", vec![0; 32]),
            ("0123456789abcdef0123456789abcdef.cnmt.nca", vec![0; 32]),
            ("0123456789abcdef0123456789abcdef.tik", vec![0; 16]),
        ]);
        let mut handle = Memory::new(bytes);

        let toc = read_toc(&mut handle).await.expect("a container");

        assert_eq!(toc.members().len(), 3);
        assert_eq!(
            toc.member("0123456789abcdef0123456789abcdef.cnmt.nca")
                .unwrap()
                .size,
            32
        );
    }

    #[tokio::test]
    async fn a_dumped_title_carries_no_metadata_of_its_own() {
        // The content meta of a dumped title lives inside an archive, not in
        // the container's table of contents: there is no plain member to read.
        let bytes = container_as_the_tools_write_it(&[
            ("0123456789abcdef0123456789abcdef.nca", vec![0; 32]),
            ("0123456789abcdef0123456789abcdef.cnmt.nca", vec![0; 32]),
        ]);
        let mut handle = Memory::new(bytes);

        assert!(read_metadata(&mut handle).await.is_none());
    }

    #[tokio::test]
    async fn reads_the_table_of_contents() {
        let bytes = container(&[("first.nca", vec![1, 2, 3]), ("meta.cnmt.npdm", vec![4, 5])]);
        let mut handle = Memory::new(bytes);

        let toc = read_toc(&mut handle).await.expect("a container");

        assert_eq!(toc.members().len(), 2);
        assert_eq!(toc.member("first.nca").unwrap().size, 3);
        assert_eq!(toc.member("meta.cnmt.npdm").unwrap().offset, 3);
    }

    #[tokio::test]
    async fn a_member_offset_points_past_the_header() {
        let bytes = container(&[("first.nca", vec![1, 2, 3])]);
        let header_len = bytes.len() - 3;
        let mut handle = Memory::new(bytes);

        let toc = read_toc(&mut handle).await.expect("a container");
        let member = toc.member("first.nca").unwrap();

        assert_eq!(toc.member_offset(member) as usize, header_len);
    }

    #[tokio::test]
    async fn a_file_that_is_not_a_container_yields_nothing() {
        let mut handle = Memory::new(b"just some bytes that are not a container".to_vec());
        assert!(read_toc(&mut handle).await.is_none());
    }

    #[tokio::test]
    async fn a_truncated_header_yields_nothing() {
        let mut handle = Memory::new(b"PFS0\x01".to_vec());
        assert!(read_toc(&mut handle).await.is_none());
    }

    #[tokio::test]
    async fn a_header_larger_than_the_file_is_refused() {
        let mut header = Vec::new();
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&0xffffu32.to_le_bytes());
        header.extend_from_slice(&0xffffu32.to_le_bytes());
        header.extend_from_slice(&[0, 0, 0, 0]);
        let mut handle = Memory::new(header);

        assert!(read_toc(&mut handle).await.is_none());
    }

    #[tokio::test]
    async fn a_name_index_beyond_the_table_yields_nothing() {
        let mut bytes = container(&[("first.nca", vec![1])]);
        // Point the first entry at a name index past the table.
        bytes[HEADER_LEN + 16..HEADER_LEN + 20].copy_from_slice(&99u32.to_le_bytes());
        let mut handle = Memory::new(bytes);

        assert!(read_toc(&mut handle).await.is_none());
    }

    #[tokio::test]
    async fn a_utf16_name_table_is_read() {
        // The containers the console writes spell their names in UTF-16.
        let bytes = container_utf16(&[("first.nca", vec![1, 2, 3])]);
        let mut handle = Memory::new(bytes);

        let toc = read_toc(&mut handle).await.expect("a container");

        assert_eq!(toc.member("first.nca").unwrap().size, 3);
    }
}
