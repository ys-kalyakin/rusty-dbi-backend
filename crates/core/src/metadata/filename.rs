//! The identity a file name carries: the conventional dump name
//! `Name [TitleId][vVersion].nsp`.

/// What a file name says about a title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameIdentity {
    /// The name part: what comes before the first bracket.
    pub name: Option<String>,
    /// The title id: 16 hexadecimal digits in brackets.
    pub title_id: Option<String>,
    /// The version: `v` and a decimal number in brackets.
    pub version: Option<(u8, u8, u8)>,
}

/// Read the identity a file name carries. A bracket that is neither a title
/// id nor a version is ignored rather than guessed at.
pub fn parse(file_name: &str) -> NameIdentity {
    let mut title_id = None;
    let mut version = None;
    for group in bracket_groups(file_name) {
        if is_title_id(group) {
            title_id = Some(group.to_string());
        } else if let Some(parsed) = parse_version(group) {
            version = Some(parsed);
        }
    }
    NameIdentity {
        name: name_part(file_name),
        title_id,
        version,
    }
}

/// The name part of a file name: what comes before the first bracket. A name
/// with no name part keeps its whole file name.
fn name_part(file_name: &str) -> Option<String> {
    let head = match file_name.find('[') {
        Some(index) => &file_name[..index],
        None => file_name,
    };
    let head = head.trim();
    if head.is_empty() {
        // A file whose name is only brackets keeps the name it has.
        return Some(file_name.to_string());
    }
    Some(head.to_string())
}

/// The bracketed groups of a file name, in the order they appear.
fn bracket_groups(file_name: &str) -> Vec<&str> {
    let mut groups = Vec::new();
    let mut rest = file_name;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            break;
        };
        groups.push(&after[..end]);
        rest = &after[end + 1..];
    }
    groups
}

/// Whether a bracketed group is a title id: sixteen hexadecimal digits.
fn is_title_id(group: &str) -> bool {
    group.len() == 16 && group.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The version a bracketed group carries: `v` and a decimal number, encoded
/// as `major << 16 | minor << 8 | patch`. A version of zero is no version.
fn parse_version(group: &str) -> Option<(u8, u8, u8)> {
    let digits = group.strip_prefix('v')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let raw: u32 = digits.parse().ok()?;
    if raw == 0 {
        return None;
    }
    Some(super::split_version(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_name_title_id_and_version() {
        let identity = parse("Some Game [0100000000010000][v65536].nsp");
        assert_eq!(identity.name.as_deref(), Some("Some Game"));
        assert_eq!(identity.title_id.as_deref(), Some("0100000000010000"));
        assert_eq!(identity.version, Some((1, 0, 0)));
    }

    #[test]
    fn a_name_without_brackets_carries_only_a_name() {
        let identity = parse("game.nsp");
        assert_eq!(identity.name.as_deref(), Some("game.nsp"));
        assert!(identity.title_id.is_none());
        assert!(identity.version.is_none());
    }

    #[test]
    fn a_bracket_that_is_not_a_title_id_is_ignored() {
        let identity = parse("Game [notes].nsp");
        assert!(identity.title_id.is_none());
        assert!(identity.version.is_none());
    }

    #[test]
    fn a_short_hex_bracket_is_not_a_title_id() {
        let identity = parse("Game [0100].nsp");
        assert!(identity.title_id.is_none());
    }

    #[test]
    fn a_name_may_contain_brackets() {
        let identity = parse("Game [Special Edition] [0100000000010000].nsp");
        assert_eq!(identity.name.as_deref(), Some("Game"));
        assert_eq!(identity.title_id.as_deref(), Some("0100000000010000"));
    }

    #[test]
    fn a_file_without_a_name_part_keeps_its_file_name() {
        let identity = parse("[0100000000010000].nsp");
        assert_eq!(identity.name.as_deref(), Some("[0100000000010000].nsp"));
        assert_eq!(identity.title_id.as_deref(), Some("0100000000010000"));
    }

    #[test]
    fn a_version_of_zero_is_no_version() {
        let identity = parse("Game [0100000000010000][v0].nsp");
        assert!(identity.version.is_none());
    }

    #[test]
    fn a_version_splits_into_major_minor_and_patch() {
        let identity = parse("Game [0100000000010000][v65536].nsp");
        assert_eq!(identity.version, Some((1, 0, 0)));
        let identity = parse("Game [0100000000010000][v196608].nsp");
        assert_eq!(identity.version, Some((3, 0, 0)));
    }

    #[test]
    fn an_update_title_id_is_read_from_the_name() {
        let identity = parse("Game [0100000000010800].nsp");
        assert_eq!(identity.title_id.as_deref(), Some("0100000000010800"));
    }

    #[test]
    fn the_last_bracket_of_a_kind_wins() {
        let identity = parse("Game [0100000000010000][0100000000010800].nsp");
        assert_eq!(identity.title_id.as_deref(), Some("0100000000010800"));
    }
}
