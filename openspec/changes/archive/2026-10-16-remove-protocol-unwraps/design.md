# Design

## Context

`crates/core/src/protocol.rs` parses DBI0 framing from untrusted USB input. Both
`Header::parse` and `FileRangeHeader::parse` begin with a `buf.len() < HEADER_LEN`
guard, then convert fixed-width sub-slices with `u32::from_le_bytes(..try_into().unwrap())`
and `u64::from_le_bytes(..try_into().unwrap())`. See proposal.md for motivation.

The guards make every conversion infallible today: each slice has a compile-time
fixed length equal to the target array (`[u8; 4]`, `[u8; 8]`). The `unwrap()` calls
are therefore dead panic paths, but they still encode a panic in a function that
must not panic on malformed input.

## Goals / Non-Goals

**Goals:**
- Remove all production `unwrap()` calls from `protocol.rs`.
- Preserve the exact `Option` signatures and observable parse behavior.

**Non-Goals:**
- Changing the wire format, constants, or public API.
- Touching `unwrap()` calls inside `#[cfg(test)]` modules or integration tests.
- Introducing new dependencies or `unsafe`.

## Decisions

**Decision: replace `try_into().unwrap()` with `try_into().ok()?`.**
Both parse functions already return `Option<Self>`, so the conversion result maps
directly onto the function's error path without adding branches.
- Alternative: explicit byte indexing (`[buf[4], buf[5], buf[6], buf[7]]`) — equally
  panic-free but more verbose and easy to desync from the slice bounds.
- Alternative: keep `unwrap()` with a comment that it cannot fail — rejected; the
  point is to remove the panic, not document it.

**Decision: keep the existing `buf.len() < HEADER_LEN` guards as the primary check.**
The `.ok()?` conversions stay defensive so a future change to slice bounds degrades
to `None` instead of a panic.

## Risks / Trade-offs

- [Behavior drift if `ok()?` is used where the value is genuinely required] -> the
  only failure mode is a slice of the wrong length, which the guard already excludes,
  so behavior is unchanged.
- [Missed occurrences] -> verify with a repo-wide search that no production
  `unwrap()` remains in `crates/core/src/protocol.rs` and that tests still pass.
