//! The byte-range classifier for download requests: one `Range` header in,
//! what to serve out.

/// What a request's `Range` header asks of a file of `size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteRange {
    /// Serve `[start, end]` inclusive.
    Slice { start: u64, end: u64 },
    /// Serve the whole file: no header, or one this server does not honor.
    Full,
    /// The range starts at or beyond the file's end: refuse it with 416.
    Unsatisfiable,
}

/// Classify a `Range` header against a file of `size` bytes. A single
/// `bytes=a-b` or `bytes=a-` range is honored (the end clamps to the file's
/// last byte); everything else — suffix ranges, multiple ranges, another
/// unit, malformed text, or no header at all — is served as the whole file,
/// which RFC 9110 allows a server to do.
pub fn parse(header: Option<&str>, size: u64) -> ByteRange {
    let Some(spec) = header
        .map(str::trim)
        .and_then(|header| header.strip_prefix("bytes="))
    else {
        return ByteRange::Full;
    };
    let mut parts = spec.split(',');
    let Some(first) = parts.next() else {
        return ByteRange::Full;
    };
    if parts.next().is_some() {
        return ByteRange::Full;
    }
    let (start, end) = match first.split_once('-') {
        // A suffix range (`-500`): not honored, serve the whole file.
        Some(("", _)) => return ByteRange::Full,
        Some((from, "")) => match from.parse::<u64>() {
            Ok(start) => (start, None),
            Err(_) => return ByteRange::Full,
        },
        Some((from, to)) => match (from.parse::<u64>(), to.parse::<u64>()) {
            (Ok(start), Ok(end)) if start <= end => (start, Some(end)),
            _ => return ByteRange::Full,
        },
        None => return ByteRange::Full,
    };
    if size == 0 || start >= size {
        return ByteRange::Unsatisfiable;
    }
    ByteRange::Slice {
        start,
        end: end.unwrap_or(size - 1).min(size - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(start: u64, end: u64) -> ByteRange {
        ByteRange::Slice { start, end }
    }

    #[test]
    fn no_header_serves_the_whole_file() {
        assert_eq!(parse(None, 10), ByteRange::Full);
    }

    #[test]
    fn a_bounded_range_serves_exact_bytes() {
        assert_eq!(parse(Some("bytes=2-3"), 10), slice(2, 3));
    }

    #[test]
    fn a_range_end_clamps_to_the_files_last_byte() {
        assert_eq!(parse(Some("bytes=2-99"), 10), slice(2, 9));
    }

    #[test]
    fn an_open_range_serves_the_tail() {
        assert_eq!(parse(Some("bytes=7-"), 10), slice(7, 9));
    }

    #[test]
    fn a_range_from_zero_is_the_whole_file() {
        assert_eq!(parse(Some("bytes=0-"), 10), slice(0, 9));
    }

    #[test]
    fn a_start_beyond_the_end_is_unsatisfiable() {
        assert_eq!(parse(Some("bytes=10-"), 10), ByteRange::Unsatisfiable);
        assert_eq!(parse(Some("bytes=100-200"), 10), ByteRange::Unsatisfiable);
    }

    #[test]
    fn an_empty_file_refuses_any_range() {
        assert_eq!(parse(Some("bytes=0-"), 0), ByteRange::Unsatisfiable);
    }

    #[test]
    fn a_suffix_range_serves_the_whole_file() {
        assert_eq!(parse(Some("bytes=-5"), 10), ByteRange::Full);
    }

    #[test]
    fn multiple_ranges_serve_the_whole_file() {
        assert_eq!(parse(Some("bytes=0-1,5-6"), 10), ByteRange::Full);
    }

    #[test]
    fn another_unit_serves_the_whole_file() {
        assert_eq!(parse(Some("items=0-1"), 10), ByteRange::Full);
    }

    #[test]
    fn malformed_ranges_serve_the_whole_file() {
        assert_eq!(parse(Some("bytes=3-1"), 10), ByteRange::Full);
        assert_eq!(parse(Some("bytes=x-y"), 10), ByteRange::Full);
        assert_eq!(parse(Some("bytes="), 10), ByteRange::Full);
        assert_eq!(
            parse(Some("bytes=99999999999999999999-"), 10),
            ByteRange::Full
        );
    }
}
