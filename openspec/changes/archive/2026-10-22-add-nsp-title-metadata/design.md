# Design

## Context

dbi-backend is a two-crate Rust workspace: `crates/core` (DBI0 protocol, server
command loop, file list, sources) and `crates/app` (ratatui TUI, CLI, config).
See proposal.md for the motivation; the requirement set lives in the spec
deltas (`nsp-metadata`, `file-list`, `cli-tui`).

The facts that shape the approach:

- All data access goes through one abstraction in `crates/core/src/source/`:
  `Sources::open(source) -> Box<dyn FileHandle>`, where a handle offers
  `read_at(buf, offset)` and `size()`. A local file and a file on a share are
  read the same way. The identity read must go through it too, or a title on
  a NAS would behave differently from a local one.
- A file is added once and then only streamed. The server opens the file per
  range request; nothing re-reads the file's header today.
- The NSP container is a PFS0 archive: a 16-byte header (the marker `PFS0`, a
  32-bit file count, a 32-bit name-table size), a UTF-16LE name table, and one
  16-byte entry per file (a 64-bit offset, a 64-bit size, a 32-bit name
  index). Its table of contents is plain — no key is involved.
- The content meta inside the container (`meta.cnmt.npdm`) carries the title
  id, the version and the content type.
- The human-readable title name is **not** in the content meta. It lives in
  the content archive's (NCA) header, which is encrypted with a key only the
  console holds — the same key material `nstool` lists as coming from a
  console's `prod.keys`. A tool that installs titles has no business asking
  for those keys, so the name is taken from the file name.
- NSZ is the same container with the archive's contents compressed; the table
  of contents stays readable.

## Goals / Non-Goals

**Goals:**

- The title id, the content type and the version of every offered title,
  shown next to its file name.
- The identity read costs a few kilobytes per file, once, at add time.
- An install never depends on the identity: a file whose metadata cannot be
  read is offered exactly as before.

**Non-Goals:**

- Reading the title's name, its publisher, its icon or its size on disc from
  the container — all of them live behind the console's keys.
- Title databases, name caches, or a lookup from a title id to a name.
- Reading anything from an XCI or a split NSP: the container reader covers
  what the backend offers today, which are NSP and NSZ files.
- Verifying the container's integrity, its hashes, or its signature.

## Decisions

### 1. The identity is read at add time, through the source abstraction, and cached on the entry

`FileEntry` gains `identity: TitleIdentity`:

```
struct TitleIdentity {
    title_id: Option<String>,     // 16 hex digits
    content_type: Option<ContentType>,   // Base | Update | Dlc
    version: Option<(u8, u8, u8)>,       // major, minor, patch
    name: Option<String>,         // the name part of the file name
}
```

The entry carries the identity itself, whose parts are optional: an entry
whose identity could not be determined carries an empty one, not a missing
field.

The read happens when the file is added, through `Sources::open` +
`read_at`, and the result is stored with the entry.

Why at add time: the server's job is to stream bytes, and a range request
must not depend on a metadata read that could fail on a share that just went
to sleep. Why cached: the identity is a property of the file, read once, and
the entry is already the place the list keeps a file's facts (its size, its
source).

Why through `Sources` and not `std::fs`: a file added from a share must get
the same identity as a local one, and the source abstraction is the only
place that knows how to read a share.

### 2. The container reader reads a header, never a body

The reader opens the file, reads the 16-byte container header, computes the
header's full size from the file count and the name-table size, reads that
header, and looks for the content-meta member in the table of contents. Only
then does it read the member itself — a few hundred bytes. The body of the
archive is never touched: a 10 GiB title costs a few kilobytes of reads.

The header size is bounded before it is used: a file count or a name-table
size that would imply a header larger than the file itself is refused, and
the file is treated as "no identity".

### 3. The container's content meta is the primary source, the file name is the fallback

The content meta gives the title id, the version and the content type
directly. When the container does not expose it — it is not a container, it
is a container this reader does not understand, or the content meta is only
reachable through an encrypted archive header — the identity is derived from
the file name.

**Spike outcome (task 1):** the answer is *no* for a dumped NSP, and the
file-name path carries the change. The evidence, from two independent
implementations of the format:

- **What an NSP's table of contents holds.** `nxdumptool`'s NSP writer fills
  the PFS0 string table with one `<nca id>.nca` per content NCA, one
  `<nca id>.cnmt.nca` for the content meta, and optionally the `.tik` and
  `.cert` entries (`source/dumper.c`). `nsz`'s reader looks the content meta
  up the same way — it searches the container's members for a name ending in
  `.cnmt.nca` (`nsz/Fs/Nsp.py`). Neither writes nor looks for a plain
  `meta.cnmt.npdm` member. The member names carry the *NCA id* and the rights
  id, not the title id.
- **Why the content meta is out of reach.** It lives inside the content meta
  NCA, whose 0xC00-byte header is encrypted with AES-XTS under the console's
  `header_key` (`nsz/Fs/Nca.py`: `0x0, 0xC00, self.header, Crypto.XTS,
  Keys.get("header_key")`), and whose data partition is encrypted with the
  title key from the ticket. Both are console key material.
- **What the content meta's header holds**, for the reader that does get that
  far: the title id at 0x0 (8 bytes, little-endian — the canonical form is
  the same bytes read big-endian, 16 hex digits), the version at 0x8 (a
  32-bit `major << 16 | minor << 8 | patch`), the content type at 0xC, and
  the extended-header size at 0xE (switchbrew's CNMT page; `nsz/Fs/Cnmt.py`).

So: a container that exposes its content metadata in the clear yields the
identity directly, and such a container is not what a dump tool produces. The
requirements stay as they are — the container requirement is conditional, and
the file-name fallback is the path a real NSP takes. The container reader
stays, thin and tested, because it is the only thing that can satisfy the
container requirement at all; for a dumped NSP it finds no exposed metadata
and the file name fills the identity in.

**The conclusion is executable.** The reader is checked against a container
built the way the tools build it — the name table UTF-8 and padded to a 0x20
boundary, a member's offset counting from the end of the header — and against
the member names a dumped title actually carries: the content NCAs, the
content meta NCA and the ticket. That container exposes no metadata of its
own, which is the spike's answer as a test rather than as a note.

The spike's outcome decides the tasks in group 2, not the requirements.

### 4. The file-name convention is a source of its own, not a rescue

The convention every dump tool uses is
`Name [TitleId][vVersion].nsp`:

- the title id is 16 hexadecimal digits in brackets;
- the version is `v` followed by a decimal number, encoded as
  `major << 16 | minor << 8 | patch` — `v65536` is 1.0.0;
- everything before the first bracket is the name.

The parser reads the brackets from the end, so a name may itself contain
brackets. A bracket that is not a title id is ignored rather than guessed at.

### 5. The content type comes from the container, and from the title id otherwise

When the content meta is read, its content type is authoritative. Otherwise
the title id's last three digits classify the file: `000` is a base game,
`800` is an update, anything else is downloadable content. This is the
convention the ecosystem uses, and it needs no keys.

### 6. Reading the identity never fails an add

Every failure — a file that is not a container, a truncated header, a content
meta that does not parse, a share that refuses the read — yields an empty
identity and no warning. The identity is a convenience for the person looking
at the list; the file's bytes are what the console needs, and they are
unaffected by a failed metadata read. A warning per unreadable file would
be noise, since a non-NSP file in the directory is a normal thing to have.

### 7. The name shown is the file name's name part

The identity's name is the file name without its bracketed parts; a file name
without a name part keeps its whole name. The name in the container is never
read, so the name shown is always the one the user already knows.

## Risks / Trade-offs

- **[The content meta is not where this design expects it]** → the spike in
  task 1 settles it before the parser is written, and the file-name fallback
  keeps the change valuable either way.
- **[The identity read costs a round trip per file on a share]** → it happens
  once, at add time, and reads a header rather than a body. A scan of 100
  titles over SMB does 100 small reads; a scan over a local disk costs
  nothing worth measuring.
- **[A file whose first bytes look like a container is not one]** → the marker
  check plus the header-size bound keep the reader's work small, and a failed
  parse yields no identity rather than a wrong one.
- **[The version encoding is read from the file name]** → `v65536` is 1.0.0,
  and a version of 0 means "unknown", so a placeholder version in a name
  shows as unknown rather than as 0.0.0.
- **[The identity can be stale]** → it is read once, at add time. A file
  replaced on disk under the same name keeps the identity read from the old
  file. The offered bytes are the new ones; only the label is stale.

## Migration Plan

No data migration and no configuration: the identity is read automatically for
every file that is added. Turning the feature off is a matter of hiding the
column.

## Open Questions

- Should the identity replace the path column or get a column of its own in
  the file list? Deferrable: either layout satisfies the requirement.
- Should the list search match the identity once the filter change lands?
  Deferrable: it widens the search's match set, not this change's rules.
