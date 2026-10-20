# Design

## Context

All rendering lives in `crates/app/src/app.rs` (`ui`, `render_*` functions) with colors
hardcoded at each call site; `picker.rs` entries carry only `name`/`path`/`kind` and no
file size. The 200 ms input/log/tick loop in `main.rs` already redraws on every tick.
`spawn_server` (also in `app.rs`) owns the connect/reconnect loop: it logs
"Waiting for Switch..." before the first connect but on a connection loss it only logs
and keeps the status at Running, so the header currently reports "running" while the
device is not connected. See proposal.md for motivation, specs/cli-tui and
specs/path-picker deltas for the behavior contract.

## Goals / Non-Goals

**Goals:**
- One palette module as the single source of visual truth for colors and border style.
- Header and hint line that reflect real lifecycle state (idle / waiting / running) and
  lockout of `a`/`c`/`d` while the server task is active.
- Aligned, scannable file-list and picker listings; friendlier empty end and progress.

**Non-Goals:**
- Floating picker, scrollbars, mouse wheel support (deferred).
- Keybinding or picker-key changes; confirmation flow changes.
- Runtime-configurable themes / truecolor support (constant ANSI-16 palette is enough).
- Log region rework beyond styling.

## Decisions

### D1: Central `theme.rs` instead of scattered colors (chosen)
New `crates/app/src/theme.rs` exposing named constants (`TEXT`, `MUTED`, `ACCENT`,
`SUCCESS`, `WARNING`, `DANGER`, selection fg/bg) plus small `Style` helpers, and a
`fn block_title(title) -> Block` producing a rounded, default-styled block used by every
`render_*`. All widgets pull from the theme; no `Color::` literal remains in render
functions. Alternative — keep inline colors — rejected because identical color choices
are already duplicated across eight render sites and have already drifted semantically.

### D2: Three-state UI status derived from real events (chosen)
Extend `ServerStatus` `{Idle, Running}` to `{Idle, Waiting, Running}`:
- press `s` (files present, task not started) → `Waiting` (today it jumps to `Running`);
- `ServerEvent::Started` → `Running` (device connected — unchanged);
- **new `ServerEvent::Disconnected`** → `Waiting`: emitted by the `spawn_server` error
  path before it sleeps and retries, so connection loss no longer leaves a "running"
  header with no device attached;
- `ServerEvent::Ended` → `Idle` (unchanged).

`server_started` (the `a`/`c`/`d` lockout flag) keeps its current meaning, covering both
`Waiting` and `Running`, so adding a state does not change edit-lock behavior. Status is
never parsed out of log lines (too brittle). Alternative — keep two states — rejected:
it is precisely the dishonesty this change fixes.

### D3: Header as a horizontal split, spinner from a frozen clock
Header area splits into left and right `Paragraph`s: left = `dbi-backend` (accent) plus
status capsule (`○ idle` gray, `◌ waiting…` yellow, `● running` green with one-word
label), right = `N files · total size`. Right-aligned counters need two widgets; a
single paragraph cannot mix alignments. The spinner is not animated by state: its frame
is derived from `started_at.elapsed().as_millis() / 200` modulo the frame list, so the
existing tick-driven redraw animates it with zero extra plumbing. No core-crate changes.

### D4: File list as a `Table`
Replace the `List` with a 3-column `Table`: Name (`Min(0)`, truncates), Size (`Length(10)`,
right-aligned), Path (`Min(0)`, muted). Row highlight keeps the `>` symbol plus
black-on-cyan emphasized style, minimizing test churn and keyboard-cursor familiarity.
Long paths get mid-truncated via a small `abbrev_path(path, max_width)` helper
(`/mnt/…/game.nsp`). Alternative — padded-string rows inside `List` — rejected: needs
the same code minus alignment robustness.

### D5: Empty-state and hint line
When the list is empty and the picker is closed, the Files block renders a centered
muted hint ("No files yet — press a to add"). Hint chips: each action rendered as two
spans (key `[a]` in accent, label in muted); while `server_started` the disabled chips
use `AddModifier::DIM` (no strikeout — spotty terminal support), enabled chips stay full
color. Hint text content stays identical, so content-pinning tests keep passing.

### D6: Progress adds bytes + ETA in `ProgressState`
`update_progress` already computes a windowed speed; extend it to also store
`eta: Option<Duration> = (total - sent) / speed`. Render:
`gauge label = "Sending {name}  {pct}%  {sent}/{total}  eta {mm:ss}"`, ETA omitted while
unknown (`human_size` and `format_elapsed` are reused). Gauge style comes from the
theme (success green).

### D7: Picker rows carry sizes; counts in the title
`Picker::reload` fills each `Entry` with `size: Option<u64>` (`Some` for regular files
via the metadata it already fetches for the symlink fallback, `None` for dirs — no
recursive size walk; one syscall per listing entry, same order as the existing
classify call). The picker renders as the aligned 2-column `Table` the Files table
uses (Name + right-aligned Size; a third Path column would repeat the same parent
path on every row — adjusted from the earlier 3-column wording during implementation),
directories styled in accent/cyan with a `/` suffix, sizes muted. Title appends
` · {n} entries`, switching to ` · {m} matches` while a filter is active.

### D8: Popups restyled only
Confirm dialog and help overlay keep their size, centering, and behavior; they switch
to the themed rounded block (confirm keeps yellow/warning accents). The existing
compact-dialog test changes only its color assertion.

## Risks / Trade-offs

- [Waiting-state spinner glyph varies with wall-clock time] → tests for the waiting
  header assert name/status words, never the exact glyph.
- [One `metadata()` per picker entry on big directories] → same order as the existing
  per-entry classify call; display-only path, no transfer interaction.
- [ETA bounces when the windowed speed is noisy] → speed sampling is unchanged; the
  label is advisory; acceptable for a transfer UI.
- [Rounded borders/dim modifier degrade on minimal terminals] → ANSI-16 palette and
  DIM chosen precisely for wide base support; worst case is plain corners/gray.
- [Tests pin today's visuals (header fg colors, cyan confirm bg, `>` symbol)] → update
  the pinned tests in the same commit; keep the `>` symbol to limit churn.

## Migration Plan

Single-binary UI change; no data or protocol migration. Ship as one commit series
(theme → header/lifecycle → tables → polish), revert by commit if a visual regresses.
Rollback needs no user action.

## Open Questions

None blocking. If `s` should also visually dim while the list is empty (it is inert),
decide during D5 implementation; the spec only requires it for `a`/`c`/`d`.
