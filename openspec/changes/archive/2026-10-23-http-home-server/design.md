# Design

## Context

DBI v150+ installs over the network from an HTTP source of type `ApacheHTTP`:
the console fetches a directory listing and then the files it picks. A working
community reference is nginx with `autoindex on` (`jski/dbibackend-docker`,
port 65080, no auth, LAN-scoped), which tells us the client side is satisfied
by an ordinary Apache/nginx autoindex page plus plain byte serving. The backend
already has everything else the server needs: `FileList` (name-keyed snapshot,
one `Source` per entry), `Sources::open` returning a `FileHandle` with
`read_at(buf, offset)`, and a USB server task pattern (`app/server.rs`) that
clones the list, streams chunks, and reports progress through a channel.
DBI's own parser source is not available for reading (the repo ships only the
backend), so compatibility targets nginx's emitted bytes, not a spec.

## Goals / Non-Goals

**Goals:**

- One TUI and one file list serving both transports; HTTP is opt-in per key
  press and touches nothing while unused.
- Reuse the existing sources, snapshot, progress, and log plumbing unchanged.

**Non-Goals:**

- Authentication or TLS (see the decision on posture below).
- Serving anything not in the offered list; directory trees over HTTP (DBI gets
  one flat listing, matching how the USB LIST works).
- Resumable-session bookkeeping across restarts; Range support already covers
  what the client needs.

## Decisions

### HTTP stack: `axum` on tokio

A new workspace dependency. The alternative — hand-rolled HTTP/1.1 over
`TcpListener` — is where subtle keep-alive, header, and HEAD bugs live, and the
client is libcurl, which is strict. `tiny_http` is blocking-model and fights the
async `Sources`. Range handling stays ours either way (single range only), so
the framework only owes us request parsing and response framing, which is
exactly what hyper is good at. `HEAD` is served by the `GET` handler with the
body elided — asserted by a test, not assumed.

### Snapshot and concurrency: a second task mirroring `spawn_server`

Starting the HTTP server clones the `FileList` and spawns a task shaped like
`app/server.rs::spawn_server`: logs forwarded, lifecycle and progress events
sent through one channel into the existing event loop. The two servers keep
independent snapshots and may run simultaneously; the UI's editing lock
generalizes to "either task active", which is a state-machine change, not a
plumbing change. HTTP progress reuses the `Progress`/`Completed` events, so the
existing progress region rules apply as-is — including the "backward sample
restarts the phase" rule, which is what keeps a fresh `GET` from a stale 100%
bar when DBI re-downloads a file.

### Listing: mirror nginx `autoindex` output

Emit the page nginx emits for a flat directory: `<html><head><title>Index of
/</title></head><body><h1>Index of /</h1><hr><pre>` then one line per file —
`<a href="H">name</a>` padded to nginx's column width, a date column, and the
size right-aligned — then `</pre><hr></body></html>`. The href percent-encodes
the UTF-8 name and the anchor text HTML-escapes it; `GET` decodes the href back
to the entry name. The date column holds the server's start time for every row:
`ListEntry` carries no mtime, DBI's use of the column is unknown, and a fixed
value keeps the line shape without widening the data model. Real mtimes can be
added later if field testing shows DBI reads them.

### Ranges: single `bytes` range or the whole file

`bytes=a-b` and `bytes=a-` are honored (`206` + `Content-Range` + the slice),
a start at or beyond the size is refused (`416`), and anything else — no
header, multiple ranges, suffix `bytes=-n` — gets the full file (`200`), which
RFC 9110 permits and keeps the client on the happy path. The slice is streamed
as an `axum` body over an async stream that loops `read_at` in the same 1 MiB
chunks the USB server uses, emitting a progress event per chunk.

### Stop: a hard stop, not a graceful drain

`H` while running signals a shutdown watch; the serve future is dropped via
`tokio::select!` rather than awaited through a graceful drain. A graceful drain
would let one stalled download pin the stop indefinitely; the cost of the hard
stop is that an in-flight install fails on the console, which is the honest
outcome of an explicit stop.

### Posture: unauthenticated by design

Rejected: HTTP Basic auth (DBI supports `user:password@host` URLs, but the
secret would live in our config file, breaking the project's no-secrets-in-config
posture, or be asked per run, which is friction without a threat model) and a
per-run token in the path (DBI's source URL is configured once on the console;
a rotating token forces a re-entry every run). The server is documented as
LAN-scoped: it serves the offered files to every host that reaches the bind
address, and `host` is configurable for users who want it tighter.

### URL display: the UDP-connect trick, no new dependency

The header shows `http://<addr>:<port>/`. When bound to a specific host, that
host is shown. When bound to every interface, the address is found by
connecting an unconnected-protocol (`UDP`) socket to a remote address and
reading the local endpoint — no packet leaves the machine; a failure falls back
to showing `0.0.0.0` so the user still sees the port.

### Config: an `[http]` table parsed like `[filters]`

`host` (string) and `port` (number) with defaults `0.0.0.0` and `65080`;
malformed values warn and leave the default, mirroring the filters posture.
The README gains a toml snippet, and the config tests that load every README
toml block keep it honest.

## Risks / Trade-offs

- [DBI's ApacheHTML parser is unverifiable from here] → Byte-mirror the nginx
  page shape; field-test against a real console before calling the change done;
  the fallback path is a `URLList` plain-text endpoint, DBI's other source
  type, as a small follow-up change.
- [Non-ASCII names may trip DBI's parser (a community server enforces ASCII
  names)] → Percent-encoded hrefs are exactly what nginx does, ASCII names are
  unaffected, and the README will say so; a normalization option can follow if
  field testing demands it.
- [Two concurrent progress streams, one progress row] → Accepted and specified:
  the row follows the most recent sample; the existing backward-sample rule
  keeps the bar sane when the two servers interleave.
- [A new dependency tree (axum/hyper/tower)] → Accepted for correctness; the
  workspace already carries sizable, focused dependencies (ratatui, smb2).

## Migration Plan

None: the feature is opt-in, the config section and key are additive, and not
pressing `H` leaves today's behavior byte-identical. Rollback is removing the
`[http]` section; no stored state exists.

## Open Questions

- Does DBI read the listing's date column or `Last-Modified` headers? Answered
  by field testing; neither blocks this change's shape.
- Is `65080` the right default port long-term, or should it follow the
  reference's convention only until users complain? Cosmetic either way.
