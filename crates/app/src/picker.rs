//! Filesystem browser state for choosing a directory or file in the TUI.

use std::path::{Path, PathBuf};

/// What an entry points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

/// A single listing entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
}

/// Browsing state: the displayed directory, its entries, and the highlight.
#[derive(Debug)]
pub struct Picker {
    dir: PathBuf,
    entries: Vec<Entry>,
    cursor: usize,
    error: Option<String>,
}

impl Picker {
    /// Open the picker at the process's current working directory.
    pub fn open() -> Self {
        match std::env::current_dir() {
            Ok(dir) => Self::at(dir),
            Err(e) => {
                let mut picker = Self {
                    dir: PathBuf::from("."),
                    entries: Vec::new(),
                    cursor: 0,
                    error: None,
                };
                picker.error = Some(format!("cannot read current directory: {e}"));
                picker
            }
        }
    }

    /// Open the picker at a specific directory.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let mut picker = Self {
            dir: dir.into(),
            entries: Vec::new(),
            cursor: 0,
            error: None,
        };
        picker.reload();
        picker
    }

    fn reload(&mut self) {
        self.entries.clear();
        self.cursor = 0;
        self.error = None;

        let read_dir = match std::fs::read_dir(&self.dir) {
            Ok(read_dir) => read_dir,
            Err(e) => {
                self.error = Some(format!("{}: {e}", self.dir.display()));
                return;
            }
        };

        for entry in read_dir {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Some(kind) = classify(&path, entry.file_type()) else {
                continue;
            };
            self.entries.push(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
                kind,
            });
        }

        self.entries.sort_by(|a, b| {
            kind_rank(a.kind)
                .cmp(&kind_rank(b.kind))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The entry under the highlight, or `None` for an empty listing.
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }

    /// Enter the highlighted directory, if there is one.
    pub fn descend(&mut self) {
        if let Some(entry) = self.current() {
            if entry.kind == Kind::Dir {
                self.dir = entry.path.clone();
                self.reload();
            }
        }
    }

    /// Move to the parent directory; the filesystem root has no parent.
    pub fn go_up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.dir = parent.to_path_buf();
            self.reload();
        }
    }
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
    use std::fs;

    fn names(picker: &Picker) -> Vec<&str> {
        picker.entries().iter().map(|e| e.name.as_str()).collect()
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
        assert_eq!(picker.entries()[2].kind, Kind::File);
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
}
