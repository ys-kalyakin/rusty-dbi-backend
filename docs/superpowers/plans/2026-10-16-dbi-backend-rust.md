# DBI Backend (Rust) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reimplement `dbibackend.py` in Rust as a USB server that streams NSP files to a Nintendo Switch running DBI, with a CLI + terminal UI.

**Architecture:** A Cargo workspace with a `dbi-backend-core` library (protocol, transport trait, server, file list) and a `dbi-backend` binary (CLI + ratatui TUI). The server is generic over a `Transport` trait so its command loop is tested end-to-end with an in-memory mock; the real USB implementation uses `nusb`.

**Tech Stack:** Rust (stable), `nusb` 0.2 (tokio feature), `tokio`, `async-trait`, `thiserror`, `clap`, `ratatui` 0.29, `crossterm` 0.28, `tempfile` (dev).

**Spec:** `docs/superpowers/specs/2026-10-16-dbi-backend-design.md`

---

## Protocol reference (from the spec)

16-byte little-endian header: `"DBI0"` (4) + `cmd_type: u32` (4) + `cmd_id: u32` (4) + `data_size: u32` (4).

- `cmd_type`: REQUEST=0, RESPONSE=1, ACK=2
- `cmd_id`: EXIT=0, LIST_OLD=1, FILE_RANGE=2, LIST=3
- chunk size `BUFFER_SEGMENT_DATA_SIZE = 0x100000`
- USB VID `0x057E`, PID `0x3000`, IN endpoint `0x81`, OUT endpoint `0x01`, interface `0`

---

## Task 0: Install the Rust toolchain

**Files:** none (environment setup)

- [ ] **Step 1: Install rustup with the stable toolchain**

Run:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
```
Expected: installs rustc, cargo, rustfmt, clippy into `$HOME/.cargo`.

- [ ] **Step 2: Load cargo into the current shell**

Run:
```bash
source "$HOME/.cargo/env" && cargo --version && rustc --version
```
Expected: prints a `cargo 1.xx` and `rustc 1.xx` version (>= 1.75).

- [ ] **Step 3: Make cargo available for all future steps**

All later `bash` steps in this plan assume cargo is on `PATH`. If a step fails with
`cargo: command not found`, prefix it with `source "$HOME/.cargo/env" &&`.

---

## Task 1: Workspace scaffold + protocol header

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/protocol.rs`
- Create: `crates/app/Cargo.toml`
- Create: `crates/app/src/main.rs`

- [ ] **Step 1: Create the workspace root**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/app"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }
nusb = { version = "0.2", features = ["tokio"] }
async-trait = "0.1"
thiserror = "2"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"
crossterm = "0.28"
tempfile = "3"
```

- [ ] **Step 2: Add `.gitignore`**

`.gitignore`:
```
/target
```

- [ ] **Step 3: Create the core crate manifest**

`crates/core/Cargo.toml`:
```toml
[package]
name = "dbi-backend-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
name = "dbi_backend_core"
path = "src/lib.rs"

[dependencies]
async-trait.workspace = true
thiserror.workspace = true
nusb.workspace = true
tokio.workspace = true

[dev-dependencies]
tokio.workspace = true
tempfile.workspace = true
async-trait.workspace = true
```

- [ ] **Step 4: Create the app crate manifest**

`crates/app/Cargo.toml`:
```toml
[package]
name = "dbi-backend"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "dbi-backend"
path = "src/main.rs"

[dependencies]
dbi-backend-core = { path = "../core" }
anyhow.workspace = true
clap.workspace = true
ratatui.workspace = true
crossterm.workspace = true
tokio.workspace = true
```

- [ ] **Step 5: Create the module roots**

`crates/core/src/lib.rs`:
```rust
pub mod filelist;
pub mod protocol;
pub mod server;
pub mod transport;
```

`crates/app/src/main.rs` (placeholder, replaced in Task 8):
```rust
fn main() {}
```

- [ ] **Step 6: Write the failing protocol tests**

Create `crates/core/src/protocol.rs` with only the tests (implementation added in
Step 9):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let h = Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, 123);
        let bytes = h.to_bytes();
        let parsed = Header::parse(&bytes).unwrap();
        assert_eq!(parsed, h);
        assert!(parsed.is_valid());
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = Header::new(CMD_TYPE_REQUEST, CMD_ID_EXIT, 0).to_bytes();
        bytes[0] = b'X';
        let parsed = Header::parse(&bytes).unwrap();
        assert!(!parsed.is_valid());
    }

    #[test]
    fn header_parse_rejects_short_buffer() {
        assert!(Header::parse(&[0u8; 15]).is_none());
    }
}
```

- [ ] **Step 7: Create empty placeholders so the crate compiles**

Create these minimal files (real content added in later tasks):

`crates/core/src/filelist.rs`:
```rust
//! File list (implemented in Task 3).
```
`crates/core/src/server.rs`:
```rust
//! DBI command server (implemented in Tasks 5-6).
```
`crates/core/src/transport/mod.rs`:
```rust
//! Transport abstraction (implemented in Task 4).
pub mod usb;
```
`crates/core/src/transport/usb.rs`:
```rust
//! nusb transport (implemented in Task 7).
```

- [ ] **Step 8: Run the tests to verify they fail**

Run: `cargo test -p dbi-backend-core`
Expected: FAIL to compile — `cannot find type Header` / `CMD_TYPE_RESPONSE` not found.

- [ ] **Step 9: Implement the protocol header**

Replace the contents of `crates/core/src/protocol.rs` with:
```rust
//! DBI0 USB protocol framing.

/// Magic bytes at the start of every message.
pub const MAGIC: [u8; 4] = *b"DBI0";
/// Length of the fixed command header.
pub const HEADER_LEN: usize = 16;
/// Maximum bytes sent per bulk transfer when streaming a file range.
pub const BUFFER_SEGMENT_DATA_SIZE: usize = 0x100000;

pub const CMD_ID_EXIT: u32 = 0;
pub const CMD_ID_LIST_OLD: u32 = 1;
pub const CMD_ID_FILE_RANGE: u32 = 2;
pub const CMD_ID_LIST: u32 = 3;

pub const CMD_TYPE_REQUEST: u32 = 0;
pub const CMD_TYPE_RESPONSE: u32 = 1;
pub const CMD_TYPE_ACK: u32 = 2;

/// A 16-byte DBI0 command header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub cmd_type: u32,
    pub cmd_id: u32,
    pub data_size: u32,
}

impl Header {
    /// Build a header using the DBI0 magic.
    pub fn new(cmd_type: u32, cmd_id: u32, data_size: u32) -> Self {
        Self { magic: MAGIC, cmd_type, cmd_id, data_size }
    }

    /// Serialize to the 16-byte little-endian wire form.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[0..4].copy_from_slice(&self.magic);
        out[4..8].copy_from_slice(&self.cmd_type.to_le_bytes());
        out[8..12].copy_from_slice(&self.cmd_id.to_le_bytes());
        out[12..16].copy_from_slice(&self.data_size.to_le_bytes());
        out
    }

    /// Parse a header from at least 16 bytes, or `None` if the buffer is too short.
    pub fn parse(buf: &[u8]) -> Option<Header> {
        if buf.len() < HEADER_LEN {
            return None;
        }
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        let cmd_type = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        let cmd_id = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        let data_size = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        Some(Header { magic, cmd_type, cmd_id, data_size })
    }

    /// Whether this header starts with the DBI0 magic.
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let h = Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, 123);
        let bytes = h.to_bytes();
        let parsed = Header::parse(&bytes).unwrap();
        assert_eq!(parsed, h);
        assert!(parsed.is_valid());
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = Header::new(CMD_TYPE_REQUEST, CMD_ID_EXIT, 0).to_bytes();
        bytes[0] = b'X';
        let parsed = Header::parse(&bytes).unwrap();
        assert!(!parsed.is_valid());
    }

    #[test]
    fn header_parse_rejects_short_buffer() {
        assert!(Header::parse(&[0u8; 15]).is_none());
    }
}
```

- [ ] **Step 10: Run the tests**

Run: `cargo test -p dbi-backend-core`
Expected: compiles and 3 protocol tests PASS.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "chore: scaffold workspace and add DBI0 protocol header"
```

---

## Task 2: Protocol file-range header and list payload

**Files:**
- Modify: `crates/core/src/protocol.rs`

- [ ] **Step 1: Write the failing tests**

Append these tests inside the existing `#[cfg(test)] mod tests { ... }` block in
`crates/core/src/protocol.rs`:
```rust
    #[test]
    fn file_range_header_parses_fields() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&0x1000u64.to_le_bytes());
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(b"a.ns");
        let fr = FileRangeHeader::parse(&buf).unwrap();
        assert_eq!(fr.range_size, 100);
        assert_eq!(fr.range_offset, 0x1000);
        assert_eq!(fr.name_len, 4);
        assert_eq!(fr.name, "a.ns");
    }

    #[test]
    fn file_range_header_rejects_short_buffer() {
        assert!(FileRangeHeader::parse(&[0u8; 15]).is_none());
    }

    #[test]
    fn list_payload_terminates_each_name() {
        let names = vec!["a.nsp".to_string(), "b.nsp".to_string()];
        let payload = build_list(names);
        assert_eq!(payload, b"a.nsp\nb.nsp\n");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dbi-backend-core protocol`
Expected: FAIL to compile — `FileRangeHeader` and `build_list` not found.

- [ ] **Step 3: Implement the file-range header and list builder**

Add to `crates/core/src/protocol.rs` (above the `#[cfg(test)] mod tests` block):
```rust
/// The payload describing which part of which file the Switch wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRangeHeader {
    pub range_size: u32,
    pub range_offset: u64,
    pub name_len: u32,
    pub name: String,
}

impl FileRangeHeader {
    /// Parse from a buffer holding at least 16 bytes followed by the UTF-8 name.
    pub fn parse(buf: &[u8]) -> Option<FileRangeHeader> {
        if buf.len() < HEADER_LEN {
            return None;
        }
        let range_size = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let range_offset = u64::from_le_bytes(buf[4..12].try_into().unwrap());
        let name_len = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        let name = String::from_utf8_lossy(&buf[16..]).into_owned();
        Some(FileRangeHeader { range_size, range_offset, name_len, name })
    }
}

/// Build the LIST payload: each name followed by a newline, UTF-8 encoded.
pub fn build_list(names: impl IntoIterator<Item = String>) -> Vec<u8> {
    let mut out = String::new();
    for name in names {
        out.push_str(&name);
        out.push('\n');
    }
    out.into_bytes()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dbi-backend-core protocol`
Expected: all protocol tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/protocol.rs
git commit -m "feat(core): parse file-range header and build list payload"
```

---

## Task 3: File list

**Files:**
- Modify: `crates/core/src/filelist.rs`

- [ ] **Step 1: Write the failing tests**

Replace `crates/core/src/filelist.rs` with the tests plus a stub that does not compile
yet is not required — write the full tests first by putting them together with the
implementation in Step 3. Instead, to follow TDD, first create this file content:
```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dbi-backend-core filelist`
Expected: FAIL to compile — `FileList` not found.

- [ ] **Step 3: Implement the file list**

Replace the entire contents of `crates/core/src/filelist.rs` with:
```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dbi-backend-core filelist`
Expected: 3 filelist tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/filelist.rs
git commit -m "feat(core): add file list with file and directory discovery"
```

---

## Task 4: Transport trait and errors

**Files:**
- Modify: `crates/core/src/transport/mod.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/core/src/transport/mod.rs` (below the trait, in a test module):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_up_to_packet_size() {
        assert_eq!(round_up(0, 512), 512);
        assert_eq!(round_up(1, 512), 512);
        assert_eq!(round_up(512, 512), 512);
        assert_eq!(round_up(513, 512), 1024);
        assert_eq!(round_up(16, 512), 512);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p dbi-backend-core transport`
Expected: FAIL to compile — `round_up` not found.

- [ ] **Step 3: Implement the trait, errors, and helper**

Replace the contents of `crates/core/src/transport/mod.rs` with:
```rust
//! Abstraction over the USB bulk endpoints so the server can be tested without hardware.

pub mod usb;

use async_trait::async_trait;

/// Errors that can occur while reading or writing bulk data.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("usb error: {0}")]
    Usb(String),
    #[error("short read: expected {expected} bytes, got {got}")]
    ShortRead { expected: usize, got: usize },
}

/// A bidirectional bulk endpoint pair.
#[async_trait]
pub trait Transport: Send {
    /// Read exactly `len` bytes.
    async fn read(&mut self, len: usize) -> Result<Vec<u8>, TransportError>;
    /// Write all of `data`.
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;
}

/// Round `n` up to the next multiple of `m` (USB IN transfers must request a
/// multiple of the endpoint's max packet size). Never returns 0 for a nonzero `m`.
pub(crate) fn round_up(n: usize, m: usize) -> usize {
    if m == 0 {
        return n;
    }
    ((n + m - 1) / m) * m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_up_to_packet_size() {
        assert_eq!(round_up(0, 512), 512);
        assert_eq!(round_up(1, 512), 512);
        assert_eq!(round_up(512, 512), 512);
        assert_eq!(round_up(513, 512), 1024);
        assert_eq!(round_up(16, 512), 512);
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p dbi-backend-core transport`
Expected: 1 transport test PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/transport/mod.rs
git commit -m "feat(core): add transport trait, errors, and packet rounding"
```

---

## Task 5: Server command loop — LIST and EXIT

**Files:**
- Modify: `crates/core/src/server.rs`
- Create: `crates/core/tests/server_test.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/core/tests/server_test.rs`:
```rust
use async_trait::async_trait;
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::protocol::*;
use dbi_backend_core::server::Server;
use dbi_backend_core::transport::{Transport, TransportError};
use std::collections::VecDeque;

pub struct MockTransport {
    reads: VecDeque<Vec<u8>>,
    writes: Vec<Vec<u8>>,
}

impl MockTransport {
    pub fn new(reads: Vec<Vec<u8>>) -> Self {
        Self { reads: reads.into(), writes: Vec::new() }
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn read(&mut self, _len: usize) -> Result<Vec<u8>, TransportError> {
        self.reads
            .pop_front()
            .ok_or_else(|| TransportError::Usb("no more reads".into()))
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.writes.push(data.to_vec());
        Ok(())
    }
}

fn req(cmd_id: u32, data_size: u32) -> Vec<u8> {
    Header::new(CMD_TYPE_REQUEST, cmd_id, data_size).to_bytes().to_vec()
}

fn ack() -> Vec<u8> {
    Header::new(CMD_TYPE_ACK, 0, 0).to_bytes().to_vec()
}

#[tokio::test]
async fn list_command_sends_file_names_then_exits() {
    let mut files = FileList::new();
    files.insert("test.nsp", "/tmp/test.nsp");

    let reads = vec![req(CMD_ID_LIST, 0), ack(), req(CMD_ID_EXIT, 0)];
    let mut server = Server::new(MockTransport::new(reads), files);

    server.run(|_line| {}).await.unwrap();
    let transport = server.into_transport();

    assert_eq!(transport.writes[0], Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, 9).to_bytes());
    assert_eq!(transport.writes[1], b"test.nsp\n");
    assert_eq!(transport.writes[2], Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p dbi-backend-core --test server_test`
Expected: FAIL to compile — `Server` not found.

- [ ] **Step 3: Implement the server (LIST + EXIT; FILE_RANGE stub)**

Replace the contents of `crates/core/src/server.rs` with:
```rust
//! The DBI command loop.

use crate::filelist::FileList;
use crate::protocol::*;
use crate::transport::{Transport, TransportError};

/// Fatal errors from the command loop. Transport errors signal a lost connection.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("protocol error: {0}")]
    Protocol(String),
}

/// Serves DBI requests read from a [`Transport`].
pub struct Server<T: Transport> {
    transport: T,
    files: FileList,
}

impl<T: Transport> Server<T> {
    pub fn new(transport: T, files: FileList) -> Self {
        Self { transport, files }
    }

    /// Consume the server and return the transport (used by tests).
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Run the command loop until the Switch issues EXIT or the transport fails.
    pub async fn run<F: FnMut(String)>(&mut self, mut log: F) -> Result<(), ServerError> {
        log("Entering command loop".into());
        loop {
            let bytes = self.transport.read(HEADER_LEN).await?;
            let header = Header::parse(&bytes)
                .ok_or_else(|| ServerError::Protocol("short header".into()))?;
            if !header.is_valid() {
                continue;
            }
            match header.cmd_id {
                CMD_ID_EXIT => {
                    self.process_exit(&mut log).await?;
                    return Ok(());
                }
                CMD_ID_FILE_RANGE => {
                    self.process_file_range(header.data_size, &mut log).await?;
                }
                CMD_ID_LIST => {
                    self.process_list(&mut log).await?;
                }
                _ => {}
            }
        }
    }

    async fn process_exit<F: FnMut(String)>(&mut self, log: &mut F) -> Result<(), ServerError> {
        log("Exit".into());
        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes();
        self.transport.write(&resp).await?;
        Ok(())
    }

    async fn process_list<F: FnMut(String)>(&mut self, log: &mut F) -> Result<(), ServerError> {
        log("Get list".into());
        let payload = build_list(self.files.names().cloned());
        let len = payload.len() as u32;
        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_LIST, len).to_bytes();
        self.transport.write(&resp).await?;
        if len > 0 {
            let _ack = self.transport.read(HEADER_LEN).await?;
            self.transport.write(&payload).await?;
        }
        Ok(())
    }

    // Implemented in Task 6.
    async fn process_file_range<F: FnMut(String)>(
        &mut self,
        _data_size: u32,
        _log: &mut F,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p dbi-backend-core --test server_test`
Expected: `list_command_sends_file_names_then_exits` PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/server.rs crates/core/tests/server_test.rs
git commit -m "feat(core): add server command loop handling LIST and EXIT"
```

---

## Task 6: Server — FILE_RANGE streaming

**Files:**
- Modify: `crates/core/src/server.rs`
- Modify: `crates/core/tests/server_test.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/core/tests/server_test.rs`:
```rust
#[tokio::test]
async fn file_range_command_streams_requested_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.nsp");
    std::fs::write(&path, [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();

    let name = "game.nsp";
    let mut files = FileList::new();
    files.insert(name, &path);

    let mut range_header = Vec::new();
    range_header.extend_from_slice(&5u32.to_le_bytes()); // range_size
    range_header.extend_from_slice(&2u64.to_le_bytes()); // range_offset
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![
        req(CMD_ID_FILE_RANGE, data_size),
        range_header,
        ack(),
        req(CMD_ID_EXIT, 0),
    ];
    let mut server = Server::new(MockTransport::new(reads), files);

    server.run(|_line| {}).await.unwrap();
    let transport = server.into_transport();

    assert_eq!(
        transport.writes[0],
        Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, data_size).to_bytes()
    );
    assert_eq!(
        transport.writes[1],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, 5).to_bytes()
    );
    assert_eq!(transport.writes[2], vec![2u8, 3, 4, 5, 6]);
    assert_eq!(
        transport.writes[3],
        Header::new(CMD_TYPE_RESPONSE, CMD_ID_EXIT, 0).to_bytes()
    );
}

#[tokio::test]
async fn file_range_with_unknown_file_is_logged_and_skipped() {
    let name = "missing.nsp";
    let mut range_header = Vec::new();
    range_header.extend_from_slice(&5u32.to_le_bytes());
    range_header.extend_from_slice(&0u64.to_le_bytes());
    range_header.extend_from_slice(&(name.len() as u32).to_le_bytes());
    range_header.extend_from_slice(name.as_bytes());
    let data_size = range_header.len() as u32;

    let reads = vec![
        req(CMD_ID_FILE_RANGE, data_size),
        range_header,
        ack(),
        req(CMD_ID_EXIT, 0),
    ];
    let files = FileList::new();
    let mut server = Server::new(MockTransport::new(reads), files);

    let mut logs = Vec::new();
    server.run(|line| logs.push(line)).await.unwrap();
    let transport = server.into_transport();

    assert!(transport.writes.iter().any(|w| w.starts_with(b"DBI0")));
    assert!(logs.iter().any(|l| l.contains("missing.nsp")));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dbi-backend-core --test server_test`
Expected: `file_range_command_streams_requested_bytes` FAILS (the stub writes nothing), and
the unknown-file test FAILS (the stub never reads the range header, so the queued bytes get
misread and `run` returns an error).

- [ ] **Step 3: Implement FILE_RANGE**

In `crates/core/src/server.rs`, replace the `process_file_range` stub with:
```rust
    async fn process_file_range<F: FnMut(String)>(
        &mut self,
        data_size: u32,
        log: &mut F,
    ) -> Result<(), ServerError> {
        use std::io::{Read, Seek, SeekFrom};

        log("File range".into());

        let ack = Header::new(CMD_TYPE_ACK, CMD_ID_FILE_RANGE, data_size).to_bytes();
        self.transport.write(&ack).await?;

        let range_bytes = self.transport.read(data_size as usize).await?;
        let range = FileRangeHeader::parse(&range_bytes)
            .ok_or_else(|| ServerError::Protocol("short file range header".into()))?;

        log(format!(
            "Range Size: {}, Range Offset: {}, Name len: {}, Name: {}",
            range.range_size, range.range_offset, range.name_len, range.name
        ));

        let resp = Header::new(CMD_TYPE_RESPONSE, CMD_ID_FILE_RANGE, range.range_size).to_bytes();
        self.transport.write(&resp).await?;

        let _ack = self.transport.read(HEADER_LEN).await?;

        let Some(path) = self.files.get(&range.name).cloned() else {
            log(format!("Unknown file: {}", range.name));
            return Ok(());
        };

        let mut file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                log(format!("Failed to open {}: {e}", path.display()));
                return Ok(());
            }
        };
        if let Err(e) = file.seek(SeekFrom::Start(range.range_offset)) {
            log(format!("Failed to seek {}: {e}", path.display()));
            return Ok(());
        }

        let mut remaining = range.range_size as usize;
        let mut buffer = vec![0u8; BUFFER_SEGMENT_DATA_SIZE];
        while remaining > 0 {
            let to_read = remaining.min(BUFFER_SEGMENT_DATA_SIZE);
            let n = file.read(&mut buffer[..to_read]).map_err(|e| {
                ServerError::Protocol(format!("read failed: {e}"))
            })?;
            if n == 0 {
                break;
            }
            self.transport.write(&buffer[..n]).await?;
            remaining -= n;
        }
        Ok(())
    }
```
Note: an actual I/O read error aborts the connection here for simplicity; missing files,
open failures, and seek failures are logged and skipped without killing the server.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dbi-backend-core`
Expected: all core tests PASS (protocol, filelist, transport, both server tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/server.rs crates/core/tests/server_test.rs
git commit -m "feat(core): stream file ranges in the server command loop"
```

---

## Task 7: nusb USB transport

**Files:**
- Modify: `crates/core/src/transport/usb.rs`

This module cannot be unit tested without hardware; it is verified by compiling.

- [ ] **Step 1: Implement `UsbTransport`**

Replace the contents of `crates/core/src/transport/usb.rs` with:
```rust
//! Real USB transport backed by `nusb`.

use std::time::Duration;

use async_trait::async_trait;
use nusb::transfer::{Bulk, Buffer, In, Out};
use nusb::{list_devices, Interface};

use super::{round_up, Transport, TransportError};

pub const VENDOR_ID: u16 = 0x057E;
pub const PRODUCT_ID: u16 = 0x3000;
pub const INTERFACE_NUMBER: u8 = 0;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x81;

/// Bulk endpoints on the Switch DBI interface.
pub struct UsbTransport {
    ep_in: nusb::Endpoint<Bulk, In>,
    ep_out: nusb::Endpoint<Bulk, Out>,
    in_packet_size: usize,
    _interface: Interface,
}

impl UsbTransport {
    /// Wait until the Switch appears, then claim its DBI interface and endpoints.
    pub async fn connect() -> Result<Self, TransportError> {
        loop {
            let info = list_devices()
                .await
                .map_err(|e| TransportError::Usb(e.to_string()))?
                .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID);

            let Some(info) = info else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let device = info
                .open()
                .await
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let interface = device
                .claim_interface(INTERFACE_NUMBER)
                .await
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let ep_in = interface
                .endpoint::<Bulk, In>(ENDPOINT_IN)
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let ep_out = interface
                .endpoint::<Bulk, Out>(ENDPOINT_OUT)
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let in_packet_size = ep_in.max_packet_size();

            return Ok(Self { ep_in, ep_out, in_packet_size, _interface: interface });
        }
    }
}

#[async_trait]
impl Transport for UsbTransport {
    async fn read(&mut self, len: usize) -> Result<Vec<u8>, TransportError> {
        let request = round_up(len, self.in_packet_size);
        let buffer = Buffer::new(request);
        self.ep_in.submit(buffer);
        let buffer = self
            .ep_in
            .next_complete()
            .await
            .into_result()
            .map_err(|e| TransportError::Usb(format!("{e:?}")))?;
        let data = buffer.into_vec();
        if data.len() < len {
            return Err(TransportError::ShortRead { expected: len, got: data.len() });
        }
        Ok(data[..len].to_vec())
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let buffer: Buffer = data.to_vec().into();
        self.ep_out.submit(buffer);
        self.ep_out
            .next_complete()
            .await
            .into_result()
            .map_err(|e| TransportError::Usb(format!("{e:?}")))?;
        Ok(())
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p dbi-backend-core`
Expected: builds with no errors. If the `nusb` API differs (method names, `into_result`,
`max_packet_size`), consult https://docs.rs/nusb/0.2/nusb/ and adjust; keep the
`Transport` impl semantics (read returns exactly `len` bytes, write sends all bytes).

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/transport/usb.rs
git commit -m "feat(core): add nusb-backed USB transport"
```

---

## Task 8: App — CLI and startup

**Files:**
- Create: `crates/app/src/cli.rs`
- Modify: `crates/app/src/main.rs`

- [ ] **Step 1: Write the failing CLI test**

Create `crates/app/src/cli.rs`:
```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "dbi-backend", version, about = "USB backend for DBI on Nintendo Switch")]
pub struct Cli {
    /// Files or directories to offer to the Switch.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_paths() {
        let cli = Cli::try_parse_from(["dbi-backend", "/a.nsp", "/dir"]).unwrap();
        assert_eq!(
            cli.paths,
            vec![PathBuf::from("/a.nsp"), PathBuf::from("/dir")]
        );
    }

    #[test]
    fn parses_no_paths() {
        let cli = Cli::try_parse_from(["dbi-backend"]).unwrap();
        assert!(cli.paths.is_empty());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p dbi-backend cli`
Expected: FAIL — `cli` module not declared in `main.rs`.

- [ ] **Step 3: Wire the module and load paths**

Replace the contents of `crates/app/src/main.rs` with:
```rust
mod cli;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use dbi_backend_core::filelist::FileList;

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut files = FileList::new();
    for path in &cli.paths {
        if let Err(e) = files.add_path(path) {
            eprintln!("warning: failed to add {}: {e}", path.display());
        }
    }

    println!("{} file(s) available", files.len());
    Ok(())
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p dbi-backend cli`
Expected: 2 CLI tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): add CLI argument parsing and file list loading"
```

---

## Task 9: App — TUI and server task

**Files:**
- Create: `crates/app/src/app.rs`
- Modify: `crates/app/src/main.rs`

- [ ] **Step 1: Implement the application state and server task**

Create `crates/app/src/app.rs`:
```rust
//! TUI application state and the background server task.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::server::Server;
use dbi_backend_core::transport::usb::UsbTransport;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

/// Maximum number of log lines retained (mirrors the Python 1000-line cap).
pub const MAX_LOG_LINES: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Idle,
    Running,
}

/// What the caller should do after a key press.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Quit,
}

pub struct App {
    pub files: FileList,
    pub log: VecDeque<String>,
    pub status: ServerStatus,
    pub input: Option<String>,
    pub should_quit: bool,
    server_started: bool,
}

impl App {
    pub fn new(files: FileList) -> Self {
        Self {
            files,
            log: VecDeque::new(),
            status: ServerStatus::Idle,
            input: None,
            should_quit: false,
            server_started: false,
        }
    }

    pub fn push_log(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > MAX_LOG_LINES {
            self.log.pop_front();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }

        if let Some(input) = self.input.as_mut() {
            match key.code {
                KeyCode::Enter => {
                    let raw = self.input.take().unwrap();
                    let trimmed = raw.trim().to_string();
                    if !trimmed.is_empty() {
                        match self.files.add_path(Path::new(&trimmed)) {
                            Ok(()) => self.push_log(format!("Added: {trimmed}")),
                            Err(e) => self.push_log(format!("Failed to add {trimmed}: {e}")),
                        }
                    }
                }
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                Action::Quit
            }
            KeyCode::Char('a') => {
                self.input = Some(String::new());
                Action::None
            }
            KeyCode::Char('c') => {
                self.files.clear();
                self.push_log("List cleared".into());
                Action::None
            }
            KeyCode::Char('s') => {
                if !self.server_started && !self.files.is_empty() {
                    self.server_started = true;
                    self.status = ServerStatus::Running;
                    Action::Start
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }
}

/// Spawn the background server loop. Logs are forwarded to `log_tx`.
pub fn spawn_server(files: FileList, log_tx: UnboundedSender<String>) {
    tokio::spawn(async move {
        loop {
            let _ = log_tx.send("Waiting for Switch...".into());
            let transport = match UsbTransport::connect().await {
                Ok(t) => t,
                Err(e) => {
                    let _ = log_tx.send(format!("USB error: {e}"));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let _ = log_tx.send("Switch connected".into());

            let mut server = Server::new(transport, files.clone());
            let tx = log_tx.clone();
            let mut log = move |line: String| {
                let _ = tx.send(line);
            };
            match server.run(&mut log).await {
                Ok(()) => {
                    let _ = log_tx.send("Server stopped".into());
                    break;
                }
                Err(e) => {
                    let _ = log_tx.send(format!("Connection lost: {e}"));
                    continue;
                }
            }
        }
    });
}

pub fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|(name, path)| ListItem::new(format!("{}  ->  {}", name, path.display())))
        .collect();
    let files = List::new(items).block(Block::default().borders(Borders::ALL).title("Files"));
    frame.render_widget(files, chunks[0]);

    let log_lines: Vec<Line> = app.log.iter().map(|l| Line::from(l.clone())).collect();
    let log = Paragraph::new(log_lines)
        .block(Block::default().borders(Borders::ALL).title("Log"))
        .wrap(Wrap { trim: true });
    frame.render_widget(log, chunks[1]);

    let status = if let Some(input) = &app.input {
        format!("Add path: {input}")
    } else {
        let state = match app.status {
            ServerStatus::Idle => "idle",
            ServerStatus::Running => "running",
        };
        format!(
            "[a]dd  [c]lear  [s]tart  [q]uit | files: {} | status: {}",
            app.files.len(),
            state
        )
    };
    let status_widget = Paragraph::new(status).style(Style::default().fg(Color::Yellow));
    frame.render_widget(status_widget, chunks[2]);
}
```

- [ ] **Step 2: Wire the TUI into `main.rs`**

Replace the contents of `crates/app/src/main.rs` with:
```rust
mod app;
mod cli;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use crossterm::event::Event;
use dbi_backend_core::filelist::FileList;
use tokio::sync::mpsc;

use app::{Action, App};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut files = FileList::new();
    for path in &cli.paths {
        if let Err(e) = files.add_path(path) {
            eprintln!("warning: failed to add {}: {e}", path.display());
        }
    }

    let mut app = App::new(files);

    let (input_tx, mut input_rx) = mpsc::channel::<Event>(64);
    std::thread::spawn(move || input_thread(input_tx));

    let (log_tx, mut log_rx) = mpsc::unbounded_channel::<String>();

    let mut terminal = ratatui::init();
    let mut ticker = tokio::time::interval(Duration::from_millis(200));

    loop {
        terminal.draw(|frame| app::ui(frame, &app))?;

        tokio::select! {
            maybe_event = input_rx.recv() => {
                if let Some(Event::Key(key)) = maybe_event {
                    match app.handle_key(key) {
                        Action::Start => app::spawn_server(app.files.clone(), log_tx.clone()),
                        Action::Quit => break,
                        Action::None => {}
                    }
                }
            }
            Some(line) = log_rx.recv() => app.push_log(line),
            _ = ticker.tick() => {}
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}

fn input_thread(tx: mpsc::Sender<Event>) {
    loop {
        match crossterm::event::read() {
            Ok(event) => {
                if tx.blocking_send(event).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
```

- [ ] **Step 3: Build the binary**

Run: `cargo build -p dbi-backend`
Expected: builds with no errors. If `ratatui::init` / `ratatui::restore` / `frame.area()`
are unavailable in the resolved ratatui version, replace them with the manual
equivalent: `crossterm::terminal::enable_raw_mode()`, `execute!(stdout, EnterAlternateScreen)`,
`Terminal::new(CrosstermBackend::new(stdout))`, and use `frame.size()` instead of
`frame.area()`.

- [ ] **Step 4: Smoke-test interactive startup**

Run: `cargo run -p dbi-backend -- /tmp`
Expected: the TUI draws (Files panel lists `/tmp` regular files, empty Log, status line).
Press `q` to quit and confirm the terminal is restored.

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): add ratatui TUI and background server task"
```

---

## Task 10: Documentation and udev rules

**Files:**
- Create: `README.md`
- Create: `contrib/99-nintendo-switch.rules`

- [ ] **Step 1: Write the udev rule**

`contrib/99-nintendo-switch.rules`:
```
# Allow users in the "plugdev" group to access the Nintendo Switch DBI USB device.
SUBSYSTEM=="usb", ATTR{idVendor}=="057e", ATTR{idProduct}=="3000", MODE="0660", GROUP="plugdev"
```

- [ ] **Step 2: Write the README**

`README.md`:
````markdown
# dbi-backend

A Rust USB backend for [DBI](https://github.com/rashevskyv/dbi), a Nintendo Switch
homebrew app for installing titles over USB. This is a reimplementation of the
Python `dbibackend.py`.

It listens for the Switch over USB (VID `0x057E`, PID `0x3000`), answers the DBI0
protocol commands `LIST`, `FILE_RANGE`, and `EXIT`, and streams the requested
byte ranges of your NSP files.

## Build

```bash
cargo build --release
```

## Usage

```bash
# Make a directory's files available and open the TUI
dbi-backend ~/games

# Or pass specific files
dbi-backend game1.nsp game2.nsp
```

Keys: `a` add a path, `c` clear the list, `s` start the server, `q` quit.

## Linux permissions

Opening the USB device requires access to the `/dev/bus/usb/...` node. Install the
provided udev rule and add your user to the `plugdev` group:

```bash
sudo cp contrib/99-nintendo-switch.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
sudo usermod -aG plugdev "$USER"   # log out and back in
```

## Protocol

Messages use a 16-byte little-endian header: `DBI0` magic, `cmd_type`, `cmd_id`,
`data_size`. See `crates/core/src/protocol.rs`.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
````

- [ ] **Step 3: Commit**

```bash
git add README.md contrib
git commit -m "docs: add README and udev rule"
```

---

## Task 11: Final verification

**Files:** none

- [ ] **Step 1: Run the full test suite**

Run: `cargo test`
Expected: all tests PASS (protocol, filelist, transport, server integration, CLI).

- [ ] **Step 2: Lint**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings. Fix any that appear.

- [ ] **Step 3: Format check**

Run: `cargo fmt --check`
Expected: no diff. Run `cargo fmt` if there is a diff, then re-check.

- [ ] **Step 4: Release build**

Run: `cargo build --release`
Expected: succeeds; binary at `target/release/dbi-backend`.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "chore: final lint and format fixes" || echo "nothing to commit"
```

---

## Notes for the implementer

- **No hardware required** for Tasks 1-6 and 8-11; only Task 7 and the Task 9 smoke
  test touch USB, and even those only need to compile.
- **`LIST` ordering:** the Python version sends names in dict insertion order; this
  implementation uses a `BTreeMap`, so names are sorted. DBI does not depend on order.
- **Reconnect:** on a transport error the background loop logs "Connection lost" and
  calls `UsbTransport::connect()` again, matching the Python reconnect behavior.
- **`q` behavior:** exiting the TUI terminates the process, which releases the USB
  interface (the Switch sees a disconnect). This matches the Python GUI's `root.quit()`.
```