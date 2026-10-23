# Proposal

## Why

DBI (v150+) can install titles wirelessly from an HTTP "Home server" source: the
console browses an Apache/nginx-style directory listing and streams the files it
picks. The backend already curates exactly what such a server should expose — a
filtered, deduplicated list of local and SMB-backed files — so a network install
today needs a separate nginx setup and a second copy of the library. Serving the
same list over HTTP from the same TUI removes both the cable and the duplicate
infrastructure.

## What Changes

- A new HTTP server, started and stopped from the TUI, that exposes the offered
  file list as an Apache/nginx autoindex-style HTML listing and streams any
  offered file — local or SMB-backed — with byte-range support, reusing the
  existing sources, progress reporting, and transfer log.
- An `[http]` section in the config file (`host`, `port`; defaults `0.0.0.0` and
  `65080`).
- The `H` key starts and stops the HTTP server; the header shows its state and,
  while it runs, the URL to enter on the console; the list-editing keys lock
  while it runs, for the same reason they lock for the USB server: the offered
  list must match the running server's snapshot.
- HTTP downloads feed the same progress region and transfer summary the USB
  server feeds.
- No authentication in this change: a trusted-LAN posture, documented in the
  README (the design records the alternatives that were rejected).

## Capabilities

### New Capabilities

- `http-server`: serving the offered file list over HTTP for DBI's "Home server"
  network install — bind configuration, the listing it serves, file streaming
  with ranges, progress and logging semantics, and the server's lifecycle.

### Modified Capabilities

- `cli-tui`: the `H` keybinding is added and the lock on the editing keys is
  generalized to cover either server; the header shows the HTTP server's state
  and URL; transfer progress and summary reporting cover HTTP downloads.

## Impact

- `crates/core`: a new `http` module beside `server.rs`, reusing
  `source::Sources`/`FileHandle` and `filelist::FileList`; a new `axum`
  dependency (hyper comes with it).
- `crates/app`: the config gains the `[http]` section; a second background task
  mirrors `spawn_server`; the state and UI gain the HTTP state, the URL display,
  and the `H` key.
- README: a usage section, the `[http]` snippet (the README's toml blocks are
  load-tested by `config.rs` tests), and the DBI-side `dbi.config` line
  (`Home server=ApacheHTTP|http://<host>:<port>/`).
- No USB or DBI0 protocol changes; existing behavior is untouched while `H` is
  never pressed.
