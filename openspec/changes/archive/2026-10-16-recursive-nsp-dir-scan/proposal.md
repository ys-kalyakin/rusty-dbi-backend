# Proposal

## Why

Today, adding a directory only picks up its immediate files, so games organized
in subfolders (a common layout for Switch libraries) are invisible to DBI. Users
must add each subfolder by hand, and unrelated sidecar files can end up listed.

## What Changes

- Adding a directory now walks it recursively, including all nested
  subdirectories.
- Only files whose extension is `.nsp` or `.nsz` (case-insensitive) are added
  from a directory scan. Other files are skipped.
- Adding a single file explicitly keeps its current behavior: the file is added
  regardless of extension.
- Empty results are not an error; adding a directory that contains no matching
  files simply adds nothing.

## Capabilities

### New Capabilities
- `file-list`: how paths are discovered and mapped to the names offered to the
  Switch, including directory scanning rules.

### Modified Capabilities
<!-- none: no existing specs in this project -->

## Impact

- `crates/core/src/filelist.rs`: `FileList::add_path` gains recursive traversal
  and extension filtering; existing unit test expecting immediate-files-only
  behavior is replaced.
- `crates/app/src/app.rs` and `crates/app/src/main.rs`: callers are unchanged;
  they inherit the new scanning behavior.
- `docs/superpowers/specs/2026-10-16-dbi-backend-design.md` "File list" section
  is now superseded by the `file-list` spec and should be updated when applied.
- No protocol, CLI, or dependency changes.
