# Proposal

## Why

The confirmation modal for quitting (`q`) and clearing the list (`c`) is sized as
a percentage of the whole terminal (60% wide, 20% tall). On a typical terminal
that produces a large, mostly empty box that dominates the screen for a
one-line prompt, so the dialog reads as disproportionately huge.

## What Changes

- Render the quit/clear confirmation as a compact, content-fitted box centered
  on screen, sized to the prompt text plus a small border/padding, instead of
  60% x 20% of the terminal.
- Give the confirmation its own sizing helper so the large percentage-based
  geometry remains available for the help overlay only.
- No keybinding, confirmation semantics, or non-UI behavior changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cli-tui`: the "Confirmation for destructive actions" requirement gains a
  presentation constraint - the confirmation prompt SHALL be shown in a compact,
  content-fitted centered dialog rather than a fixed fraction of the terminal.

## Impact

- `crates/app/src/app.rs`: `render_confirm` sizing and a small content-fit rect
  helper; existing confirmation tests remain valid in intent.
- Specs: delta file for `cli-tui`.
- No new dependencies.
