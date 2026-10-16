# Tasks

## 1. Verify specs against the shipped code

- [x] 1.1 Cross-check `specs/protocol/spec.md` against `crates/core/src/protocol.rs` and its tests; confirm framing, command ids, file-range fields, LIST payload, and chunk size match
- [x] 1.2 Cross-check `specs/server/spec.md` against `crates/core/src/server.rs` and `crates/core/tests/server_test.rs`; confirm dispatch, EXIT/LIST/FILE_RANGE behavior, and error handling match
- [x] 1.3 Cross-check `specs/transport/spec.md` against `crates/core/src/transport/mod.rs` and `transport/usb.rs`; confirm the read/write contract, discovery ids, and packet alignment match
- [x] 1.4 Cross-check `specs/cli-tui/spec.md` against `crates/app/src/{cli,app,main}.rs`; confirm CLI paths, log cap, keybindings, and server lifecycle match

## 2. Sync and validate main specs

- [x] 2.1 Sync the delta specs into `openspec/specs/{protocol,server,transport,cli-tui}/spec.md` and verify `openspec validate --specs` passes

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived capabilities are present under `openspec/specs/`.
