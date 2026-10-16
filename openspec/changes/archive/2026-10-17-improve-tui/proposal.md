# Proposal

## Why

The TUI is the only interface to the backend, but it is currently write-mostly:
the file list cannot be scrolled or selected, a single file cannot be removed
(only cleared wholesale), no sizes or totals are shown, status and log lines are
uncolored, and there is no visibility into an in-progress install. Users with
more than a screenful of NSP files cannot see or manage what they are offering,
and cannot tell whether a transfer is progressing or stuck.

## What Changes

- **File list becomes interactive**: a movable highlight (`j`/`k`, arrows,
  `g`/`G`), scrolling, and per-file removal with `d`. Editing stays locked while
  the server runs.
- **Sizes are shown**: each file's size and the total offered size, computed once
  when the path is added.
- **Status is legible**: color-coded status (`idle`/`running`), a header status
  region, and timestamped log lines with `warning`/`error` emphasis.
- **Live transfer progress**: while a file is being served the UI shows the
  current file, a percentage progress bar, and transfer speed, driven by new
  progress events from the server loop.
- **Path picker gets a filter**: `/` filters the listing by substring.
- **Help and safety**: a `?` help overlay lists all bindings; `q` and `c`
  require confirmation so a stray keypress does not discard the list.
- **BREAKING (internal)**: `Server::run`'s `FnMut(String)` log callback is
  replaced by a typed event callback carrying both log lines and progress. No
  DBI protocol bytes change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cli-tui`: terminal layout (header/status, progress region, scrollable file
  list, sizes), keybindings (selection, `d`, `?`, confirmation, picker filter
  routing), and the background server lifecycle (typed progress/log events).
- `file-list`: entries now carry the file's size, exposed for display.
- `path-picker`: the listing can be filtered by a substring of the name.

## Impact

- `crates/app/src/app.rs`: list selection/state, deletion, confirmation mode,
  help overlay, status coloring, timestamps, progress rendering.
- `crates/app/src/picker.rs`: filter state and matching.
- `crates/app/src/main.rs`: routing typed server events to UI state.
- `crates/core/src/filelist.rs`: `Entry { path, size }` computed at add time.
- `crates/core/src/server.rs`: typed event callback replacing the string logger.
- Specs: delta files for `cli-tui`, `file-list`, `path-picker`.
- No new dependencies; `ratatui`/`crossterm` already vendored.
