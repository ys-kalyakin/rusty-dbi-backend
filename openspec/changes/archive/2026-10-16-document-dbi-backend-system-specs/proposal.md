# Proposal

## Why

The system's behavior is currently documented only in
`docs/superpowers/specs/2026-10-16-dbi-backend-design.md`, while the OpenSpec
main specs cover just the `file-list` capability. OpenSpec should be the single
source of truth for what the backend does, so the whole system is captured as
capabilities.

## What Changes

- Add OpenSpec capability specs describing the shipped behavior of the DBI
  protocol framing, the server command loop, the USB transport, and the CLI/TUI.
- This is a documentation change: it records existing behavior as requirements.
  It does not change code, the wire protocol, or the CLI.

## Capabilities

### New Capabilities
- `protocol`: DBI0 message framing (16-byte little-endian header, magic,
  command types/ids), file-range header parsing, and LIST payload encoding.
- `server`: the USB command loop and how it answers `EXIT`, `LIST`, and
  `FILE_RANGE`, including how file bytes are streamed and errors are handled.
- `transport`: the bulk-endpoint contract (read exactly N bytes, write all
  bytes) and the USB device discovery/claim behavior.
- `cli-tui`: command-line path arguments, the terminal UI layout and
  keybindings, and the background server/reconnect lifecycle.

### Modified Capabilities
<!-- none: file-list already has a main spec and its behavior is unchanged -->

## Impact

- New spec files under `openspec/specs/{protocol,server,transport,cli-tui}/`
  after sync. `openspec/specs/file-list/` is unchanged.
- No code, dependency, or runtime changes.
- Supersedes the behavior sections of
  `docs/superpowers/specs/2026-10-16-dbi-backend-design.md` as the canonical
  description; the design doc remains as historical implementation design.
