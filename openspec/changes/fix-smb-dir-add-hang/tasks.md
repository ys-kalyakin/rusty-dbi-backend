# Tasks

## 1. SMB listings drop the dot pseudo-entries

- [x] 1.1 In `Smb2Session::list_dir` (`crates/core/src/source/smb.rs`), drop entries named exactly `.` or `..` beside the existing `is_link()` filter; re-run the smb unit tests (mocks including a stat/listing-level test) and confirm they pass unchanged.
- [x] 1.2 Teach `MockSession`'s `list_dir` in `crates/core/src/source/smb.rs` to include `.` and `..` the way the real `smb2` crate does, then add a hub-level test asserting `ShareHub::list_dir` on that share returns neither name in any position; verify `cargo test crates/core/src/source/smb.rs` passes with the new test.

## 2. Scanner never revisits through dot entries

- [x] 2.1 In `FileList::scan_dir` (`crates/core/src/filelist.rs`), skip any entry named exactly `.` or `..` (not descended into, not reported as skipped); add a store-based `add_source` test whose listing reports the pseudo-entries as directories and expects the add to terminate with the real matching files; verify `cargo test crates/core/src/filelist.rs` passes.
- [x] 2.2 Add an `add_source` test for a directory whose listing contains only `.` and `..`, expecting an unchanged file list and a normal (non-error) outcome; verify `cargo test crates/core/src/filelist.rs` passes.

## 3. Workspace verification

- [x] 3.1 Run `cargo test` for the whole workspace and confirm app, core, and integration tests pass, including the new dot-filter and scan-termination tests.
- [x] 3.2 Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` with no findings.
- [x] 3.3 Note that live confirmation needs a real share server with a directory to add: attach on a NAS share, press the add key on a directory inside the picker, and confirm the add completes instead of freezing; the UI staying unresponsive during a large finite scan is the documented non-goal from the design.

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived result.
