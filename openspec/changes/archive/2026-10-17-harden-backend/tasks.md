# Tasks

## 1. Protocol: strict FILE_RANGE parsing

- [x] 1.1 Require `buf.len() >= HEADER_LEN + name_len` and decode exactly that slice as UTF-8, returning `None` on a short name or invalid UTF-8; verify with new unit tests for both cases running under `cargo test -p dbi-backend-core`
- [x] 1.2 Confirm the existing round-trip, field-parse, and short-buffer tests still pass so no valid payload is rejected

## 2. File list: resilient scan and replacement reporting

- [x] 2.1 Make `insert`/`add_file` return the path they replaced; verify a unit test where adding a duplicate name reports the previous path
- [x] 2.2 Make directory scans continue past an unreadable entry or subdirectory and collect the skipped paths, returning an outcome of added/skipped/replaced; verify a test with an unreadable subdirectory still adds the readable files
- [x] 2.3 Update `add_path` callers (startup loading and picker selection) to log skipped and replaced entries as warnings; verify with a test asserting a replaced name produces a warning

## 3. Server: failed FILE_RANGE ends the connection

- [x] 3.1 Add a server error for an unservable range and return it (after logging) on unknown file, open/seek failure, and short read; verify integration tests assert the run ends on each case
- [x] 3.2 Confirm the happy-path streaming, chunk-splitting, and oversized-header tests still pass unchanged

## 4. CLI/TUI: lifecycle, edit freeze, log tail

- [x] 4.1 Report server lifecycle as events from the background task and apply them in the UI loop, clearing running status and `server_started` when the run ends; verify an app test that applies an end event returns the app to idle and allows `s` again
- [x] 4.2 Make `a` and `c` inert while the server is running; verify app tests that the list is unchanged while running and editable again after the run ends
- [x] 4.3 Render the log's most recent lines so new output is visible when the log overflows; verify a render test where the newest line is present and the oldest is not
- [x] 4.4 Update the README key notes to state that `a`/`c` are disabled while the server runs

## 5. USB: reuse transfer buffers

- [x] 5.1 Reuse transfer buffers while streaming instead of copying each chunk; verify the server integration tests pass unchanged over the mock transport

## 6. CI

- [x] 6.1 Add `.github/workflows/ci.yml` running `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --workspace`; verify locally that all three commands pass

## 7. Integration verification

- [x] 7.1 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --workspace` on the full workspace and confirm all pass
- [x] 7.2 Smoke-test the TUI: start with files, press `s`, confirm edits are blocked while running, and confirm status returns to idle after the run ends

## Workflow follow-up

- Archive the change after review and a successful integration run.
