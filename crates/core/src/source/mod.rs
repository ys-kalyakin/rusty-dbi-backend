//! File sources the server and the picker read from: the local filesystem
//! today, SMB shares later, behind one small abstraction.

pub mod local;
pub mod smb;
#[cfg(feature = "test-util")]
pub mod testutil;

use async_trait::async_trait;
use std::path::PathBuf;

/// A location on a share, pathed with `/` separators relative to the share
/// root (`""` is the browsable root itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbLocation {
    pub share: String,
    pub path: String,
}

/// Where an entry's bytes come from: the local filesystem or a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Local(PathBuf),
    Smb(SmbLocation),
}

impl std::fmt::Display for Source {
    /// Local paths render as today; share locations render as
    /// `<share>:/<path within the share>`, the root as `<share>:/`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Local(path) => write!(f, "{}", path.display()),
            Source::Smb(loc) => write!(f, "{}:/{}", loc.share, loc.path),
        }
    }
}

impl Source {
    /// The child source named `name` inside this directory source.
    pub fn join(&self, name: &str) -> Source {
        match self {
            Source::Local(path) => Source::Local(path.join(name)),
            Source::Smb(loc) => {
                let path = if loc.path.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", loc.path, name)
                };
                Source::Smb(SmbLocation {
                    share: loc.share.clone(),
                    path,
                })
            }
        }
    }

    /// The parent source, or `None` for a root: nothing above the filesystem
    /// root, and nothing above a share's root (which acts as the root while
    /// browsing the share).
    pub fn parent(&self) -> Option<Source> {
        match self {
            Source::Local(path) => path.parent().map(|p| Source::Local(p.to_path_buf())),
            Source::Smb(loc) => {
                let trimmed = loc.path.rsplit_once('/').map(|(parent, _)| parent);
                trimmed.map(|parent| {
                    Source::Smb(SmbLocation {
                        share: loc.share.clone(),
                        path: parent.to_string(),
                    })
                })
            }
        }
    }

    /// The last path component of this source, or `None` for a root.
    pub fn file_name(&self) -> Option<String> {
        match self {
            Source::Local(path) => path.file_name().and_then(|s| s.to_str()).map(String::from),
            Source::Smb(loc) => {
                let name = match loc.path.rsplit_once('/') {
                    Some((_, name)) => name.to_string(),
                    // No separator: the path itself is one component.
                    None => loc.path.clone(),
                };
                (!name.is_empty()).then_some(name)
            }
        }
    }
}

/// What a listed entry is. Files carry their size in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
}

/// One entry of a directory listing, at any source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
}

/// Errors from opening, reading, or listing. The message is what the app
/// logs; the SMB implementation guarantees it never carries a password.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct SourceError {
    pub message: String,
}

impl SourceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for SourceError {
    fn from(e: std::io::Error) -> Self {
        Self::new(e.to_string())
    }
}

/// A handle to one open file, for sequential chunked reads.
#[async_trait]
pub trait FileHandle: Send {
    /// Read up to `buf.len()` bytes at `offset`; zero means the file is over.
    async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError>;

    /// The file's byte length as it is now, used to scale progress bars; the
    /// caller falls back to the recorded size when this is `None`.
    async fn size(&self) -> Option<u64>;
}

/// All data access over every source: open one file for reads, or list a
/// directory. The server takes this (like its transport) so tests run
/// without a filesystem or a NAS.
#[async_trait]
pub trait Sources: Send + Sync {
    async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError>;
    async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError>;
    /// What `source` is: `Ok(None)` when it does not exist. A directory's
    /// size is not meaningful and reads as zero.
    async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError>;
}

/// Local files plus the share hub, selectable by the source itself: this is
/// the store the app's server task serves files through.
pub struct CombinedStore {
    shares: std::sync::Arc<tokio::sync::Mutex<smb::ShareHub>>,
}

impl CombinedStore {
    pub fn new(shares: std::sync::Arc<tokio::sync::Mutex<smb::ShareHub>>) -> Self {
        Self { shares }
    }
}

#[async_trait]
impl Sources for CombinedStore {
    async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
        match source {
            Source::Local(_) => local::LocalStore::new().open(source).await,
            Source::Smb(_) => self.shares.open(source).await,
        }
    }

    async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
        match source {
            Source::Local(_) => local::LocalStore::new().list_dir(source).await,
            Source::Smb(_) => self.shares.list_dir(source).await,
        }
    }

    async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
        match source {
            Source::Local(_) => local::LocalStore::new().stat(source).await,
            Source::Smb(_) => self.shares.stat(source).await,
        }
    }
}
