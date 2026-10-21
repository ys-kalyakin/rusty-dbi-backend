# Design

## Context

`Smb2Session::list_dir` (`crates/core/src/source/smb.rs:370-392`) maps crate
entries to `ListEntry`s with a single filter: `!entry.is_link()` (reparse
points). The `smb2` 0.34 crate documents that listings come back exactly as
the server maps them — `.` and `..` included (`smb2-0.34.0` `DirectoryReader`
docs and its tests assert `[".", "..", "a.txt"]`). Nothing downstream filters
them either:

- the picker renders every `ListEntry`, and since listings are sorted and
  directories group first, `.` is the first entry shown;
- `FileList::scan_dir` (`crates/core/src/filelist.rs:189-221`) pushes every
  `EntryKind::Dir` child and recurses (`Box::pin`, depth grows unboundedly);
- `Action::Add` is awaited inline in the TUI select arm
  (`crates/app/src/main.rs:70-95`), so the loop neither redraws nor reads input
  until the add finishes.

So a Space-add of any share directory loops `D → D/. → D/./. → …` even for an
empty directory; path strings, nested frames, and the stack grow with each
level, the UI is frozen the whole time, and after some minutes the process
dies. Local paths never hit this because std `fs::read_dir` never yields the
pseudo-entries. This is not a network timeout issue: the `smb2` crate bounds
each wire call (connect 5 s, response 30 s), so the hang is purely the
non-terminating walk.

## Goals / Non-Goals

**Goals:**

- Adds of share directories terminate with a bounded, normal result.
- `.`/`..` never reach any consumer of a share listing (picker, scanner).
- Tests reproduce the real crate quirk so the regression cannot silently
  return.

**Non-Goals:**

- No background/async add, progress indicator, or cancel control for long
  scans — deliberately deferred; the TUI remains unresponsive for the finite
  duration of a large add.
- No change to reparse-point handling (`is_link()` stays), share connection
  or reconnection, config, protocol, CLI, or server code.
- No general cycle detection beyond the dot guards (local symlink loops,
  exotic third mounts) — separate concern if it ever materializes.

## Decisions

1. **Exclude `.`/`..` at the real adapter and at the share-source boundary.**
   `Smb2Session::list_dir` (the single place where the crate's listing
   contract meets ours) drops the pseudo-entries beside the reparse-point
   filter, and `ShareHub::list_dir` — the boundary every share listing
   surfaces through — guarantees the exclusion with `drop_dot_entries`.
   Making both layers carry it does not double the burden: in production the
   hub call sees an already-clean listing, and the guarantee no longer
   depends on which session implementation a factory hands the hub. This
   turned out necessary while implementing task 1.2: the mandated hub-level
   test injects the pseudo-entries through a session stand-in, so a filter
   living only inside the real adapter would be bypassed there and the
   spec's "no matter what the server returns" would hold only for one
   implementation.
   - Alternative (filter only in `scan_dir`): rejected — the picker would keep
     displaying and pre-selecting `.`/`..`, and Enter-navigation could still
     build ever-growing `x/.` paths.
   - Alternative (filter only in the adapter, hub stays share-agnostic):
     rejected during apply — the system-level listing contract lives at the
     source boundary, and only a hub-level guard keeps it testable with
     stand-in sessions.

2. **Also skip `.`/`..` defensively in `FileList::scan_dir`.**
   One exact-match condition per entry. The scanner is the junction every
   source flows through; the guard makes the `file-list` termination
   requirement hold for any store, present or future, and keeps its unit tests
   able to model listing quirks directly.
   - Alternative (rely solely on the SMB filter): rejected — the likelihood of
     a future store repeating the mistake is high, the cost of the guard is
     negligible.

3. **Make the test mocks reflect reality.**
   `MockSession` in `smb.rs` currently never returns `.`/`..`, so the real
   behavior is untested. Its listings gain the pseudo-entries, proving the
   filter at hub level; `crates/core/src/source/testutil.rs` and the local
   store stay dot-free, mirroring the crates' actual contracts.
   - Alternative (a dedicated unit test for a helper function only): rejected
     — the user-visible failure was at the integration seam.

4. **No visited-set cycle guard in this change.**
   With the pseudo-entries removed at both listing layers, scanning cannot
   revisit a directory by how it builds its descent: child paths are plain
   names joined to the walked directory, so every step yields a strictly
   growing path, and the realistic cycle carrier on shares — reparse points —
   is already excluded by `is_link()`. The "revisited directory" clause was
   therefore dropped from the `file-list` delta during apply: no store in
   this system can produce such a listing, so the clause would have demanded
   dead protection and a fabricated store contract just to test. A visited
   set keyed on path strings could not catch aliasing anyway (symlink loops
   need realpath canonicalization), so if a real non-reparse cycle ever
   surfaces, it belongs to its own change with its own spec.

## Risks / Trade-offs

- [Non-dot cycles on a share (symlinks surfaced other than as reparse points) could still loop] → out of modeled scope: plain-name joins plus the dot and reparse filters make the walk structurally acyclic; if such a cycle ever surfaces, realpath canonicalization (not a string-path visited set) is the honest fix, in its own spec'd change.
- [Users may miss seeing `.`/`..` in the picker] → intended parity with local browsing wins; the entries were sort-first hazards, not features.
- [Exact-match filter could someday hide a real file named `.`] → impossible on any compliant server (`.`/`..` are reserved names); a `matches!` on exact names keeps the filter tight anyway.
- [Dot guarding at three layers (adapter, source boundary, scanner) invites a "cleanup"] → each layer is owned by a spec and a test: dropping the adapter or boundary guard fails the dotted-mock hub test, dropping the scanner guard fails the dotted-store scan test, so removal cannot pass silently.

## Migration Plan

None needed: no config, data, or interface shapes change. Rollback is a plain
revert of the implementation commit; the change is additive filtering.

## Open Questions

None.
