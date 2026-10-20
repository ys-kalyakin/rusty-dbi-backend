# Tasks

## 1. Core: quiet the per-request logging, report completions

- [x] 1.1 Remove the "File range" and "Range Size / Range Offset / Name len / Name" log
      calls from `process_file_range` in `crates/core/src/server.rs`, keeping every
      failure log (oversized header, unknown file, failed open, failed seek, read
      failure, short transfer); verify `cargo test -p dbi-backend-core` still passes.
- [x] 1.2 Add `ServerUpdate::Completed { name: String, bytes: u64 }` to
      `crates/core/src/server.rs` and emit it once after a range has been streamed
      successfully, with the file name and the number of bytes streamed for that range;
      verify a new test in `crates/core/tests/server_test.rs` asserts exactly one
      completion update carrying the file name and the byte count.
- [x] 1.3 Add a core test asserting that a successful range emits no log line
      describing the request itself, and that the failure paths still emit their log
      lines; verify `cargo test -p dbi-backend-core` passes.

## 2. App: one summary line per completed transfer

- [x] 2.1 Add `ServerEvent::Completed { name: String, bytes: u64 }` to
      `crates/app/src/app.rs` and map `ServerUpdate::Completed` onto it in
      `spawn_server`; verify the app still compiles with `cargo build`.
- [x] 2.2 Handle the completion event in `App::handle_server_event`: clear
      `self.progress` and push exactly one log line `Sent: {name} ({human_size(bytes)})`;
      verify a new unit test asserts the log holds that single line and that the
      progress region is gone from the rendered screen.
- [x] 2.3 Color log lines starting with "Sent:" with the success color in `log_color`;
      verify a new unit test asserts the summary line renders in the success color.

## 3. App: quiet the connection retry loop

- [x] 3.1 Extract the connection-log policy out of `spawn_server` into a helper in
      `crates/app/src/app.rs` that decides whether "Waiting for Switch..." and
      "USB error: {e}" should be logged, with no I/O of its own; verify the helper is
      unit-testable without a device attached.
- [x] 3.2 Drive the helper from `spawn_server` so the waiting state is logged once per
      waiting period and a failing connection is logged once while the same error
      persists, with the remembered error cleared after a successful connection;
      verify `cargo build` succeeds and the log volume of the retry loop is bounded.
- [x] 3.3 Add unit tests for the helper: repeated attempts with the same error yield a
      single error line, a changed error yields a new line, and a successful connection
      resets the state so the next waiting period logs again; verify
      `cargo test -p dbi-backend` passes.

## 4. Workspace verification

- [x] 4.1 `cargo test` passes for the whole workspace.
- [x] 4.2 `cargo clippy --all-targets -- -D warnings` reports no warnings.
- [x] 4.3 `cargo fmt --check` reports no diffs.
