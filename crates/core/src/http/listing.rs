//! The root listing page: an nginx-autoindex-shaped HTML document over the
//! offered files, so DBI's `ApacheHTTP` network source can browse it.

use crate::filelist::FileList;
use std::time::{SystemTime, UNIX_EPOCH};

/// The width the anchor text is padded to, mirroring nginx's autoindex column.
const NAME_COLUMN: usize = 50;

/// The width the size is right-aligned into, as nginx prints it.
const SIZE_COLUMN: usize = 19;

/// The month abbreviations nginx's autoindex spells the dates with.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Percent-encode a file name for an href the way nginx's autoindex does:
/// every byte outside the URL-unreserved set becomes `%XX`, so a name with
/// spaces or non-ASCII characters survives the request path.
pub fn encode_href(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decode a percent-encoded request path back into the entry name it was
/// built from. `None` when the encoding is malformed or not UTF-8.
pub fn decode_href(href: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = href.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let high = hex(*bytes.get(i + 1)?)?;
            let low = hex(*bytes.get(i + 2)?)?;
            out.push(high << 4 | low);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Escape a file name for use as HTML text: the characters that open or close
/// markup never reach the page raw.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// The listing page for the offered files: nginx's autoindex shape — one
/// anchor per file, padded name column, a date column, a right-aligned size —
/// so whatever parses nginx's pages parses ours.
pub fn listing_page(files: &FileList, date: &str) -> String {
    let mut rows = String::new();
    for (name, entry) in files.iter() {
        let text = escape_html(name);
        let href = encode_href(name);
        let pad = " ".repeat(NAME_COLUMN.saturating_sub(name.chars().count()).max(1));
        rows.push_str(&format!(
            "<a href=\"{href}\">{text}</a>{pad}{date} {:>width$}\n",
            entry.size,
            width = SIZE_COLUMN,
        ));
    }
    format!(
        "<html>\n<head><title>Index of /</title></head>\n<body>\n<h1>Index of /</h1><hr><pre>{rows}</pre><hr></body>\n</html>\n"
    )
}

/// The date column every row of one server run carries: the server's start
/// time, nginx-formatted. The listing's shape keeps the column; nothing on
/// the console is known to read it.
pub fn list_date_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    list_date(secs)
}

/// One timestamp as nginx spells it in a listing row: `23-Oct-2026 12:34`, UTC.
pub fn list_date(secs: u64) -> String {
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let time = secs % 86_400;
    format!(
        "{day:02}-{}-{year} {:02}:{:02}",
        MONTHS[month - 1],
        time / 3600,
        (time % 3600) / 60,
    )
}

/// Days since the Unix epoch to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, usize, u64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_point = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_point + 2) / 5 + 1;
    let month = if month_point < 10 {
        month_point + 3
    } else {
        month_point - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        month as usize,
        day,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SmbLocation, Source};
    use std::path::PathBuf;

    fn file(name: &str, size: u64) -> (String, crate::filelist::FileEntry) {
        (
            name.to_string(),
            crate::filelist::FileEntry {
                source: Source::Local(PathBuf::from(format!("/games/{name}"))),
                size,
                identity: crate::metadata::TitleIdentity::default(),
            },
        )
    }

    fn share_file(share: &str, path: &str, size: u64) -> (String, crate::filelist::FileEntry) {
        let name = path.rsplit('/').next().unwrap_or(path);
        (
            name.to_string(),
            crate::filelist::FileEntry {
                source: Source::Smb(SmbLocation {
                    share: share.to_string(),
                    path: path.to_string(),
                }),
                size,
                identity: crate::metadata::TitleIdentity::default(),
            },
        )
    }

    #[test]
    fn hrefs_keep_unreserved_bytes_and_encode_the_rest() {
        assert_eq!(encode_href("Game-1_0.~NSP"), "Game-1_0.~NSP");
        assert_eq!(encode_href("a b&c.nsp"), "a%20b%26c.nsp");
    }

    #[test]
    fn a_non_ascii_name_encodes_in_the_href_and_stays_readable_in_the_text() {
        let name = "Игра [0100000000010000].nsp";
        let page = {
            let mut files = FileList::new();
            files.insert_source(name, Source::Local(PathBuf::from("/g/Игра.nsp")), 1);
            listing_page(&files, "23-Oct-2026 12:34")
        };
        assert!(
            page.contains(">Игра [0100000000010000].nsp</a>"),
            "the anchor text is the name itself: {page}"
        );
        assert!(
            !page.contains("href=\"Игра"),
            "the href carries no raw non-ASCII bytes: {page}"
        );
    }

    #[test]
    fn an_encoding_round_trips_to_the_entry_name() {
        for name in [
            "Game.nsp",
            "a b.nsz",
            "Игра [v0].nsp",
            "100% [x].nsp",
            "plus+slash?.nsp",
        ] {
            assert_eq!(decode_href(&encode_href(name)), Some(name.to_string()));
        }
    }

    #[test]
    fn a_malformed_encoding_decodes_to_none() {
        assert_eq!(decode_href("a%2"), None);
        assert_eq!(decode_href("a%zz"), None);
    }

    #[test]
    fn markup_characters_are_escaped_in_the_text() {
        assert_eq!(
            escape_html("a&b<c>\"d\"'e"),
            "a&amp;b&lt;c&gt;&quot;d&quot;&#39;e"
        );
    }

    #[test]
    fn three_files_yield_three_anchors_with_their_sizes() {
        let mut files = FileList::new();
        for (name, entry) in [
            file("b.nsp", 2048),
            file("a.nsp", 10),
            file("c.nsz", 123_456_789),
        ] {
            files.insert_source(name, entry.source, entry.size);
        }
        let page = listing_page(&files, "23-Oct-2026 12:34");

        assert_eq!(
            page.matches("<a href=").count(),
            3,
            "exactly one anchor per file"
        );
        assert!(page.contains("<a href=\"a.nsp\">a.nsp</a>"));
        assert!(page.contains("10"));
        assert!(page.contains("123456789"));
        // The list order is kept: sorted by name, as the list holds them.
        let a = page.find("a.nsp</a>").unwrap();
        let b = page.find("b.nsp</a>").unwrap();
        let c = page.find("c.nsz</a>").unwrap();
        assert!(a < b && b < c);
    }

    #[test]
    fn an_empty_list_yields_no_anchors() {
        let page = listing_page(&FileList::new(), "23-Oct-2026 12:34");
        assert_eq!(page.matches("<a href=").count(), 0);
        assert!(page.contains("<h1>Index of /</h1>"));
    }

    #[test]
    fn the_page_keeps_the_nginx_shape() {
        let mut files = FileList::new();
        let (name, entry) = share_file("nas", "switch/x.nsp", 7);
        files.insert_source(name, entry.source, entry.size);
        let page = listing_page(&files, "23-Oct-2026 12:34");

        assert!(page.starts_with("<html>\n<head><title>Index of /</title></head>"));
        assert!(page.contains("<h1>Index of /</h1><hr><pre>"));
        assert!(page.ends_with("</pre><hr></body>\n</html>\n"));
    }

    #[test]
    fn dates_render_as_nginx_spells_them() {
        assert_eq!(list_date(0), "01-Jan-1970 00:00");
        assert_eq!(list_date(86_400), "02-Jan-1970 00:00");
        assert_eq!(list_date(3_661), "01-Jan-1970 01:01");
        // A leap day: 29 Feb 2000, the day before 951868800 (1 Mar 2000).
        assert_eq!(list_date(951_868_800 - 86_400), "29-Feb-2000 00:00");
        assert_eq!(list_date(951_868_800), "01-Mar-2000 00:00");
    }
}
