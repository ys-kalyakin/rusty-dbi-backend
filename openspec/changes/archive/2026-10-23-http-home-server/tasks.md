# Tasks

## 1. Config and dependency groundwork

- [x] 1.1 Add `axum` to the workspace dependencies and to `crates/core`, and verify `cargo build` succeeds
- [x] 1.2 Parse the `[http]` config section — `host` (string) and `port` (number), defaults `0.0.0.0` and `65080`, a malformed value warns and keeps the default — and verify unit tests cover: absent section, explicit values, a bad port, an unknown key warning
- [x] 1.3 Add the `[http]` snippet to the README's config examples and verify the config test that loads every README toml block still passes warning-free

## 2. Core: the listing page

- [x] 2.1 Implement the href percent-encoding and anchor-text HTML-escaping helpers and verify unit tests: a non-ASCII name encodes in the href and stays readable in the text, `&`/`<`/`>` are escaped, the decode of an encoding round-trips to the entry name
- [x] 2.2 Build the nginx-autoindex-style page from a `FileList` snapshot (title, one line per file with padded anchor, fixed date column, right-aligned size) and verify unit tests: three files yield exactly three anchors each naming one file with its size, list order is kept, an empty list yields no anchors

## 3. Core: byte ranges

- [x] 3.1 Implement the single-range classifier — bounded and open-ended ranges honored, suffix/multi/malformed ranges fall back to the whole file, a start at or beyond the size is unsatisfiable — and verify unit tests for each classification

## 4. Core: the HTTP server

- [x] 4.1 Serve `GET /` with the listing and `GET`/`HEAD /<name>` with the entry's bytes streamed through `Sources` in 1 MiB chunks, emitting progress and one completion event per file, `404` for an unknown name, and logging failures — verify with an integration test on an ephemeral `127.0.0.1` port over mock sources asserting the listing bytes, a full download, bounded and open-ended range responses, HEAD's headers-without-body, and the `404`
- [x] 4.2 Wire the shutdown watch into a hard stop of the serve future and verify a test: signalling stop while a download is idle ends the task promptly and reports the run ended

## 5. App: background task and state

- [x] 5.1 Add `spawn_http_server` mirroring `spawn_server` — clone the `FileList` snapshot, forward logs, send start (with the display URL), progress, completion, and end events, and log a bind failure once before ending the run — and verify tests for the event-to-state mapping including the bind-failure path
- [x] 5.2 Implement the URL display address lookup (UDP-connect for a wildcard bind, the configured host verbatim otherwise, `0.0.0.0` fallback) and verify a test: a specific host is shown as configured and the wildcard lookup returns without hanging

## 6. App: keys, header, progress, docs

- [x] 6.1 Map `H` to start the HTTP server (non-empty list, not already running) and to stop it while it runs, keep `H` and `s` independent, and generalize the editing lock and hint marking to either server task — verify app tests: `H` toggling, `H` with an empty list doing nothing, `a`/`c`/`d`/`S` inert while either server is active, one server starting while the other runs
- [x] 6.2 Show the HTTP state in the header with the URL while running and idle without it — verify a UI test for both states
- [x] 6.3 Feed HTTP progress and completion into the existing progress region and summary lines, with the region following the most recent sample while both servers stream — verify app tests: an HTTP download advances the region, completion writes exactly one success line, a concurrent USB sample replaces the shown file
- [x] 6.4 List `H` in the help overlay and verify the help-content test includes it
- [x] 6.5 Write the README's HTTP section — the `H` key, the LAN-scoped no-auth posture, the non-ASCII name note, and the console-side `dbi.config` line `Home server=ApacheHTTP|http://<host>:<port>/` — and verify the README toml blocks still load warning-free and the documented keys match the implementation

## 7. Integration verification

- [x] 7.1 Run the full gate — `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` — and verify all three pass
- [x] 7.2 Smoke-test by hand: run the app over a directory of test files, press `H`, `curl` the listing and a ranged download, stop with `H`, and verify the log shows the start line, one completion per finished file, and no per-request noise

## Workflow follow-up

- Field-test against a real console (DBI's "Home server" source) when hardware is available; the design's `URLList` fallback is the contingency if the ApacheHTML parsing fails.
- Archive the change after review and the field check.
