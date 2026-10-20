# Proposal

## Why

The TUI log is a firehose. Every `FILE_RANGE` request writes two lines ("File range"
plus the requested size, offset, and name length), and while the Switch is not
connected the reconnect loop writes "Waiting for Switch..." and "USB error: ..." once
per second. The interesting events are pushed out of view by per-request noise, even
though the transfer itself is already reported by the progress bar.

## What Changes

- Stop logging every file-range request. The server no longer writes the "File range"
  line nor the "Range Size / Range Offset / Name len / Name" detail line.
- Log one summary line per successfully streamed file range, naming the file and the
  number of bytes sent (human-readable), so the log keeps a record of what was
  transferred without narrating every request.
- Keep every failure log as-is: unknown file, failed open, failed seek, read failure,
  short transfer, and oversized range header.
- Clear the progress bar when a transfer completes, so the bar and the summary line
  describe the same event, and color the summary line as a success.
- Quiet the reconnect loop: "Waiting for Switch..." is written once per waiting period
  instead of once per attempt, and a failing connection is written once while the same
  error persists (it is written again if the error message changes).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `server`: the command loop no longer logs each file-range request or its range
  details, and reports a completion update (file name plus bytes sent) once per
  successfully streamed range. Failure logging is unchanged.
- `cli-tui`: the log gains a single human-readable line per completed transfer and
  loses the per-request lines; the progress region is cleared when a transfer
  completes; connection retries no longer write a log line per attempt.

## Impact

- Code: `crates/core/src/server.rs` (drop the two per-request log calls, add a
  completion update to `ServerUpdate`), `crates/app/src/app.rs` (handle the completion
  event, render the summary line, clear the progress state, color the summary line,
  and quiet the reconnect loop in `spawn_server`).
- Tests: `crates/core/tests/server_test.rs` (assert no per-request log lines and one
  completion update per successful range), `crates/app/src/app.rs` unit tests (summary
  line and progress clearing, reconnect log behavior).
- API: `ServerUpdate` in the core crate gains a `Completed` variant and
  `ServerEvent` in the app crate gains a matching one. Both crates are workspace
  internal, so no external consumers are affected.
- The protocol, the transfer behavior, and the keybindings are untouched. No new
  dependencies.
