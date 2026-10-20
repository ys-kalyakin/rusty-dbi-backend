# Proposal

## Why

Before a real install begins, the TUI progress bar briefly appears fully filled
(green at 100%), then resets to the file's start and only then behaves
correctly while the install streams. The cause is in the backend's progress
reporting: `FILE_RANGE` handling reports progress on **any** range that reaches
(the end of) the file, and DBI's install preparation reads the ticket and
certificate entries that live at the very end of an NSP *before* the content
stream starts, so a small trailing read is presented as a completed 4 GiB
transfer. The bar must not flash a complete file before any installation bytes
flow.

## What Changes

- The server distinguishes the ranges that (re)start or continue a file's
  streamed sequence from the file begins (contiguous, advancing position) from
  auxiliary reads elsewhere in the file (header lookups, ticket/certificate,
  NCZ section tables). Only streaming ranges advance the progress state,
  reach the 100% fill, and signal completion; non-streaming reads are served
  as before but stay silent (no progress samples, no completion report).
  - This removes the bogus "fully green bar" flash and the pre-install
    "Sent: file" success summary that currently fires when a trailing
    preparation read ends at the file's last byte.
- The TUI keeps its progress region semantics but tracks per-file sample
  continuity: when a sample's byte offset decreases for the same file (a new
  streaming phase), the bar restarts at the new position and the speed/ETA
  estimation restarts instead of continuing from stale samples.
- No wire-protocol change: DBI-visible byte streams, headers, and response
  framing are unchanged. Only the server-update/log reporting and the TUI's
  treatment of those updates change.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `server`: progress and completion reporting is tied to a file's streamed
  sequence (restart from offset 0 or continuation of the streamed position);
  auxiliary reads elsewhere in the file are served silently instead of
  advancing or completing progress.
- `cli-tui`: the transfer progress reporting requirement gains incoherence
  rules (a progress sample must reset the speed estimation on a backward
  jump, and a backward jump may not be shown as a forward continuation), and
  the log summary requirement is sharpened so a completion summary can only
  follow a streamed-to-the-end report.

## Impact

- `crates/core/src/server.rs`: `Server` gains per-connection, per-file streamed
  position state; `process_file_range` decides which ranges emit
  `ServerUpdate::Progress`/`ServerUpdate::Completed` and the "Sending:" start
  announcement.
- `crates/app/src/app.rs`: `App::update_progress_at` restarts the speed/ETA
  estimation when a sample for the same file rewinds the bar.
- `crates/core/tests/server_test.rs` / app tests: new cases for silent
  auxiliary reads, trailing-read-not-completing, and restart-at-zero.
- Users of the DBI protocol see no protocol change; only the backend's log and
  progress reporting change.
