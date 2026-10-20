# Proposal

## Why

The two-row progress region added for readable transfer statistics flickers: every
successfully streamed `FILE_RANGE` range clears the progress state (`Completed`), and the
next range re-creates it within milliseconds, so the whole region appears and disappears
between requests and shifts the file list and log regions up and down — the bar looks
broken and constantly blinks, especially where only one row fits between the regions.
Meanwhile the log writes one `Sent: <name> (<bytes>)` line per streamed range — one line
per roughly a megabyte — burying the significant events under the very movement the
progress bar already shows.

## What Changes

- Collapse the progress region from two rows back to a single row: the file name,
  percentage, transferred/total bytes, speed, and remaining-time estimate share one
  line with a progress bar that fills the rest of that line.
- Stop clearing the progress region per streamed range. Within one file the region
  stays put across ranges (it clears when that file finishes streaming, the connection
  is lost, or the server ends), so the layout stops jumping several times per second.
- The server reports transfer start and end once per FILE, not once per range:
  - when a file's streaming begins, write one `Sending: <name>` line to the log;
  - when a range request reaches the file's end (`offset + size >= file total`), report
    one file completion naming the file and its size — the log keeps the
    `Sent: <name> (<size>)` line for that file, colored as success.
- No log line per megabyte/per range anymore; failures (unknown file, open, seek,
  read, short transfer, oversized header) keep their existing lines.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cli-tui`: the progress region becomes a single layout row that combines the
  statistics text and the bar on one line; the region ends when a file completes (not
  per range), the connection is lost, or the server ends; the transfer log gains one
  `Sending:` line per file start and keeps exactly one success line per completed file
  with no per-range noise.
- `server`: after a range is streamed the server reports file completion only when the
  streamed range reaches the file's end, announces the start of a file's transfer once,
  and no longer completes each range; failure logging is unchanged.

## Impact

- Code: `crates/core/src/server.rs` (track the file being served across ranges, emit the
  start log line and the file-level completion), `crates/app/src/app.rs` (single-row
  layout constraint and rendering, keep progress across ranges, handle the file-level
  completion log).
- Tests: `crates/core/tests/server_test.rs` (one start line and one completion per file
  instead of per range; failure logs unchanged), `crates/app/src/app.rs` unit and render
  tests (one-row progress region, no per-range clearing, log lines).
- API: the `Completed` variant of `ServerUpdate`/`ServerEvent` keeps its shape but
  changes meaning to file-level completion; both crates are workspace-internal.
- No protocol, transport, dependency, or keybinding changes.
