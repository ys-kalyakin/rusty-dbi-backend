# Proposal

## Why

The TUI is functional but visually flat and sometimes dishonest about state: the header
is a plain gray text line that never names the application, "running" appears the moment
`s` is pressed even though the Switch may not be connected for seconds, disabled keys
look identical to enabled ones, and the file list, picker, and hint line render columns
and emphasis without alignment or hierarchy. A first pass of visual polish makes the
interface easier to scan, communicate state truthfully, and set the styling foundation
(one theme module, rounded frames) for future UI work.

## What Changes

- Introduce a single theme/palette module (status colors, text, muted, selection) used by
  all rendered widgets, and switch all block borders to rounded corners.
- Rebuild the header: application name, a status indicator that visibly distinguishes
  idle / waiting-for-Switch / running, a spinner while waiting for the device, and the
  file count and total size pushed to the right.
- Track the real connection lifecycle in the UI: pressing `s` shows "waiting for Switch"
  while the background task connects (and while it reconnects after a loss), "running"
  once the device is connected, "idle" again when the server ends.
- Visually mark `a`, `c`, `d` as unavailable in the hint line while the server task is
  active; the keys stay inert (existing behavior).
- Render the file list as an aligned table: file name, right-aligned humanized size, and
  a muted, mid-truncated path column.
- Show a friendly onboarding hint in the file list region when no files are offered.
- Extend the progress row with the transferred/total byte counts and, once a transfer
  speed is known, an estimated time remaining.
- Upgrade the picker listing: show file sizes next to file names, color directories
  differently from files, and show the entry count (and the match count while filtering).
- Restyle the confirmation dialog and help overlay as rounded, centered popups (behavior
  unchanged).
- Update the affected pinned-visual tests (header colors, `>` highlight symbol, compact
  confirm dialog) alongside the code.

Out of scope (deliberately deferred): floating picker over the file list, scrollbars and
mouse-wheel scrolling, any keybinding changes.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cli-tui`: the header/status presentation gains the app name, a three-state status
  indicator (idle / waiting / running) tied to the real connection lifecycle, and the
  hint line marks disabled actions; the transfer progress display gains byte counts and
  estimated time remaining; the empty file list shows an onboarding hint.
- `path-picker`: the listing shows each file's size and the number of listed entries
  (match count while filtering).

## Impact

- Code: `crates/app/src/app.rs` (render functions, `ServerEvent` gains a waiting
  transition, server spawn loop emits it while connecting/reconnecting, progress ETA
  tracking), new `crates/app/src/theme.rs`, `crates/app/src/picker.rs` (file sizes come
  from the entries now loaded with metadata), tests in both files.
- No new dependencies; ratatui/crossterm already provide everything needed.
- Behavior risk is low: keybindings, picker keys, confirmation semantics, and the
  protocol are untouched. Existing tests that pin exact colors, the `>` highlight symbol
  and the compact confirm dialog dimensions will be updated in the same change.
