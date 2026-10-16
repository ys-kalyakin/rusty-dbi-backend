# Tasks

## 1. Compact confirmation dialog

- [x] 1.1 Add a fixed-size centering helper in `crates/app/src/app.rs` that centers a given width/height within an area, clamping both to the area with `saturating_sub`; verify with a unit test that a normal area centers correctly and a tiny area clamps without panicking
- [x] 1.2 Rewrite `render_confirm` to size the dialog from the prompt text (`width = prompt.len() + 2`, `height = 3`), clamped to the frame area, using the new helper instead of `centered_rect`; verify `render_confirm` no longer references `centered_rect` for the confirm path and the app compiles
- [x] 1.3 Add a `TestBackend` test asserting the confirm dialog's rendered bounds are the prompt width plus border and 3 rows tall (not the previous 60%-wide box) for both quit and clear, and that the prompt text is still visible; verify with `cargo test -p dbi-backend`
- [x] 1.4 Keep the existing confirmation behavior tests (`q_opens_confirmation_and_y_quits`, `q_confirmation_can_be_cancelled`, `c_requires_confirmation`, `c_confirmation_can_be_cancelled`) green, and confirm the help overlay still uses `centered_rect`; verify with `cargo test -p dbi-backend`

## 2. Integration verification

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --workspace` and confirm all pass with no new warnings

## Workflow follow-up

- Archive the change after review and verification are complete.
