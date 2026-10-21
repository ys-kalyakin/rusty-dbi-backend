//! The set of files offered to the Switch, keyed by file name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::filters::Filters;
use crate::metadata::{self, TitleIdentity};
use crate::source::{EntryKind, Source, SourceError, Sources};

/// A single offered file: where its bytes come from, its size in bytes, and
/// what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub source: Source,
    pub size: u64,
    /// What the file is: its title id, the kind of content it is and its
    /// version. Empty when the file's identity could not be determined.
    pub identity: TitleIdentity,
}

/// Name-to-entry map. A `BTreeMap` keeps the listing deterministic (sorted by name).
#[derive(Debug, Default, Clone)]
pub struct FileList {
    files: BTreeMap<String, FileEntry>,
    /// The rules a directory scan accepts files by.
    filters: Filters,
}

/// What an [`FileList::add_path`] call did, so callers can report warnings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AddOutcome {
    /// Names added to the list.
    pub added: Vec<String>,
    /// Sources that could not be read and were skipped.
    pub skipped: Vec<Source>,
    /// Names that replaced a previously added entry.
    pub replaced: Vec<String>,
    /// How many files the filters rejected during a scan.
    pub rejected: usize,
}

impl FileList {
    pub fn new() -> Self {
        Self::default()
    }

    /// A list whose directory scans accept the files the given filters accept.
    pub fn with_filters(filters: Filters) -> Self {
        Self {
            filters,
            ..Self::default()
        }
    }

    /// Insert an entry by name, returning the source it replaced, if any.
    pub fn insert(&mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Option<Source> {
        let path = path.into();
        let size = file_size(&path);
        self.files
            .insert(
                name.into(),
                FileEntry {
                    source: Source::Local(path),
                    size,
                    identity: TitleIdentity::default(),
                },
            )
            .map(|entry| entry.source)
    }

    /// Insert an exact entry: any source with a known size, no filesystem
    /// access. Exercised by tests; share adds take `add_source` instead.
    pub fn insert_source(
        &mut self,
        name: impl Into<String>,
        source: Source,
        size: u64,
    ) -> Option<Source> {
        self.files
            .insert(
                name.into(),
                FileEntry {
                    source,
                    size,
                    identity: TitleIdentity::default(),
                },
            )
            .map(|entry| entry.source)
    }

    /// Add a file, or every `.nsp`/`.nsz` file found recursively under a directory.
    ///
    /// A directly added file is always included, regardless of extension. A directory
    /// is walked iteratively, including nested subdirectories. Entries and
    /// subdirectories that cannot be read are skipped and reported in the outcome
    /// rather than aborting the scan; only a failure to read the given path itself
    /// is returned as an error.
    ///
    /// The store reads each accepted file's identity, so a file on a share is
    /// read the same way as a local one.
    pub async fn add_path(
        &mut self,
        path: &Path,
        store: &dyn Sources,
    ) -> std::io::Result<AddOutcome> {
        let mut outcome = AddOutcome::default();
        if path.is_file() {
            self.add_file(path, store, &mut outcome).await;
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
                    outcome.skipped.push(Source::Local(dir));
                    continue;
                }
            };
            first = false;
            for entry in read_dir {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => {
                        outcome.skipped.push(Source::Local(dir.clone()));
                        continue;
                    }
                };
                let child = entry.path();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => {
                        outcome.skipped.push(Source::Local(child));
                        continue;
                    }
                };
                if file_type.is_dir() {
                    stack.push(child);
                } else if file_type.is_file() {
                    if self.accepts_name(&child) {
                        self.add_file(&child, store, &mut outcome).await;
                    } else {
                        outcome.rejected += 1;
                    }
                } else if file_type.is_symlink() {
                    // Follow file symlinks, but never descend into directory symlinks
                    // (avoids cycles). Broken links are ignored.
                    if let Ok(meta) = std::fs::metadata(&child) {
                        if meta.is_file() {
                            if self.accepts_name(&child) {
                                self.add_file(&child, store, &mut outcome).await;
                            } else {
                                outcome.rejected += 1;
                            }
                        }
                    }
                }
            }
        }
        Ok(outcome)
    }

    async fn add_file(&mut self, path: &Path, store: &dyn Sources, outcome: &mut AddOutcome) {
        let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let identity = self
            .read_identity(store, &Source::Local(path.to_path_buf()), &name)
            .await;
        self.record(Source::Local(path.to_path_buf()), size, identity, outcome);
    }

    /// Read a file's identity through the store, so a file on a share is read
    /// the same way as a local one. A file that is not a container the reader
    /// knows is not read at all, and a read that fails yields an empty
    /// identity: the identity never decides whether a file is offered.
    async fn read_identity(
        &self,
        store: &dyn Sources,
        source: &Source,
        name: &str,
    ) -> TitleIdentity {
        if !metadata::is_container_file(name) {
            return TitleIdentity::default();
        }
        let Ok(mut handle) = store.open(source).await else {
            return TitleIdentity::default();
        };
        metadata::read_identity(&mut *handle, name).await
    }

    /// Whether a scan accepts the file at this path. The filters judge the
    /// file name, never the path it was found at.
    fn accepts_name(&self, path: &Path) -> bool {
        match path.file_name() {
            Some(name) => self.filters.accepts(&name.to_string_lossy()),
            None => false,
        }
    }

    /// Record one entry under its file name, reporting a replacement when the
    /// name was already present. Local sources resolve symlinks here, as the
    /// old path-based add always did.
    fn record(
        &mut self,
        source: Source,
        size: u64,
        identity: TitleIdentity,
        outcome: &mut AddOutcome,
    ) {
        let resolved = match source {
            Source::Local(path) => {
                Source::Local(path.canonicalize().unwrap_or_else(|_| path.clone()))
            }
            other => other,
        };
        if let Some(name) = resolved.file_name() {
            if self
                .files
                .insert(
                    name.clone(),
                    FileEntry {
                        source: resolved,
                        size,
                        identity,
                    },
                )
                .is_some()
            {
                outcome.replaced.push(name.clone());
            }
            outcome.added.push(name);
        }
    }

    /// Add one file, or a scanned directory, from any source. Carries
    /// [`FileList::add_path`]'s semantics over share listings: a directly
    /// added file is unfiltered, a directory is scanned recursively and
    /// filtered, unreadable parts are skipped and reported, and sizes come
    /// from the listings. A missing or unreadable root is an error.
    pub async fn add_source(
        &mut self,
        store: &dyn Sources,
        source: &Source,
    ) -> Result<AddOutcome, SourceError> {
        let Some(stat) = store.stat(source).await? else {
            return Err(SourceError::new(format!("no such path: {source}")));
        };
        let mut outcome = AddOutcome::default();
        match stat.kind {
            EntryKind::File => {
                let identity = self.read_identity(store, source, &name_of(source)).await;
                self.record(source.clone(), stat.size, identity, &mut outcome);
            }
            EntryKind::Dir => {
                self.scan_dir(store, source, &mut outcome, true).await?;
            }
        }
        Ok(outcome)
    }

    /// List one directory, add its matching files, and descend into every
    /// subdirectory. Only the root's listing failure is fatal.
    async fn scan_dir(
        &mut self,
        store: &dyn Sources,
        dir: &Source,
        outcome: &mut AddOutcome,
        root: bool,
    ) -> Result<(), SourceError> {
        let entries = match store.list_dir(dir).await {
            Ok(entries) => entries,
            Err(e) => {
                if root {
                    return Err(e);
                }
                outcome.skipped.push(dir.clone());
                return Ok(());
            }
        };
        let mut stack = Vec::new();
        for entry in entries {
            // A self or parent entry refers to the walked directory itself;
            // descending through one, however the listing reports it, would
            // never end.
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            let child = dir.join(&entry.name);
            match entry.kind {
                EntryKind::Dir => stack.push(child),
                EntryKind::File if self.filters.accepts(&entry.name) => {
                    let identity = self.read_identity(store, &child, &entry.name).await;
                    self.record(child, entry.size, identity, outcome);
                }
                EntryKind::File => outcome.rejected += 1,
            }
        }
        for sub in stack {
            Box::pin(self.scan_dir(store, &sub, outcome, false)).await?;
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// Remove an entry by name, returning it if it was present.
    pub fn remove(&mut self, name: &str) -> Option<FileEntry> {
        self.files.remove(name)
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Sum of every entry's size in bytes.
    pub fn total_size(&self) -> u64 {
        self.files.values().map(|entry| entry.size).sum()
    }

    pub fn get(&self, name: &str) -> Option<&FileEntry> {
        self.files.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &FileEntry)> {
        self.files.iter()
    }
}

/// Read a path's size in bytes, tolerating unreadable metadata with `0`.
fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// The file name of a source: the last component of its path.
fn name_of(source: &Source) -> String {
    source.file_name().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::ContentType;
    use crate::source::local::LocalStore;
    use std::fs;

    #[tokio::test]
    async fn add_file_inserts_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.nsp");
        fs::write(&f, b"data").unwrap();
        let mut list = FileList::new();
        list.add_path(&f, &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("x.nsp").is_some());
    }

    #[tokio::test]
    async fn the_default_filters_accept_nsp_and_nsz() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("GAME.NSP"), b"a").unwrap();
        fs::write(dir.path().join("Patch.NsZ"), b"b").unwrap();
        fs::write(dir.path().join("notes.txt"), b"c").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("GAME.NSP").is_some());
        assert!(list.get("Patch.NsZ").is_some());
        assert!(list.get("notes.txt").is_none());
    }

    #[tokio::test]
    async fn a_configured_filter_set_changes_what_the_local_walk_accepts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsz"), b"b").unwrap();
        let mut list = FileList::with_filters(Filters::with_extensions(["nsz"]));
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("b.nsz").is_some());
        assert!(list.get("a.nsp").is_none());
    }

    #[tokio::test]
    async fn a_configured_exclude_pattern_changes_what_the_local_walk_accepts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Game.nsp"), b"a").unwrap();
        fs::write(dir.path().join("Old.nsp"), b"b").unwrap();
        let mut filters = Filters::new();
        filters.add_exclude("Old*").unwrap();
        let mut list = FileList::with_filters(filters);
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("Game.nsp").is_some());
        assert!(list.get("Old.nsp").is_none());
    }

    #[tokio::test]
    async fn a_directly_added_file_ignores_the_filters() {
        let dir = tempfile::tempdir().unwrap();
        let readme = dir.path().join("readme.txt");
        fs::write(&readme, b"a").unwrap();
        let old = dir.path().join("Old.nsp");
        fs::write(&old, b"b").unwrap();
        let mut filters = Filters::new();
        filters.add_exclude("Old*").unwrap();
        let mut list = FileList::with_filters(filters);
        list.add_path(&readme, &LocalStore::new()).await.unwrap();
        list.add_path(&old, &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("readme.txt").is_some());
        assert!(list.get("Old.nsp").is_some());
    }

    #[tokio::test]
    async fn a_scan_reports_the_files_the_filters_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsz"), b"b").unwrap();
        fs::write(dir.path().join("notes.txt"), b"c").unwrap();
        fs::write(dir.path().join("cover.jpg"), b"d").unwrap();
        fs::write(dir.path().join("readme.md"), b"e").unwrap();
        let mut list = FileList::new();
        let outcome = list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(outcome.added.len(), 2);
        assert_eq!(outcome.rejected, 3);
    }

    #[tokio::test]
    async fn a_scan_that_rejects_everything_reports_no_accepted_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), b"c").unwrap();
        fs::write(dir.path().join("cover.jpg"), b"d").unwrap();
        let mut list = FileList::new();
        let outcome = list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert!(outcome.added.is_empty());
        assert_eq!(outcome.rejected, 2);
    }

    /// A store that records the files it was asked to open, and can be told
    /// to fail every open.
    struct RecordingStore {
        inner: LocalStore,
        opened: std::sync::Mutex<Vec<String>>,
        fail: bool,
    }

    impl RecordingStore {
        fn new() -> Self {
            Self {
                inner: LocalStore::new(),
                opened: std::sync::Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::new()
            }
        }

        fn opened(&self) -> Vec<String> {
            self.opened.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Sources for RecordingStore {
        async fn open(
            &self,
            source: &Source,
        ) -> Result<Box<dyn crate::source::FileHandle>, SourceError> {
            self.opened.lock().unwrap().push(source.to_string());
            if self.fail {
                return Err(SourceError::new("cannot open"));
            }
            self.inner.open(source).await
        }

        async fn list_dir(
            &self,
            source: &Source,
        ) -> Result<Vec<crate::source::ListEntry>, SourceError> {
            self.inner.list_dir(source).await
        }

        async fn stat(
            &self,
            source: &Source,
        ) -> Result<Option<crate::source::ListEntry>, SourceError> {
            self.inner.stat(source).await
        }
    }

    #[tokio::test]
    async fn a_file_carries_the_identity_its_name_holds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Game [0100000000010000].nsp");
        fs::write(&path, b"not a container").unwrap();

        let mut list = FileList::new();
        list.add_path(&path, &LocalStore::new()).await.unwrap();

        let entry = list.get("Game [0100000000010000].nsp").unwrap();
        assert_eq!(entry.identity.title_id.as_deref(), Some("0100000000010000"));
        assert_eq!(entry.identity.content_type, Some(ContentType::Base));
        assert_eq!(entry.identity.name.as_deref(), Some("Game"));
    }

    #[tokio::test]
    async fn the_identity_is_read_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.nsp");
        fs::write(&path, b"not a container").unwrap();
        let store = RecordingStore::new();

        let mut list = FileList::new();
        list.add_path(&path, &store).await.unwrap();

        assert_eq!(
            store.opened(),
            vec![format!("{}", path.display())],
            "the store was asked to open the file"
        );
    }

    #[tokio::test]
    async fn a_file_that_cannot_be_read_for_its_identity_is_still_added() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.nsp");
        fs::write(&path, b"not a container").unwrap();
        let store = RecordingStore::failing();

        let mut list = FileList::new();
        let outcome = list.add_path(&path, &store).await.unwrap();

        assert_eq!(list.len(), 1, "the file is still offered");
        assert!(
            list.get("game.nsp").unwrap().identity.is_empty(),
            "it carries no identity"
        );
        assert!(outcome.skipped.is_empty(), "no path is reported as skipped");
    }

    #[tokio::test]
    async fn a_file_that_is_not_a_container_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, b"hello").unwrap();
        let store = RecordingStore::new();

        let mut list = FileList::new();
        list.add_path(&path, &store).await.unwrap();

        assert_eq!(list.len(), 1);
        assert!(
            store.opened().is_empty(),
            "the file's contents are not read: {:?}",
            store.opened()
        );
    }

    #[tokio::test]
    async fn add_dir_recurses_into_nested_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        let sub = dir.path().join("sub").join("deep");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("b.nsz"), b"b").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("a.nsp").is_some());
        assert!(list.get("b.nsz").is_some());
    }

    #[tokio::test]
    async fn add_dir_skips_non_matching_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.nsp"), b"g").unwrap();
        fs::write(dir.path().join("notes.txt"), b"n").unwrap();
        fs::write(dir.path().join("cover.jpg"), b"c").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("game.nsp").is_some());
    }

    #[tokio::test]
    async fn add_dir_matches_extensions_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("GAME.NSP"), b"g").unwrap();
        fs::write(dir.path().join("Patch.NsZ"), b"p").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.get("GAME.NSP").is_some());
        assert!(list.get("Patch.NsZ").is_some());
    }

    #[tokio::test]
    async fn add_file_is_not_extension_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("readme.txt");
        fs::write(&f, b"r").unwrap();
        let mut list = FileList::new();
        list.add_path(&f, &LocalStore::new()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.get("readme.txt").is_some());
    }

    #[tokio::test]
    async fn add_empty_dir_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn insert_overwrites_same_name() {
        let mut list = FileList::new();
        list.insert("a", "/1");
        list.insert("a", "/2");
        assert_eq!(list.len(), 1);
        assert_eq!(list.get("a").unwrap().source.to_string(), "/2");
    }

    #[tokio::test]
    async fn insert_returns_the_replaced_source() {
        let mut list = FileList::new();
        assert_eq!(list.insert("a", "/1"), None);
        assert_eq!(
            list.insert("a", "/2"),
            Some(Source::Local(PathBuf::from("/1")))
        );
    }

    #[tokio::test]
    async fn add_path_reports_replaced_name() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        fs::write(one.path().join("game.nsp"), b"1").unwrap();
        fs::write(two.path().join("game.nsp"), b"2").unwrap();

        let mut list = FileList::new();
        list.add_path(one.path(), &LocalStore::new()).await.unwrap();
        let outcome = list.add_path(two.path(), &LocalStore::new()).await.unwrap();

        assert_eq!(outcome.replaced, vec!["game.nsp".to_string()]);
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn add_path_reports_added_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();

        let mut list = FileList::new();
        let outcome = list.add_path(dir.path(), &LocalStore::new()).await.unwrap();

        assert_eq!(outcome.added, vec!["a.nsp".to_string()]);
        assert!(outcome.skipped.is_empty());
    }

    #[tokio::test]
    async fn add_file_records_size() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.nsp");
        fs::write(&f, b"12345").unwrap();
        let mut list = FileList::new();
        list.add_path(&f, &LocalStore::new()).await.unwrap();
        assert_eq!(list.get("x.nsp").unwrap().size, 5);
    }

    #[tokio::test]
    async fn recorded_size_survives_the_file_being_removed() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.nsp");
        fs::write(&f, b"1234567").unwrap();
        let mut list = FileList::new();
        list.add_path(&f, &LocalStore::new()).await.unwrap();
        fs::remove_file(&f).unwrap();
        assert_eq!(list.get("x.nsp").unwrap().size, 7);
    }

    #[tokio::test]
    async fn total_size_sums_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"abc").unwrap();
        fs::write(dir.path().join("b.nsp"), b"defg").unwrap();
        let mut list = FileList::new();
        list.add_path(dir.path(), &LocalStore::new()).await.unwrap();
        assert_eq!(list.total_size(), 7);
    }

    #[tokio::test]
    async fn remove_drops_an_entry() {
        let mut list = FileList::new();
        list.insert("a", "/1");
        assert!(list.remove("a").is_some());
        assert!(list.is_empty());
        assert!(list.remove("a").is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn add_dir_skips_unreadable_subdirectory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ok.nsp"), b"o").unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("hidden.nsp"), b"h").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let mut list = FileList::new();
        let outcome = list.add_path(dir.path(), &LocalStore::new()).await.unwrap();

        // Restore permissions so the tempdir can be cleaned up.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(list.get("ok.nsp").is_some());
        assert!(list.get("hidden.nsp").is_none());
        assert!(outcome
            .skipped
            .iter()
            .any(|s| s.to_string().ends_with("locked")));
    }

    /// Tests for [`FileList::add_source`], running against [`MemStore`] and a
    /// small wrapper that can deny listings like an unreadable share directory.
    #[cfg(feature = "test-util")]
    mod add_source_tests {
        use super::*;
        use crate::source::local::LocalStore;
        use crate::source::testutil::MemStore;
        use crate::source::{EntryKind, FileHandle, ListEntry, SmbLocation};
        use async_trait::async_trait;

        fn smb(share: &str, path: &str) -> Source {
            Source::Smb(SmbLocation {
                share: share.to_string(),
                path: path.to_string(),
            })
        }

        /// Delegates everything to `MemStore` except listing the denied
        /// source, which fails the way an unreadable directory would.
        struct DenyDir {
            inner: MemStore,
            deny: Source,
        }

        #[async_trait]
        impl Sources for DenyDir {
            async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
                self.inner.open(source).await
            }

            async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
                if *source == self.deny {
                    Err(SourceError::new(format!("denied: {source}")))
                } else {
                    self.inner.list_dir(source).await
                }
            }

            async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
                self.inner.stat(source).await
            }
        }

        #[tokio::test]
        async fn add_source_scans_a_share_directory_recursively() {
            let store = MemStore::new();
            store.add_file(&smb("games", "switch/z.nsp"), b"zzz" as &[u8]);
            store.add_file(&smb("games", "switch/sub/a.nsz"), b"a" as &[u8]);

            let mut list = FileList::new();
            let outcome = list
                .add_source(&store, &smb("games", "switch"))
                .await
                .unwrap();

            assert_eq!(list.len(), 2);
            assert_eq!(
                list.get("a.nsz").unwrap().source,
                smb("games", "switch/sub/a.nsz")
            );
            assert_eq!(list.get("a.nsz").unwrap().size, 1);
            assert_eq!(
                list.get("z.nsp").unwrap().source,
                smb("games", "switch/z.nsp")
            );
            assert_eq!(list.get("z.nsp").unwrap().size, 3);
            assert!(outcome.skipped.is_empty());
        }

        #[tokio::test]
        async fn add_source_filters_scan_results_by_extension() {
            let store = MemStore::new();
            store.add_file(&smb("nas", "archive/game.nsp"), b"x" as &[u8]);
            store.add_file(&smb("nas", "archive/README.TXT"), b"x" as &[u8]);

            let mut list = FileList::new();
            let outcome = list
                .add_source(&store, &smb("nas", "archive"))
                .await
                .unwrap();

            assert_eq!(outcome.added, vec!["game.nsp".to_string()]);
            assert!(list.get("README.TXT").is_none());
        }

        #[tokio::test]
        async fn add_source_applies_the_configured_filters() {
            let store = MemStore::new();
            store.add_file(&smb("nas", "archive/a.nsp"), b"x" as &[u8]);
            store.add_file(&smb("nas", "archive/b.nsz"), b"y" as &[u8]);

            let mut list = FileList::with_filters(Filters::with_extensions(["nsz"]));
            list.add_source(&store, &smb("nas", "archive"))
                .await
                .unwrap();

            assert_eq!(list.len(), 1);
            assert!(list.get("b.nsz").is_some());
            assert!(list.get("a.nsp").is_none());
        }

        #[tokio::test]
        async fn add_source_reports_the_files_the_filters_rejected() {
            let store = MemStore::new();
            store.add_file(&smb("nas", "archive/a.nsp"), b"x" as &[u8]);
            store.add_file(&smb("nas", "archive/notes.txt"), b"y" as &[u8]);

            let mut list = FileList::new();
            let outcome = list
                .add_source(&store, &smb("nas", "archive"))
                .await
                .unwrap();

            assert_eq!(outcome.added.len(), 1);
            assert_eq!(outcome.rejected, 1);
        }

        #[tokio::test]
        async fn add_source_adds_a_directly_listed_file_without_filtering() {
            let store = MemStore::new();
            store.add_file(&smb("nas", "readme.txt"), b"hello" as &[u8]);

            let mut list = FileList::new();
            let outcome = list
                .add_source(&store, &smb("nas", "readme.txt"))
                .await
                .unwrap();

            assert_eq!(outcome.added, vec!["readme.txt".to_string()]);
            assert_eq!(list.get("readme.txt").unwrap().size, 5);
            assert_eq!(
                list.get("readme.txt").unwrap().source,
                smb("nas", "readme.txt")
            );
        }

        #[tokio::test]
        async fn add_source_reports_skipped_subdirectories() {
            let inner = MemStore::new();
            inner.add_file(&smb("games", "switch/a.nsp"), b"a" as &[u8]);
            // The file makes `locked` show up as a directory in the listing,
            // where DenyDir then denies descending into it.
            inner.add_file(&smb("games", "switch/locked/hidden.nsp"), b"h" as &[u8]);
            let denied = smb("games", "switch/locked");
            let store = DenyDir {
                inner,
                deny: denied.clone(),
            };

            let mut list = FileList::new();
            let outcome = list
                .add_source(&store, &smb("games", "switch"))
                .await
                .unwrap();

            assert!(list.get("a.nsp").is_some());
            assert_eq!(outcome.skipped, vec![denied]);
        }

        #[tokio::test]
        async fn add_source_errors_on_a_missing_root() {
            let store = MemStore::new();
            let mut list = FileList::new();

            let err = list
                .add_source(&store, &smb("games", "nowhere"))
                .await
                .err()
                .unwrap();

            assert_eq!(err.message, "no such path: games:/nowhere");
        }

        #[tokio::test]
        async fn add_source_adds_a_local_file_directly() {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("readme.nsp"), b"r").unwrap();

            let mut list = FileList::new();
            let outcome = list
                .add_source(
                    &LocalStore::new(),
                    &Source::Local(dir.path().join("readme.nsp")),
                )
                .await
                .unwrap();

            assert_eq!(outcome.added, vec!["readme.nsp".to_string()]);
            assert_eq!(list.get("readme.nsp").unwrap().size, 1);
        }

        /// Delegates to [`MemStore`] but appends the self and parent
        /// pseudo-entries to every listing the way a share server does, and
        /// optionally reports one source as an existing (empty) directory so
        /// a listing can hold nothing real.
        struct DotServer {
            inner: MemStore,
            dir_root: Option<Source>,
        }

        #[async_trait]
        impl Sources for DotServer {
            async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
                self.inner.open(source).await
            }

            async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
                let mut entries = self.inner.list_dir(source).await?;
                let mut dots = vec![
                    ListEntry {
                        name: ".".to_string(),
                        kind: EntryKind::Dir,
                        size: 0,
                    },
                    ListEntry {
                        name: "..".to_string(),
                        kind: EntryKind::Dir,
                        size: 0,
                    },
                ];
                entries.append(&mut dots);
                Ok(entries)
            }

            async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
                if self.dir_root.as_ref().is_some_and(|root| root == source) {
                    return Ok(Some(ListEntry {
                        name: String::new(),
                        kind: EntryKind::Dir,
                        size: 0,
                    }));
                }
                self.inner.stat(source).await
            }
        }

        /// Scanning a share directory must ignore the `.` and `..` entries a
        /// listing reports as directories: descending into them can only loop
        /// forever, whatever the walker does.
        #[tokio::test]
        async fn add_source_ignores_dot_pseudo_entries_in_listings() {
            let inner = MemStore::new();
            inner.add_file(&smb("games", "switch/a.nsp"), b"a" as &[u8]);
            inner.add_file(&smb("games", "switch/sub/b.nsz"), b"b" as &[u8]);
            let store = DotServer {
                inner,
                dir_root: None,
            };

            let mut list = FileList::new();
            let outcome = list
                .add_source(&store, &smb("games", "switch"))
                .await
                .unwrap();

            assert_eq!(
                outcome.added,
                vec!["a.nsp".to_string(), "b.nsz".to_string()]
            );
            assert_eq!(
                list.get("a.nsp").unwrap().source,
                smb("games", "switch/a.nsp")
            );
            assert_eq!(list.get("a.nsp").unwrap().size, 1);
            assert_eq!(
                list.get("b.nsz").unwrap().source,
                smb("games", "switch/sub/b.nsz")
            );
            assert!(outcome.skipped.is_empty());
        }

        /// A directory whose listing holds nothing but `.` and `..` adds
        /// nothing and still terminates normally.
        #[tokio::test]
        async fn add_source_of_a_dot_only_listing_adds_nothing() {
            let root = smb("games", "empty");
            let store = DotServer {
                inner: MemStore::new(),
                dir_root: Some(root.clone()),
            };

            let mut list = FileList::new();
            let outcome = list.add_source(&store, &root).await.unwrap();

            assert!(list.is_empty());
            assert!(outcome.added.is_empty());
            assert!(outcome.skipped.is_empty());
        }
    }
}
