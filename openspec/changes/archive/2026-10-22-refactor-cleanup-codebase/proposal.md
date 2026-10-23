# Proposal

## Why

The backend grew one feature at a time — the TUI, the path picker, the share
form, the file filters, the title metadata — and each one left a layer of code
that nothing calls any more. `cargo clippy --all-targets -- -D warnings` and
rustc's dead-code lint are both clean, so none of this is machine-detectable:
it takes a reading of the code to find that 652 lines of test code have never
been compiled, that `crates/app/src/app.rs` has grown to 4291 lines holding
five unrelated concerns, and that the same directory walk, the same
share-path arithmetic and the same share-validation rules are each written two
or three times in slightly different shapes.

The next feature would land on top of that. A cleanup pass now — before the
next change is opened — keeps that change cheap and keeps the parts that
already work from drifting further apart.

## What Changes

- **The never-compiled tests become part of the test suite.** The `test-util`
  cargo feature in `crates/core/Cargo.toml` is declared but enabled by nobody,
  so `crates/core/src/source/testutil.rs` (363 lines) and the `add_source_tests`
  module in `crates/core/src/filelist.rs` (289 lines) have never been compiled
  or run. The feature is dropped and the block is compiled into the crate's own
  test build, which makes the spec requirements "Adding a share directory
  scans it over the share", "Directory scans stay finite" and "A directly added
  share file ignores extension filtering" verified for the first time — the ten
  tests in that block pass when the feature is switched on.
- **`crates/app/src/app.rs` is split by concern.** Its 1825 production lines
  hold the application state, the key handling, the rendering, the background
  server task and the formatting helpers in one file; the 2466 lines of tests
  make it 4291 lines in total. The file becomes a directory of modules
  (`app/state.rs`, `app/input.rs`, `app/ui.rs`, `app/server.rs`,
  `app/format.rs`) with the tests kept in one place.
- **Dead code is removed.** The command id `CMD_ID_LIST_OLD`
  (`crates/core/src/protocol.rs`), the never-called `Source::join` and
  `Source::parent`, the test-only `App::new` and its `allow(dead_code)`,
  the write-only `Action::Connect.selection` field, the discarded return value
  of `App::share_added`, the `App::server_started` field that is exactly
  `status != Idle`, the app-local `picker::Kind` enum that duplicates core's
  `EntryKind`, and the unreachable "defensive replace" branch in
  `App::share_added`.
- **Duplicated implementations are consolidated.** The three hand-rolled
  "centered hint" renderers and the three hand-rolled table builders in the
  TUI become one helper each; the share validation rules, written once for the
  config file and once for the add-share form, become one; the three ways
  `FileList` puts an entry into its map become one; the reconnect-once
  scaffolding in `ShareHub::{open, list_dir, stat}` becomes one; the
  share-path arithmetic moves to `SmbLocation` in core and the app's copies
  are deleted; the version split, the file-extension extraction and the
  in-memory `FileHandle` and container fixtures are each written once.
- **Behaviour does not change.** Every requirement in the `cli-tui`,
  `file-list`, `filters`, `nsp-metadata`, `path-picker`, `protocol`, `server`,
  `smb` and `transport` specs keeps its current wording and its current
  observable behaviour: the same files are offered to the Switch, the same
  bytes are streamed, the same keys do the same things, the same messages are
  logged, and the same configuration is read. Nothing is added to the
  command line, the config file or the DBI0 wire protocol. This is a pure
  refactor, so the change declares `skip_specs: true`.

## Capabilities

### New Capabilities

- None. This change introduces no new system behaviour.

### Modified Capabilities

- None. No requirement in `cli-tui`, `file-list`, `filters`, `nsp-metadata`,
  `path-picker`, `protocol`, `server`, `smb` or `transport` changes its
  wording or its observable behaviour; the change is a pure refactor and
  declares `skip_specs: true`.

## Impact

- `crates/core/Cargo.toml`: the unused `test-util` feature is removed.
- `crates/core/src/source/mod.rs`: the `testutil` module is compiled into the
  crate's test build instead of behind a feature nobody enables; `Source::join`
  and `Source::parent` are replaced by `SmbLocation::join` and
  `SmbLocation::parent_path`.
- `crates/core/src/filelist.rs`: the three entry-insert paths become one; the
  share-scan tests become part of the suite.
- `crates/core/src/source/smb.rs`: the three reconnect-once call sites become
  one; `ShareHub::is_connected` becomes test-only.
- `crates/core/src/server.rs`: `Server::process_file_range` is split into
  named steps; the duplicated source clone and the per-request payload
  allocation go away.
- `crates/core/src/protocol.rs`: the unused command id is deleted.
- `crates/core/src/filters.rs`, `crates/core/src/metadata/*`: the duplicated
  version split, extension extraction and test fixtures are shared.
- `crates/app/src/app.rs` becomes `crates/app/src/app/` with the modules
  listed above; `crates/app/src/picker.rs` uses core's `EntryKind` and
  `SmbLocation`; `crates/app/src/config.rs` validates a share in one place.
- No dependency is added, removed or upgraded. No change to the command line,
  to the config file format, to the DBI0 protocol, or to the bytes streamed to
  the Switch.
