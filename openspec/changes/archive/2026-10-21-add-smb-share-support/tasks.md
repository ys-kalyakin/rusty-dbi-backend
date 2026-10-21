# Tasks

## 1. SMB crate spike

- [x] 1.1 Run the `smb2` spike against the real NAS (NTLM login, directory listing with sizes, ranged read at a non-zero offset, ~100 MB throughput) and record findings in the change folder (`spike.md`); verify: the notes state the login, listing, ranged-read, and throughput results and the pinned crate choice (`smb2`, or the `smb` fallback with reasons).

## 2. Core source layer

- [x] 2.1 Add `Source`/`SmbLocation`, `ListEntry`, `SourceError`, `FileHandle`, and `Sources` traits under `crates/core/src/source/`, with the local implementation over `std::fs` and a feature-gated in-memory mock (`test-util`); verify with unit tests for local `list_dir`/`open`/`read_at`: `cargo test -p dbi-backend-core`.
- [x] 2.2 Extend `FileEntry` with `source` and add `FileList::add_source` walking directories through `Sources::list_dir` (recursive, `.nsp`/`.nsz` case-insensitive, directly added files unfiltered, skipped entries reported, sizes taken from listings); verify with mock-store tests for nested add, extension filter, skipped subdir, listing size, and direct add: `cargo test -p dbi-backend-core`.
- [x] 2.3 Thread `Sources` into `Server` (`Server::new(transport, files, store)`) so `FILE_RANGE` opens the named entry through its source with `read_at`, keeping existing error semantics; migrate existing server tests to the mock store and add a share-sourced streaming test; verify `cargo test -p dbi-backend-core`.

## 3. Share configuration

- [x] 3.1 Add `crates/app/src/config.rs` loading shares from `~/.config/dbi-backend/config.toml` (`name`/`server`/`share`/`username`, optional `domain`/`root`, `server` as host or `host:port` defaulting to 445), skipping malformed entries and duplicate names with warnings and storing no password; verify with unit tests over sample TOML files (valid, malformed entry, duplicate name, missing file): `cargo test -p dbi-backend-app`.

## 4. SMB source implementation

- [x] 4.1 Add the pinned `smb2` dependency and implement the SMB source in `crates/core/src/source/smb.rs`: a testable client-wrapper trait, a hub with `connect(username, password, domain)`, `open` with a single reconnect-and-retry when the session is dead, and `list_dir`; map all errors to `SourceError` messages that never contain the password; verify hub reconnect/open/list behavior with mocked-client tests: `cargo test -p dbi-backend-core`.
- [x] 4.2 Wire the hub into the app behind `Arc<tokio::sync::Mutex<...>>`, share it with `spawn_server`'s store, and hold the password only inside the hub (cleared on drop); verify the app builds and a test asserts the password is gone after drop: `cargo test`.

## 5. TUI share experience

- [x] 5.1 Add `Mode::Shares` (rows of name / server·share / username, Esc/q cancels, hint naming the config path when empty), open it with `S`, keep it inert and dimmed while the server task is active like `a`/`d`/`c`, and cover it with render tests; verify `cargo test -p dbi-backend-app`.
- [x] 5.2 Add `Mode::Password` (centered modal, `*`-masked buffer, Enter submits, Esc cancels) that connects through the hub, logs failures naming the share but never the password, and opens the picker at the share root on success; verify with hub-mock key-handling and render tests: `cargo test -p dbi-backend-app`.
- [x] 5.3 Generalize the picker to list through `Sources` (share root as a browsable directory, go-up stopping at the share root, unchanged filter/add semantics, share-sourced adds) and show share-qualified paths (`nas:/dir/file.nsp`) in the Files table; verify with mock-store picker tests and render tests: `cargo test -p dbi-backend-app`.
- [x] 5.4 Document the share config and the `S` flow in README and the help overlay (`?`); verify the README config snippet parses with the config loader fixture and the overlay lists `S`.

## 6. Integration verification

- [x] 6.1 Run the full gate `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check` and confirm local-only behavior (CLI paths, picker, start/stop, transfer progress) is unchanged.

## Workflow follow-up

- Manual end-to-end check: install a title from the real NAS to the Switch (needs hardware on hand).
- Archive the change with `/opsx-archive` after review.
