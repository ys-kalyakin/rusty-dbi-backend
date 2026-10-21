# Tasks

## 1. Core: the filter rules and the scan

- [x] 1.1 Add `crates/core/src/filters.rs` with a `Filters` value holding the accepted extension set (bare extensions, case-insensitive, default `nsp` and `nsz`) and the include and exclude patterns, plus `Filters::accepts(name)` and the matching rules from the `filters` spec: an excluded name is rejected, a name matching no include pattern is rejected when an include pattern exists, an empty or absent extension set means the default; verify with unit tests covering the default set, a configured set replacing it, an empty set falling back to the default, case-insensitive extension matching, an excluded name, an include-only match, and a pattern that matches nothing
- [x] 1.2 Add the `globset` dependency to the workspace and compile the patterns once per `Filters`, matching the file name case-insensitively; verify with a test that `*`, `?` and a character class match as expected and that a pattern written against a directory path matches no file name
- [x] 1.3 Give `FileList` a `filters` field with `FileList::new()` keeping the defaults and `FileList::with_filters(filters)` taking a configured set, and route the local walk (`add_path`) and the share walk (`scan_dir`) through the same acceptance rule in place of `has_target_ext`; verify with tests that the default filters accept `GAME.NSP` and `Patch.NsZ` and skip `notes.txt`, and that a configured filter set changes what both walks accept
- [x] 1.4 Keep a directly added file unfiltered: a regular file added by path is added regardless of its extension and regardless of the include and exclude patterns; verify with a test that adds `readme.txt` and a pattern-matched name directly and asserts both are present
- [x] 1.5 Add the rejected count to `AddOutcome` and count the files the filters rejected during both walks; verify with tests that a scan of a directory holding 2 accepted and 5 rejected files reports both counts, and that a scan rejecting every file reports 0 accepted and the rejected count

## 2. Configuration: the command line and the config file

- [x] 2.1 Extend `crates/app/src/cli.rs` with the repeatable options `--ext`, `--include` and `--exclude`, parsing them into a `Filters` value, and rejecting an unknown flag as today; verify with CLI tests that the three options parse, that repeating `--ext` accumulates, and that an unknown flag still fails
- [x] 2.2 Extend `crates/app/src/config.rs` with a `[filters]` section carrying `extensions`, `include` and `exclude`, parsed with the same tolerance as the shares: a malformed filter entry is reported as a warning and the remaining shares and filter settings are kept; verify with tests that a valid section is read, that a malformed entry warns without hiding the shares, and that a file without the section yields the defaults
- [x] 2.3 Build the effective `Filters` in `crates/app/src/main.rs` from the defaults, the config file and the command line, with the command line winning per setting: a command-line `--ext` overrides the config file's `extensions` while the config file's `exclude` still applies; verify with a test that builds the effective filters from a config file and a command line and asserts the per-setting precedence
- [x] 2.4 Hand the effective filters to the file list the app starts with, so the startup paths, the picker's adds and the share adds all scan through them; verify with a test that a path added at startup with a configured filter set honors the set

## 3. TUI: searching the offered list

- [x] 3.1 Add the search state to `App` and derive the visible list from the offered list and the search string, matching the file names case-insensitively; verify with tests that a search narrows the visible list, that a search matching nothing shows an empty list, and that clearing it restores every file
- [x] 3.2 Move the highlight, the clamping and the removal onto the visible list: the highlight stays within the visible list's bounds, it is clamped when the search changes, and `d` removes the visible highlighted file; verify with tests that a search hides a file, that `d` then removes the visible file rather than the hidden one, and that the highlight is clamped after the visible list shrinks
- [x] 3.3 Wire the keys in the normal view, mirroring the picker's semantics: `/` starts the search, typing narrows it, Backspace erases, Enter ends the input and keeps the filter, Esc clears the filter and ends the input, and the picker keeps the key for itself; verify with key-handling tests for each of the keys, and that the search works while the server task is active
- [x] 3.4 Show the search state in the UI: the file-list region renders the matching rows and the hint line names the search key while no search is active; verify with a rendering test that the visible rows are the matching ones and that the offered set the server receives is unchanged by an active search

## 4. Reporting and documentation

- [x] 4.1 Log the accepted and rejected counts of every add, so an empty result explains itself; verify with a test that the log line for an add names the accepted and the rejected counts, and that a scan rejecting everything says so
- [x] 4.2 Document the filter options, the `[filters]` config section and the list search in `README.md`, stating that the filters apply when a path is added and not to what the picker lists; verify that every documented option and key matches the implemented names

## 5. Cross-layer verification

- [x] 5.1 Run the project checks — `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` — and fix any fallout in the touched files
- [x] 5.2 Drive a full flow end to end: start with a directory holding a mix of accepted and rejected files, confirm the offered set and the counts, run a search, confirm the offered set is unchanged, start the server and confirm the Switch receives the whole offered set; verify with a test that drives the app's key handling and the server's file list together

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied (`openspec archive add-file-filters`), then optionally confirm the filters against a real library directory.
