# Proposal

## Why

Adding a directory from a connected Samba share freezes the whole TUI within
moments and the program dies soon after. The `smb2` crate returns the `.` and
`..` entries in every share listing, the SMB listing layer forwards them as
directories, and the recursive directory scanner descends into them without any
guard — `D → D/. → D/./. → …` never terminates, even for an empty directory.
The unbounded growth of nested paths exhausts memory and the scan runs inline
in the TUI event loop, so everything hangs until the process crashes.

## What Changes

- SMB share listings no longer contain the self and parent pseudo-entries
  (`.` and `..`): `Smb2Session::list_dir` drops them before they reach the
  picker or the scanner, matching what the local filesystem listings already
  look like.
- The recursive directory scanner defensively skips any `.`/`..` entries it may
  still receive from a source, so an add request always terminates regardless
  of which store produced the listing.
- The already-present reparse-point filter (`is_link()`) stays unchanged.
- The add operation remains synchronous (in the TUI event loop); keeping the UI
  animated during very large scans is deliberately out of scope for this fix.
- No protocol, config, or transport changes; share behavior is untouched
  otherwise.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `smb`: add a requirement that share directory listings exclude the `.` and
  `..` pseudo-entries, so share browsing matches the local filesystem behavior.
- `file-list`: add a requirement that recursive directory scans are finite —
  they do not descend into self/parent pseudo-entries, an add of any directory
  terminates, and an add of a directory whose walk revisits the same tree
  cannot hang the program.

## Impact

- **Code**: `crates/core/src/source/smb.rs` (`Smb2Session::list_dir`,
  dropping `.`/`..` next to the existing reparse-point filter) and
  `crates/core/src/filelist.rs` (`scan_dir`, skipping dot entries in addition
  to the extension filter).
- **Tests**: `smb.rs` mocks must learn to return `.`/`..` the way the real
  `smb2` crate does (its `DirectoryReader` documents and asserts this) so the
  quirk is actually covered; new `filelist` tests prove termination.
- **Behavior observed**: the picker stops listing `.`/`..` on shares
  (`.` currently sorts first and is pre-selected), "add" on a share directory
  completes instead of hanging, and memory stays bounded.
- **Not affected**: local paths (std `fs::read_dir` never yields `.`/`..`),
  protocol/server/transport, share connection and reconnection logic.
