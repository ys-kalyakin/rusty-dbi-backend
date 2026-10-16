# Proposal

## Why

Adding content to the file list today requires the user to type an exact
filesystem path from memory into a one-line prompt (`a`, then type, then Enter).
This is error-prone and unfriendly. An interactive picker lets the user browse
the filesystem visually and select a directory or file directly.

## What Changes

- Add an in-TUI filesystem picker that lists the entries of a directory and lets
  the user navigate with the keyboard.
- The picker starts in the current working directory and supports moving the
  cursor, descending into a directory, going up to the parent directory, and
  selecting the highlighted entry.
- Selecting a directory adds it (recursively scanned, unchanged `add_path`
  semantics); selecting a file adds that file directly.
- **BREAKING** (TUI behavior): the `a` key no longer opens the manual typed-path
  prompt; it opens the picker instead. The typed-path prompt is removed.
- Show the picker as an overlay/panel with its own key hints; cancel returns to
  the normal file list view.

## Capabilities

### New Capabilities
- `path-picker`: browsing a directory's entries in the TUI, navigating the
  filesystem, and choosing a directory or file to add to the file list.

### Modified Capabilities
- `cli-tui`: the `a` keybinding and path-entry prompt change to open the picker;
  the UI gains a picker view and its status/help line.

## Impact

- `crates/app/src/app.rs`: picker state, key handling, and rendering.
- `crates/app/src/main.rs`: event routing for the picker key flow.
- `openspec/specs/cli-tui/spec.md`: keybinding and layout requirements.
- No new dependencies (filesystem access via `std::fs`).
