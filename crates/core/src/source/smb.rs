//! The SMB source: one hub over every configured share.
//!
//! [`ShareHub`] keeps a session per share. Connection happens only through
//! [`ShareHub::connect`] with a password handed in at that moment; the
//! password is stored inside the hub alone and wiped when the hub drops. A
//! dead session gets one bounded reconnect-and-retry on the next open, list,
//! or stat — everything else fails transparently. Errors are scrubbed so they
//! never carry the password.

use crate::source::{EntryKind, FileHandle, ListEntry, SmbLocation, Source, SourceError, Sources};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// One share operation's future, boxed so that it can borrow the session it
/// runs against.
type SessionCall<'a, T> = Pin<Box<dyn Future<Output = Result<T, SourceError>> + Send + 'a>>;

/// One configured share, password-free by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub share: String,
    pub username: String,
    pub domain: Option<String>,
    /// The browsable root inside the share; empty means the share root.
    pub root: String,
}

impl ShareConfig {
    /// SMB server endpoint: `host:port`.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// The transient credential bundle a factory logs in with.
#[derive(Debug, PartialEq, Eq)]
pub struct ShareSetup {
    pub endpoint: String,
    pub share: String,
    pub username: String,
    pub password: String,
    pub domain: Option<String>,
}

impl ShareSetup {
    pub fn for_config(config: &ShareConfig, password: &str) -> Self {
        Self {
            endpoint: config.endpoint(),
            share: config.share.clone(),
            username: config.username.clone(),
            password: password.to_string(),
            domain: config.domain.clone(),
        }
    }
}

/// Builds one share's SMB session from credentials. The real network stays in
/// the implementation; tests plug in mocks.
#[async_trait]
pub trait SessionFactory: Send + Sync {
    async fn create(&self, setup: &ShareSetup) -> Result<Box<dyn ShareSessionLike>, SourceError>;
}

/// One share's live session: ranged reads and listings.
#[async_trait]
pub trait ShareSessionLike: Send {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<ListEntry>, SourceError>;
    async fn open(&mut self, path: &str) -> Result<Box<dyn FileHandle>, SourceError>;
    async fn stat(&mut self, path: &str) -> Result<Option<ListEntry>, SourceError>;
    /// True when the server connection is gone (reboot, network loss).
    fn is_dead(&self) -> bool;
}

/// Returns an error whose message never repeats the password, even when the
/// underlying error text did.
fn sanitized(password: &str, error: impl Display) -> SourceError {
    let message = error.to_string();
    if !password.is_empty() && message.contains(password) {
        SourceError::new("server error withheld: its message contained the password")
    } else {
        SourceError::new(message)
    }
}

fn not_connected(entry: &ShareEntry) -> SourceError {
    SourceError::new(format!("share {} is not connected", entry.config.name))
}

struct ShareEntry {
    config: ShareConfig,
    session: Option<Box<dyn ShareSessionLike>>,
    password: Option<String>,
}

/// All configured shares and their sessions.
pub struct ShareHub {
    factory: Arc<dyn SessionFactory>,
    shares: HashMap<String, ShareEntry>,
}

impl ShareHub {
    pub fn new(factory: Arc<dyn SessionFactory>, configs: Vec<ShareConfig>) -> Self {
        let shares = configs
            .into_iter()
            .map(|config| {
                let name = config.name.clone();
                (
                    name,
                    ShareEntry {
                        config,
                        session: None,
                        password: None,
                    },
                )
            })
            .collect();
        Self { factory, shares }
    }

    /// The configured shares, sorted by name, for the share list UI.
    pub fn configs(&self) -> Vec<ShareConfig> {
        let mut configs: Vec<_> = self
            .shares
            .values()
            .map(|entry| entry.config.clone())
            .collect();
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        configs
    }

    /// Register another configured share after construction: an unconnected
    /// entry with no password. A same-named entry is replaced, which matters
    /// only defensively — the share-add form refuses duplicate names.
    pub fn add_config(&mut self, config: ShareConfig) {
        self.shares.insert(
            config.name.clone(),
            ShareEntry {
                config,
                session: None,
                password: None,
            },
        );
    }

    /// Log in fresh and enter the share, replacing any previous session. On
    /// failure the share stays disconnected. The password lives here only.
    pub async fn connect(&mut self, name: &str, password: &str) -> Result<(), SourceError> {
        let entry = entry_for(&mut self.shares, name)?;
        let setup = ShareSetup::for_config(&entry.config, password);
        let session = self
            .factory
            .create(&setup)
            .await
            .map_err(|e| sanitized(password, e))?;
        entry.session = Some(session);
        entry.password = Some(password.to_string());
        Ok(())
    }

    /// Whether the named share currently has a live session.
    #[cfg(test)]
    pub(crate) fn is_connected(&self, name: &str) -> bool {
        self.shares
            .get(name)
            .map(|entry| {
                entry
                    .session
                    .as_ref()
                    .is_some_and(|session| !session.is_dead())
            })
            .unwrap_or(false)
    }

    /// List a source's directory, reconnecting a dead session once.
    ///
    /// The self and parent pseudo-entries the protocol carries in every
    /// listing are dropped here too: no list this source exposes may contain
    /// them, whatever session implementation is backing the share.
    pub async fn list_dir(&mut self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
        self.with_reconnect(source, |session, path| {
            Box::pin(async move { session.list_dir(path).await })
        })
        .await
        .map(drop_dot_entries)
    }

    /// Open a source's file, reconnecting a dead session once.
    pub async fn open(&mut self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
        self.with_reconnect(source, |session, path| {
            Box::pin(async move { session.open(path).await })
        })
        .await
    }

    /// Stat a source, reconnecting a dead session once.
    pub async fn stat(&mut self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
        self.with_reconnect(source, |session, path| {
            Box::pin(async move { session.stat(path).await })
        })
        .await
    }

    /// Run one operation against the share's session, rebuilding a dead
    /// session once and running the operation again.
    ///
    /// The operation is a closure so that every call site shares this
    /// sequence — resolve the path, find the share, make sure the session is
    /// alive, run, and on a dead session rebuild and run once more — without
    /// repeating it. The future is boxed so it can borrow the session it runs
    /// against.
    async fn with_reconnect<T, F>(&mut self, source: &Source, call: F) -> Result<T, SourceError>
    where
        T: Send,
        F: for<'x> Fn(&'x mut Box<dyn ShareSessionLike>, &'x str) -> SessionCall<'x, T>,
    {
        let location = share_location(source)?;
        let path = location.path.clone();
        let entry = entry_for(&mut self.shares, &location.share)?;
        ensure_live(entry, self.factory.as_ref()).await?;
        let session = live_session(entry)?;
        match call(session, &path).await {
            Ok(value) => Ok(value),
            Err(first) => {
                if entry.session.as_ref().is_some_and(|s| s.is_dead()) {
                    rebuild(entry, self.factory.as_ref()).await?;
                    let session = live_session(entry)?;
                    call(session, &path).await
                } else {
                    Err(first)
                }
            }
        }
    }
}

impl Drop for ShareHub {
    fn drop(&mut self) {
        // The password must not outlive the hub in readable form.
        for entry in self.shares.values_mut() {
            if let Some(password) = &mut entry.password {
                wipe_string(password);
            }
            entry.password = None;
            entry.session = None;
        }
    }
}

/// Zero a string's bytes: the password leaves memory unset, not merely dropped.
fn wipe_string(s: &mut String) {
    wipe_bytes(unsafe { s.as_bytes_mut() });
    s.clear();
}

fn wipe_bytes(bytes: &mut [u8]) {
    unsafe { std::ptr::write_bytes(bytes.as_mut_ptr(), 0, bytes.len()) };
}

/// The entry named exactly, for `connect`.
fn entry_for<'a>(
    shares: &'a mut HashMap<String, ShareEntry>,
    name: &str,
) -> Result<&'a mut ShareEntry, SourceError> {
    shares
        .get_mut(name)
        .ok_or_else(|| SourceError::new(format!("share {name} is not configured")))
}

/// A share source's location within its share.
fn share_location(source: &Source) -> Result<&SmbLocation, SourceError> {
    match source {
        Source::Smb(location) => Ok(location),
        _ => Err(SourceError::new(format!("{source} is not a share"))),
    }
}

/// Reconnect when the session is known to be dead; an unconnected share is the
/// caller's message to make.
async fn ensure_live(
    entry: &mut ShareEntry,
    factory: &dyn SessionFactory,
) -> Result<(), SourceError> {
    let dead = entry
        .session
        .as_ref()
        .is_some_and(|session| session.is_dead());
    if dead {
        rebuild(entry, factory).await
    } else {
        Ok(())
    }
}

/// Grab the existing session or complain the share was never connected.
fn live_session(entry: &mut ShareEntry) -> Result<&mut Box<dyn ShareSessionLike>, SourceError> {
    if entry.session.is_none() {
        return Err(not_connected(entry));
    }
    Ok(entry.session.as_mut().unwrap())
}

/// Rebuild a share's session from the stored password. Keeps the old (dead)
/// session when the rebuild fails.
async fn rebuild(entry: &mut ShareEntry, factory: &dyn SessionFactory) -> Result<(), SourceError> {
    let password = entry.password.clone().unwrap_or_default();
    let setup = ShareSetup::for_config(&entry.config, &password);
    match factory.create(&setup).await {
        Ok(session) => {
            entry.session = Some(session);
            Ok(())
        }
        Err(e) => Err(sanitized(&password, e)),
    }
}

/// The protocol's self and parent pseudo-entries carry no content: they must
/// not appear in any share listing this source hands out, whatever the session
/// implementation maps through.
fn drop_dot_entries(mut entries: Vec<ListEntry>) -> Vec<ListEntry> {
    entries.retain(|entry| entry.name != "." && entry.name != "..");
    entries
}

/// [`Sources`] is taken by the hub through a tokio mutex: one lock guards the
/// sessions and the stored password for the whole app (the TUI and the server
/// task serialize their share work and never run concurrently).
#[async_trait]
impl Sources for tokio::sync::Mutex<ShareHub> {
    async fn open(&self, source: &Source) -> Result<Box<dyn FileHandle>, SourceError> {
        let mut hub = self.lock().await;
        hub.open(source).await
    }

    async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError> {
        let mut hub = self.lock().await;
        hub.list_dir(source).await
    }

    async fn stat(&self, source: &Source) -> Result<Option<ListEntry>, SourceError> {
        let mut hub = self.lock().await;
        hub.stat(source).await
    }
}

/// The real `smb2` factory: NTLM login, then tree connect.
pub struct Smb2SessionFactory;

#[async_trait]
impl SessionFactory for Smb2SessionFactory {
    async fn create(&self, setup: &ShareSetup) -> Result<Box<dyn ShareSessionLike>, SourceError> {
        let mut client = smb2::connect(&setup.endpoint, &setup.username, &setup.password)
            .await
            .map_err(|e| sanitized(&setup.password, e))?;
        let tree = client
            .connect_share(&setup.share)
            .await
            .map_err(|e| sanitized(&setup.password, e))?;
        Ok(Box::new(Smb2Session { client, tree }))
    }
}

/// One real share session: listings, stats, and ranged reads.
struct Smb2Session {
    client: smb2::SmbClient,
    tree: smb2::Tree,
}

#[async_trait]
impl ShareSessionLike for Smb2Session {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<ListEntry>, SourceError> {
        let entries = self
            .client
            .list_directory(&mut self.tree, path)
            .await
            .map_err(|e| SourceError::new(e.to_string()))?;
        let mut out: Vec<ListEntry> = entries
            .iter()
            // Reparse points stay out of the walk so it cannot cycle. The
            // protocol's self and parent pseudo-entries are not content.
            .filter(|entry| !entry.is_link() && !matches!(entry.name.as_str(), "." | ".."))
            .map(|entry| ListEntry {
                name: entry.name.clone(),
                kind: if entry.is_directory {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                size: entry.size,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn open(&mut self, path: &str) -> Result<Box<dyn FileHandle>, SourceError> {
        let reader = self
            .client
            .open_file_reader(&self.tree, path)
            .await
            .map_err(|e| SourceError::new(e.to_string()))?;
        Ok(Box::new(SmbFileReader { reader }))
    }

    async fn stat(&mut self, path: &str) -> Result<Option<ListEntry>, SourceError> {
        match self.client.stat(&mut self.tree, path).await {
            Ok(info) => Ok(Some(ListEntry {
                name: String::new(),
                kind: if info.is_directory {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                size: info.size,
            })),
            Err(e) if e.kind() == smb2::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SourceError::new(e.to_string())),
        }
    }

    fn is_dead(&self) -> bool {
        self.client.is_disconnected()
    }
}

/// Ranged reads over one SMB file handle.
struct SmbFileReader {
    reader: smb2::FileReader,
}

#[async_trait]
impl FileHandle for SmbFileReader {
    async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError> {
        let data = self
            .reader
            .read_at(offset, buf.len() as u64)
            .await
            .map_err(|e| SourceError::new(e.to_string()))?;
        let n = data.len();
        buf[..n].copy_from_slice(&data);
        Ok(n)
    }

    async fn size(&self) -> Option<u64> {
        Some(self.reader.size())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SmbLocation;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// The record one `create` observed, for asserting credential flow.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Seen {
        endpoint: String,
        share: String,
        username: String,
        password: String,
        domain: Option<String>,
    }

    /// Hands out sessions backed by pre-built asset states in queue order;
    /// when the queue is empty a fresh healthy session is returned. Can be
    /// told to fail wholesale with a message embedding the password so
    /// sanitization has something to catch.
    struct ScriptedFactory {
        queue: Mutex<VecDeque<Arc<Mutex<MockAssets>>>>,
        fail: Mutex<Option<String>>,
        seen: Mutex<Vec<Seen>>,
    }

    impl ScriptedFactory {
        fn new(queue: Vec<Arc<Mutex<MockAssets>>>) -> Self {
            Self {
                queue: Mutex::new(queue.into()),
                fail: Mutex::new(None),
                seen: Mutex::new(Vec::new()),
            }
        }

        fn set_fail(&self, message: Option<String>) {
            *self.fail.lock().unwrap() = message;
        }

        fn seen(&self) -> Vec<Seen> {
            self.seen.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SessionFactory for ScriptedFactory {
        async fn create(
            &self,
            setup: &ShareSetup,
        ) -> Result<Box<dyn ShareSessionLike>, SourceError> {
            self.seen.lock().unwrap().push(Seen {
                endpoint: setup.endpoint.clone(),
                share: setup.share.clone(),
                username: setup.username.clone(),
                password: setup.password.clone(),
                domain: setup.domain.clone(),
            });
            if let Some(message) = &*self.fail.lock().unwrap() {
                return Err(SourceError::new(message.clone()));
            }
            let assets = self.queue.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::new(MockSession { assets }))
        }
    }

    #[derive(Default)]
    struct MockAssets {
        entries: Vec<ListEntry>,
        bytes: Vec<u8>,
        dead: bool,
        /// Fail (and kill) the very next open, imitating a server drop.
        fail_next_open: bool,
    }

    impl MockAssets {
        fn file(name: &str, len: usize) -> ListEntry {
            ListEntry {
                name: name.to_string(),
                kind: EntryKind::File,
                size: len as u64,
            }
        }

        /// The protocol's self/parent entry, exactly as the real `smb2`
        /// crate maps it: always present, always a directory, zero size.
        fn dot(name: &str) -> ListEntry {
            ListEntry {
                name: name.to_string(),
                kind: EntryKind::Dir,
                size: 0,
            }
        }
    }

    struct MockSession {
        assets: Arc<Mutex<MockAssets>>,
    }

    fn ten_bytes() -> Vec<u8> {
        (0..10u8).collect()
    }

    #[async_trait]
    impl ShareSessionLike for MockSession {
        async fn list_dir(&mut self, _path: &str) -> Result<Vec<ListEntry>, SourceError> {
            let assets = self.assets.lock().unwrap();
            if assets.dead {
                return Err(SourceError::new("session is dead"));
            }
            // The real `smb2` crate maps listings verbatim: every response
            // carries the `.`, `..` entries, as directories.
            let mut entries = vec![MockAssets::dot("."), MockAssets::dot("..")];
            entries.extend(assets.entries.clone());
            Ok(entries)
        }

        async fn open(&mut self, _path: &str) -> Result<Box<dyn FileHandle>, SourceError> {
            let mut assets = self.assets.lock().unwrap();
            if assets.dead {
                return Err(SourceError::new("session is dead"));
            }
            if assets.fail_next_open {
                assets.fail_next_open = false;
                assets.dead = true;
                return Err(SourceError::new("connection lost mid-request"));
            }
            Ok(Box::new(MockReader {
                assets: self.assets.clone(),
            }))
        }

        async fn stat(&mut self, path: &str) -> Result<Option<ListEntry>, SourceError> {
            let assets = self.assets.lock().unwrap();
            if assets.dead {
                return Err(SourceError::new("session is dead"));
            }
            if path == "missing.nsp" {
                Ok(None)
            } else {
                Ok(Some(ListEntry {
                    name: String::new(),
                    kind: EntryKind::File,
                    size: assets.bytes.len() as u64,
                }))
            }
        }

        fn is_dead(&self) -> bool {
            self.assets.lock().unwrap().dead
        }
    }

    /// Ranged reads over the session's fake bytes.
    struct MockReader {
        assets: Arc<Mutex<MockAssets>>,
    }

    #[async_trait]
    impl FileHandle for MockReader {
        async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError> {
            let assets = self.assets.lock().unwrap();
            if assets.dead {
                return Err(SourceError::new("session is dead"));
            }
            let start = offset as usize;
            if start >= assets.bytes.len() {
                return Ok(0);
            }
            let end = (start + buf.len()).min(assets.bytes.len());
            buf[..end - start].copy_from_slice(&assets.bytes[start..end]);
            Ok(end - start)
        }

        async fn size(&self) -> Option<u64> {
            Some(self.assets.lock().unwrap().bytes.len() as u64)
        }
    }

    fn config(name: &str) -> ShareConfig {
        ShareConfig {
            name: name.to_string(),
            host: "192.0.2.1".to_string(),
            port: 445,
            share: "games".to_string(),
            username: "user".to_string(),
            domain: Some("WG".to_string()),
            root: "library/switch".to_string(),
        }
    }

    fn smb_source(path: &str) -> Source {
        Source::Smb(SmbLocation {
            share: "nas".to_string(),
            path: path.to_string(),
        })
    }

    fn assets_with(
        bytes: Vec<u8>,
        entries: Vec<ListEntry>,
        fail_next_open: bool,
    ) -> Arc<Mutex<MockAssets>> {
        Arc::new(Mutex::new(MockAssets {
            entries,
            bytes,
            dead: false,
            fail_next_open,
        }))
    }

    #[tokio::test]
    async fn connect_builds_a_session_and_listings_flow_through() {
        let assets = assets_with(ten_bytes(), vec![MockAssets::file("game.nsp", 10)], false);
        let factory = Arc::new(ScriptedFactory::new(vec![assets]));
        let mut hub = ShareHub::new(factory.clone(), vec![config("nas")]);

        hub.connect("nas", "secret-password").await.unwrap();

        assert!(hub.is_connected("nas"));
        let seen = factory.seen();
        assert_eq!(
            seen,
            vec![Seen {
                endpoint: "192.0.2.1:445".to_string(),
                share: "games".to_string(),
                username: "user".to_string(),
                password: "secret-password".to_string(),
                domain: Some("WG".to_string()),
            }]
        );

        let entries = hub.list_dir(&smb_source("switch")).await.unwrap();
        assert_eq!(entries, vec![MockAssets::file("game.nsp", 10)]);
    }

    /// The protocol's self and parent entries never reach a hub listing:
    /// the picker and the file-list scanner must not see them.
    #[tokio::test]
    async fn a_listing_never_contains_the_dot_pseudo_entries() {
        let assets = assets_with(
            ten_bytes(),
            vec![
                MockAssets::file("game.nsp", 10),
                MockAssets::file("readme.txt", 5),
            ],
            false,
        );
        let factory = Arc::new(ScriptedFactory::new(vec![assets]));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();

        let entries = hub.list_dir(&smb_source("switch")).await.unwrap();
        assert!(
            !entries
                .iter()
                .any(|entry| entry.name == "." || entry.name == ".."),
            "the pseudo-entries are not listed: {entries:?}"
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["game.nsp", "readme.txt"]
        );
    }

    #[tokio::test]
    async fn opens_a_file_and_reads_at_an_offset() {
        let factory = Arc::new(ScriptedFactory::new(vec![assets_with(
            ten_bytes(),
            vec![],
            false,
        )]));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();

        let mut handle = hub.open(&smb_source("switch/game.nsp")).await.unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(handle.read_at(&mut buf, 2).await.unwrap(), 4);
        assert_eq!(&buf, &[2, 3, 4, 5]);
        assert_eq!(handle.size().await, Some(10));
    }

    #[tokio::test]
    async fn a_session_dead_between_reads_reconnects_once_with_the_stored_password() {
        let first = assets_with(ten_bytes(), vec![MockAssets::file("game.nsp", 10)], false);
        let second = assets_with(ten_bytes(), Vec::new(), false);
        let factory = Arc::new(ScriptedFactory::new(vec![first.clone(), second]));
        let mut hub = ShareHub::new(factory.clone(), vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();

        // The NAS reboots between adding files and the first install read.
        first.lock().unwrap().dead = true;
        assert!(!hub.is_connected("nas"));

        let mut handle = hub.open(&smb_source("switch/game.nsp")).await.unwrap();
        let mut buf = [0u8; 2];
        assert_eq!(handle.read_at(&mut buf, 0).await.unwrap(), 2);
        assert_eq!(&buf, &[0, 1]);

        let seen = factory.seen();
        assert_eq!(seen.len(), 2, "the retry logged in again from the hub");
        assert_eq!(seen[1].password, "secret-password");
        assert!(hub.is_connected("nas"));
    }

    #[tokio::test]
    async fn a_listing_through_a_dead_session_reconnects_once() {
        let first = assets_with(ten_bytes(), vec![MockAssets::file("a.nsp", 3)], false);
        let second = assets_with(ten_bytes(), vec![MockAssets::file("a.nsp", 3)], false);
        let factory = Arc::new(ScriptedFactory::new(vec![first.clone(), second]));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();
        first.lock().unwrap().dead = true;

        let entries = hub.list_dir(&smb_source("switch")).await.unwrap();
        assert_eq!(entries, vec![MockAssets::file("a.nsp", 3)]);
    }

    #[tokio::test]
    async fn a_retry_that_still_fails_reports_the_error_and_stays_disconnected() {
        let first = assets_with(ten_bytes(), Vec::new(), true);
        let second = assets_with(ten_bytes(), Vec::new(), true);
        let factory = Arc::new(ScriptedFactory::new(vec![first, second]));
        let mut hub = ShareHub::new(factory.clone(), vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();

        let err = hub
            .open(&smb_source("switch/game.nsp"))
            .await
            .err()
            .unwrap();
        assert_eq!(err.message, "connection lost mid-request");
        assert_eq!(
            factory.seen().len(),
            2,
            "one connect plus exactly one reconnect"
        );
        assert!(!hub.is_connected("nas"));
    }

    #[tokio::test]
    async fn a_failed_login_never_carries_the_password() {
        let factory = Arc::new(ScriptedFactory::new(Vec::new()));
        let mut hub = ShareHub::new(factory.clone(), vec![config("nas")]);
        factory.set_fail(Some(
            "login failed: check 'secret-password' and try again".to_string(),
        ));

        let err = hub.connect("nas", "secret-password").await.err().unwrap();
        assert!(
            !err.message.contains("secret-password"),
            "the password must not leak: {err}"
        );
        assert!(!hub.is_connected("nas"));
    }

    #[tokio::test]
    async fn stat_knows_files_and_reports_missing_entries() {
        let factory = Arc::new(ScriptedFactory::new(vec![assets_with(
            ten_bytes(),
            Vec::new(),
            false,
        )]));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();

        let hit = hub
            .stat(&smb_source("switch/game.nsp"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hit.kind, EntryKind::File);
        assert_eq!(hit.size, 10);

        let miss = hub.stat(&smb_source("missing.nsp")).await.unwrap();
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn sources_the_hub_cannot_serve_report_clean_names() {
        let factory = Arc::new(ScriptedFactory::new(Vec::new()));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);

        let err = hub.connect("other", "pw").await.err().unwrap();
        assert_eq!(err.message, "share other is not configured");

        let err = hub.list_dir(&smb_source("switch")).await.err().unwrap();
        assert_eq!(err.message, "share nas is not connected");

        let local = Source::Local(std::path::PathBuf::from("/library"));
        let err = hub.list_dir(&local).await.err().unwrap();
        assert!(err.message.contains("is not a share"), "{err}");
        let err = hub.open(&local).await.err().unwrap();
        assert!(err.message.contains("is not a share"), "{err}");
    }

    #[tokio::test]
    async fn a_dropped_hub_leaves_the_new_run_asking_again() {
        let factory = Arc::new(ScriptedFactory::new(vec![assets_with(
            ten_bytes(),
            Vec::new(),
            false,
        )]));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();
        assert!(hub.is_connected("nas"));
        drop(hub);

        // The app rebuilds the hub on the next run: still configured, but no
        // session and no password — connecting asks again.
        let fresh = ShareHub::new(
            Arc::new(ScriptedFactory::new(Vec::new())),
            vec![config("nas")],
        );
        assert_eq!(fresh.configs(), vec![config("nas")]);
        assert!(!fresh.is_connected("nas"));
    }

    /// The hub learns a share after the run started: it lists among the
    /// configured ones and connects like any startup share.
    #[tokio::test]
    async fn add_config_registers_a_share_that_connects_like_a_startup_one() {
        let assets = assets_with(ten_bytes(), vec![MockAssets::file("game.nsp", 10)], false);
        let factory = Arc::new(ScriptedFactory::new(vec![assets]));
        let mut hub = ShareHub::new(factory.clone(), Vec::new());
        assert!(hub.configs().is_empty(), "nothing configured at first");

        hub.add_config(config("late"));

        assert_eq!(hub.configs(), vec![config("late")]);
        assert!(!hub.is_connected("late"), "the added share is unconnected");
        hub.connect("late", "secret-password").await.unwrap();
        assert!(hub.is_connected("late"));
        let entries = hub
            .list_dir(&Source::Smb(SmbLocation {
                share: "late".into(),
                path: "switch".into(),
            }))
            .await
            .unwrap();
        assert_eq!(entries, vec![MockAssets::file("game.nsp", 10)]);
        assert_eq!(
            factory.seen(),
            vec![Seen {
                endpoint: "192.0.2.1:445".to_string(),
                share: "games".to_string(),
                username: "user".to_string(),
                password: "secret-password".to_string(),
                domain: Some("WG".to_string()),
            }]
        );
    }

    #[test]
    fn add_config_replaces_a_same_named_entry() {
        let mut hub = ShareHub::new(Arc::new(ScriptedFactory::new(Vec::new())), Vec::new());
        hub.add_config(config("nas"));
        hub.add_config(config("nas"));

        assert_eq!(hub.configs().len(), 1, "the name is not duplicated");
    }

    #[test]
    fn wiping_zeroes_the_password_bytes() {
        // Over a leaked buffer so the memory itself is observable, not just
        // the string's logical length.
        let mut boxed: Box<[u8]> = vec![b's'; 8].into_boxed_slice();
        let raw = boxed.as_mut_ptr();
        wipe_bytes(&mut boxed);
        let view = unsafe { std::slice::from_raw_parts(raw, 8) };
        assert!(view.iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn the_hub_serves_sources_through_a_lock() {
        let factory = Arc::new(ScriptedFactory::new(vec![assets_with(
            ten_bytes(),
            vec![],
            false,
        )]));
        let mut hub = ShareHub::new(factory, vec![config("nas")]);
        hub.connect("nas", "secret-password").await.unwrap();

        let store = tokio::sync::Mutex::new(hub);
        let mut handle = Sources::open(&store, &smb_source("switch/game.nsp"))
            .await
            .unwrap();
        let mut buf = [0u8; 2];
        assert_eq!(handle.read_at(&mut buf, 8).await.unwrap(), 2);
        assert_eq!(&buf, &[8, 9]);
    }
}
