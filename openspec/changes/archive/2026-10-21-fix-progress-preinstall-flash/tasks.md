# Tasks

## 1. Server: streamed-sequence tracking

- [x] 1.1 In `crates/core/src/server.rs`, replace the single `serving: Option<String>` announcement slot with per-file stream state (`HashMap` keyed by file name, holding the streamed position, an announced flag, and a participating flag for the in-flight range) and verify `cargo build -p dbi-backend-core` compiles with existing tests untouched
- [x] 1.2 Implement the participation rule in `process_file_range`: a range participates when it begins at offset zero without a streamed position, or continues exactly at the file's current streamed position; on participation, update the position, keep the throttled `Progress` emission and the once-per-sequence `Log("Sending: <file>")` announcement exactly as today, and report `Completed` once when the position reaches the file size; verify with a new test that a streaming phase from offset 0 to the file's end behaves as the current tests for `file_range_reports_throttled_progress` expect
- [x] 1.3 Gate auxiliary reads to silence: a range that neither starts at offset zero nor continues the streamed position streams unchanged (headers and bytes identical) but emits no `Progress`, no `Completed`, no announcement, and leaves the streamed position untouched; verify with a test where a trailing range ending at the file's last byte produces no completion report while the response bytes still match the requested range
- [x] 1.4 Cover the reported DBI preparation sequence in `crates/core/tests/server_test.rs`: offset-0 read (announce + progress), contiguous continuation, trailing auxiliary read at the file end (silent, no completion), continuation from the paused position (still participates), restart at offset zero (announced once, progress restarts), sequential ranges to EOF yielding exactly one completion; run `cargo test -p dbi-backend-core --test server_test`

## 2. TUI: backward samples start a new streamed phase

- [x] 2.1 In `App::update_progress_at` (`crates/app/src/app.rs`), reset the speed estimate, ETA, smoothed rate, and speed-sample baseline when a sample for the same file reports a lower transferred count than the previously shown count, keeping the speed/ETA rules of unchanged samples intact; verify with the existing unit tests in the app tests module and new cases: `update_progress_at` with a backward sample yields `speed: None`/`eta: None`, and a following forward sample re-measures speed from the new baseline
- [x] 2.2 Confirm the region handling stays correct for phase restarts by extending one existing app test: after a full-file sample sequence reaching the total, a new sample at offset 0 shows the region at the new position (clearing, not carrying, the previous fill) and the "Sent:" summary still appears only from an explicit `Completed` event; run `cargo test -p dbi-backend`

## 3. Cross-layer verification

- [x] 3.1 Simulate the end-to-end event flow of the reported defect (trailing preparation read followed by a restart-at-zero install) through `App::handle_server_event` and assert the region never shows a 100% fill before any install bytes flow and shows exactly one completion summary, via a test that drives `Server` events into an `App`; run `cargo test -p dbi-backend progress` and `cargo test --workspace`
- [x] 3.2 Run the project checks: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, and fix any fallout in the touched files

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied (`openspec archive fix-progress-preinstall-flash`), then optionally re-test against real hardware with DBI.
