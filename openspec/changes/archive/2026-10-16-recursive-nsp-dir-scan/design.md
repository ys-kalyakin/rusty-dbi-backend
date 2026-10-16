# Design

## Context

See proposal.md. The discovery logic lives entirely in
`crates/core/src/filelist.rs::add_path`, which today branches on
`path.is_file()` and, for a directory, reads only immediate entries. The
`FileList` stores a `BTreeMap<String, PathBuf>`, so listing order is already
deterministic and independent of traversal order. The callers
(`app.rs`, `main.rs`) pass a path and surface any `io::Error` as a message.

## Goals / Non-Goals

**Goals:**
- Recursively discover `.nsp`/`.nsz` files under an added directory.
- Keep the existing `add_path(&Path) -> io::Result<()>` signature so callers
  and the UI are untouched.

**Non-Goals:**
- Changing the DBI protocol, CLI flags, or TUI behavior.
- Configurable extension lists or ignore-pattern support.

## Decisions

### Iterative traversal instead of recursion

Walk directories with an explicit `Vec<PathBuf>` work stack rather than a
recursive helper. This avoids unbounded call-stack growth on deeply nested
libraries and keeps the error path in one loop.

Alternative considered: recursive function. Rejected for stack-depth risk with
no readability benefit at this size.

### Extension matching via `Path::extension`

Match by taking `path.extension()`, converting to lowercase, and comparing
against `"nsp"`/`"nsz"`. This uses the platform's path parsing instead of
string suffix checks, and satisfies the case-insensitive requirement with one
`to_ascii_lowercase()`.

Alternative considered: `to_string_lossy().to_lowercase().ends_with(...)`.
Rejected because it treats `game.nsp.bak` inconsistently and does redundant
allocation.

### Directory entries decide recursion; files are filtered

For each entry, use its file type: recurse into directories, and apply the
extension filter only to regular files. The explicitly-added path keeps its
current branch — a file path is added directly with no filter, matching the
`file-list` spec.

### Symlinked directories are not followed

To prevent infinite loops from cyclic symlinks, do not descend into a directory
entry that is itself a symlink; symlinked regular files are still added if they
resolve to a file. This is a small, safe deviation from the current behavior,
which follows any entry that `is_file()`.

## Risks / Trade-offs

- [Large trees slow the synchronous add] → Traversal is bounded by the file
  system and runs once per add; acceptable for a local game library. If it
  becomes a problem, filtering could use an ignore of non-target directories,
  tracked separately.
- [Unreadable nested directory aborts the whole add] → Traversal propagates the
  first `io::Error`, preserving the existing single-error-message behavior. Best-
  effort skipping can be added later if users hit permission errors.
- [Name collisions across subdirectories overwrite silently] → Already existing
  `FileList` semantics and documented in the `file-list` spec; unchanged here.
