# Proposal

## Why

Shares can today only be added by hand-editing `~/.config/dbi-backend/config.toml`; the
share list's empty state even instructs the user to do exactly that, and the previous
change (`add-smb-share-support`) explicitly deferred adding shares from inside the TUI.
An in-TUI add form closes the loop from "no shares configured" to connecting without
leaving the app, while keeping the existing rule that a password is never stored and is
asked at every connection.

## What Changes

- New `a` key inside the share list opens a compact add-share form with six fields:
  `name`, `server` (`host` or `host:port`), `share`, `username` (required), and optional
  `domain` and `root`. The form HAS NO password field and never asks for one; connecting
  still asks for the password as it does today.
- Inline validation on submit: required fields non-empty, `server` parses as
  `host[/:port]`, and the display name is not already used by another configured share.
  Invalid data keeps the form open with the errors shown.
- Submitting valid data persists the share to the config file under `[[shares]]` (written
  atomically) and makes it immediately usable in the same run: it appears in the share
  list, and connecting to it works right away with the usual per-connection password.
- A write failure is logged as a warning; the share still stays available for the current
  run, and nothing else in the config file is lost.
- Esc cancels the form and discards everything typed.
- The empty share list's hint now names the add key so the form is discoverable; the
  share list view's hints include it.
- Out of scope: editing or removing existing shares, a `--share` CLI flag, passwords in
  any file, and OS mounting.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cli-tui`: new share-add form view opened from the share list with `a` — its fields,
  inline validation, submit/cancel semantics, and key hints; the empty share list hint
  gains the add key.
- `smb`: shares added through the UI SHALL be saved to the config file without passwords,
  SHALL be usable at once in the same run, and SHALL NOT lose the file's other content.

## Impact

- `crates/app/src/config.rs`: gains config **writing** — serialize the `[[shares]]` list
  and save the file (preserving unrelated top-level file content); until now config.rs
  only read.
- `crates/app/src/app.rs`: new `Mode::ShareForm` view with per-field buffers, focus
  movement, inline validation, and the submit action returning to the share list
  highlighting the added share; updated share-list and hint rendering.
- `crates/core/src/source/smb.rs`: `ShareHub` gains a way to register a share config after
  startup lists are shared between the app and the hub, so a share added mid-run is
  connectable in that run.
- No new dependencies: the `toml` crate used for parsing also serializes. No changes to
  `protocol`, `transport`, `file-list`, `server`, or `path-picker` requirements.
