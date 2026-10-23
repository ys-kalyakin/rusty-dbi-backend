# Tasks

## 1. Capped log pane layout

- [ ] 1.1 Add `LOG_PANE_MAX_HEIGHT` (8, with a doc comment naming what fits: the titled border plus up to 6 log lines) beside the other UI constants in `crates/app/src/app/ui.rs`, and change the log region's constraint in `ui()` from `Min(3)` to `Max(LOG_PANE_MAX_HEIGHT)`; verify `cargo test -p dbi-backend` still passes, including the existing `log_shows_the_newest_lines` (its 40×12 render now draws a 7-row log).

## 2. Layout regression tests

- [ ] 2.1 Add a buffer test asserting the tall-terminal split: rendering the normal view (with several files and log lines) on a 40-row terminal shows the log block at most 8 rows high and the Files block taking every remaining row; verify the new test passes against the rendered buffer geometry.
- [ ] 2.2 Add a buffer test asserting the short-terminal yield: on a terminal too short for header + 3 primary rows + 8 log rows + help line, the log block is shorter than 8 rows while the Files block keeps at least 3 rows; verify the test pins the behavior the design table records (e.g. 12 rows → 3 primary / 7 log).
- [ ] 2.3 Verify the capped pane in the other modes and with the progress row: one render assertion for the open picker and one for the serving state (progress row present) showing the log block still at most 8 rows; verify both assertions pass.

## 3. Full check

- [ ] 3.1 Run `cargo test`, `cargo clippy --all-targets`, and `cargo fmt --check` for the workspace and verify they all pass.

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived result.
