# Design

## Context

`ui()` in `crates/app/src/app/ui.rs` builds one vertical layout for every mode:
header `Length(1)`, primary region `Min(3)`, optional progress row
`Length(1)`, log `Min(3)`, help line `Length(1)`. Ratatui splits leftover
height between two `Min` constraints roughly equally, so the log pane takes
about half the terminal — e.g. ~19 rows of a 40-row terminal for a tail that
needs a handful of lines. The log itself is fine (newest lines, capped
retention); only its on-screen share is wrong.

## Goals / Non-Goals

**Goals:**

- Cap the log pane at 8 rows total (titled border + up to 6 visible lines) in
  every mode, with and without the progress row.
- Give all remaining height to the primary region; on short terminals the log
  yields first while the primary region keeps its 3-row minimum.

**Non-Goals:**

- No new panes, data, or keybindings; no change to log retention (1000 lines),
  rendering (timestamps, level colors), or any non-UI behavior.
- No user-adjustable log height.

## Decisions

### Cap the log with `Constraint::Max(8)`, keep the primary region `Min(3)`

The second `Min(3)` in `ui()`'s constraint list becomes
`Constraint::Max(LOG_PANE_MAX_HEIGHT)` with
`const LOG_PANE_MAX_HEIGHT: u16 = 8;` defined beside the other UI constants
(`IDENTITY_WIDTH` style: value plus a doc comment saying what fits inside).
This states the intent exactly — "grow to at most 8, never beyond, yield
beyond" — and needs no hand-rolled height arithmetic.

Verified against ratatui 0.29.0 (the version pinned in `Cargo.lock`) with a
standalone probe of `[Length(1), Min(3), Max(8), Length(1)]`:

| terminal rows | primary | log |
| --- | --- | --- |
| 40 | 30 | 8 |
| 24 | 14 | 8 |
| 12 | 3 | 7 |
| 10 | 3 | 5 |
| 8  | 3 | 3 |

So tall terminals give the primary region everything past 8 rows, and the log
starts yielding once the primary region reaches its minimum — the spec's two
new scenarios.

Alternatives considered:

- Explicit arithmetic (`log_rows = min(8, spare)`) with `Length` — duplicates
  what the solver already does, more code to keep in sync with the other rows.
- `Percentage` — terminal-relative, so still unbounded on large terminals
  without adding a second cap.
- `Length(8)` fixed — on short terminals it fights the primary region's
  minimum instead of yielding.

### Keep the single shared layout, no mode-specific heights

All modes already share the one constraint list, and the picker and share list
stand to gain the same rows as the file list. A per-mode height would be
state the app does not need; one constant covers all views, with or without
the progress row (a fixed `Length(1)` row that shifts the same split up by
one).

## Risks / Trade-offs

- [Six visible log lines hide older context after a burst of warnings] → the
  1000-line retention is untouched; a scrollable or expandable log pane is a
  separate change if it is ever wanted.
- [A ratatui upgrade could redistribute `Min`/`Max` leftovers differently] →
  the version is pinned in `Cargo.lock`, and the new layout tests assert the
  split, so a behavior change surfaces in CI rather than silently on screen.
- [On very short terminals (≤6 rows) the log pane degenerates to its border] →
  same class of compromise the solver already makes today; the primary region
  stays at its minimum, which is the point of yielding order.

## Migration Plan

Single code change, no data or configuration to migrate; rollback is a plain
revert.
