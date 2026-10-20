# Design

## Context

See `proposal.md` — Why.

The app keeps transfer state in `ProgressState` (`crates/app/src/app.rs`). The server
emits `Progress { name, sent, total }` only when the integer percentage changes, then
one `Completed` update when the range is fully written. `App::handle_server_event`
clears progress on `Completed` and on `Ended`, but `Disconnected` only changes the
status to `Waiting`; the old `ProgressState` stays in memory and becomes visible again
when `Started` returns the status to `Running`.

`update_progress` computes speed from the immediately preceding event: elapsed time is
whatever gap the percent throttle produced, so a short gap can produce a large speed
spike and an equally unstable ETA. `render_progress` puts the file name, percentage,
sizes, speed, and ETA into a single `Gauge` label. The gauge style is green; Ratatui
renders the label against the filled portion, making the name lose contrast as the bar
fills. The progress region is one row high.

## Goals / Non-Goals

**Goals:**

- Make progress state lifetime match the transfer and connection lifetime.
- Produce a stable, gradually changing ETA from noisy percent-throttled samples.
- Keep the file name readable independently of the bar fill.
- Keep the change inside the app crate and preserve the server's progress contract.

**Non-Goals:**

- Changing protocol framing, FILE_RANGE streaming, chunk size, or the server's
  percent-based progress cadence.
- Aggregating progress across multiple ranges or files.
- Adding pause/resume, transfer history, or a persistent ETA model.
- Changing the completion summary log or connection retry policy.

## Decisions

### D1: Clear progress on every terminal lifecycle transition

`ServerEvent::Disconnected` sets `progress = None` in addition to switching to
`Waiting`. `Completed` and `Ended` keep clearing it as they do now. `Started` also
clears any leftover progress as a defensive guard before `Running` becomes visible.

Rationale: progress belongs to one live connection. Once the connection is lost, any
partial progress is history, not current state. Clearing on `Disconnected` is the
primary fix; clearing on `Started` prevents a stale value from flashing during a
reconnect even if a future event path forgets the first clear.

Alternative considered: attach a connection generation to each progress event and
ignore stale generations. That also works, but it adds an identifier to the event
flow for a state that is simply not valid after a disconnect.

### D2: Time-weighted exponential smoothing for speed and ETA

`ProgressState` keeps a private smoothed rate and the timestamp/byte count of the last
accepted rate sample. On each progress event for the same file:

1. If no bytes advanced, carry the previous speed and ETA.
2. If the elapsed time since the last accepted rate sample is below a small floor
   (100 ms), do not update the rate; keep the previous speed. This discards the
   meaningless intervals caused by back-to-back percent transitions.
3. Otherwise compute the instantaneous rate from the bytes and elapsed time since the
   last accepted rate sample, then blend it into the previous smoothed rate with a
   time-weighted exponential factor (`1 - e^(-elapsed / tau)`, `tau = 2 s`).
4. Derive ETA from the smoothed rate, not from the instantaneous sample.

The public `speed: Option<u64>` and `eta: Option<u64>` fields remain the display values,
so rendering and existing tests keep a simple interface. The new estimator fields are
private. A timestamp-taking internal update method allows tests to exercise uneven
sample spacing deterministically instead of sleeping.

Alternatives considered:

- *Rolling window of samples*: equally stable, but stores a queue and has to define a
  window for sparse percent-driven events. The exponential form is constant-memory and
  gracefully weights recent samples.
- *Keep the last speed and update ETA every 200 ms tick*: smoother updates but still
  based on noisy single intervals; it hides rather than fixes the estimator.
- *Median of the last N instantaneous rates*: more robust to outliers but still jumps
  whenever the median sample changes, and needs sample storage.

### D3: Two-row progress region: text above, unlabeled bar below

The progress constraint becomes two rows. `render_progress` splits its rectangle into a
text row and a bar row:

- text row: `Sending {name}` plus percentage, `sent/total`, speed, and optional ETA,
  using theme text/muted styles;
- bar row: a `Gauge` with the success color and an empty label (`.label("")`).

The file name is the first span of the text row, so it stays readable at any fill level
and at narrow widths the statistics clip before the name does. The gauge carries no
text, eliminating the green-on-green/contrast problem instead of trying to choose a
label color that works over both filled and unfilled cells.

Alternative considered: keep one row and style the gauge label with a contrasting
foreground. Ratatui draws the label across both filled and unfilled cells and clips it
to the gauge width, so a single foreground cannot contrast against both backgrounds and
the file name still competes with the bar for width.

### D4: Tests pin observable behavior, not wall-clock timing

State tests use the timestamp-taking update path for speed/ETA. Render tests locate the
file-name row and the block-character bar row in a `TestBackend` buffer and assert they
are different rows; they also assert the name's background is not the success fill.
Lifecycle tests drive `Progress` → `Disconnected` → `Started` and assert the progress
state and rendered region stay absent until a new `Progress` event.

## Risks / Trade-offs

- [Exponential smoothing lags a genuine speed change] → `tau = 2 s` keeps the estimate
  responsive within a few seconds while removing single-interval spikes; the cost is
  acceptable for an advisory ETA.
- [Server samples are percent-throttled, so ETA updates are sparse for slow transfers] →
  sparse updates are inherently stable; the estimator still uses the full elapsed
  interval rather than assuming a fixed sample rate.
- [The two-row region takes one more terminal row from the log] → intended trade-off
  for a readable file name; the log keeps its scroll/retention behavior.
- [Clearing progress on `Disconnected` also clears a range that might have completed but
  whose completion event was lost] → this is the desired truthful behavior: without a
  completion signal, the UI must not claim the transfer finished.
- [Tests that pin one-row progress text need updating] → update them in the same change
  and keep assertions on content and row placement rather than exact columns.

## Migration Plan

No data, protocol, or configuration migration. The change is confined to the app
crate; reverting restores the one-row gauge and previous ETA estimator with no external
effect.

## Open Questions

None.
