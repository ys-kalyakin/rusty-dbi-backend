# Tasks

## 1. Theme foundation

- [x] 1.1 Create `crates/app/src/theme.rs` with the palette constants and `Style` helpers from design D1 plus a `block_title(title)` helper returning a rounded `Block`; register the module. Verify: `cargo build` succeeds and `cargo test -p dbi-backend-app` still passes (no behavior change yet).
- [x] 1.2 Swap every hardcoded color in the existing render functions (`render_header`, `render_files`, `render_progress`, `render_log`, `render_help`, `render_picker`, `render_confirm`, `status_color`, `log_color`) to theme references and route blocks through `block_title`. Verify: `cargo test -p dbi-backend` passes with no test edits — the theme must map idle→gray, running→green, warnings→yellow, errors→red, so the pinned-color tests survive. (Package name corrected from `dbi-backend-app`; the app crate is `dbi-backend`.)

## 2. Status lifecycle and header

- [x] 2.1 Extend `ServerStatus` with `Waiting` (design D2): `s` now sets Waiting, `ServerEvent::Started` → Running, `Ended` → Idle; add the `ServerEvent::Disconnected` variant, emit it from the `spawn_server` error path before the retry sleep, and handle it in `main.rs`. Verify: new unit tests — press `s` → Waiting; Started → Running; Disconnected sent after a simulated loss → Waiting; Ended → Idle; `a`/`c`/`d` stay inert in both Waiting and Running.
- [x] 2.2 Render the header as a horizontal split (design D3): left paragraph with application name, status capsule (`idle` / `waiting…` with the elapsed-clock spinner) and schema colors per theme; right paragraph with `N files · total`. Update the pinned `status_header_is_colored_by_state` test to the three-state colors. Verify: render tests assert the app name, the waiting/running/idle words and the counters, and never the spinner glyph.

## 3. File list table and empty state

- [x] 3.1 Replace the `List` with a 3-column `Table` per design D4 (Name `Min(0)`, Size `Length(10)` right-aligned, muted Path) keeping the `>` highlight symbol and the emphasized row style. Verify: the existing `files_list_shows_sizes_and_total` test still passes; add a buffer test that the size column is right-aligned for same-prefixed files of different length.
- [x] 3.2 Add the `abbrev_path(path, max_width)` mid-truncation helper with unit tests (fits / needs truncation / degenerate tiny width) and use it for the Path column. Verify: helper tests pass and a render test shows a truncated narrow-path view at small widths.
- [x] 3.3 Show the centered onboarding hint inside the Files block when the list is empty and the picker is closed (design D5). Verify: render test — empty app shows the hint naming `a`; with files present the hint is absent.

## 4. Progress bytes and ETA

- [x] 4.1 Extend `ProgressState` with `eta: Option<Duration>` computed in `update_progress` from the existing windowed speed (design D6). Verify: unit test — first sample has no eta; a second sample on the same file yields eta > 0; a newer file resets eta to None.
- [x] 4.2 Extend the gauge label with `sent/total` in human units and, when eta is known, an eta in `mm:ss`/`h:mm:ss`; keep the gauge omitted entirely when no progress. Update the `progress_event_updates_and_clears` test. Verify: render test — with samples known the label contains the size pair and the eta; before speed is known it contains no eta; after `Ended` the region is cleared.

## 5. Picker upgrades

- [x] 5.1 Add `size: Option<u64>` to `picker::Entry`, filling it from metadata in `reload` (`None` for directories), and render the picker as the same 3-column `Table` shape with directories styled in the accent color (design D7, adjusted to a 2-column Name+Size shape — see design D7 note). Verify: existing picker tests pass unmodified; added tests — file rows show `human_size`, directory rows show an empty size cell.
- [x] 5.2 Append the entry count to the picker title, switching to the match count while a filter is active. Verify: tests — with no filter the title contains `N entries`; with a filter it contains `M matches` matching the filtered length.

## 6. Hint line and popups polish

- [x] 6.1 Render the hint line as keyed chips (key span in accent, label span muted) and set `DIM` on the `a`/`d`/`c` chips while `server_started` (design D5). Keep the hint text content identical. Verify: a buffer test asserts the muted fg styles on disabled chips while the task is active and full-color styles when idle, alongside the existing content checks (e.g. `cancel_restores_the_normal_help_line`).
- [x] 6.2 Restyle the confirm dialog and help overlay with the themed rounded block, keep sizes/centering/behavior; update the `confirm_dialog_is_compact_for_both_prompts` color assertion to the themed value. Verify: the compact-dialog tests pass with the new color and unchanged width/height assertions.

## 7. Full verification

- [x] 7.1 Run `cargo test` (workspace) and confirm all suites pass, including the updated pinned-visual tests.
- [x] 7.2 Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`.
- [x] 7.3 Smoke-run `cargo run` against a temp directory: confirm the empty-state hint, header name/counts, picker sizes/counts, chips and dimmed disabled keys after `s`, and the waiting state (no device needed); note any transfer-dependent visuals to verify with a Switch. (Executed via python3 pty capture: header name/counts, chips, waiting state and inert `a` confirmed live; picker sizes/counts confirmed live; empty-state hint, chip dimming and confirm	dialog visuals are covered by TestBackend render tests; gauge/ETA colors need a Switch.)

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived result.
