//! Directory browser state for choosing a path in the TUI.
//!
//! Directories are either on the local filesystem (listed synchronously) or
//! inside a connected share (listed through the store, one request at a time).
//! Either way the picker never touches the network itself: share listings
//! travel through [`Picker::reload_request`] to the app's action loop.

use dbi_backend_core::source::{ListEntry, SmbLocation, Source};
use std::path::{Path, PathBuf};

/// What an entry points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

/// A single listing entry, carrying everything needed to add it later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub source: Source,
    /// The display name (one path component).
    pub name: String,
    pub kind: Kind,
    /// Regular files carry a size; directories do not.
    pub size: Option<u64>,
}

/// Where the picker currently browses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Local(PathBuf),
    /// A share directory: the display name plus the path inside the share.
    Share(SmbLocation),
}

impl Origin {
    /// The human-readable location. Local paths show as themselves; share
    /// locations show as `<share>:<path>` (the share root as `<share>:/`).
    pub fn display(&self) -> String {
        match self {
            Origin::Local(path) => path.display().to_string(),
            Origin::Share(location) => Source::Smb(location.clone()).to_string(),
        }
    }
}

/// Browsing state: the displayed origin, its full listing, the filtered view,
/// an optional filter string, the highlight, and whether a share listing is
/// still in flight. A share picker keeps its opening root: going up stops
/// at that root instead of climbing out of the configured share.
#[derive(Debug)]
pub struct Picker {
    origin: Origin,
    root_share_path: String,
    all: Vec<Entry>,
    visible: Vec<Entry>,
    filter: String,
    filtering: bool,
    cursor: usize,
    error: Option<String>,
    busy: bool,
}

impl Picker {
    /// Open the picker at the process's current working directory.
    pub fn open() -> Self {
        match std::env::current_dir() {
            Ok(dir) => Self::at(dir),
            Err(e) => {
                let mut picker = Self::empty(Origin::Local(PathBuf::from(".")), "");
                picker.error = Some(format!("cannot read current directory: {e}"));
                picker
            }
        }
    }

    /// Open the picker at a specific local directory.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let mut picker = Self::empty(Origin::Local(dir.into()), "");
        picker.reload_local();
        picker
    }

    /// Open the picker at a share's root (the configured root, possibly empty
    /// meaning the share root). The listing arrives through the store.
    pub fn at_share(share: &str, root: &str) -> Self {
        let origin = Origin::Share(SmbLocation {
            share: share.to_string(),
            path: root.to_string(),
        });
        let mut picker = Self::empty(origin, root);
        picker.busy = true;
        picker
    }

    fn empty(origin: Origin, root_share_path: &str) -> Self {
        Self {
            origin,
            root_share_path: root_share_path.to_string(),
            all: Vec::new(),
            visible: Vec::new(),
            filter: String::new(),
            filtering: false,
            cursor: 0,
            error: None,
            busy: false,
        }
    }

    /// Local listing. Share origins ignore this hand-off: callers list them
    /// through [`Picker::reload_request`] and the store.
    fn reload_local(&mut self) {
        self.all.clear();
        self.error = None;

        let Origin::Local(dir) = &self.origin else {
            return;
        };
        let read_dir = match std::fs::read_dir(dir) {
            Ok(read_dir) => read_dir,
            Err(e) => {
                self.error = Some(format!("{}: {e}", dir.display()));
                self.apply_filter();
                return;
            }
        };

        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Some(kind) = classify(&path, entry.file_type()) else {
                continue;
            };
            let size = if kind == Kind::File {
                std::fs::metadata(&path).map(|meta| meta.len()).ok()
            } else {
                None
            };
            self.all.push(Entry {
                source: Source::Local(path),
                name: entry.file_name().to_string_lossy().into_owned(),
                kind,
                size,
            });
        }

        self.all.sort_by(|a, b| {
            kind_rank(a.kind)
                .cmp(&kind_rank(b.kind))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        self.apply_filter();
    }

    /// Store share listings onto the picker for its current origin.
    pub fn apply_share_listing(&mut self, location: &SmbLocation, entries: Vec<ListEntry>) {
        debug_assert_eq!(
            &self.origin,
            &Origin::Share(location.clone()),
            "listing must match the browsed origin"
        );
        self.all.clear();
        self.error = None;
        self.busy = false;
        for entry in entries {
            let kind = match entry.kind {
                dbi_backend_core::source::EntryKind::Dir => Kind::Dir,
                dbi_backend_core::source::EntryKind::File => Kind::File,
            };
            self.all.push(Entry {
                source: Source::Smb(SmbLocation {
                    share: location.share.clone(),
                    path: join_share(&location.path, &entry.name),
                }),
                name: entry.name,
                kind,
                size: (kind == Kind::File).then_some(entry.size),
            });
        }
        self.all.sort_by(|a, b| {
            kind_rank(a.kind)
                .cmp(&kind_rank(b.kind))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.apply_filter();
    }

    /// The store listing for when the origin is a share: this is exactly what
    /// needs to be requested right now. `None` when nothing is pending.
    pub fn reload_request(&self) -> Option<Source> {
        self.busy.then(|| match &self.origin {
            Origin::Local(_) => unreachable!("busy is only set for share origins"),
            Origin::Share(location) => Source::Smb(location.clone()),
        })
    }

    /// Recompute the visible view from the full listing and reset the highlight.
    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.visible = if needle.is_empty() {
            self.all.clone()
        } else {
            self.all
                .iter()
                .filter(|entry| entry.name.to_lowercase().contains(&needle))
                .cloned()
                .collect()
        };
        self.cursor = 0;
    }

    /// The local directory, for tests; share origins have no local path.
    #[cfg(test)]
    pub fn dir(&self) -> &Path {
        match &self.origin {
            Origin::Local(path) => path,
            Origin::Share(_) => Path::new(""),
        }
    }

    /// The location as shown: a local path, or `share: <path>`.
    pub fn origin_display(&self) -> String {
        self.origin.display()
    }

    /// The entries currently shown (the full listing when no filter is set).
    pub fn entries(&self) -> &[Entry] {
        &self.visible
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Record a failed share listing: the message shows and the busy marker
    /// clears.
    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
        self.busy = false;
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    /// True while a share listing is still in flight.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn start_filter(&mut self) {
        self.filtering = true;
    }

    pub fn stop_filter(&mut self) {
        self.filtering = false;
    }

    pub fn push_filter(&mut self, c: char) {
        self.filter.push(c);
        self.apply_filter();
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.apply_filter();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.apply_filter();
    }

    /// The entry under the highlight, or `None` for an empty listing.
    pub fn current(&self) -> Option<&Entry> {
        self.visible.get(self.cursor)
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.visible.len() {
            self.cursor += 1;
        }
    }

    /// Enter the highlighted directory, if there is one. Local directories
    /// reload synchronously; share directories leave a pending listing for
    /// [`Picker::reload_request`].
    pub fn descend(&mut self) {
        let Some(entry) = self.current() else { return };
        if entry.kind != Kind::Dir {
            return;
        }
        self.set_origin(match &entry.source {
            Source::Local(path) => Origin::Local(path.clone()),
            Source::Smb(location) => Origin::Share(location.clone()),
        });
    }

    /// Move to the parent directory; stop at the filesystem root and, for a
    /// share, at the share's opening root. Returns the parent share source to
    /// list, if the new origin needs one.
    pub fn go_up(&mut self) -> Option<Source> {
        let new_origin = match &self.origin {
            Origin::Local(path) => Origin::Local(path.parent()?.to_path_buf()),
            Origin::Share(location) => {
                let root = &self.root_share_path;
                let parent = parent_share_path(&location.path)?;
                let within = root.is_empty()
                    || parent == root.as_str()
                    || parent.starts_with(&format!("{root}/"));
                if !within {
                    return None;
                }
                Origin::Share(SmbLocation {
                    share: location.share.clone(),
                    path: parent,
                })
            }
        };
        self.set_origin(new_origin.clone());
        match new_origin {
            Origin::Share(location) => Some(Source::Smb(location)),
            _ => None,
        }
    }

    fn set_origin(&mut self, origin: Origin) {
        self.origin = origin;
        match &self.origin {
            Origin::Local(_) => self.reload_local(),
            Origin::Share(_) => {
                // The old directory's listing must not linger under the new
                // origin while its own listing is in flight.
                self.busy = true;
                self.all.clear();
                self.apply_filter();
            }
        }
    }
}

/// `<root>/<name>`; the root path has no leading or trailing slash.
fn join_share(root: &str, name: &str) -> String {
    if root.is_empty() {
        name.to_string()
    } else {
        format!("{root}/{name}")
    }
}

/// The parent path inside the share, or `None` at the share root. A top-level
/// entry's parent is the root itself (the empty path).
fn parent_share_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    Some(match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    })
}

fn kind_rank(kind: Kind) -> u8 {
    match kind {
        Kind::Dir => 0,
        Kind::File => 1,
    }
}

/// Classify an entry, following symlinks via metadata; unreadable entries yield `None`.
fn classify(path: &Path, file_type: std::io::Result<std::fs::FileType>) -> Option<Kind> {
    match file_type {
        Ok(ft) if ft.is_dir() => Some(Kind::Dir),
        Ok(ft) if ft.is_file() => Some(Kind::File),
        _ => {
            let meta = std::fs::metadata(path).ok()?;
            if meta.is_dir() {
                Some(Kind::Dir)
            } else if meta.is_file() {
                Some(Kind::File)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::Kind::File;
    use std::fs;

    fn names(picker: &Picker) -> Vec<&str> {
        picker.entries().iter().map(|e| e.name.as_str()).collect()
    }

    fn filter_with(picker: &mut Picker, text: &str) {
        picker.start_filter();
        for c in text.chars() {
            picker.push_filter(c);
        }
    }

    #[test]
    fn lists_directories_before_files_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Zeta.nsp"), b"z").unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
        fs::create_dir(dir.path().join("beta")).unwrap();
        fs::create_dir(dir.path().join("Alpha")).unwrap();

        let picker = Picker::at(dir.path());
        assert_eq!(
            names(&picker),
            vec!["Alpha", "beta", "alpha.nsp", "Zeta.nsp"]
        );
        assert_eq!(picker.entries()[0].kind, Kind::Dir);
        assert_eq!(picker.entries()[2].kind, File);
    }

    #[test]
    fn local_entries_carry_their_source() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();

        let picker = Picker::at(dir.path());
        assert_eq!(
            picker.entries()[0].source,
            Source::Local(dir.path().join("a.nsp"))
        );
    }

    #[test]
    fn move_up_clamps_at_first_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsp"), b"b").unwrap();

        let mut picker = Picker::at(dir.path());
        assert_eq!(picker.cursor(), 0);
        picker.move_up();
        assert_eq!(picker.cursor(), 0);
    }

    #[test]
    fn move_down_clamps_at_last_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsp"), b"b").unwrap();

        let mut picker = Picker::at(dir.path());
        picker.move_down();
        picker.move_down();
        assert_eq!(picker.cursor(), 1);
    }

    #[test]
    fn descend_enters_highlighted_directory_and_resets_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("inside.nsp"), b"i").unwrap();
        fs::write(dir.path().join("top.nsp"), b"t").unwrap();

        let mut picker = Picker::at(dir.path());
        picker.move_down();
        assert_eq!(picker.cursor(), 1);
        picker.move_up();
        picker.descend();
        assert_eq!(picker.dir(), sub.as_path());
        assert_eq!(picker.cursor(), 0);
        assert_eq!(names(&picker), vec!["inside.nsp"]);
    }

    #[test]
    fn descend_does_nothing_on_a_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("only.nsp"), b"o").unwrap();

        let mut picker = Picker::at(dir.path());
        picker.descend();
        assert_eq!(picker.dir(), dir.path());
    }

    #[test]
    fn go_up_moves_to_the_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        let mut picker = Picker::at(&sub);
        picker.go_up();
        assert_eq!(picker.dir(), dir.path());
    }

    #[test]
    fn go_up_stays_at_the_filesystem_root() {
        let mut picker = Picker::at("/");
        picker.go_up();
        assert_eq!(picker.dir(), Path::new("/"));
    }

    #[test]
    fn opening_an_unreadable_directory_records_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");

        let picker = Picker::at(&missing);
        assert!(picker.entries().is_empty());
        assert!(picker.error().is_some());
        assert_eq!(picker.current(), None);
    }

    #[test]
    fn filter_narrows_the_listing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
        fs::write(dir.path().join("beta.nsp"), b"b").unwrap();

        let mut picker = Picker::at(dir.path());
        filter_with(&mut picker, "BETA");
        assert_eq!(names(&picker), vec!["beta.nsp"]);
        assert_eq!(picker.cursor(), 0);
    }

    #[test]
    fn filter_with_no_matches_empties_the_listing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();

        let mut picker = Picker::at(dir.path());
        filter_with(&mut picker, "zzz");
        assert!(picker.entries().is_empty());
        assert_eq!(picker.current(), None);
    }

    #[test]
    fn clearing_the_filter_restores_the_listing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
        fs::write(dir.path().join("beta.nsp"), b"b").unwrap();

        let mut picker = Picker::at(dir.path());
        filter_with(&mut picker, "beta");
        assert_eq!(picker.entries().len(), 1);
        picker.clear_filter();
        assert_eq!(picker.entries().len(), 2);
    }

    #[test]
    fn move_down_clamps_within_the_filtered_listing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
        fs::write(dir.path().join("beta.nsp"), b"b").unwrap();

        let mut picker = Picker::at(dir.path());
        filter_with(&mut picker, "a.nsp");
        assert_eq!(picker.entries().len(), 2);
        picker.move_down();
        picker.move_down();
        assert_eq!(picker.cursor(), 1);
    }

    // --- share browsing -------------------------------------------------

    fn list_entry(name: &str, kind: dbi_backend_core::source::EntryKind, size: u64) -> ListEntry {
        ListEntry {
            name: name.to_string(),
            kind,
            size,
        }
    }

    #[test]
    fn a_share_picker_starts_busy_and_takes_the_listing() {
        let mut picker = Picker::at_share("nas", "");
        assert!(picker.is_busy());
        assert_eq!(picker.origin_display(), "nas:/");
        assert!(picker.entries().is_empty());
        assert_eq!(
            picker.reload_request(),
            Some(Source::Smb(SmbLocation {
                share: "nas".into(),
                path: "".into()
            }))
        );

        picker.apply_share_listing(
            &SmbLocation {
                share: "nas".into(),
                path: "".into(),
            },
            vec![
                list_entry("Switch", dbi_backend_core::source::EntryKind::Dir, 0),
                list_entry("game.nsp", dbi_backend_core::source::EntryKind::File, 42),
            ],
        );
        assert!(!picker.is_busy());
        assert_eq!(picker.reload_request(), None);
        assert_eq!(names(&picker), vec!["Switch", "game.nsp"]);
        assert_eq!(picker.entries()[0].kind, Kind::Dir);
        assert_eq!(picker.entries()[1].size, Some(42));
        assert_eq!(
            picker.entries()[1].source,
            Source::Smb(SmbLocation {
                share: "nas".into(),
                path: "game.nsp".into()
            })
        );
    }

    #[test]
    fn share_listing_errors_record_a_message() {
        let mut picker = Picker::at_share("nas", "");
        // Listing failures come as a recorded error with the busy marker off.
        picker.set_error("nas:/switch: the share is gone".to_string());
        assert!(picker.error().is_some());
        assert!(!picker.is_busy());
    }

    #[test]
    fn descend_into_a_share_directory_requests_its_listing() {
        let mut picker = Picker::at_share("nas", "");
        picker.apply_share_listing(
            &SmbLocation {
                share: "nas".into(),
                path: "".into(),
            },
            vec![list_entry(
                "Switch",
                dbi_backend_core::source::EntryKind::Dir,
                0,
            )],
        );
        picker.move_down();
        picker.descend();
        assert!(picker.is_busy());
        assert_eq!(picker.origin_display(), "nas:/Switch");
        assert_eq!(picker.entries().len(), 0);
        assert_eq!(
            picker.reload_request(),
            Some(Source::Smb(SmbLocation {
                share: "nas".into(),
                path: "Switch".into()
            }))
        );
    }

    #[test]
    fn go_up_stops_at_the_configured_share_root() {
        // Opened at a configured root: going up must never leave it.
        let mut picker = Picker::at_share("nas", "library/switch");
        let root = SmbLocation {
            share: "nas".into(),
            path: "library/switch".into(),
        };
        picker.apply_share_listing(
            &root,
            vec![list_entry(
                "Games",
                dbi_backend_core::source::EntryKind::Dir,
                0,
            )],
        );
        picker.move_down();
        picker.descend();
        assert_eq!(picker.origin_display(), "nas:/library/switch/Games");

        // Up: back to the root, which is listed again (its listing was
        // discarded on the way down).
        let request = picker.go_up();
        assert_eq!(
            request,
            Some(Source::Smb(SmbLocation {
                share: "nas".into(),
                path: "library/switch".into()
            }))
        );
        assert!(picker.is_busy());

        // At the root itself, go up does nothing at all — not even towards
        // `library`, which is outside the configured root.
        picker.apply_share_listing(
            &root,
            vec![list_entry(
                "Games",
                dbi_backend_core::source::EntryKind::Dir,
                0,
            )],
        );
        assert_eq!(picker.go_up(), None);
        assert_eq!(picker.origin_display(), "nas:/library/switch");
        assert!(!picker.is_busy());
    }

    #[test]
    fn go_up_within_a_share_lists_the_parent() {
        let mut picker = Picker::at_share("nas", ""); // the app lists after this
        picker.apply_share_listing(
            &SmbLocation {
                share: "nas".into(),
                path: "".into(),
            },
            vec![list_entry(
                "Switch",
                dbi_backend_core::source::EntryKind::Dir,
                0,
            )],
        );
        picker.move_down();
        picker.descend();
        picker.busy = false; // pretend the listing arrived

        assert_eq!(
            picker.go_up(),
            Some(Source::Smb(SmbLocation {
                share: "nas".into(),
                path: "".into()
            }))
        );
        assert!(picker.is_busy());
        assert_eq!(picker.origin_display(), "nas:/");
    }

    #[test]
    fn share_filtering_works_on_listings() {
        let mut picker = Picker::at_share("nas", "");
        picker.apply_share_listing(
            &SmbLocation {
                share: "nas".into(),
                path: "".into(),
            },
            vec![
                list_entry("alpha.nsp", dbi_backend_core::source::EntryKind::File, 1),
                list_entry("beta.nsp", dbi_backend_core::source::EntryKind::File, 2),
            ],
        );
        filter_with(&mut picker, "BETA");
        assert_eq!(names(&picker), vec!["beta.nsp"]);
    }

    #[test]
    fn share_root_with_configured_root_names_it() {
        let picker = Picker::at_share("nas", "library/switch");
        assert_eq!(picker.origin_display(), "nas:/library/switch");
    }
}
