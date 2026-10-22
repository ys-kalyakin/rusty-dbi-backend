# Tasks

## 1. Core: make the unreachable tests reachable

- [x] 1.1 Change `crates/core/src/source/mod.rs:6` to gate `pub mod testutil` with `#[cfg(test)]` instead of `#[cfg(feature = "test-util")]`, and change `crates/core/src/filelist.rs:753` to gate `mod add_source_tests` with `#[cfg(test)]`; verify `cargo test -p dbi-backend-core` now reports 118 passing unit tests instead of 104 and that the ten `filelist::tests::add_source_tests::*` tests are among them
- [x] 1.2 Remove the `test-util` feature from `crates/core/Cargo.toml`; verify `cargo build --release` still succeeds, `cargo test --workspace` is green, and `grep -rn "test-util" crates/ .github/` finds nothing
- [x] 1.3 Confirm the three spec requirements that had no coverage are now exercised — "Adding a share directory scans it over the share", "Directory scans stay finite" and "A directly added share file ignores extension filtering" — by checking that the corresponding `add_source_tests` cases exist and pass; verify by running `cargo test -p dbi-backend-core add_source_tests`

## 2. Core: remove the dead protocol and source items

- [x] 2.1 Delete `CMD_ID_LIST_OLD` from `crates/core/src/protocol.rs:11`; verify `cargo clippy --all-targets -- -D warnings` is clean and `crates/core/tests/server_test.rs` still passes, including the case that sends an unknown command id
- [x] 2.2 Add `SmbLocation::join(&self, name: &str) -> SmbLocation` and `SmbLocation::parent_path(&self) -> Option<String>` to `crates/core/src/source/mod.rs`, with `parent_path` yielding `None` at the share root and `Some("")` for a top-level entry's parent; verify with unit tests that `join` yields `name` at the root and `{path}/{name}` below it, and that `parent_path` yields `None` for the empty path, `Some("")` for `"a"` and `Some("dir")` for `"dir/a"`
- [x] 2.3 Delete `Source::parent` from `crates/core/src/source/mod.rs` and make `Source::join` delegate its share arm to `SmbLocation::join`, so the share-path arithmetic exists once; verify `cargo clippy --all-targets -- -D warnings` reports no dead code and the whole workspace still compiles

## 3. Core: one reconnect path in the share hub

- [x] 3.1 Replace the three copies of the ensure-live / try / rebuild-once sequence in `crates/core/src/source/smb.rs` (`list_dir` at `:179-195`, `open` at `:200-216`, `stat` at `:219-235`) with one helper that takes the session call as an async closure; verify the existing `source::smb::tests` still pass, in particular the cases that reconnect a stale session and that report a dropped connection
- [x] 3.2 Delete the unreachable "is not a share" error arm in `entry_for_source` (`crates/core/src/source/smb.rs:273-281`) now that `share_path` fails first with the same message; verify `cargo clippy --all-targets -- -D warnings` is clean and that the "sources the hub cannot serve report clean names" test still passes
- [x] 3.3 Make `ShareHub::is_connected` (`crates/core/src/source/smb.rs:162-172`) test-only, since only the hub's own tests call it; verify the hub's tests still compile and pass and that `cargo clippy --all-targets -- -D warnings` is clean

## 4. Core: one way to put an entry in the file list

- [x] 4.1 Extract the map insertion shared by `FileList::insert` (`crates/core/src/filelist.rs:56-69`), `FileList::insert_source` (`:73-89`) and `FileList::record` (`:211-241`) into one private helper, keeping `insert` and `insert_source` free of canonicalisation and of replacement reporting and keeping `record`'s canonicalisation and replacement reporting exactly as they are; verify the filelist unit tests and `crates/core/tests/server_test.rs` still pass, including the duplicate-name replacement case
- [x] 4.2 Confirm that adding an entry whose name already exists still reports the replacement and that `FileList::insert` used by the integration tests still overwrites silently; verify with the existing "Duplicate names overwrite" case and with `crates/core/tests/server_test.rs`

## 5. Core: share the helpers that were written twice

- [x] 5.1 Add one `split_version(raw: u32) -> (u8, u8, u8)` in `crates/core/src/metadata/mod.rs` and use it from `cnmt.rs:52-56` and `filename.rs:80`; verify the metadata unit tests still decode the same title ids and versions
- [x] 5.2 Share one untrimmed `extension_of` between `crates/core/src/filters.rs:165-169` and `crates/core/src/metadata/mod.rs:69-75`, keeping the trimming `normalize_extension` (`filters.rs:157-162`) for the config side only, and share the accepted-extension set so `is_container_file` and the filter defaults cannot drift apart; verify the filter tests still accept `GAME.NSP` and `Patch.NsZ` and still reject a name with a trailing space in its extension
- [x] 5.3 Replace the two identical in-memory `FileHandle` fakes (`crates/core/src/metadata/mod.rs:111-137` and `crates/core/src/metadata/container.rs:183-208`) with one shared fake, and the two identical content-meta builders (`metadata/mod.rs:160-171` and `cnmt.rs:76-87`) with one; verify the metadata and container unit tests still pass
- [x] 5.4 Replace the five hand-written PFS0 container builders (`crates/core/src/metadata/container.rs:213-293` and `crates/core/src/metadata/mod.rs:140-157`) with one parameterised builder; verify the container unit tests still pass, including the UTF-16 and the padded-table cases

## 6. Core: split the file-range handler and drop its excess copies

- [x] 6.1 Split `Server::process_file_range` (`crates/core/src/server.rs:129-263`) into named steps for the acknowledgement, the range-header parse, the stream-state decision, the streaming loop and the completion; verify `crates/core/tests/server_test.rs` still passes in full, in particular the six-range preparation sequence, the throttled-progress case and the re-announce case
- [x] 6.2 Stop cloning the `FileEntry` and then its `Source` again (`crates/core/src/server.rs:153` and `:157`): borrow the entry, clone the `Source` once and copy the size out; verify the server integration tests still pass
- [x] 6.3 Reuse one buffer on the `Server` instead of allocating and zeroing up to 1 MiB per `FILE_RANGE` request (`crates/core/src/server.rs:207`); verify the multi-chunk streaming test and the short-transfer test still pass and that only the bytes the read produced are written to the transport
- [x] 6.4 Change `build_list` (`crates/core/src/protocol.rs:105-113`) to take borrowed names and `FileList::names` (`crates/core/src/filelist.rs:338-340`) to yield `&str`, so the list command no longer materialises a `Vec<String>` of every name; verify the "list command sends file names then exits" test still passes with the same payload

## 7. App: split the application module

- [x] 7.1 Turn `crates/app/src/app.rs` into the directory `crates/app/src/app/` with `mod.rs`, `state.rs`, `input.rs`, `ui.rs`, `server.rs`, `format.rs` and `tests.rs`, moving the code as it stands; verify `cargo build`, `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are all green and that every line of the original file is accounted for in the new modules — a one-to-seven split is reported by git as a delete and seven adds, not as renames
- [x] 7.2 Make `App::started_at`, `App::server_started`, the methods the key handlers and the tests share (`move_selection`, `clamp_selection`, `remove_selected`, `update_progress_at`), the share form's field constants and the formatting and layout helpers visible to the sibling modules (`pub(crate)`); verify the compiler accepts the split and `cargo clippy --all-targets -- -D warnings` is clean
- [x] 7.3 Re-export `App`, `Action`, `ServerEvent`, `ui` and `spawn_server` from `crates/app/src/app/mod.rs` so `crates/app/src/main.rs` needs no change; verify `cargo build` succeeds and `crates/app/src/main.rs` is unchanged by the split

## 8. App: remove the dead state

- [x] 8.1 Delete `App::new` and its `allow(dead_code)` (pre-split `app.rs:285-288`), and change the test module's call sites to the `no_shares` helper calling `App::with_shares(files, Vec::new())`; verify the app's tests still pass and that `cargo clippy --all-targets -- -D warnings` finds no dead code
- [x] 8.2 Delete the `selection` field from `Action::Connect` (`crates/app/src/app.rs:52`) and the write that fills it, and rewrite the one test that asserted it to assert the share-list selection the app actually sets; verify the app's tests still pass
- [x] 8.3 Drop the return value of `App::share_added` (`crates/app/src/app.rs:935`) and delete its unreachable "defensive replace" branch (`:941-947`), which `config::build_share` already rules out; verify the app's tests still pass; adding a same-named entry needs no replacement because the form refuses a duplicate name
- [x] 8.4 Delete `App::server_started` (`crates/app/src/app.rs:278`) and derive it from `status != ServerStatus::Idle` at the four read sites (`:691`, `:732`, `:1476` and the test at `:2711`); verify the app's tests still pass, in particular the one asserting the server task is active
- [x] 8.5 Make the `App::search` and `App::searching` fields private so the getters (`crates/app/src/app.rs:611-618`) are the only way to read them, and drop the `Clone` derive from `Action` and the `PartialEq` derives that nothing compares; verify the app's tests still pass and `cargo clippy --all-targets -- -D warnings` is clean

## 9. App: one helper for each repeated piece of the UI

- [x] 9.1 Add one `render_centered_hint(title, text, area)` helper and use it for the three centred hints (`crates/app/src/app.rs:1322-1337`, `:1340-1358` and the empty-share-list branch at `:1545-1574`); verify the rendering tests for the no-match hint, the empty-file-list hint and the empty-share-list hint still pass
- [x] 9.2 Add one `render_table(title, rows, widths, selected, area)` helper and use it for the three tables (`crates/app/src/app.rs:1309-1318`, `:1529-1538` and `:1600-1607`), keeping each table's own widths, rows and clamped index; verify the rendering tests for the file list, the picker and the share list still pass
- [x] 9.3 Add one `plural(count, singular, plural)` helper and use it for the file count, the share count and the picker's entry and match counts (`crates/app/src/app.rs:1249-1253`, `:1500-1510` and `:1590-1594`); verify the rendering tests still show the same wording for one and for many

## 10. App: one way to handle the repeated key cases

- [ ] 10.1 Replace the two copies of the printable-character guard (`crates/app/src/app.rs:835` and `:883`) with one helper, keeping the accepted set identical — every character that is not a control character, exactly as today; verify the share-form and password-modal tests still accept the same characters
- [ ] 10.2 Replace the four copies of the `j`/`k` and arrow-key movement arms with one clamped-movement helper for the file list, the search input and the share list (`crates/app/src/app.rs:487-493`, `:723-729`, `:783-790` and `:804-811`); verify the cursor tests for the file list, the share list and the picker still pass, in particular the "stops at the last row" and "clamped at the top" cases

## 11. App: validate a share in one place

- [ ] 11.1 Extract the shared share rules — required fields, the default port 445, the `host:port` split, the empty domain and the trimmed root — out of `config::parse_share` (`crates/app/src/config.rs:48-82`) and `config::build_share` (`:102-158`) into one function, keeping each caller's own error shape (`String` versus `Vec<String>`) and its own message wording; verify the config tests and the share-form tests still assert the same messages
- [ ] 11.2 Add the named constant for the default SMB port, used by `parse_share`, `build_share` and the share setup (`crates/app/src/config.rs:93`, `:130`, `:136` and `:251`), replacing the four bare `445` literals; verify the share tests still connect to the default port

## 12. App: let the picker use the shared core types

- [ ] 12.1 Delete the app-local `picker::Kind` enum (`crates/app/src/picker.rs:11-16`) and the mapping at `:168-171`, and use `dbi_backend_core::source::EntryKind` throughout the picker; verify the picker tests and the app's picker tests still pass
- [ ] 12.2 Replace `join_share` (`crates/app/src/picker.rs:357-363`) and `parent_share_path` (`:367-375`) with `SmbLocation::join` and `SmbLocation::parent_path` from core, keeping the picker's "go up lands on the share root" behaviour for a top-level entry; verify the picker tests for going up from a subdirectory and for going up at the share root still pass
- [ ] 12.3 Delete the `Picker::origin_display` pass-through (`crates/app/src/picker.rs:224-226`) and call `Origin::display` directly from its one caller; verify the picker and app tests still assert the same location strings

## 13. Final verification

- [ ] 13.1 Run the project's checks — `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` — and confirm all three are clean; verify by running each command and reading its output
- [ ] 13.2 Confirm the observable behaviour is unchanged: the same files are offered to the Switch for the same inputs, the same bytes are streamed for the same range requests, the same keys do the same things, the same messages are logged and the same configuration is read. Verify by running the whole test suite with no test changed or skipped except the three call sites named in task 4.1, 8.1 and 8.2, and by diffing the logged messages and the rendered rows against the pre-change behaviour
- [ ] 13.3 Confirm nothing user-facing moved: the command line, the config file format, the DBI0 wire protocol and the README's documented keys and options are unchanged. Verify by reading `crates/app/src/cli.rs`, `crates/app/src/config.rs` and `crates/core/src/protocol.rs` against their pre-change contents, and by checking that `README.md` needs no edit

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied
  (`openspec archive refactor-cleanup-codebase`).
- The analysis that motivated this change also found behavioural problems that
  are deliberately **not** fixed here, because each one changes what a user
  sees and needs its own decision. They are recorded here so they are not
  lost:
  - The accepted-extension set and the include patterns are intersected
    (`crates/core/src/filters.rs:110-131`), so `--ext xci` together with
    `include = ["*.nsp"]` accepts nothing.
  - The picker follows directory symlinks (`crates/app/src/picker.rs:385-400`)
    while the scanner refuses to descend into them
    (`crates/core/src/source/local.rs:96-99`), so the picker can walk a cycle.
  - The share password is cloned twice on reconnect
    (`crates/core/src/source/smb.rs:319-320`) and only one of the copies is
    wiped.
  - Log severity is re-derived from the message prefix at render time
    (`crates/app/src/app.rs:1753-1766`) instead of being carried by the log
    line.
  - The container metadata reader allocates up to 16 MiB per parsed container
    (`crates/core/src/metadata/container.rs:82`) and, for the containers a dump
    tool writes, never finds anything.
