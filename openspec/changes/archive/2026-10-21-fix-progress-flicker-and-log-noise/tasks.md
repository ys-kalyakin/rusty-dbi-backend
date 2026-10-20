# Tasks

## 1. Server: per-file transfer reporting

- [x] 1.1 In `crates/core/src/server.rs` `process_file_range`, track the file currently being streamed on the connection and, when a range names a different file, write `ServerUpdate::Log("Sending: {name}")` before streaming it; verify with a core test that one range of one file yields exactly one `Sending:` line and no per-range log lines.
- [x] 1.2 In `process_file_range`, send `ServerUpdate::Completed { name, bytes: file_total }` only when a fully streamed range reaches the file's end (`range_offset + range_size >= file total`) and clear the tracker then, so a later re-read of the same file re-announces `Sending:`; verify with a core test that two consecutive mid-file ranges plus one end-of-file range produce exactly one completion naming the file with its total size, and that a subsequent range re-announces the start.
- [x] 1.3 Update the existing core tests that pin one completion per streamed range (`crates/core/tests/server_test.rs`) to the per-file cadence and run `cargo test -p dbi-backend-core` to confirm they, the throttled-progress test, and the unchanged failure-logging tests all pass.

## 2. App: single-row progress region

- [x] 2.1 In `crates/app/src/app.rs` `ui`, reserve a single `Constraint::Length(1)` row for the progress region (down from `Length(2)`) and update the layout tests that pinned the two-row split so `cargo test -p dbi-backend-app` locates the statistics and the bar on the same row.
- [x] 2.2 Reshape `render_progress` to split the single row horizontally after measuring the statistics spans (file name, percent, sent/total, speed, ETA), render the paragraph first and the unlabeled success-colored gauge on the remainder; verify with render tests that the full bar keeps the name readable on the same row and that a narrow area prioritizes the text over the bar.

## 3. App: stable progress and per-file log cadence

- [x] 3.1 Keep progress clearing in `App::handle_server_event` on `Completed`, `Disconnected`, `Ended` and the defensive `Started` clear only, and verify with state/render tests that consecutive `Progress` updates of the same file keep the region shown without layout churn while a completed file hides it and logs exactly one success `Sent: {name} ({size})` line.
- [x] 3.2 Verify the new `Sending: {name}` log line flows from the server task to the log in the default text color (extend the log-color unit tests alongside the existing success-color test for `Sent:`).

## 4. Final verification

- [x] 4.1 Run `cargo test` for the whole workspace and confirm the core and app tests, including the integration tests, pass.
- [x] 4.2 Smoke-check via `TestBackend` at wide and narrow terminal sizes that the single-row progress region renders text plus bar and that no test or layout pin of the two-row region remains; a live USB transfer still requires a Switch for final visual confirmation of the non-flickering behavior.

## Workflow follow-up

- Archive the change once the accept criteria above are met (`/opsx-archive`) and verify the archived result updates `openspec/specs/cli-tui` and `openspec/specs/server`.
