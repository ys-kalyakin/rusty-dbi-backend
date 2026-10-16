# Design

## Context

The UI lives entirely in `crates/app` (`app.rs`, `picker.rs`, `main.rs`) on top of
`ratatui`/`crossterm`; the command loop in `crates/core/src/server.rs` currently
communicates with the UI only through a `FnMut(String)` logger. `FileList` is a
`BTreeMap<String, PathBuf>`. The render loop redraws on a 200 ms ticker and on
every input/log/event, so any new UI state must be cheap to read. This design
covers both phases at once: see `proposal.md` for motivation and
`specs/**/spec.md` for the behavior contract.

## Goals / Non-Goals

**Goals:**
- Add per-file size, selection/scrolling, single-file removal, confirmation,
  help, filtering, and live progress without new dependencies.
- Keep the change testable with the existing `TestBackend` render harness.
- Change as little of `crates/core` as the behavior contract requires.

**Non-Goals:**
- No DBI protocol change; the bytes on the USB wire stay identical.
- No theming/config file, no mouse support, no persistent settings.
- No sub-file preview, no install history, no multi-select in this change.
- No minimum-terminal-size error screen (tracked as a risk, not solved here).

## Decisions

### Sizes are computed once at add time and stored in the entry

Change `FileList` from `BTreeMap<String, PathBuf>` to a name-keyed map of a small
entry struct `{ path: PathBuf, size: u64 }`. `add_file` already canonicalizes the
path; it computes size from `std::fs::metadata` there. This keeps the render loop
free of `stat` calls (it currently runs every 200 ms).

- Alternatives: `stat` on each render (rejected: repeated syscalls, can block the
  UI thread); a side cache keyed by path (rejected: redundant with the list).
- `FileList::get` must change from `&PathBuf` to the entry (or gain a separate
  `path(name)` accessor) for `server.rs`, which only needs the path. Prefer
  returning the entry and letting the server read `.path`.
- If metadata is unavailable (broken symlink, race), record size `0` rather than
  failing the add; the direct-file add already tolerates unreadable paths.

### The server callback becomes a typed event, not a string

Replace `Server::run<F: FnMut(String)>` with a typed callback carrying both log
lines and progress, e.g. an enum `ServerUpdate { Log(String), Progress { name,
sent, total } }`. `process_file_range` already knows `range.range_offset`,
`range.range_size`, the file path, and (after opening) the file's total size via
`file.metadata()`.

- Alternatives: a second progress callback (rejected: two plumb lines for one
  producer); writing directly to a channel from `core` (rejected: couples `core`
  to the async runtime's channel type and hurts tests).
- Throttle progress emission: emit on whole-percent change plus one final event,
  not once per `BUFFER_SEGMENT_DATA_SIZE` chunk, so the UI channel is not
  flooded by a fast transfer.
- Progress is meaningful when a request covers the whole file (the common DBI
  case). For a partial range, report progress against the local file size from
  `range_offset`, which stays monotonic across sequential range requests.

### `spawn_server` translates core updates into app events

`spawn_server` maps `ServerUpdate::Log` onto the existing log channel and
`ServerUpdate::Progress` onto the app's `ServerEvent` enum (extended with a
`Progress { name, sent, total }` variant). The UI stores the latest progress in
`App::progress` and clears it on `ServerEvent::Ended`.

### App state: selection, modes, and progress

- Add `selected: usize` to `App`, clamped to `files.len()` after add/remove/clear.
  Render the Files widget with a `ListState` (day-one `List` has no state today).
- Extend `Mode` from `Normal | Picker` to `Normal | Picker | Confirm(ConfirmKind)
  | Help`. Both new modes are modal: any key other than the ones they handle is
  swallowed, mirroring how the picker already captures keys.
- Confirmation is a small centered popup (ratatui `Clear` + `Paragraph`); `y`
  confirms, anything else cancels.
- Help is a centered popup listing bindings; `?`/`Esc` closes it.
- Log timestamps use elapsed time since startup (`Instant`), not wall-clock, to
  avoid a date/time dependency. Severity coloring is a prefix heuristic
  (`Warning:`, `USB error:`, `Connection lost:`, `Failed`) rather than threading
  log levels through `core`; heuristic now, typed levels only if it proves
  insufficient.

### Picker filter is an in-memory view

`Picker` keeps the full listing (`all: Vec<Entry>`) plus a `filter: String` and a
derived `visible: Vec<Entry>` recomputed whenever the directory reloads or the
filter changes; `cursor` indexes `visible`. This keeps existing bounds/clamp logic
valid on the filtered view and makes clearing the filter a pure recompute.

- Alternatives: filter inside `read_dir` (rejected: re-reads disk per keystroke);
  no `all` copy (rejected: clearing the filter would need a reload).
- Filter mode adds a small input state (a `filtering: bool` flag or a dedicated
  mode) so printable keys append to the filter instead of firing global actions.

### Layout is state-dependent

Header `Length(1)`, Files `Min(5)`, progress `Length(3)` only while serving,
Log `Min(3)`, help line `Length(1)`. Progress uses ratatui `Gauge` with a speed
figure derived from consecutive progress samples and their `Instant`s (omitted on
the first sample, when there is no delta).

## Risks / Trade-offs

- [Existing app tests encode the old contract] → `q_requests_quit`,
  `edits_are_inert_while_the_server_runs`, and `cancel_restores_the_normal_help_line`
  must be updated for confirmation and the new picker routing; the render harness
  stays and new scenarios are added.
- [`FileList::get` signature change ripples into `server.rs` and its tests.] →
  Mechanical fix, done in the same task as the entry struct.
- [Chunk-granular progress floods the channel.] → Throttle to whole-percent
  changes plus a final event.
- [Heuristic log coloring mislabels lines.] → Keep it a presentation-only concern;
  promoting to typed levels is a later, isolated change.
- [Speed and ETA need samples and can read jumpy.] → Show speed only after a
  second sample; omit ETA if it proves noisy.
- [Very small terminals clip the added regions.] → Constraint math uses `Min`
  floors; a dedicated minimum-size screen is out of scope for this change.

## Migration Plan

Internal binary; no data or protocol migration. The `ServerUpdate` signature
change is source-level and fully contained in `crates/core` + `crates/app`, with
tests updated in the same change. No rollback steps beyond reverting the commit.
