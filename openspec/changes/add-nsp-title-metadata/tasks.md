# Tasks

## 1. Spike: the container layout, against real files

- [ ] 1.1 Take one or more real NSP files and one or more real NSZ files, and record for each whether the container's table of contents lists a content-meta member, whether that member is readable as plain bytes in an NSZ, and the offsets of the title id, the version and the content type inside it; verify by dumping the header and the member's first bytes and comparing them with the recorded values
- [x] 1.2 Decide from the spike whether the container path yields the identity or only the file-name path does, and record the outcome in the change's design: if the content meta is reachable only through an encrypted archive header, keep the requirements as they are and reduce the implementation to the file-name path, with the container reader kept as a thin, unused, tested reader or dropped from the task list; verify that the decision is written down before task 2 starts

## 2. Core: reading the identity

- [x] 2.1 Add `crates/core/src/metadata/` with a container reader that reads the 16-byte container header, computes the full header size from the file count and the name-table size, refuses a header size larger than the file, reads the header, and finds a named member in the table of contents; verify with unit tests over a hand-built container that the member's offset and size are found, and that a file count or name-table size that would imply an oversized header is refused
- [x] 2.2 Parse the content meta of a member found by the container reader: the title id, the version and the content type, with the version reported as a major/minor/patch triple; verify with unit tests over a hand-built content meta that each field is read, and that a truncated or malformed content meta yields no identity instead of a wrong one
- [x] 2.3 Add the file-name parser for the conventional dump name `Name [TitleId][vVersion].nsp`: the title id from a bracket holding 16 hexadecimal digits, the version from a `v` bracket, the name from everything before the first bracket; verify with unit tests that `Some Game [0100000000010000][v65536].nsp` yields the name `Some Game`, the title id `0100000000010000` and the version 1.0.0, that `game.nsp` yields nothing, that `Game [notes].nsp` yields no title id, and that a name with brackets before the title id is read correctly
- [x] 2.4 Combine the two sources into one identity: the container's values win where they exist, the file name fills the rest, and the content type falls back to the title id's last three digits — `000` a base game, `800` an update, anything else downloadable content; verify with unit tests over each combination of what the container and the file name provide

## 3. Core: the identity on the entry

- [x] 3.1 Add the identity to `FileEntry` and read it when a file is added, through `Sources::open` and the handle's `read_at`, so a file on a share is read the same way as a local one; verify with tests over the mock source that a file whose container is readable gets the identity and that the read went through the source abstraction
- [x] 3.2 Make the identity read best-effort: every failure — not a container, a truncated header, an unparsable content meta, a source that refuses the read — yields an empty identity, keeps the file in the list, and reports no read error; verify with tests for each failure that the file is present and carries no identity, and that the add outcome reports no skipped path
- [x] 3.3 Read the identity only for files whose extension marks them as a container, and skip the read for every other file; verify with a test that a file with another extension is added without any read of its contents

## 4. TUI: showing the identity

- [x] 4.1 Show the title id, the content type and the version next to each file that has an identity, and show the name alone for a file without one; verify with a rendering test that an entry with an identity shows all three and an entry without one shows none of them
- [x] 4.2 Keep the list's columns aligned and the identity readable at the width of a narrow terminal — the identity is dropped or shortened rather than wrapped, and the file name stays readable; verify with a rendering test at a narrow width that the name is not pushed out of the region

## 5. Documentation and verification

- [x] 5.1 Document in `README.md` what the backend shows about a title — the title id, the content type and the version — and state that the title's name comes from the file name because the container keeps the real one behind the console's keys; verify that the README's description matches what the UI shows
- [x] 5.2 Run the project checks — `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` — and fix any fallout in the touched files
- [x] 5.3 Drive the flow end to end: add a directory of titles, confirm each entry carries the identity the spike established, confirm a file that is not a container is added without one, and confirm the offered names and bytes to the Switch are unchanged; verify with a test that drives the file list and the server's list together

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied (`openspec archive add-nsp-title-metadata`), then optionally confirm the identity against a real library.
