# Design

## Context

See proposal.md. The shipped behavior is implemented in `crates/core`
(`protocol.rs`, `server.rs`, `transport/`) and `crates/app` (`cli.rs`,
`app.rs`, `main.rs`), and is described prose-only in
`docs/superpowers/specs/2026-10-16-dbi-backend-design.md`. The main specs hold
only `file-list`.

## Goals / Non-Goals

**Goals:**
- Record the observed behavior as OpenSpec capabilities without changing it.
- Keep capability boundaries aligned with the existing module layout.

**Non-Goals:**
- Changing any code, wire format, or CLI surface.
- Adding requirements for behavior the code does not already have.

## Decisions

### One capability per module boundary

Split into `protocol`, `server`, `transport`, and `cli-tui`, mirroring
`protocol.rs`, `server.rs`, `transport/`, and the app crate. `file-list` is
already a capability and is left unchanged. This keeps each spec cohesive and
mappable to one area of the codebase.

Alternative considered: a single system capability. Rejected because OpenSpec
capabilities should own related requirements as the system evolves, and a
catch-all spec would be hard to deltas against later.

### Record observed behavior, not aspiration

Requirements reflect what the code does today (verified by reading the sources
and their tests), so the synced main specs describe the system as shipped.
Known reference points: `protocol.rs`, `server.rs`, `transport/mod.rs`,
`transport/usb.rs`, `app.rs`, `main.rs`.

## Risks / Trade-offs

- [Prose spec may drift from code over time] → Future behavior changes go
  through OpenSpec changes, which update these capabilities as deltas.
- [WebUSB details (packet sizing, endpoint ids) are hard to test without
  hardware] → Captured as transport requirements; the mock transport keeps the
  server tests hardware-free.
