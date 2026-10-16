//! The set of files offered to the Switch, keyed by file name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Name-to-path map. A `BTreeMap` keeps the listing deterministic (sorted by name).
#[derive(Debug, Default, Clone)]
pub struct FileList {
    files: BTreeMap<String, PathBuf>,
}

impl FileList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an entry by name (later inserts with the same name overwrite).
    pub fn insert(&mut self, name: impl Into<String>, path: impl Into<PathBuf>) {
        self.files.insert(name.into(), path.into());
    }

    /// Add a file, or every immediate regular file in a directory.
    pub fn add_path(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_file() {
            self.add_file(path);
        } else {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let child = entry.path();
                if child.is_file() {
                    self.add_file(&child);
                }
            }
        }
        Ok(())
    }

    fn add_file(&mut self, path: &Path) {
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            self.files.insert(name.to_string(), abs);
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
    fn add_dir_inserts_immediate_files_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsp"), b"b").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path()).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("sub").is_none());
    }

    #[test]
    fn insert_overwrites_same_name() {
        let mut list = FileList::new();
        list.insert("a", "/1");
        list.insert("a", "/2");
        assert_eq!(list.len(), 1);
        assert_eq!(list.get("a").unwrap(), &PathBuf::from("/2"));
    }
}
