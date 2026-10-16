# Design

## Context

See proposal.md. Current shape: `spawn_server` (app.rs) only forwards log
strings; `ServerStatus` is set once and never cleared; `FileList::add_path`
returns `io::Result<()>` and aborts a scan on the first unreadable entry;
`FileRangeHeader::parse` ignores `name_len` and decodes lossily; a failed
`FILE_RANGE` returns `Ok` without sending the promised bytes. The transport
allocates and copies a fresh buffer per chunk.

## Goals / Non-Goals

**Goals:**
- Make UI state reflect the server's real lifecycle and stop it drifting from
  the snapshot the server serves.
- Make scans and name replacement observable instead of silent.
- Make a failed range end cleanly rather than desync the Switch.
- Keep all changes internal: no wire-format, CLI-surface, or dependency changes.

**Non-Goals:**
- Live-pushing file-list edits into a running server.
- Changing the DBI0 protocol or the accepted payload sizes.
- Reworking the TUI layout beyond the log tail.

## Decisions

### Lifecycle via a server event channel

Route one-shot lifecycle transitions from the server task to the UI over a
channel (started / ended), in addition to the existing log channel. The UI
applies them in its `select!` loop alongside input and ticks.

- Alternative: a shared `Arc<AtomicBool>` status. Rejected: transitions are
  events, not a level; the UI already owns a single-threaded loop that can apply
  them deterministically.
- `Running` is entered on start and cleared when the task ends (EXIT or a fatal
  transport error that ends the run). The server still retries connection loss
  internally, so only a terminal end returns the UI to `Idle`.

### Freeze list edits while running

`a` and `c` are inert while `status == Running`; they work again once the run
ends and `server_started` is cleared, so the user can edit and `s` again.

- Alternative: push edits to the server via shared state. Rejected: adds locking
  and a second source of truth for a feature the original design already
  intended to freeze. Snapshot-on-start stays.

### Log shows the tail

Render only the last lines that fit the log region (compute a start index from
the retained count and the area height, then render that slice). This also stops
cloning every retained line each frame.

### Scan returns an outcome instead of aborting

`add_path` continues past unreadable entries/subdirectories and returns what it
did: added names, skipped paths, replaced names. Only a failure to read the
given path itself is an `Err`. `insert` returns the previous path when it
replaces one, which feeds the "replaced" list.

- Alternative: keep `Result<()>` and log inside `core`. Rejected: `core` has no
  logger; the caller (TUI/CLI) owns presentation.
- Callers (main startup, picker selection) render skipped/replaced as warnings.

### Strict range payload parsing

`FileRangeHeader::parse` requires `buf.len() >= HEADER_LEN + name_len` and
decodes exactly that slice as UTF-8, returning `None` on mismatch or invalid
UTF-8.

- Alternative: keep `from_utf8_lossy` over trailing bytes. Rejected: it hides
  malformed input and can silently change a requested name.

### Failed range ends the connection

When the file is unknown, cannot be opened/seeked, or a read is short, the
server logs and returns an error that ends the run (rather than `Ok`). The app's
retry loop then reconnects.

- Alternative: zero-fill the remaining bytes to satisfy the peer. Rejected:
  silently installs corrupt data and hides the failure. Ending the connection is
  detectable by the Switch.
- Note: the existing spec scenario "Unknown file is logged and skipped" is kept
  by name; "skipped" now means "not streamed", with the connection ending.

### Buffer reuse on the USB write path

Reuse transfer buffers while streaming instead of `data.to_vec()` per chunk.
Behavior is unchanged; this is allocation/copy reduction on the hot path.

### CI

Add `.github/workflows/ci.yml` running `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, and `cargo test --workspace`.

## Risks / Trade-offs

- [Aborting on a bad range may interrupt an install that could otherwise limp on]
  -> the transfer was already going to be wrong; a detectable failure is safer
  than a silent short/corrupt stream.
- [`add_path` signature change ripples through callers and tests] -> update all
  call sites in the same change; keep the outcome type small and explicit.
- [Strict UTF-8 parsing could reject a device that sends odd names] -> names are
  UTF-8 by protocol; malformed names fail closed instead of silently.
- [Buffer-reuse changes are easy to get subtly wrong] -> keep behavior identical
  and rely on the existing server integration tests over the mock transport.

## Migration Plan

No data or protocol migration. Users see behavior changes only in the TUI
(edit freeze, status, log tail). Rollback is reverting the change; the wire
format is untouched.
