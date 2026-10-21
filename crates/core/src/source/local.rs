//! The local filesystem as a [`Sources`] implementation.

use super::{EntryKind, FileHandle, ListEntry, Source, SourceError, Sources};
use async_trait::async_trait;
use std::path::Path;

/// The local filesystem. Stateless: every call goes through `std::fs`, so
/// local behavior today stays exactly what it was when the server read
/// files itself.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LocalStore;

impl LocalStore {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Sources for LocalStore {
    async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
        match source {
            Source::Local(path) => {
                // The message stays bare so callers like the server compose it
                // with the source's display themselves.
                let file = std::fs::File::open(path).map_err(SourceError::from)?;
                Ok(Box::new(LocalFile { file }))
            }
            source => Err(SourceError::new(format!(
                "{source} is not served by the local store"
            ))),
        }
    }

    async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
        match source {
            Source::Local(path) => list_fs(path),
            source => Err(SourceError::new(format!(
                "{source} is not served by the local store"
            ))),
        }
    }

    async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
        match source {
            Source::Local(path) => match std::fs::metadata(path) {
                Ok(meta) => Ok(Some(ListEntry {
                    name: String::new(),
                    kind: if meta.is_dir() {
                        EntryKind::Dir
                    } else {
                        EntryKind::File
                    },
                    size: meta.len(),
                })),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(SourceError::from(e)),
            },
            source => Err(SourceError::new(format!(
                "{source} is not served by the local store"
            ))),
        }
    }
}

/// List one directory, sorted by name, mirroring the scan's tolerance: an
/// entry that cannot be inspected is skipped rather than failing the listing.
///
/// Symlinks follow the file-list scan's rule: file symlinks are listed (their
/// target's size), directory symlinks and broken links are left out so a
/// walk never cycles.
fn list_fs(dir: &Path) -> Result<Vec<ListEntry>, SourceError> {
    let read_dir = std::fs::read_dir(dir).map_err(SourceError::from)?;
    let mut entries = Vec::new();
    for entry in read_dir {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let (kind, size) = if file_type.is_file() || file_type.is_dir() {
            let Ok(meta) = entry.metadata() else { continue };
            let kind = if file_type.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            // A directory has no meaningful byte size for our callers.
            let size = if kind == EntryKind::Dir {
                0
            } else {
                meta.len()
            };
            (kind, size)
        } else {
            // A symlink of some kind: list it when it resolves to a file.
            match std::fs::metadata(entry.path()) {
                Ok(meta) if meta.is_file() => (EntryKind::File, meta.len()),
                _ => continue,
            }
        };
        entries.push(ListEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            kind,
            size,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

struct LocalFile {
    file: std::fs::File,
}

#[async_trait]
impl FileHandle for LocalFile {
    async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.file.read_at(buf, offset).map_err(SourceError::from)
        }
        #[cfg(not(unix))]
        {
            use std::io::{Read, Seek, SeekFrom};
            self.file
                .seek(SeekFrom::Start(offset))
                .and_then(|_| self.file.read(buf))
                .map_err(SourceError::from)
        }
    }

    async fn size(&self) -> Option<u64> {
        self.file.metadata().ok().map(|meta| meta.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SmbLocation;
    use std::fs;

    #[tokio::test]
    async fn list_dir_distinguishes_kinds_and_sizes_sorted() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("b_dir")).unwrap();
        fs::write(dir.path().join("a.nsp"), b"1234567890").unwrap();

        let entries = LocalStore::new()
            .list_dir(&Source::Local(dir.path().into()))
            .await
            .unwrap();

        assert_eq!(entries[0].name, "a.nsp");
        assert_eq!(entries[0].kind, EntryKind::File);
        assert_eq!(entries[0].size, 10);
        assert_eq!(entries[1].name, "b_dir");
        assert_eq!(entries[1].kind, EntryKind::Dir);
        assert_eq!(entries[1].size, 0);
    }

    #[tokio::test]
    async fn list_dir_lists_file_symlinks_and_skips_dir_and_broken_ones() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("target.nsp"), b"abc").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("target.nsp"), dir.path().join("file_link"))
            .unwrap();
        std::os::unix::fs::symlink(dir.path().join("sub"), dir.path().join("dir_link")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("nowhere"), dir.path().join("broken")).unwrap();

        let entries = LocalStore::new()
            .list_dir(&Source::Local(dir.path().into()))
            .await
            .unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names, ["file_link", "sub", "target.nsp"]);
    }

    #[tokio::test]
    async fn read_at_reads_from_the_offset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.nsp");
        fs::write(&path, b"0123456789").unwrap();

        let mut handle = LocalStore::new().open(&Source::Local(path)).await.unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(handle.read_at(&mut buf, 3).await.unwrap(), 4);
        assert_eq!(&buf, b"3456");
    }

    #[tokio::test]
    async fn read_at_beyond_the_end_reports_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.nsp");
        fs::write(&path, b"0123").unwrap();

        let mut handle = LocalStore::new().open(&Source::Local(path)).await.unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(handle.read_at(&mut buf, 4).await.unwrap(), 0);
        assert_eq!(handle.read_at(&mut buf, 404).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn open_fails_on_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.nsp");

        let err = LocalStore::new()
            .open(&Source::Local(path.clone()))
            .await
            .err()
            .unwrap();
        assert_eq!(err.message, "No such file or directory (os error 2)");
    }

    #[tokio::test]
    async fn list_dir_fails_on_a_missing_directory_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");

        let err = LocalStore::new()
            .list_dir(&Source::Local(missing.clone()))
            .await
            .err()
            .unwrap();
        assert_eq!(err.message, "No such file or directory (os error 2)");
    }

    #[tokio::test]
    async fn share_sources_are_rejected_by_the_local_store() {
        let source = Source::Smb(SmbLocation {
            share: "games".into(),
            path: "switch".into(),
        });

        let err = LocalStore::new().open(&source).await.err().unwrap();
        assert!(err.message.contains("not served by the local store"));

        let err = LocalStore::new().list_dir(&source).await.err().unwrap();
        assert!(err.message.contains("not served by the local store"));
        assert_eq!(source.to_string(), "games:/switch");
    }
}
