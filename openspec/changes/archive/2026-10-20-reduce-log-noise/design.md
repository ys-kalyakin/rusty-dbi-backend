# Design

## Context

See `proposal.md` — Why for the motivation, and `specs/` for the required behavior.

Current state that shapes the approach:

- The core server (`crates/core/src/server.rs`) reports everything to its caller through
  a single callback: `ServerUpdate::Log(String)` for text lines and
  `ServerUpdate::Progress { name, sent, total }` for transfer progress. The app
  (`crates/app/src/app.rs`) forwards `Log` lines to the log channel and `Progress` to
  the event channel; `main.rs` drains both channels and calls `App::handle_server_event`.
- `crates/core/src/server.rs` currently logs two lines per `FILE_RANGE` request
  ("File range" and the range-size/offset/name detail line) in addition to the
  progress updates.
- The reconnect loop lives inside `spawn_server` in `crates/app/src/app.rs`. It talks
  to real USB hardware, so it cannot be exercised by a unit test as written; today it
  logs "Waiting for Switch..." and "USB error: ..." on every attempt.
- Human-readable byte formatting (`human_size`) already exists in the app crate; the
  core crate has no presentation helpers and should not grow any.

## Goals / Non-Goals

**Goals:**

- One log line per successfully streamed file range, and no line per request.
- A completion signal that both the log and the progress region can react to.
- Connection-attempt logging that reflects state changes instead of attempt counts.
- Keep the policy unit-testable without a Switch or USB device attached.

**Non-Goals:**

- Changing the protocol, the transfer loop, the chunking, or the progress throttling.
- Restructuring the log panel, its cap, or its coloring scheme beyond the one new
  summary line.
- Persisting or exporting logs.

## Decisions

### 1. The completion is a structured core update, not a formatted string

The core server gains `ServerUpdate::Completed { name: String, bytes: u64 }`, emitted
once after a range has been streamed successfully. The app maps it to a matching
`ServerEvent::Completed`, and `App::handle_server_event` clears the progress state and
pushes the log line `Sent: {name} ({human_size(bytes)})`.

Alternatives considered:

- *Core logs a ready-made string ("Sent: game.nsp (4508875660 bytes)")*: rejected —
  the core crate would have to know about presentation and about byte units, and the
  app would still have no signal to clear the progress region.
- *The app derives completion from the progress stream* (the last progress sample
  implies the end): rejected — there is no explicit end-of-transfer event, so a
  truncated transfer would look identical to a finished one.
- *No completion signal at all* (log nothing about successful transfers): rejected —
  the log would no longer record what was installed, which is the one piece of
  information the progress bar cannot provide after it disappears.

### 2. Only the two per-request lines are removed from the server

Removed: "File range" and "Range Size: ..., Range Offset: ..., Name len: ..., Name: ...".
Kept: "Entering command loop", "Get list", "Exit", "Ignoring unknown framing", and every
failure line (oversized range header, unknown file, failed open, failed seek, read
failure, short transfer).

Rationale: the removed lines describe a request that the completion line already
summarizes, and they are the only per-request output in the server. The kept lines are
either lifecycle events or failures, i.e. the "important events" the log is for. A
file transferred in several range requests therefore produces one summary line per
range — honest, and consistent with the per-range progress reporting the DBI client
expects.

### 3. The reconnect policy is extracted into a small, testable helper

`spawn_server` keeps two pieces of state across attempts today; the new behavior needs
the same state but with a policy attached:

- `waiting_logged` — "Waiting for Switch..." is emitted only when this flag is false,
  and the flag is set again after a connection is established and later lost, so each
  waiting period logs the state once.
- `last_usb_error` — the "USB error: {e}" line is emitted only when the message differs
  from the last one, and the remembered message is cleared on a successful connection.
  A failure that persists is therefore logged once, while a failure whose reason
  changes is logged again.

Because the loop talks to real USB hardware, the policy lives in a helper type with
no I/O, and the helper is what the unit tests drive. `spawn_server` only performs the
I/O and asks the helper what (if anything) to log.

Alternatives considered:

- *Rate-limit by time (one line every N seconds)*: rejected — it needs a timer, still
  repeats, and blurs genuine state transitions.
- *Drop the USB error line entirely*: rejected — the reason the device is not found is
  the information the user needs.

### 4. The progress region clears when the transfer completes

`ServerEvent::Completed` clears `App::progress`, so the bar disappears as soon as the
range is done and the log line takes over as the record of the transfer. Without this,
the bar would stay at 100% until the next transfer or until the server ends, and the
log and the bar would describe different moments.

### 5. The summary line is colored as a success

`log_color` gains a branch for lines starting with "Sent:" that returns the success
color, so the important event stands out against the muted default. The existing
"Warning:"/"Failed" branches are untouched.

## Risks / Trade-offs

- [A file transferred in several ranges logs several summary lines] → accepted; the
  lines are accurate per range, and the alternative (per-file aggregation) would need
  the core server to keep per-file state across requests.
- [The progress region now disappears slightly earlier than before] → intended; the
  transfer is finished, and the summary line carries the same information.
- [Deduplicating USB errors can hide a recurring failure] → the line is re-emitted as
  soon as the message changes, and every failure still ends the affected run with
  "Connection lost: ...".
- [`ServerUpdate` and `ServerEvent` gain a variant] → both are workspace-internal
  types; the exhaustive `match` sites are updated in the same change and the compiler
  flags any site that is missed.
- [The core test that counts progress reports is unaffected] → progress reporting is
  unchanged; the new completion update is a separate variant.

## Migration Plan

No data or protocol migration. The change is a single commit in the workspace; rolling
back means reverting it, which restores the previous log volume with no other effect.

## Open Questions

None.
