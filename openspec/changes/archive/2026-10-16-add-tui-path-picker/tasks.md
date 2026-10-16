# Tasks

## 1. Picker browsing module

- [x] 1.1 Create `crates/app/src/picker.rs` declaring `Entry` (name + `Dir`/`File` kind) and `Picker` state (`dir`, `entries`, `cursor`, optional `error`), and register `mod picker;` in `crates/app/src/main.rs`; verify `cargo build -p dbi-backend` compiles
- [x] 1.2 Implement opening at `std::env::current_dir()` and loading a directory via `read_dir`, classifying entries (following symlinks via `metadata`, skipping entries whose metadata fails) and sorting directories before files case-insensitively; verify with a unit test over a `tempfile` directory containing subdirectories and files
- [x] 1.3 Implement `move_up`/`move_down` that are clamped to the listing bounds, and verify unit tests for both bounds (first/last entry)
- [x] 1.4 Implement `descend` for the highlighted directory (reloads entries and resets cursor) and verify with a unit test that descends into a subdirectory
- [x] 1.5 Implement `go_up` to the parent directory and verify unit tests that go up from a subdirectory and stay put at the filesystem root
- [x] 1.6 Represent an unreadable current/displayed directory as a stored error with an empty listing, and verify with a unit test that opening a non-existent directory records an error without panicking

## 2. App integration and keybindings

- [x] 2.1 Replace `App::input: Option<String>` with a `Mode` enum (`Normal` / `Picker(Picker)`) and update `App::new`; verify `cargo build -p dbi-backend` compiles
- [x] 2.2 Route `handle_key` through the picker when it is open — `j`/Down and `k`/Up move, Enter descends a directory or selects a file, Space selects the highlighted entry, Backspace/Left/`h` go up, Esc/`q` cancel — and verify unit tests for descend, go-up, cancel, and select (directory adds recursively, file adds directly, both close the picker)
- [x] 2.3 Make `a` open the picker in Normal mode (replacing the typed-path prompt) and remove the text-input handling, verifying via unit tests that `a` enters the picker and that `c`/`s`/`q` behave as before only when the picker is closed
- [x] 2.4 Reuse `FileList::add_path` for selection and log the outcome (success or failure) while closing the picker; verify with a unit test that a selected `.nsp` file and a selected directory both appear in the file list afterward

## 3. Rendering and documentation

- [x] 3.1 Render the picker listing as a stateful ratatui `List` with the cursor highlighted and the hint line `Esc cancel  Enter open  Space add  Bksp up`; verify the rendered lines change when the cursor moves via a rendering unit test
- [x] 3.2 Restore the normal file-list/help view when the picker closes; verify with a rendering unit test that the help line returns after cancel
- [x] 3.3 Update the README key legend to describe `a` opening the picker and the picker keys; verify the documented keys match `handle_key`

## 4. Integration verification

- [x] 4.1 Run `cargo test` and confirm all tests pass
- [x] 4.2 Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` and fix any findings
- [x] 4.3 Manually run `dbi-backend` in a directory and confirm browsing, descending, going up, selecting a directory, selecting a file, and cancelling all work as specified

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived result.
