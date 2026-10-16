# Proposal

## Why

`crates/core/src/protocol.rs` contains six `unwrap()` calls in production parsing
code (`Header::parse` and `FileRangeHeader::parse`). They are currently infallible
because each is guarded by a `buf.len() >= HEADER_LEN` check, but they encode a
panic path that is easy to break if the guard or slice bounds ever change. Removing
them eliminates latent panics in code that handles untrusted USB input.

## What Changes

- Replace the six `try_into().unwrap()` calls in `crates/core/src/protocol.rs`
  (`Header::parse` lines 55-57, `FileRangeHeader::parse` lines 87-89) with
  panic-free conversions.
- No public API, wire format, or parsing result changes: the functions keep their
  existing `Option` signatures and behavior.
- No test-code `unwrap()` calls are touched (they are confined to `#[cfg(test)]`
  modules and integration tests).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This is a pure refactor with no spec-level behavior change, so the change
opts out of spec deltas via `skip_specs: true` in `.openspec.yaml`.

## Impact

- Code: `crates/core/src/protocol.rs` only.
- Tests: `crates/core/src/protocol.rs` unit tests continue to pass unchanged;
  no new behavior to cover.
- APIs/dependencies: none.
