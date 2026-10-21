# Design

## Context

dbi-backend is a two-crate Rust workspace: `crates/core` (DBI0 protocol, server command
loop, file list, USB transport) and `crates/app` (ratatui TUI, CLI). Today the whole data
path is local-filesystem-bound: `FileList` stores `PathBuf` + size, the server opens files
with `std::fs::File` inline in `FILE_RANGE`, and the picker lists directories with
`std::fs`. The server and the TUI already communicate through channels, and the server is
async (tokio). See proposal.md for motivation; the requirement set lives in the spec
deltas (`smb`, `file-list`, `server`, `cli-tui`).

## Goals / Non-Goals

**Goals:**

- Stream install data straight from SMB2/3 shares, with local files still working exactly
  as today through one shared abstraction.
- Every SMB operation testable without a NAS (mockable source layer).
- Password never persisted, never logged; asked per connection.
- Thin isolation around the third-party SMB crate so it stays swappable.

**Non-Goals:**

- Retrying a range read after a mid-stream failure (future change).
- Editing the share config from the TUI; `--share` CLI flag; OS-level mounting.
- Writing to shares, SMB1, or server-side copy / other client features we don't need.

## Decisions

### 1. A source layer in `crates/core`, not a mount and not a virtual FS

`FileEntry` gains `source: Source` where

```
enum Source {
    Local(PathBuf),                     // as today
    Smb { share: String, path: String } // path uses '/' separators, "" = browsable root
}
```

and a small async abstraction owns all data access:

```
trait FileHandle: Send {                    // per-open handle
    async fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize, SourceError>;
}
trait Sources: Send + Sync {                // the store handed to server and picker
    async fn open(&self, source: &Source)   -> Result<Box<dyn FileHandle>, SourceError>;
    async fn list_dir(&self, source: &Source) -> Result<Vec<ListEntry>, SourceError>;
}
```

`Local` implements both over `std::fs` (the server keeps its current blocking-reads-in-async
behavior; that wart is pre-existing and not worsened). `Smb` implements both over the SMB
client. `Server::new` takes the store (mockable in tests, like `Transport`); the picker's
listing and the recursive share scan go through `list_dir`, mirroring `add_path` semantics
(stack walk, `.nsp`/`.nsz` case-insensitive filter, skipped-entry reporting).

Why not more: a full virtual-fs trait (stat, create, watch, ...) is unneeded surface; an
OS mount was rejected in exploration (root required, credentials outside the app, `hard`
CIFS hangs). A `read_at` handle matches the server's per-range sequential reads and both
candidate crates' primitives.

### 2. SMB client: `smb2` crate, chosen behind the abstraction

`smb2` (vdavid, v0.34): pure Rust, no FFI (the project has no C deps today), tokio-native,
pipelined reads (large NSP streams), `reconnect()` designed for network failure,
`list_directory` returning sizes, tested against Docker Samba, MIT OR Apache-2.0.
Alternatives: the `smb` crate (pure Rust, shows `read_at`, but one request at a time and
fewer tests) — kept as the documented fallback; `pavao`/`remotefs-smb` (libsmbclient FFI +
system package) — rejected. Both crates are 0.x: the source layer is the version firewall,
and the dependency is pinned.

**Spike first (task 1):** against the real NAS, verify NTLM login, directory listing with
sizes, and ranged reads (open reader → seek/read at `range_offset`), and measure ~100 MB
throughput. The streaming download in `smb2` starts at offset 0, so the exact ranged-read
primitive (file reader vs low-level CREATE+READ+CLOSE) must be confirmed before the
implementation leans on it; both crates demonstrably have an offset-read path. If `smb2`
fails on the target server (dialect/auth), re-evaluate `smb` against the same checklist
with no design change.

### 3. One shared hub for connections, behind a tokio Mutex

An app-level `ShareHub` maps share name → `{ config, client, tree, password }`, lives in
`Arc<tokio::sync::Mutex<...>>`, and is passed both to the TUI (browse, connect) and to the
background server task (open for `FILE_RANGE`). Reconnect-on-open uses the in-memory
password and retries the open once; a failed re-establishment surfaces as an ordinary open
failure, keeping the server's "log and end the connection" semantics untouched.

Alternative considered: an actor task with an mpsc command protocol. Rejected: more moving
parts for no benefit, because contention cannot actually occur — while the server task is
active the TUI locks `S` and the picker, so browsing and streaming never overlap; during an
install only the server touches the hub.

### 4. Config: TOML at `~/.config/dbi-backend/config.toml`

`serde` + `toml` + `dirs` (XDG `config_dir`). Shape:

```toml
[[shares]]
name     = "nas"             # display name, unique
server   = "192.168.1.50"    # host or host:port, port defaults to 445
share    = "games"
username = "ivan"
# domain = "WORKGROUP"       # optional
# root   = "/switch"         # optional browsable root inside the share
```

A malformed entry is skipped with a warning and the remaining entries load; a duplicate
`name` keeps the first occurrence and warns. No password field exists at all.

### 5. TUI: two new modes, `S` mirrors `a`

`Mode::Shares(ShareList)` (table of name / server·share / username; cancel with Esc/q) and
`Mode::Password(PasswordInput)` (centered modal like the confirm dialog; chars collected
into a buffer and rendered as `*`, Enter submits, Esc cancels). `S` joins the hint chips
(`[S]hares`) and is inert + dimmed while the server task is active, like `a`/`d`/`c`.
After a successful connect the picker opens at the share's `Source::Smb` root; go-up from
the root is a no-op (the root plays the role the filesystem root plays locally).

### 6. Origin display

The Files table renders the entry's source: local paths as today, share entries as
`<share-name>:<path-within-share>` (root itself as `nas:`). Names, sizes, and the
replacement warning are unchanged, so name collisions across shares behave exactly like
collisions across directories today.

## Risks / Trade-offs

- [0.x crate API churn in `smb2`] → pin the version; all crate usage confined to the SMB
  source implementation; the `smb` fallback shares the same abstraction.
- [Ranged-read API shape unconfirmed] → spike is task 1; both crates expose an
  offset-read primitive (reader or low-level CREATE+READ); worst case is a thin low-level
  helper, never a discard-streaming fallback (mid-file offsets on multi-GB files).
- [NAS dialect/auth incompatibility] → spike against the real server; `smb2` covers
  SMB 2.x/3.x signing and encryption and refuses silent guest fallback with a typed error.
- [Password held in memory for the run] → required for transparent reconnect; never in
  argv/env/config/logs; dropped at exit. Alternative (re-prompt per open) would break
  mid-install reconnect.
- [Mid-stream failure still kills the install] → accepted for v1 (matches the server
  spec); retry-in-range is a follow-up change if field experience demands it.
- [Blocking local reads inside async] → pre-existing behavior, unchanged by this design;
  SMB reads are genuinely async, so no new blocking is introduced.
- [Server-opened reads race with TUI reconnect] → single hub mutex serializes hub state;
  open+reconnect is one critical section, so a browse-triggered reconnect cannot interleave.

## Migration Plan

Additive change; no data or protocol migration. Rollback is reverting the change: the
config file is simply ignored by the previous binary.

## Open Questions

None. The crate-level unknowns (ranged-read primitive shape, throughput, NAS dialect
compatibility) are delegated to the spike task and cannot change the specs, the approach,
or the task breakdown — only which crate gets pinned.
