# Tasks

## 1. Remove production unwraps in protocol.rs

- [x] 1.1 Replace the three `try_into().unwrap()` conversions in `Header::parse` (`crates/core/src/protocol.rs:55-57`) with `try_into().ok()?`, and verify `cargo test -p dbi-backend-core protocol` passes
- [x] 1.2 Replace the three `try_into().unwrap()` conversions in `FileRangeHeader::parse` (`crates/core/src/protocol.rs:87-89`) with `try_into().ok()?`, and verify `cargo test -p dbi-backend-core protocol` passes

## 2. Verification

- [x] 2.1 Run `rg -n "\.unwrap\(\)" crates/core/src/protocol.rs` and confirm every remaining match is inside the `#[cfg(test)] mod tests` block (lines 110+), so no production `unwrap()` remains
- [x] 2.2 Run `cargo clippy --all-targets -- -D warnings` and `cargo test` from the workspace root, and confirm both complete without errors

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
