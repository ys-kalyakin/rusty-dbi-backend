//! The content meta's header: the title id, the version and the type.

use super::ContentType;

/// The header's fixed part: the fields the reader reads.
const HEADER_LEN: usize = 0x20;

/// The content meta types the backend knows, as the content meta names them.
const TYPE_APPLICATION: u8 = 0x80;
const TYPE_PATCH: u8 = 0x81;
const TYPE_ADD_ON_CONTENT: u8 = 0x82;
const TYPE_DELTA: u8 = 0x83;
const TYPE_DATA_PATCH: u8 = 0x84;

/// The fields the content meta's header carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMeta {
    /// The title id: 16 hexadecimal digits.
    pub title_id: String,
    /// The version, as major, minor and patch.
    pub version: (u8, u8, u8),
    /// The content meta type, as the content meta names it.
    pub content_type: u8,
}

impl ContentMeta {
    /// The kind of content the title is, when the type is one the backend
    /// knows.
    pub fn content_type(&self) -> Option<ContentType> {
        match self.content_type {
            TYPE_APPLICATION => Some(ContentType::Base),
            TYPE_PATCH => Some(ContentType::Update),
            TYPE_ADD_ON_CONTENT | TYPE_DELTA | TYPE_DATA_PATCH => Some(ContentType::Dlc),
            _ => None,
        }
    }
}

/// Parse a content meta header. A header too short to hold the fields, or one
/// whose title id is not sixteen digits, yields nothing.
pub fn parse(bytes: &[u8]) -> Option<ContentMeta> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    let title_id = title_id(&bytes[0..8]);
    if title_id.len() != 16 {
        return None;
    }
    let raw_version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    Some(ContentMeta {
        title_id,
        version: (
            (raw_version >> 16) as u8,
            (raw_version >> 8) as u8,
            raw_version as u8,
        ),
        content_type: bytes[12],
    })
}

/// The title id as sixteen hexadecimal digits: the header holds the id as a
/// little-endian number, and the canonical form reads it big-endian.
fn title_id(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(16);
    for byte in bytes.iter().rev() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A content meta header: title id, version and type.
    fn meta(title_id: u64, version: u32, content_type: u8) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&title_id.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.push(content_type);
        out.push(0);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 0x10]);
        out
    }

    #[test]
    fn parses_the_title_id_version_and_type() {
        let parsed = parse(&meta(0x0100000000010000, 65536, TYPE_APPLICATION)).unwrap();
        assert_eq!(parsed.title_id, "0100000000010000");
        assert_eq!(parsed.version, (1, 0, 0));
        assert_eq!(parsed.content_type, TYPE_APPLICATION);
    }

    #[test]
    fn the_title_id_is_read_big_endian() {
        let parsed = parse(&meta(0x0100000000010800, 131072, TYPE_PATCH)).unwrap();
        assert_eq!(parsed.title_id, "0100000000010800");
    }

    #[test]
    fn a_version_splits_into_major_minor_and_patch() {
        let parsed = parse(&meta(0x0100000000010000, 0x0001_0234, TYPE_APPLICATION)).unwrap();
        assert_eq!(parsed.version, (1, 2, 0x34));
    }

    #[test]
    fn a_short_header_yields_nothing() {
        assert!(parse(&[0u8; 8]).is_none());
    }

    #[test]
    fn the_content_type_is_classified() {
        let base = parse(&meta(1, 0, TYPE_APPLICATION)).unwrap();
        assert_eq!(base.content_type(), Some(ContentType::Base));
        let update = parse(&meta(1, 0, TYPE_PATCH)).unwrap();
        assert_eq!(update.content_type(), Some(ContentType::Update));
        let dlc = parse(&meta(1, 0, TYPE_ADD_ON_CONTENT)).unwrap();
        assert_eq!(dlc.content_type(), Some(ContentType::Dlc));
        let delta = parse(&meta(1, 0, TYPE_DELTA)).unwrap();
        assert_eq!(delta.content_type(), Some(ContentType::Dlc));
    }

    #[test]
    fn an_unknown_type_has_no_kind() {
        let unknown = parse(&meta(1, 0, 0x00)).unwrap();
        assert_eq!(unknown.content_type(), None);
    }
}
