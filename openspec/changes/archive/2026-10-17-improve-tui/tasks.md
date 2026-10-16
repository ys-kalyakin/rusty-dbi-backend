# Tasks

## 1. Core: file sizes

- [x] 1.1 Change `FileList` to store a name-keyed entry `{ path, size }`, computing size from `std::fs::metadata` in `add_file` (fallback `0` when unavailable), and update `get`/`iter`/`names` callers; verify with `cargo test -p dbi-backend-core filelist::tests`
- [x] 1.2 Add `filelist` tests asserting a known-size file reports that size and that size is available without re-reading disk; verify the new tests pass
- [x] 1.3 Update `server.rs` to read the entry's `path`; verify `cargo test -p dbi-backend-core` passes

## 2. Core: typed server updates and progress

- [x] 2.1 Introduce a typed `ServerUpdate { Log(String), Progress { name, sent, total } }` and change `Server::run` to take it instead of `FnMut(String)`, mapping existing log calls to `Log`; verify `cargo test -p dbi-backend-core` passes with `server_test.rs` updated
- [x] 2.2 Emit `Progress` from `process_file_range` using the file's `metadata().len()` and the running sent byte count, throttled to whole-percent changes plus one final event; verify a test with a fake transport asserts the progress sequence

## 3. App: selection, sizes, and status legibility

- [x] 3.1 Add a `selected` cursor to `App` with `j`/`k`/arrows movement and clamping, render the Files widget with a `ListState` that scrolls to the highlight, and clamp the cursor after add/remove/clear; verify render tests cover highlight movement and scrolling
- [x] 3.2 Implement `d` to remove the highlighted file, inert while the server runs; verify tests cover removal and the locked-while-running case
- [x] 3.3 Show each file's size in the Files list and the total size in the header; verify a render test asserts the size and total text
- [x] 3.4 Color the status header by state and add timestamps plus severity emphasis to log lines; verify a render test asserts colored status and a timestamped warning line

## 4. App: confirmation and help

- [x] 4.1 Add a `Confirm` mode requiring confirmation for `q` and `c`, cancellable without effect; verify tests cover confirm and cancel for both keys
- [x] 4.2 Add a `Help` overlay toggled by `?` while the picker is closed and dismissable with `?`/`Esc`; verify tests cover open and close
- [x] 4.3 Update existing tests that assumed unconfirmed quit/clear; verify the full `cargo test -p dbi-backend-app` suite passes

## 5. Picker filter

- [x] 5.1 Give `Picker` a full listing, a `filter` string, and a derived visible set recomputed on reload and filter change, with the cursor indexing the visible set; verify picker unit tests cover narrowing, empty match, and clearing
- [x] 5.2 Wire `/` filter input into the picker key handling so printable keys edit the filter and global keys stay inert; verify tests cover filter entry, restore on clear, and suppressed global actions

## 6. App: live progress

- [x] 6.1 Extend the app's `ServerEvent` with a `Progress` variant and translate core `ServerUpdate`s to it in `spawn_server`; verify a test asserts progress state updates on the event
- [x] 6.2 Render a `Gauge` with the current file name, percentage, and post-warmup speed in a state-dependent layout that appears only while serving; verify a render test shows the percentage while running and its absence once idle

## 7. Integration verification

- [x] 7.1 Run `cargo fmt --check` and fix any formatting drift
- [x] 7.2 Run `cargo clippy --all-targets -- -D warnings` and resolve all warnings
- [x] 7.3 Run `cargo test` across the workspace and confirm all tests pass
- [x] 7.4 Manually exercise the TUI on a directory: add, filter, select, remove, confirm quit/clear, and start the server; verify each observable behavior matches the specs

## Workflow follow-up

- Archive the change after review requirements are satisfied.
