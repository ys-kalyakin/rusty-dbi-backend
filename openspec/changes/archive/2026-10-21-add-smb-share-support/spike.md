# SMB crate spike — `smb2` 0.34 against the real NAS

Run 2026-10-21, standalone project in `/tmp/opencode/smb2-spike` (not merged).
Endpoint `192.168.0.4`, real credentials, password never written to any file or log.

## Results

| Check | Result |
|---|---|
| NTLM login | OK, SMB 3.1.1 negotiated, `max_read_size` 8 MiB |
| Port | 445 works; **5001 is not SMB** (login fails there, it's the DSM HTTPS port) |
| `list_shares()` | OK — 12 shares enumerated incl. `games` (via IPC$/srvsvc RPC) |
| Per-share access | `backup` TreeConnect → `STATUS_ACCESS_DENIED` for the test user; access is per share |
| `list_directory` | OK — names, sizes, `is_directory`; root path `""` works; games share holds a `switch/` tree with NSP files |
| Ranged read | OK — `FileReader::read_at(offset, len)` honors offsets (windows at 0 and at size/2 differ) and exact lengths |
| Throughput | 100 MiB via 1 MiB `read_at` chunks from offset 50 MiB: **55.2 MiB/s (~57.9 MB/s)** in 1.81 s |

## Verdict

`smb2` 0.34 is pinned. Specifically:

- No fallback to `smb` needed: sequential `read_at` alone reaches ~58 MB/s, far past the
  Switch's USB2-bound install rate; pipelined streaming is unnecessary for v1.
- Ranged reads (the `FILE_RANGE` primitive) exist and are correct — no low-level
  CREATE+READ+CLOSE workaround required.
- Listing gives everything `FileList` needs (name, size, kind).

## API notes carried into implementation

- `smb2::connect(host:port, user, pass)`; try the configured endpoint, fall back to the
  standard 445 if the server rejects the handshake there.
- `client.connect_share(name)` → `Tree`; a share can legitimately refuse the tree connect
  (ACCESS_DENIED) — connect failures are per-share and surfaced at connect time, never
  app-wide.
- `client.list_directory(&mut tree, path)` → entries with `name`/`size`/`is_directory`
  (+ `is_link()` reparse awareness, unused in v1).
- `client.open_file_reader(&tree, path)` → `FileReader`: `size()`, `read_at(offset, len)
  -> Vec<u8>` (allocating per call — acceptable at 1 MiB granularity), `close()`.
- `client.reconnect()` for the dead-session recovery path.
