# Proposal

## Why

Game libraries commonly live on a NAS exposed over Samba (SMB), but dbi-backend can only
serve files from the local filesystem, so games must be copied onto the install machine
before they can be installed on the Switch. Teaching the backend to read directly from an
SMB share lets a user install straight from the NAS: no copies, no root, no mounts.

## What Changes

- Add a share configuration file at `~/.config/dbi-backend/config.toml` listing named
  shares (`name`, `server`, `share`, `username`, optional `domain` and `root`). Passwords
  are never stored in the config.
- Add an SMB file source: the backend speaks SMB2/3 to the configured shares through a
  pure-Rust client crate (`smb2`, with `smb` as fallback after a spike against a real
  server), behind a source abstraction that also covers local files.
- Make file-list entries carry a source (local path or share location) so the server can
  stream `FILE_RANGE` bytes from either, including ranged (seek) reads on a share.
- Add lazy share connection in the TUI: a new key `S` lists the configured shares, asks
  for the password in a masked modal input, connects, and opens the path picker inside
  the share root (browsing and recursive adding follow the existing picker semantics).
- Ask for the share password each time a share is connected; keep the password in memory
  for the rest of the application run so a dead SMB session can be re-established
  transparently the next time a file on that share is opened.
- Keep current failure semantics: a read failure while streaming ends the connection as
  it does today ("Connection lost"); retrying mid-range is out of scope.
- Show a share-qualified path (e.g. `nas:/dir/file.nsp`) as the resolved path of
  share-sourced files in the file list.
- Out of scope for this change: retrying within a streamed range after a failure, editing
  the share configuration from inside the TUI, a `--share` CLI flag, and OS mounting.

## Capabilities

### New Capabilities

- `smb`: Configuring Samba shares, connecting to them lazily with a per-run password, and
  browsing and reading their files inside the terminal UI.

### Modified Capabilities

- `file-list`: entries gain a source (local path or share location); adding a share
  location scans it with the existing recursive scan and extension-filter semantics.
- `server`: `FILE_RANGE` opens the named file through the entry's source rather than the
  local filesystem only, and re-establishes a dropped SMB session when opening.
- `cli-tui`: new `S` keybinding with its share-list and password-modal views, locked while
  the server is active like the other editing keys, plus share-qualified paths in the
  file list display.

## Impact

- `crates/core`: new source abstraction (`open`/`read`/`list_dir`) wrapping `std::fs`
  today and SMB tomorrow; `FileEntry` and `Server` consume it. No protocol or USB
  transport changes.
- `crates/app`: config loading, the `S` flow (share list, password modal), and the picker
  listing files through the source abstraction.
- New dependencies: `smb2` (pure-Rust SMB2/3 client, tokio-based), `toml` + `serde` for
  the config, and a config-directory locator; all pure Rust, no FFI.
- Spec-level behavior of `file-list`, `server`, and `cli-tui` changes; `protocol`,
  `transport`, and `path-picker` requirements are unaffected.
