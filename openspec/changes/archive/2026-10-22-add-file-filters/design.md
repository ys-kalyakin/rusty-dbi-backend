# Design

## Context

dbi-backend is a two-crate Rust workspace: `crates/core` (DBI0 protocol, server
command loop, file list, sources) and `crates/app` (ratatui TUI, CLI, config).
See proposal.md for the motivation; the requirement set lives in the spec
deltas (`filters`, `file-list`, `cli-tui`).

The facts that shape the approach:

- The scan rules live in `crates/core/src/filelist.rs`. `has_target_ext` decides
  what a "matching file" is, and it is used twice: by the local directory walk
  (`add_path`) and by the share walk (`scan_dir` via `add_source`). Both walks
  must keep agreeing on what a scan accepts.
- A directly added file is added without any filtering today
  (`add_path` → `add_file` before the walk starts). That rule is deliberate
  and stays.
- `crates/app/src/cli.rs` holds exactly one field: `paths: Vec<PathBuf>`.
  There are no options yet, so this change introduces the first ones.
- `crates/app/src/config.rs` parses `~/.config/dbi-backend/config.toml`, which
  today holds only `[[shares]]` entries. The parser keeps a malformed share
  entry from hiding the other shares; the same tolerance is wanted for a
  malformed filter entry.
- The picker already has a substring filter (`Picker::push_filter` and the
  `/` key). The main file list has none, and `App::selected` is an index into
  `FileList::names()` — the BTreeMap's key order — which is what
  `move_selection`, `clamp_selection`, `remove_selected` and `render_files`
  all work against.

## Goals / Non-Goals

**Goals:**

- One filter definition, applied identically to the local walk and the share
  walk, and to the picker's adds.
- The defaults reproduce today's behavior exactly: a user who configures
  nothing sees the same offered set, the same names, and the same bytes.
- The set of files offered to the Switch is decided by the filters alone; a
  display filter never changes it.
- A filter that rejects everything is visible in the log instead of looking
  like an empty directory.

**Non-Goals:**

- Filtering by size, date, or content; regular expressions; per-directory
  rules; rules that differ between the local filesystem and a share.
- Filtering the picker's *listing* — the picker still shows a directory's
  contents, and a directly added file is still unfiltered.
- Renaming the offered files, deduplicating them, or changing the order the
  Switch receives them in.

## Decisions

### 1. A `Filters` value in `crates/core`, held by the `FileList`

```
// crates/core/src/filters.rs
pub struct Filters {
    extensions: BTreeSet<String>,     // without the dot; default {nsp, nsz}
    include: Vec<GlobMatcher>,         // empty = no include rule
    exclude: Vec<GlobMatcher>,         // empty = no exclude rule
}
```

`FileList` gains a `filters` field; `FileList::new()` keeps the defaults and
`FileList::with_filters(filters)` takes a configured set. The scan consults
`self.filters` in both walks.

Why held by the list rather than passed per call: `add_path` and `add_source`
are called from the CLI startup path, from the picker, and from the share
listing flow; threading a filter value through each call site means every
caller must remember to pass the same one, and the list would no longer
describe what it contains. Holding it in the list makes the rule travel with
the data, and the `FileList` is already cloned into the server task, so the
server's snapshot carries the same filters.

Why not filter in the app layer after the scan: the scan already decides
what a matching file is; filtering afterwards would keep the rejected files'
sizes and would leave the share walk and the local walk answering the question
"what does a scan accept?" differently.

### 2. Extensions are matched without the dot, case-insensitively

The accepted set holds bare extensions (`nsp`, `nsz`), so the config file
reads `extensions = ["nsp", "nsz"]` and a value with a leading dot is
normalized away. The comparison is the same case-insensitive one the current
`has_target_ext` performs.

An empty or absent set means the default set rather than "accept nothing": a
half-written config file should not silently empty the offered list.

### 3. Patterns are globs matched against the file name, case-insensitively

The matcher is `globset` (compiled once per pattern, case-insensitive), with
`*`, `?` and character classes. The match target is the *file name*, not the
path.

Why the name and not the path: a path glob reads ambiguously next to a name
glob, and the same pattern would mean different things for a local file
(`/games/x.nsp`) and a share file (`nas:/games/x.nsp`), which is exactly the
kind of surprise the shared scan must not have.

Why `globset` and not `glob`: `glob` compiles a pattern per candidate file,
which a 10 000-file directory scan would pay for 10 000 times. `globset`
compiles the pattern once and matches many candidates.

### 4. Two configuration surfaces, one per setting, CLI first

- Command line: `--ext <EXT>`, `--include <GLOB>`, `--exclude <GLOB>`, each
  repeatable. Repeatable rather than comma-separated, so a pattern is never
  split on a comma it contains.
- Config file: a `[filters]` section with `extensions`, `include` and
  `exclude` keys.

The precedence is per setting, not per source: a command line `--ext` wins
over the config file's `extensions`, while the config file's `exclude` still
applies when the command line says nothing about it. The config file is
parsed with the same tolerance as the shares: a malformed filter entry warns
and the rest of the file — the shares included — is kept.

### 5. The list search is a display filter over the offered list

The search narrows what the file-list region shows and nothing else. The
`FileList` the server receives keeps every file the scan accepted.

The app keeps the search string and derives the visible list from it;
`move_selection`, `clamp_selection` and `remove_selected` work over the
visible list, and the highlight index is clamped whenever the search changes.
`remove_selected` therefore removes the *visible* highlighted file.

The key semantics mirror the picker's, so the two filters feel the same:
`/` starts the search, typing narrows it, Backspace erases, Enter ends the
input and keeps the filter, Esc clears the filter and ends the input. The
search works while the server runs — it is a read-only view — while the
editing keys stay locked.

### 6. The scan reports what the filters rejected

`AddOutcome` gains a count of the files the filters rejected, and the log
line for an add reports the accepted and the rejected counts. Without it, an
over-broad exclude pattern and an empty directory look identical, and the
user has no way to tell that the filters are doing the work.

The rejected count is a count, not a list: a directory of 10 000 files must
not produce 10 000 log lines.

## Risks / Trade-offs

- **[A pattern rejects everything and the list is empty]** → the add outcome
  reports the rejected count, so the log says why the list stayed empty.
- **[The extension set is read as a filter on the picker, but the picker
  still shows everything]** → the picker's help text and the README state
  that the filters apply when a path is added, not to what the picker lists.
- **[The search hides a file the user wanted to remove]** → `d` removes the
  highlighted *visible* file, and clearing the search restores the full list.
- **[The offer and the display disagree]** → the server's snapshot is taken
  from the `FileList`, and the search lives in the app, so the two cannot
  drift into one another.
- **[Glob syntax surprises]** → the patterns are matched against the file name
  only, and the config file's key names say so.

## Migration Plan

No data migration. A config file without a `[filters]` section keeps the
current behavior, and the flags are optional. Rolling back is a matter of
not passing the flags and not adding the section.

## Open Questions

- Should the list search also match a title's identity (its title id or name)
  once the NSP metadata change lands? Deferrable: it widens the search's
  match set without changing this change's rules or its task breakdown.
