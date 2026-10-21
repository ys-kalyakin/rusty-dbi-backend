# Tasks

## 1. Config writing and validation (`crates/app/src/config.rs`)

- [x] 1.1 Add a build-and-validate function turning the six form fields into `Result<ShareConfig, Vec<String>>`, reusing `parse_server` (required fields, host/host:port, duplicate-name check against configured names) and verify with unit tests: empty required field, bad server (`host:bogus`), duplicate name, valid full entry
- [x] 1.2 Implement saving: parse the current file text into a generic `toml::Table`, replace only the `shares` array with the given list serialized back as `[[shares]]` entries (`server` as `host` when port 445 else `host:port`; `domain`/`root` omitted when empty), write a temp file in the same directory and rename over the target, and verify with unit tests: a save adds the entry with no password, existing shares survive, unknown top-level keys survive, a save over an unparseable file still produces a canonical document, an IO failure leaves the file unchanged
- [x] 1.3 Run `cargo test -p dbi-backend-app config` and confirm all config-module tests pass

## 2. Hub registration (`crates/core/src/source/smb.rs`)

- [x] 2.1 Add `ShareHub::add_config` inserting an unconnected entry (replacing a same-named entry) and verify with a unit test: after `add_config` on a fresh hub, `configs()` lists it and `connect` works for it

## 3. TUI form (`crates/app/src/app.rs`)

- [x] 3.1 Add `Mode::ShareForm(ShareFormState)` with the six labeled buffers, focus index, and error list; wire `a` in the share list to open it and verify: the form opens empty, renders as a compact centered dialog, and its key hints replace the help line
- [x] 3.2 Implement form keys — typing/backspace on the focused field, Tab/arrows focus movement, Esc back to the share list — and verify with unit tests that each field keeps its own text and Esc discards everything
- [x] 3.3 Implement Enter-submit validation and rendering of inline field errors (invalid submit keeps the form and its typed values; required/`server`/duplicate messages from 1.1) and verify with unit tests for rejected submits
- [x] 3.4 Add `Action::AddShare { config }`; in `run_action` register it in the hub, append it to `App`'s list with the share-list highlight on it (E must not connect or ask for a password), attempt the save, and verify in tests: valid submit returns to the share list listing and highlighting the share; save is attempted once; a save failure logs a warning while the share stays listed
- [x] 3.5 Update the share-list hint line (`a add` next to existing hints), the empty-list hint to name the add key plus the config path, and the help overlay's share section, and verify with the hint-rendering tests

## 4. Integration verification

- [x] 4.1 Run the full test suite (`cargo test --workspace`) and fix any regressions
- [x] 4.2 Manual pass in a test config location with `cargo run` against a stub server if available: add a share, see it listed highlighted, connect asking for the password, quit, and confirm config.toml gained the entry with no password and prior shares intact

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived result.
