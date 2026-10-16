# Design

## Context

See `proposal.md` - Why. The confirmation modal is rendered by `render_confirm`
in `crates/app/src/app.rs`, which calls the percentage-based `centered_rect(60,
20, area)` helper. The same helper also sizes the help overlay (70% x 60%), so it
must keep working for that caller. The prompt strings are short and ASCII-only
("Quit dbi-backend? [y/N]", "Clear the file list? [y/N]").

## Goals / Non-Goals

**Goals:**
- Render the quit/clear confirmation as a compact, centered, content-fitted box.
- Keep the prompt text and confirmation key semantics identical.
- Keep the result testable through the existing `TestBackend` buffer tests.

**Non-Goals:**
- Changing the help overlay or any other UI region.
- Changing keybindings, confirmation logic, or non-UI behavior.
- Introducing theming or reusable dialog infrastructure.

## Decisions

**Do not use the percentage-based helper for the confirmation.** Build the
confirm rect directly from the prompt size: `width = prompt.len() + 2` (one
column of border each side, plus the block's own padding) and `height = 3`
(top border, one text line, bottom border), then clamp both to `area` and center
the result. Rationale: the content is a single short line, so a terminal
fraction always wastes space on wide terminals. Alternative considered:
shrinking the percentages (e.g. 40% x 10%) - rejected because it is still
terminal-relative and still large on a wide terminal.

**Add a fixed-size centering helper and leave `centered_rect` intact.** A small
helper computes the top-left offset from the clamped width/height and the area,
using `saturating_sub` so tiny terminals degrade to a clamped, still-visible
dialog rather than a panic or negative offset. `centered_rect` stays for the
help overlay.

**Measure width with the string length.** Prompts are ASCII, so byte length
equals display width; no Unicode-width dependency is added. A fixed border of 1
per side is assumed, matching `Block::default().borders(Borders::ALL)`.

## Risks / Trade-offs

- [A very long future prompt could exceed the terminal width] -> Clamp width to
  `area.width`; the `Paragraph` with a block border already clips gracefully.
- [Tiny terminal yields a 0- or 1-height rect] -> `saturating_sub` keeps the
  rect valid; content may be clipped but the app does not panic.
- [Duplicated centering logic alongside `centered_rect`] -> Accepted; the two
  helpers express different intents (fraction vs. fixed) and the fixed one is a
  few lines.
