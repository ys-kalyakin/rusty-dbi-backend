# Design

## Context

The previous fix (`2026-10-20-fix-transfer-progress`) reshaped the progress region into
two rows (statistics text above an unlabeled `Gauge`) and made the server emit a
`Completed` update after every successfully streamed range. The app clears progress on
each `Completed`, and the DBI loop streams a file through many consecutive
`FILE_RANGE` requests (roughly one per megabyte), so the region toggles off→on between
ranges and shifts the files/log layout on every toggle — the perceived flicker. The log
gains one `Sent: <name> (<bytes>)` line per range in the process.

Relevant current pieces: `ServerUpdate::{Log, Progress, Completed}` in
`crates/core/src/server.rs`; `ProgressState`, `render_progress`, the `Length(2)` layout
constraint and the log coloring in `crates/app/src/app.rs`. See proposal.md for the
requested outcomes.

## Goals / Non-Goals

**Goals:**

- A stable, flicker-free single-row progress region during one file's streaming.
- Log noise drops from one line per streamed range to one `Sending:` line per file
  start plus one `Sent:` line per completed file; failure lines unchanged.
- Keep the existing progress statistics (percent, sent/total, smoothed speed, ETA) and
  the success-colored completion line.

**Non-Goals:**

- Wire-protocol, transport, or cadence changes (percent-throttled `Progress` stays).
- Aggregated or multi-file progress display; the region describes the current file only.
- Renaming, reordering, or re-wording existing lifecycle log lines.

## Decisions

### D1: One-row region: statistics text first, bar fills the remainder

`ui()` reserves a single `Constraint::Length(1)` for progress. `render_progress` builds
the statistics spans (name, percent, sent/total, speed, ETA), measures their width,
splits the row horizontally into [text width, remainder], renders the paragraph on the
left and the gauge on the remainder. The name leads so it survives narrow widths.

- Alternative: a `Gauge` with the text as its center label — rejected: the label
  truncates/obscures long file names and was the reason the region was split into two
  rows.
- Alternative: a manual block-character bar string — rejected: duplicates `Gauge`
  behavior for no benefit.
- On very narrow terminals the text keeps priority; the bar gets whatever remains
  (possibly nothing). The percentage/byte statistics already truncate the same way today.

### D2: The server tracks the file being served and knows file boundaries

`process_file_range` keeps per-connection state: the name of the file currently being
streamed and its total size. On each range request:

- If the requested file differs from the tracked one, announce the start by writing
  `Log("Sending: <name>")` and switch the tracker to that file.
- After the range streams fully, if `range_offset + range_size >= file total`, emit
  `Completed { name, bytes: file_total }` and clear the tracker so a later re-read of
  the same file announces a fresh start; otherwise no completion is reported.

Rationale: the server alone knows range boundaries and file totals, so start/end
pairing has one source of truth and is directly testable in `server_test.rs`. Putting
the start line in the app (deriving it from `Progress` name changes) would guess at
re-reads and would pair the two log lines across two crates. The `Completed` variant
keeps its shape; its meaning changes to file-level completion.

- Alternative: report the sum of streamed bytes for the file — rejected: re-reads
  inflate the number; the file's size is the meaningful, stable value for "the file
  went over".

### D3: The app keeps progress across ranges; clearing stays file-level

`App::handle_server_event` keeps clearing progress on `Completed`, `Disconnected`,
`Ended`, and the defensive `Started` clear — unchanged code, new cadence, because
`Completed` now arrives once per file. Within one file the region therefore stays up
(only the bars/statistics update), and name changes between files replace the
`ProgressState` in place without hiding the region.

Rationale: the flicker comes from clearing between ranges, not from the redraws
themselves; removing the per-range clear removes the layout shift.

### D4: Log lines and colors stay in current wording

`Sending: <name>` parallels the existing `Sent: <name> (<size>)` summary, which keeps
its wording and success color; the start line takes the default text color like other
informational lines. Tests that pin the two-row region and per-range completion move to
the new cadence in the same change.

## Risks / Trade-offs

- [End-of-file detection assumes the device streams through the file's last byte]
  → A file abandoned mid-stream never logs `Sent:`; the existing `Connection lost:`
  line covers the session end, and header-phase partial reads intentionally produce no
  completion.
- [A one-row shift remains at file boundaries] → When a file completes and the device
  starts the next one milliseconds later the region briefly disappears once per file;
  boundaries are minutes apart, versus the per-second toggling being removed.
- [The `Sending:` announcement can repeat for re-reads (e.g. verification passes)]
  → Each read-through is a distinct transfer of that file; accepted as a significant
  event by definition.
- [Merged row truncates on very narrow terminals] → Text wins priority over the bar
  (D1); worst case the bar is squeezed out rather than the file name.

## Migration Plan

Workspace-internal change shipped with the single binary: update the crates and their
tests together, run the workspace tests, smoke-check the region at wide and narrow
sizes via `TestBackend`. Rollback is a plain revert of the crates.

## Open Questions

None — the per-file start/end logging granularity was confirmed with the requester.
