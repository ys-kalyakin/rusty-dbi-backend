# Proposal

## Why

A review of the shipped backend found trust-boundary, lifecycle, and robustness
gaps. The TUI can mislead the user (editing a list the running server no longer
sees, a status that never clears), directory scans can silently abort and drop
titles, and a failed `FILE_RANGE` can leave the Switch waiting for bytes that
never arrive. These weaken the "it just works" guarantee users rely on when
installing titles.

## What Changes

- **Server lifecycle (cli-tui)**: surface the server's real state — `Running`
  while the task is active, back to `Idle` when it ends — and allow starting
  again after the previous run ends. Disable file-list edits (`a`, `c`) while
  the server is running, since the server works from a start-time snapshot.
- **Log tail (cli-tui)**: the log panel SHALL show the most recent lines so new
  output is visible without scrolling.
- **Resilient scan (file-list)**: a directory scan SHALL continue past an
  unreadable entry or subdirectory, collecting what it can and reporting what it
  skipped, instead of aborting the whole add.
- **Collision warning (file-list)**: adding an entry whose name already exists
  SHALL still replace it (unchanged keying) but SHALL report the replacement.
- **Range parsing (protocol)**: parsing a `FILE_RANGE` payload SHALL validate the
  declared `name_len` and require valid UTF-8, rejecting malformed payloads
  rather than lossily decoding trailing bytes.
- **No range desync (server)**: when the named file is missing/unreadable, or a
  read yields fewer bytes than `range_size`, the server SHALL log the failure and
  abort the connection so the Switch cannot hang waiting for an incomplete range.
- **USB hot path (non-behavioral)**: reuse transfer buffers while streaming
  ranges instead of copying each chunk.
- **CI (tooling)**: add a workflow running `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, and `cargo test --workspace`.

## Capabilities

### New Capabilities
<!-- none: every change below refines an existing capability -->

### Modified Capabilities
- `file-list`: scans tolerate per-entry failures and report skips; name
  replacement is reported instead of silent.
- `protocol`: `FILE_RANGE` payload parsing validates `name_len` and UTF-8.
- `server`: `FILE_RANGE` failures end the connection instead of leaving the peer
  short of the promised range.
- `cli-tui`: server lifecycle/status/restart, edit freeze while running, and a
  last-lines log view.

## Impact

- `crates/app/src/app.rs`: `ServerStatus` transitions, edit gating, log view.
- `crates/app/src/main.rs`: route server-end events back to the UI.
- `crates/core/src/filelist.rs`: resilient scan and collision reporting; return
  type carries warnings.
- `crates/core/src/protocol.rs`: stricter `FileRangeHeader::parse`.
- `crates/core/src/server.rs`: `FILE_RANGE` failure aborts the connection.
- `crates/core/src/transport/usb.rs`: buffer reuse (no behavior change).
- `.github/workflows/ci.yml`: new CI workflow.
- `openspec/specs/{file-list,protocol,server,cli-tui}/spec.md`: updated by deltas
  after sync.
- No new dependencies; no wire-format or CLI-surface changes.
