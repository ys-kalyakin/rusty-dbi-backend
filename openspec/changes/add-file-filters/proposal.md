# Proposal

## Why

Which files end up in the list offered to the Switch is decided by two
hard-coded rules: a directory scan accepts only `.nsp` and `.nsz` files
(`has_target_ext` in `crates/core/src/filelist.rs`), and a directly added file
is always accepted. There is no way to narrow that down, and no way to search
the list once it is built — the picker has a filter, the main file list does
not. A library that holds a base game, an update, a DLC and a repack with the
same file name cannot be curated: the user has to reorganize the disk to
control what the console sees.

## What Changes

- The set of extensions a directory scan accepts becomes configurable. The
  default stays `.nsp` and `.nsz`, so today's behavior is unchanged for
  existing users.
- Directory scans gain include and exclude patterns, matched against the
  file name, so a subset of a directory can be offered (for example, exclude
  every `*PATCH*` file or offer only `*.nsp`).
- The file list in the TUI gains a search filter that narrows the *displayed*
  rows. It does not change which files are offered to the Switch: the list
  handed to the Switch keeps every file the scan accepted.
- Filters are configurable from the command line and from the user's config
  file; the command line wins over the config file, and the config file wins
  over the built-in defaults.
- A scan reports how many files it accepted and how many the filters
  rejected, so an empty result is distinguishable from a scan that found
  nothing.
- A directly added file keeps the current rule and is added regardless of its
  extension and of the include/exclude patterns: an explicitly named file is
  an explicit request.
- Not breaking: with no filter configured, the accepted set, the offered
  names, and the DBI-visible wire behavior are exactly what they are today.

## Capabilities

### New Capabilities

- `filters`: which files the backend offers and how that choice is
  configured — the accepted extension set, the include and exclude patterns,
  where the settings come from, and how a pattern is matched.

### Modified Capabilities

- `file-list`: directory scans accept only the files the filters accept, over
  the local filesystem and over a share alike; a directly added file stays
  unfiltered.
- `cli-tui`: the command line accepts filter options, and the file list can
  be narrowed by a search string without changing the set of files offered to
  the Switch.

## Impact

- `crates/core/src/filelist.rs`: the scan consults the filter settings; the
  rules for a directly added file stay as they are.
- `crates/app/src/cli.rs`: new options for the extension set and the
  include/exclude patterns.
- `crates/app/src/config.rs`: a `[filters]` section in
  `~/.config/dbi-backend/config.toml`, read with the same tolerance as the
  shares (a malformed entry warns and the rest of the file is kept).
- `crates/app/src/app.rs` / `crates/app/src/picker.rs`: the search filter over
  the file list, its key handling, and the file-list rendering.
- `Cargo.toml`: a glob matcher for the include/exclude patterns.
- No change to the DBI protocol, to the offered file names, or to the bytes
  streamed to the Switch.
