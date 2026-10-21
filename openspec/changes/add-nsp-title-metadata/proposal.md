# Proposal

## Why

The file list names every offered file and nothing else. A library of dumped
titles looks like `[0100000000010000].nsp`, `[0100000000010800].nsp`,
`[0100000000011001].nsp` — from the list alone there is no way to tell a base
game from its update from its DLC, and no way to search for a title whose file
name says nothing. The container already carries that identity: its title id,
its content type and its version sit in the archive's own metadata, and the
backend simply does not read it.

## What Changes

- The backend reads the identity of every offered NSP/NSZ file: the title id,
  the content type (base game, update or DLC) and the version.
- The identity comes from the container's own metadata where the container
  exposes it, and otherwise from the file-name convention every dump tool
  uses (`Name [0100000000010000][v65536].nsp`).
- The human-readable title name is **not** read from the container. Nintendo
  keeps it in the content archive's header, which is encrypted with a key only
  the console holds; reading it would mean asking the user for console keys.
  The name shown is the one the file already carries.
- The identity is read once, when the file is added, and is kept with the
  entry. It never depends on the file still being readable when the Switch
  asks for it: an install must not fail because a title's metadata could not
  be read.
- The TUI shows the identity next to each file.
- Not breaking: the files offered to the Switch keep their names, their order
  and their bytes. The identity is extra information for the user.

## Capabilities

### New Capabilities

- `nsp-metadata`: how a title's identity — title id, content type, version —
  is read from an NSP/NSZ container, how it is derived when the container
  does not expose it, and what is deliberately not read.

### Modified Capabilities

- `file-list`: an entry carries the identity of its file when one could be
  determined, and reading it never keeps the file from being added.
- `cli-tui`: the file list shows each file's identity next to its name.

## Impact

- `crates/core/src/metadata/`: a new module reading a container's metadata —
  the archive's own table of contents, the content meta inside it, and the
  file-name convention.
- `crates/core/src/filelist.rs`: the identity is read when a file is added and
  stored with the entry; the read goes through the existing source
  abstraction, so a file on a share is read the same way as a local one.
- `crates/app/src/app.rs`: the file list renders the identity.
- `crates/app/src/config.rs`: no new configuration.
- No change to the DBI protocol, to the offered file names, or to the bytes
  streamed to the Switch.
