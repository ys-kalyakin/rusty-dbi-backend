# DBI Backend (Rust) — Design

## Purpose

A Rust reimplementation of the Python `dbibackend.py`. It runs a USB server that
serves NSP files to a Nintendo Switch running the DBI homebrew application. DBI
connects over USB and requests a file list and file ranges; the backend streams
the requested bytes back.

## Goals

- Feature parity with the Python implementation's protocol handling
  (`EXIT`, `LIST_OLD`, `FILE_RANGE`, `LIST`).
- Cross-platform (primary target Linux, must also build on macOS/Windows).
- Testable without USB hardware.

## Non-Goals

- No additional DBI protocol commands beyond the Python implementation.
- No native file-dialog GUI; interaction is CLI + terminal UI.

## Environment Constraints

- Rust toolchain is not installed. It will be installed via `rustup` (stable).
- No USB hardware assumed during development; core logic is tested with a mock
  transport.

## Technology Stack

| Concern      | Choice                          |
|--------------|---------------------------------|
| USB          | `nusb` (pure Rust, async)       |
| Async runtime| `tokio`                         |
| Terminal UI  | `ratatui` + `crossterm`         |
| CLI parsing  | `clap`                          |

## Architecture

Cargo workspace with two crates:

```
dbi-backend/
├── Cargo.toml                # workspace root
├── crates/
│   ├── core/                 # library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── protocol.rs   # DBI0 framing, commands, (de)serialization
│   │       ├── transport.rs  # Transport trait + nusb implementation
│   │       ├── server.rs     # command loop, file range, list, exit
│   │       └── filelist.rs   # name -> path map, file/dir discovery
│   └── app/                  # binary
│       └── src/
│           ├── main.rs
│           ├── cli.rs
│           ├── tui.rs
│           └── app.rs
└── (integration tests live in crates/core/tests/)
```

### Why two crates

- `core` has no UI or process-exit concerns and is unit/integration testable in
  isolation.
- A `Transport` trait abstracts the USB bulk endpoints so the server logic can be
  exercised end-to-end against an in-memory mock, with no hardware.

## Protocol (parity with `dbibackend.py`)

Every message begins with a 16-byte header, little-endian:

| Offset | Size | Field       | Notes                                   |
|--------|------|-------------|-----------------------------------------|
| 0      | 4    | `magic`     | ASCII `"DBI0"`                          |
| 4      | 4    | `cmd_type`  | u32 LE                                  |
| 8      | 4    | `cmd_id`    | u32 LE                                  |
| 12     | 4    | `data_size` | u32 LE, length of any following payload |

Constants:

- `cmd_type`: `REQUEST = 0`, `RESPONSE = 1`, `ACK = 2`
- `cmd_id`: `EXIT = 0`, `LIST_OLD = 1`, `FILE_RANGE = 2`, `LIST = 3`
- `BUFFER_SEGMENT_DATA_SIZE = 0x100000` (1 MiB)
- USB: VID `0x057E`, PID `0x3000`; OUT endpoint `0x01`, IN endpoint `0x81`

Headers whose magic is not `"DBI0"` are skipped (loop continues).

`LIST_OLD` is parsed but not acted upon, matching the Python behavior.

### Command loop

1. Read a 16-byte header from the IN endpoint.
2. Dispatch on `cmd_id`:
   - `EXIT` → `process_exit`
   - `FILE_RANGE` → `process_file_range(data_size)`
   - `LIST` → `process_list`
   - anything else → ignore
3. On a USB error, log "connection lost" and reconnect (wait for the device).

### `process_exit`

- Send `DBI0, RESPONSE, EXIT, 0`.
- Signal the application to stop.

### `process_file_range(data_size)`

1. Send `DBI0, ACK, FILE_RANGE, data_size`.
2. Read `data_size` bytes from IN: this is the range header:
   - `range_size: u32` at `[0:4]`
   - `range_offset: u64` at `[4:12]`
   - `name_len: u32` at `[12:16]`
   - `name: utf8` in the remaining bytes
3. Send `DBI0, RESPONSE, FILE_RANGE, range_size`.
4. Wait for a 16-byte ACK from the Switch.
5. Open `file_list[name]`, `seek(range_offset)`, and stream `range_size` bytes
   in chunks of at most 1 MiB to the OUT endpoint.

### `process_list`

1. Build the payload: each file name followed by `\n`, UTF-8 encoded.
2. Send `DBI0, RESPONSE, LIST, payload_len`.
3. If `payload_len > 0`: wait for a 16-byte ACK, then send the payload bytes.

### Connection handling

`connect` loops, scanning for the device by VID/PID and sleeping ~1s between
attempts. On any USB error during the loop, the server logs the loss and
reconnects. USB errors never terminate the process.

## File list

`file_list` maps file name → absolute path (mirrors Python's `dict`). Adding a
directory adds each immediate regular file; adding a file adds that file. Paths
are resolved to absolute. Duplicate names overwrite (name is the key).

## CLI and TUI

### CLI

```
dbi-backend [PATHS...]
```

Each `PATH` may be a file or a directory; entries are loaded into the file list
before the UI starts.

### Terminal UI

Three regions:

- **File list** (top, scrollable): absolute paths, sorted by name.
- **Log** (middle, scrollable): capped ring buffer (~1000 lines, matching the
  Python cap).
- **Status / help line** (bottom).

Keybindings:

| Key | Action                                        |
|-----|-----------------------------------------------|
| `a` | Prompt for a path, add it to the list         |
| `c` | Clear the list                                |
| `s` | Start the server (allowed only if list non-empty) |
| `q` | Quit (sends the exit flow if running)         |

Once the server starts, the file list is frozen for that run (matching the
Python UI, which disables editing). The server snapshots the list on start.

### Concurrency

- `tokio` runtime in `main`.
- The server runs as a `tokio` task; it reports log lines to the UI over an
  `mpsc` channel.
- The TUI runs an event loop that `select!`s between terminal input events and a
  periodic tick for redraw. Terminal input is read on a dedicated blocking thread
  and forwarded over a channel.

## Error Handling

- Protocol parse errors (short reads, bad magic) are skipped/ignored per the
  original behavior; unexpected framing is logged.
- USB errors trigger reconnect.
- File open/seek errors are logged and abort the current transfer without
  killing the server.
- The UI surfaces server errors in the log panel.

## Testing

- **Unit (`core::protocol`)**: header build/parse round-trip; `FILE_RANGE`
  header field parsing; `LIST` payload serialization (`name\n`, length).
- **Integration (`core::server`)** with a mock `Transport`: drive a `LIST`
  command and a `FILE_RANGE` command end-to-end against a temp file, asserting
  the exact bytes written to the OUT endpoint.
- **CLI/UI**: construction and file-list population tested; interactive loop
  not unit-tested.

## Success Criteria

- `cargo build` and `cargo test` pass on stable Rust.
- With a real Switch running DBI, the backend lists NSPs and installs one using
  the same command sequence as the Python backend.
- USB disconnect/reconnect is handled without crashing.
