# Proposal

## Why

The log pane and the primary region (the file list, the path picker, or the
share list) currently split the terminal's free vertical space roughly in half,
because both are `Min(3)` constraints. A tail of recent log lines needs only a
few rows, so on a typical terminal the log occupies far more height than its
content justifies while the file list — the information the user actually
browses — is squeezed.

## What Changes

- Cap the log pane's on-screen height at a small fixed maximum (8 rows
  including its titled border, so up to 6 visible log lines).
- Give every remaining vertical row to the primary region (file list, picker,
  or share list), in every mode and with or without the progress row.
- On terminals too short for the cap, the log pane yields its height first,
  shrinking below the cap, while the primary region keeps its minimum height.
- Log behavior is otherwise unchanged: 1000 retained lines, newest lines
  visible, timestamps and level colors as today.

Assumption recorded: "more useful information" means the primary region's
content; this change adds no new panes or data — it only redistributes height.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cli-tui`: the Terminal UI layout requirement gains an on-screen height cap
  for the log region and a rule for how free rows are distributed between the
  log and the primary region.

## Impact

- `crates/app/src/app/ui.rs`: the vertical layout constraints in `ui()`
  (the log's `Min(3)` becomes a capped constraint).
- `crates/app/src/app/tests.rs`: layout assertions for the capped log pane and
  the primary region taking the freed rows.
- No changes to `crates/core`, configuration, or the server; no breaking
  changes to any interface.
