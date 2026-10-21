//! An in-memory [`Sources`] fake for tests without a filesystem or a NAS.

use super::{EntryKind, FileHandle, ListEntry, Source, SourceError, Sources};
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MemInner {
    /// Local file contents keyed by the normalized path string.
    local: BTreeMap<String, Arc<[u8]>>,
    /// Share file contents keyed by `(share, normalized path)`.
    smb: BTreeMap<(String, String), Arc<[u8]>>,
}

/// Holds whole file contents keyed by [`Source`]. Directories are implicit:
/// they derive from the file keys, so an added file also makes every parent
/// appear in listings. Set up with [`MemStore::add_file`], then use it
/// wherever a [`Sources`] is needed.
pub struct MemStore {
    inner: Mutex<MemInner>,
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(MemInner::default()),
        }
    }

    /// Register one file whose bytes back reads.
    pub fn add_file(&self, source: &Source, data: impl Into<Vec<u8>>) {
        let data: Arc<[u8]> = data.into().into();
        match source {
            Source::Local(path) => self
                .inner
                .lock()
                .unwrap()
                .local
                .insert(path_key(path), data),
            Source::Smb(loc) => self.inner.lock().unwrap().smb.insert(
                (loc.share.clone(), path_key(&PathBuf::from(&loc.path))),
                data,
            ),
        };
    }
}

/// A normalized path key: no leading or trailing `/`, `""` is a root. Inner
/// segments compare literally; tests pass plain paths.
fn path_key(path: &Path) -> String {
    path.to_string_lossy().trim_matches('/').to_string()
}

/// Files and directories directly under `parent`: a key under the parent
/// whose remainder holds no `/` is a file; the component before the first
/// `/` of a deeper key is a directory.
fn children_of(parent: &str, keys: Vec<(String, u64)>) -> Vec<ListEntry> {
    let prefix = if parent.is_empty() {
        String::new()
    } else {
        format!("{parent}/")
    };
    let mut files = Vec::new();
    let mut dirs = BTreeSet::new();
    for (key, size) in keys {
        let Some(rest) = key.strip_prefix(&prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        match rest.split_once('/') {
            None => files.push(ListEntry {
                name: rest.to_string(),
                kind: EntryKind::File,
                size,
            }),
            Some((dir, _)) => {
                dirs.insert(dir.to_string());
            }
        }
    }
    let mut entries = files;
    for dir in dirs {
        entries.push(ListEntry {
            name: dir,
            kind: EntryKind::Dir,
            size: 0,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn local_children(tree: &BTreeMap<String, Arc<[u8]>>, parent: &str) -> Vec<ListEntry> {
    children_of(
        parent,
        tree.iter()
            .map(|(key, data)| (key.clone(), data.len() as u64))
            .collect(),
    )
}

fn smb_children(
    tree: &BTreeMap<(String, String), Arc<[u8]>>,
    share: &str,
    parent: &str,
) -> Vec<ListEntry> {
    children_of(
        parent,
        tree.iter()
            .filter(|((key_share, _), _)| key_share == share)
            .map(|((_, path), data)| (path.clone(), data.len() as u64))
            .collect(),
    )
}

#[async_trait]
impl Sources for MemStore {
    async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
        let data = match source {
            Source::Local(path) => self
                .inner
                .lock()
                .unwrap()
                .local
                .get(&path_key(path))
                .cloned(),
            Source::Smb(loc) => self
                .inner
                .lock()
                .unwrap()
                .smb
                .get(&(loc.share.clone(), path_key(&PathBuf::from(&loc.path))))
                .cloned(),
        };
        data.map(|data| Box::new(MemFile { data }) as Box<dyn FileHandle>)
            .ok_or_else(|| SourceError::new(format!("no such file: {source}")))
    }

    async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
        let entries = match source {
            Source::Local(dir) => {
                let inner = self.inner.lock().unwrap();
                local_children(&inner.local, &path_key(dir))
            }
            Source::Smb(loc) => {
                let inner = self.inner.lock().unwrap();
                smb_children(&inner.smb, &loc.share, &path_key(&PathBuf::from(&loc.path)))
            }
        };
        Ok(entries)
    }

    async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
        fn stat_in(parent: &str, keys: &[(String, u64)]) -> Option<ListEntry> {
            let key_prefix = if parent.is_empty() {
                String::new()
            } else {
                format!("{parent}/")
            };
            // An exact file hit wins; otherwise any child beneath the parent
            // (a file key with the parent as a prefix) means a directory.
            for (key, size) in keys {
                if key == parent {
                    return Some(ListEntry {
                        name: String::new(),
                        kind: EntryKind::File,
                        size: *size,
                    });
                }
            }
            if keys
                .iter()
                .any(|(key, _)| key.starts_with(&key_prefix) && key.len() > key_prefix.len())
            {
                return Some(ListEntry {
                    name: String::new(),
                    kind: EntryKind::Dir,
                    size: 0,
                });
            }
            None
        }

        Ok(match source {
            Source::Local(path) => {
                let inner = self.inner.lock().unwrap();
                let keys: Vec<(String, u64)> = inner
                    .local
                    .iter()
                    .map(|(k, v)| (k.clone(), v.len() as u64))
                    .collect();
                stat_in(&path_key(path), &keys)
            }
            Source::Smb(loc) => {
                let inner = self.inner.lock().unwrap();
                let parent = path_key(&PathBuf::from(&loc.path));
                let keys: Vec<(String, u64)> = inner
                    .smb
                    .iter()
                    .filter(|((share, _), _)| share == &loc.share)
                    .map(|((_, path), v)| (path.clone(), v.len() as u64))
                    .collect();
                stat_in(&parent, &keys)
            }
        })
    }
}

struct MemFile {
    data: Arc<[u8]>,
}

#[async_trait]
impl FileHandle for MemFile {
    async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError> {
        let len = self.data.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = start.saturating_add(buf.len()).min(self.data.len());
        buf[..end - start].copy_from_slice(&self.data[start..end]);
        Ok(end - start)
    }

    async fn size(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::super::SmbLocation;
    use super::*;

    fn smb(share: &str, path: &str) -> Source {
        Source::Smb(SmbLocation {
            share: share.to_string(),
            path: path.to_string(),
        })
    }

    #[tokio::test]
    async fn lists_files_and_derived_directories_on_both_sources() {
        let store = MemStore::new();
        store.add_file(
            &Source::Local(PathBuf::from("/games/a.nsp")),
            b"aa".to_vec(),
        );
        store.add_file(
            &Source::Local(PathBuf::from("/games/sub/b.nsz")),
            b"b".to_vec(),
        );
        store.add_file(&smb("nas", "switch/x.nsp"), b"x".to_vec());

        let root = store
            .list_dir(&Source::Local(PathBuf::from("/games")))
            .await
            .unwrap();
        assert_eq!(
            root,
            vec![
                ListEntry {
                    name: "a.nsp".into(),
                    kind: EntryKind::File,
                    size: 2
                },
                ListEntry {
                    name: "sub".into(),
                    kind: EntryKind::Dir,
                    size: 0
                },
            ]
        );

        let share_root = store.list_dir(&smb("nas", "")).await.unwrap();
        assert_eq!(
            share_root,
            vec![ListEntry {
                name: "switch".into(),
                kind: EntryKind::Dir,
                size: 0
            }]
        );

        let sub = store.list_dir(&smb("nas", "switch")).await.unwrap();
        assert_eq!(
            sub,
            vec![ListEntry {
                name: "x.nsp".into(),
                kind: EntryKind::File,
                size: 1
            }]
        );
    }

    #[tokio::test]
    async fn reads_at_offsets_and_reports_eof() {
        let store = MemStore::new();
        store.add_file(&smb("nas", "big.nsp"), (0..10u8).collect::<Vec<u8>>());

        let mut handle = store.open(&smb("nas", "/big.nsp")).await.unwrap();
        let mut buf = [0u8; 3];
        assert_eq!(handle.read_at(&mut buf, 2).await.unwrap(), 3);
        assert_eq!(&buf, &[2, 3, 4]);
        assert_eq!(handle.read_at(&mut buf, 10).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn open_reports_missing_files() {
        let store = MemStore::new();
        let err = store.open(&smb("nas", "nope.nsp")).await.err().unwrap();
        assert_eq!(err.message, "no such file: nas:/nope.nsp");

        let missing = store
            .open(&Source::Local(PathBuf::from("/missing")))
            .await
            .err()
            .unwrap();
        assert_eq!(missing.message, "no such file: /missing");
    }

    #[tokio::test]
    async fn listings_are_orthogonal_per_share() {
        let store = MemStore::new();
        store.add_file(&smb("nas", "a.nsp"), b"a".to_vec());
        store.add_file(&smb("other", "b.nsp"), b"b".to_vec());

        let nas = store.list_dir(&smb("nas", "")).await.unwrap();
        let other = store.list_dir(&smb("other", "")).await.unwrap();

        assert_eq!(
            nas,
            vec![ListEntry {
                name: "a.nsp".into(),
                kind: EntryKind::File,
                size: 1
            }]
        );
        assert_eq!(
            other,
            vec![ListEntry {
                name: "b.nsp".into(),
                kind: EntryKind::File,
                size: 1
            }]
        );

        let cross = store.open(&smb("nas", "b.nsp")).await.err().unwrap();
        assert!(cross.message.contains("no such file"));
    }
}
