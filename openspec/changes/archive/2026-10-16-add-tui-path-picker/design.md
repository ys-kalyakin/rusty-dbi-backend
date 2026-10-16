# Design

## Context

The TUI lives in `crates/app/src/app.rs`. Today `App` holds `input:
Option<String>`; pressing `a` sets it to `Some("")` and `handle_key` routes
subsequent characters into the string, adding it on Enter (see
`crates/app/src/app.rs:68` and `crates/app/src/app.rs:95`). Rendering happens in
`app::ui` from a `&App`, and `main.rs` forwards crossterm `KeyEvent`s and draws
on a 200 ms tick.

The core `FileList::add_path` (`crates/core/src/filelist.rs:26`) already owns the
add semantics we want to reuse: a directory is walked recursively for `.nsp`/
`.nsz`, a directly selected file is added regardless of extension. See
proposal.md for motivation.

## Goals / Non-Goals

**Goals:**

- Browse the filesystem and select a directory or file without typing a path.
- Keep the browsing logic independent of rendering so it is unit-testable
  without a terminal.
- Reuse `FileList::add_path` unchanged for selection semantics.

**Non-Goals:**

- No fuzzy search, filters, previews, bookmarks, or multi-select in this change.
- No changes to the core, protocol, or server crates.
- No new third-party dependencies.

## Decisions

### Replace `input` with a UI mode

`App` gets a `mode: Mode` enum (`Normal` / `Picker(Picker)`) replacing the
`input: Option<String>` field. `handle_key` dispatches to picker handling when a
picker is present, which is what makes the "picker keys are not global actions"
requirement hold naturally.

Alternative: keep `input` and add a separate `picker` field. Rejected because
two optional states invite invalid combinations.

### A `Picker` struct in a new module

New `crates/app/src/picker.rs` holds `dir: PathBuf`, `entries: Vec<Entry>`,
`cursor: usize`, and an optional `error: Option<String>`. `Entry` is a name plus
a kind (`Dir` / `File`). Navigation methods (`move_up`, `move_down`, `descend`,
`go_up`, `open`) mutate this state and are unit-tested directly, mirroring how
`FileList` is tested in core. Rendering only reads it.

Alternative: put the browser inline in `App`. Rejected because it is harder to
test and keeps `app.rs` growing.

### Listing order and content

On entering a directory, `read_dir` entries are classified with `DirEntry::
file_type`; symlinks are followed via `metadata` to decide the kind, and an
entry whose metadata fails is skipped. Directories are sorted first, then files,
each case-insensitively by name, so the listing is deterministic (matching the
`BTreeMap` determinism already used by `FileList`).

### Keybindings

- Up / `k`, Down / `j`: move the highlight.
- Enter: descend into the highlighted directory; for a file, add it.
- Space: add the highlighted entry (directory recursively, file directly).
- Backspace / Left / `h`: go to the parent directory.
- Esc / `q`: cancel.

Enter on a directory descends, so a dedicated key (Space) is needed to add a
directory; making Enter add a directory too would make it impossible to descend.
No synthetic `..` row is added — the parent is reached with Backspace — which
keeps the listing limited to real directory entries.

### Rendering

`app::ui` renders the current listing as a ratatui `List` with a
`ListState` whose `selected` is the picker cursor and a highlight style, and
shows `Esc cancel  Enter open  Space add  Bksp up` as the hint line. This uses
the existing ratatui dependency and keeps the file list and log regions
unchanged.

## Risks / Trade-offs

- [Very large directories make the listing long] → the window scrolls with the
  selection; no virtualization is added because typical NSP folders are small.
- [Symlink loops while descending] → descending only changes the displayed
  directory, it does not recurse, so loops cannot hang the UI; `FileList`'s
  scanner already refuses to descend directory symlinks.
- [The `a` behavior change surprises existing users] → call it out in the README
  key legend so the new binding is discoverable.

## Migration Plan

Internal TUI change with no persisted state, so there is nothing to migrate.
Rollback is reverting the change. Update the README key list to describe the
picker.

## Open Questions

None.
