# Proposal

## Why

The transfer progress display can misrepresent the transfer it is describing. A
progress state survives a lost connection, so reconnecting can make an old, unfinished
bar reappear after the transfer has already ended. The remaining-time estimate is
derived from a single pair of progress samples, so uneven sample spacing makes it jump
between unrelated values. The file name is drawn inside the gauge label; once the green
fill reaches the text it becomes hard to read.

## What Changes

- Make progress state end with the transfer: clear it when a range completes, when the
  connection is lost, and when the server ends. A stale bar MUST NOT reappear after a
  reconnect or a later run.
- Derive transfer speed and the remaining-time estimate from a smoothed rate over
  several recent samples instead of one sample interval, and avoid recomputing from
  sub-100 ms intervals. The estimate should change gradually as progress advances.
- Split the progress region into a text row and a bar row. The text row shows the file
  name, percentage, byte counts, speed, and ETA in normal theme colors; the bar row has
  no overlaid text, so the name stays readable at any fill level.
- Keep the existing behavior of clearing the progress region when a range completes and
  writing one success summary line per completed range.
- Add state and render tests for stale-state clearing, smoothed estimates, and the
  split progress layout.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cli-tui`: progress presentation gains a separate readable file-name/stats row above
  an unlabeled gauge, smoothed speed and ETA, and explicit clearing of stale progress
  after a disconnect or reconnect.

## Impact

- Code: `crates/app/src/app.rs` (`ProgressState`, `App::handle_server_event`,
  `update_progress`, `render_progress`, UI layout constraints, tests).
- No core protocol, server streaming, CLI, or dependency changes. The progress events
  produced by the server keep their current shape and cadence.
- Existing progress, completion-summary, and layout tests are updated; new regression
  tests cover disconnect/reconnect and the two-row progress region.
