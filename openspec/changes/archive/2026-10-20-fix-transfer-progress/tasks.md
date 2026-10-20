# Tasks

## 1. Progress lifecycle correctness

- [x] 1.1 In `App::handle_server_event`, clear `progress` on `ServerEvent::Disconnected` and as a defensive guard on `Started`, keeping the existing clears on `Completed` and `Ended`; verify with a unit test that `Progress` → `Disconnected` → `Started` leaves no progress state and no progress region rendered until another `Progress` arrives.
- [x] 1.2 Verify the completed-transfer and server-ended paths still clear the region and that a completed range still writes exactly one `Sent:` summary line; run the affected app tests.

## 2. Smooth speed and ETA estimation

- [x] 2.1 Add private rate-sampling fields to `ProgressState` and a timestamp-taking update path so tests can supply deterministic sample times; implement a 100 ms minimum sample interval and a time-weighted exponential blend (tau = 2 s) over the previous smoothed rate, resetting it for a new file; verify unit tests show uneven samples produce a rate between the old and new instantaneous values and that a new file resets speed/ETA.
- [x] 2.2 Derive the displayed `eta` from the smoothed rate, preserving `None` before a speed is known and carrying the previous estimate when no bytes advance; verify tests cover first-sample absence, stable ETA with uneven intervals, zero remaining time at completion, and reset on file change.

## 3. Readable two-row progress region

- [x] 3.1 Reserve two rows for the progress region and split it into a text row plus an unlabeled success-colored `Gauge`; render `Sending {name}`, percentage, `sent/total`, speed, and optional ETA in the text row using theme styles, with the name first so it survives narrow widths; verify a render test finds the file name on a different row from the block-character bar.
- [x] 3.2 Verify the name remains readable when the bar is full by asserting the name cell's background is not the success fill and the bar row contains no file-name text; verify the bar still advances from reported bytes and the region disappears on completion/disconnect.

## 4. Workspace verification

- [x] 4.1 Run `cargo test` for the workspace and confirm all app, core, and integration tests pass, including the updated progress and layout tests.
- [x] 4.2 Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` with no findings.
- [x] 4.3 Smoke-check the progress region through `TestBackend` at wide and narrow terminal sizes; note that a live USB transfer still requires a Switch for final visual confirmation.

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived result.
