//! The title identity of an offered file: what it is, read from the file's
//! own metadata and from its name.

pub mod cnmt;
pub mod container;
pub mod filename;
#[cfg(test)]
pub(crate) mod fixtures;

use crate::source::FileHandle;

/// What a title is: the base game, an update, or downloadable content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Base,
    Update,
    Dlc,
}

impl ContentType {
    /// The short label the UI shows.
    pub fn label(self) -> &'static str {
        match self {
            ContentType::Base => "base",
            ContentType::Update => "update",
            ContentType::Dlc => "dlc",
        }
    }

    /// Classify a title id by its last three digits: `000` is a base game,
    /// `800` an update, anything else downloadable content.
    pub fn from_title_id(title_id: &str) -> Option<ContentType> {
        let start = title_id.len().checked_sub(3)?;
        let digits = title_id.get(start..)?;
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        match digits {
            "000" => Some(ContentType::Base),
            "800" => Some(ContentType::Update),
            _ => Some(ContentType::Dlc),
        }
    }
}

/// What the backend knows about a title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TitleIdentity {
    /// The title id: 16 hexadecimal digits.
    pub title_id: Option<String>,
    /// The kind of content the title is.
    pub content_type: Option<ContentType>,
    /// The version, as major, minor and patch.
    pub version: Option<(u8, u8, u8)>,
    /// The name part of the file name.
    pub name: Option<String>,
}

impl TitleIdentity {
    /// Whether the identity carries nothing at all.
    pub fn is_empty(&self) -> bool {
        self.title_id.is_none()
            && self.content_type.is_none()
            && self.version.is_none()
            && self.name.is_none()
    }
}

/// A version encoded as `major << 16 | minor << 8 | patch`, split into its
/// three parts. Both the content meta header and a bracketed version group
/// carry a version this way.
pub(crate) fn split_version(raw: u32) -> (u8, u8, u8) {
    ((raw >> 16) as u8, (raw >> 8) as u8, raw as u8)
}

/// Whether a file's name marks it as a container the reader knows: the
/// archive formats a title is offered in.
pub fn is_container_file(name: &str) -> bool {
    match crate::filters::extension_of(name) {
        Some(extension) => crate::filters::DEFAULT_EXTENSIONS.contains(&extension.as_str()),
        None => false,
    }
}

/// Read the identity of a file: from its container when the container exposes
/// its metadata, from its name otherwise. A read that fails yields an empty
/// identity — the identity is a convenience, never a reason to refuse a file.
pub async fn read_identity(handle: &mut dyn FileHandle, file_name: &str) -> TitleIdentity {
    let from_container = container::read_metadata(handle).await;
    let from_name = filename::parse(file_name);
    merge(from_container, from_name)
}

/// Combine what the container carried with what the file name carried: the
/// container wins, the name fills the gaps.
fn merge(container: Option<cnmt::ContentMeta>, name: filename::NameIdentity) -> TitleIdentity {
    let title_id = container
        .as_ref()
        .map(|meta| meta.title_id.clone())
        .or_else(|| name.title_id.clone());
    let content_type = container
        .as_ref()
        .and_then(|meta| meta.content_type())
        .or_else(|| title_id.as_deref().and_then(ContentType::from_title_id));
    TitleIdentity {
        title_id,
        content_type,
        version: container.as_ref().map(|meta| meta.version).or(name.version),
        name: name.name,
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{content_meta, Memory};
    use super::*;

    /// A container holding one content meta, built the way the format says.
    fn container_with_meta(meta: &[u8]) -> Vec<u8> {
        fixtures::container(&[("meta.cnmt.npdm", meta.to_vec())], false, false)
    }

    #[tokio::test]
    async fn the_container_carries_the_identity() {
        let meta = content_meta(0x0100000000010000, 65536, 0x80);
        let bytes = container_with_meta(&meta);
        let mut handle = Memory::new(bytes);

        let identity = read_identity(&mut handle, "game.nsp").await;

        assert_eq!(identity.title_id.as_deref(), Some("0100000000010000"));
        assert_eq!(identity.content_type, Some(ContentType::Base));
        assert_eq!(identity.version, Some((1, 0, 0)));
    }

    #[tokio::test]
    async fn the_file_name_fills_what_the_container_does_not_carry() {
        let mut handle = Memory::new(b"not a container at all".to_vec());

        let identity =
            read_identity(&mut handle, "Some Game [0100000000010800][v131072].nsp").await;

        assert_eq!(identity.title_id.as_deref(), Some("0100000000010800"));
        assert_eq!(identity.content_type, Some(ContentType::Update));
        assert_eq!(identity.version, Some((2, 0, 0)));
        assert_eq!(identity.name.as_deref(), Some("Some Game"));
    }

    #[tokio::test]
    async fn the_container_wins_over_the_file_name() {
        let meta = content_meta(0x0100000000010000, 65536, 0x80);
        let bytes = container_with_meta(&meta);
        let mut handle = Memory::new(bytes);

        let identity = read_identity(&mut handle, "Other [0100000000019999].nsp").await;

        assert_eq!(identity.title_id.as_deref(), Some("0100000000010000"));
    }

    #[tokio::test]
    async fn a_file_that_is_not_a_container_yields_only_what_its_name_carries() {
        let mut handle = Memory::new(b"just some bytes".to_vec());

        let identity = read_identity(&mut handle, "game.nsp").await;

        assert!(identity.title_id.is_none());
        assert!(identity.content_type.is_none());
        assert!(identity.version.is_none());
    }

    #[test]
    fn a_title_id_is_classified_by_its_last_three_digits() {
        assert_eq!(
            ContentType::from_title_id("0100000000010000"),
            Some(ContentType::Base)
        );
        assert_eq!(
            ContentType::from_title_id("0100000000010800"),
            Some(ContentType::Update)
        );
        assert_eq!(
            ContentType::from_title_id("0100000000011001"),
            Some(ContentType::Dlc)
        );
        assert_eq!(ContentType::from_title_id("nope"), None);
    }

    #[test]
    fn the_content_type_has_a_short_label() {
        assert_eq!(ContentType::Base.label(), "base");
        assert_eq!(ContentType::Update.label(), "update");
        assert_eq!(ContentType::Dlc.label(), "dlc");
    }
}
