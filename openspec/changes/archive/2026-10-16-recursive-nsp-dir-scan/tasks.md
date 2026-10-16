# Tasks

## 1. Recursive filtered directory scan

- [x] 1.1 Replace the directory branch of `FileList::add_path` in `crates/core/src/filelist.rs` with an iterative work-stack walk that recurses into subdirectories, adds only regular files whose extension is `.nsp`/`.nsz` case-insensitively, and leaves direct file paths unfiltered; verify with `cargo test -p dbi-backend-core filelist`
- [x] 1.2 Update/replace the `filelist.rs` unit tests: nested `sub/deep/b.nsz` is found, non-matching files (`.txt`, `.jpg`) are skipped, `GAME.NSP`/`Patch.NsZ` match, a directly added `readme.txt` is included, and an empty directory adds nothing; verify `cargo test -p dbi-backend-core` passes
- [x] 1.3 Update the "File list" section of `docs/superpowers/specs/2026-10-16-dbi-backend-design.md` to describe recursive `.nsp`/`.nsz` discovery; verify the description matches the behavior implemented in 1.1

## 2. Integration verification

- [x] 2.1 Run `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check`, and confirm all pass with no new warnings

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived `file-list` spec matches the shipped behavior.
