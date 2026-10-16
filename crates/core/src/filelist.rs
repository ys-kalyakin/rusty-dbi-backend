//! The set of files offered to the Switch, keyed by file name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Name-to-path map. A `BTreeMap` keeps the listing deterministic (sorted by name).
#[derive(Debug, Default, Clone)]
pub struct FileList {
    files: BTreeMap<String, PathBuf>,
}

/// What an [`FileList::add_path`] call did, so callers can report warnings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AddOutcome {
    /// Names added to the list.
    pub added: Vec<String>,
    /// Paths that could not be read and were skipped.
    pub skipped: Vec<PathBuf>,
    /// Names that replaced a previously added entry.
    pub replaced: Vec<String>,
}

impl FileList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an entry by name, returning the path it replaced, if any.
    pub fn insert(&mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Option<PathBuf> {
        self.files.insert(name.into(), path.into())
    }

    /// Add a file, or every `.nsp`/`.nsz` file found recursively under a directory.
    ///
    /// A directly added file is always included, regardless of extension. A directory
    /// is walked iteratively, including nested subdirectories. Entries and
    /// subdirectories that cannot be read are skipped and reported in the outcome
    /// rather than aborting the scan; only a failure to read the given path itself
    /// is returned as an error.
    pub fn add_path(&mut self, path: &Path) -> std::io::Result<AddOutcome> {
        let mut outcome = AddOutcome::default();
        if path.is_file() {
            self.add_file(path, &mut outcome);
            return Ok(outcome);
        }

        let mut stack = vec![path.to_path_buf()];
        let mut first = true;
        while let Some(dir) = stack.pop() {
            let read_dir = match std::fs::read_dir(&dir) {
                Ok(read_dir) => read_dir,
                Err(e) => {
                    if first {
                        return Err(e);
                    }
                    outcome.skipped.push(dir);
                    continue;
                }
            };
            first = false;
            for entry in read_dir {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => {
                        outcome.skipped.push(dir.clone());
                        continue;
                    }
                };
                let child = entry.path();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => {
                        outcome.skipped.push(child);
                        continue;
                    }
                };
                if file_type.is_dir() {
                    stack.push(child);
                } else if file_type.is_file() {
                    if has_target_ext(&child) {
                        self.add_file(&child, &mut outcome);
                    }
                } else if file_type.is_symlink() {
                    // Follow file symlinks, but never descend into directory symlinks
                    // (avoids cycles). Broken links are ignored.
                    if let Ok(meta) = std::fs::metadata(&child) {
                        if meta.is_file() && has_target_ext(&child) {
                            self.add_file(&child, &mut outcome);
                        }
                    }
                }
            }
        }
        Ok(outcome)
    }

    fn add_file(&mut self, path: &Path, outcome: &mut AddOutcome) {
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            let name = name.to_string();
            if self.files.insert(name.clone(), abs).is_some() {
                outcome.replaced.push(name.clone());
            }
            outcome.added.push(name);
        }
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn get(&self, name: &str) -> Option<&PathBuf> {
        self.files.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &PathBuf)> {
        self.files.iter()
    }
}

/// Whether the path's extension is one of the Switch title formats, case-insensitively.
fn has_target_ext(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("nsp") | Some("nsz")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn add_file_inserts_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.nsp");
        fs::write(&f, b"data").unwrap();
        let mut list = FileList::new();
        list.add_path(&f).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("x.nsp").is_some());
    }

    #[test]
    fn add_dir_recurses_into_nested_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        let sub = dir.path().join("sub").join("deep");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("b.nsz"), b"b").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path()).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("a.nsp").is_some());
        assert!(list.get("b.nsz").is_some());
    }

    #[test]
    fn add_dir_skips_non_matching_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.nsp"), b"g").unwrap();
        fs::write(dir.path().join("notes.txt"), b"n").unwrap();
        fs::write(dir.path().join("cover.jpg"), b"c").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("game.nsp").is_some());
    }

    #[test]
    fn add_dir_matches_extensions_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("GAME.NSP"), b"g").unwrap();
        fs::write(dir.path().join("Patch.NsZ"), b"p").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path()).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("GAME.NSP").is_some());
        assert!(list.get("Patch.NsZ").is_some());
    }

    #[test]
    fn add_file_is_not_extension_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("readme.txt");
        fs::write(&f, b"r").unwrap();
        let mut list = FileList::new();
        list.add_path(&f).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("readme.txt").is_some());
    }

    #[test]
    fn add_empty_dir_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path()).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn insert_overwrites_same_name() {
        let mut list = FileList::new();
        list.insert("a", "/1");
        list.insert("a", "/2");
        assert_eq!(list.len(), 1);
        assert_eq!(list.get("a").unwrap(), &PathBuf::from("/2"));
    }

    #[test]
    fn insert_returns_the_replaced_path() {
        let mut list = FileList::new();
        assert_eq!(list.insert("a", "/1"), None);
        assert_eq!(list.insert("a", "/2"), Some(PathBuf::from("/1")));
    }

    #[test]
    fn add_path_reports_replaced_name() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        fs::write(one.path().join("game.nsp"), b"1").unwrap();
        fs::write(two.path().join("game.nsp"), b"2").unwrap();

        let mut list = FileList::new();
        list.add_path(one.path()).unwrap();
        let outcome = list.add_path(two.path()).unwrap();

        assert_eq!(outcome.replaced, vec!["game.nsp".to_string()]);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn add_path_reports_added_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();

        let mut list = FileList::new();
        let outcome = list.add_path(dir.path()).unwrap();

        assert_eq!(outcome.added, vec!["a.nsp".to_string()]);
        assert!(outcome.skipped.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn add_dir_skips_unreadable_subdirectory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ok.nsp"), b"o").unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("hidden.nsp"), b"h").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let mut list = FileList::new();
        let outcome = list.add_path(dir.path()).unwrap();

        // Restore permissions so the tempdir can be cleaned up.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(list.get("ok.nsp").is_some());
        assert!(list.get("hidden.nsp").is_none());
        assert!(outcome.skipped.iter().any(|p| p.ends_with("locked")));
    }
}
