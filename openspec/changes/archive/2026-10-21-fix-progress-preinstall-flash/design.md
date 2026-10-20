# Design

## Context

The user-visible defect: before an install's content streaming begins, the TUI
progress row shows the file at a full green fill (100%), then the bar restarts
and behaves correctly. The backend's `Server::process_file_range`
(`crates/core/src/server.rs`) emits `ServerUpdate::Progress`/`Completed` for
every range whose `sent` (starting at the requested `range_offset`) reaches or
passes the file size. DBI's install preparation performs small reads near the
end of the NSP (ticket and certificate entries live at the file's tail; see the
clean-room reconstruction in `mrbeige1/open-dbi`, whose install phase reads the
PFS0 header/tables, then the ticket, before streaming content from offset 0).
A trailing preparation read therefore lands the first progress sample at
100%, the TUI clamps it to a full bar, and a `Completed` "Sent: file" summary
fires before any install bytes flow. When content streaming starts at offset
0, the sample position drops and the bar "starts from the beginning".

Constraints:

- The DBI switch-side app is closed source; its request order is inferred (the
  clean-room reconstruction, the observed bar behavior, and the reference
  `dbibackend.py`, which stays silent about ranges).
- The wire protocol must not change: DBI streams, headers, and framing are
  unaffected by reporting decisions.
- The percent metric is "position in the file / file size"; it is what makes
  the bar useful during the streamed install phase.

## Goals / Non-Goals

**Goals:**

- Progress samples, the 100% fill, the completion signal, and the "Sending:"
  start announcement come only from ranges that (re)start or continue a file's
  streamed sequence (offset 0, or exactly at the position earlier streamed
  ranges reached).
- Auxiliary reads elsewhere in the file are served identically but stay
  silent, and leave the streamed position untouched.
- The TUI restarts speed/ETA estimation when a sample rewinds the bar, so a
  new streamed phase never inherits or continues an old fill.

**Non-Goals:**

- No attempt to hide legitimate offset-0 metadata reads: a read at offset 0
  (e.g., the tiny PFS0 header) still opens a streamed sequence, showing the
  region near 0%. Only the full-green flash is the defect.
- No protocol or byte-stream changes; no new commands; no changes to how
  ranges are served on error or disconnect.
- No model of DBI's install plan (multi-file installs map naturally to
  per-file state); no idea of "percent across the whole install".

## Decisions

1. **Track the streamed position per connection and file in the server.**
   `Server` holds `streams: HashMap<String, StreamState>` where
   `StreamState { position: Option<u64>, announced: bool }`. This replaces the
   single `serving: Option<String>` announcement slot. A range participates
   when, for its file: no position exists and `range_offset == 0` (start), or
   `static position exists and range_offset == position` (continuation, then
   `position += range_size`). Anything else is an auxiliary read: serve it,
   but emit no update. On `position >= total` during a participating range,
   emit completion once and drop the entry. Rationale: a map is robust to
   interleaved reads of several files (aux reads must not clobber another
   file's state, which a single slot would); the alternative of inferring
   phases from DBI's request pattern would couple the backend to
   reverse-engineered switch behavior we cannot verify.
2. **Keep the existing per-range percent-change throttle, gated by
   participation.** The range loop already logs a `Progress` update only when
   the integer percentage changes and nothing per read; gate those emissions
   (and `Completed`, and the header announce) on the participation decision.
   Rationale: minimal diff on the hot path; identical reporting within a
   streamed phase.
3. **Reset, do not clamp, on a backward sample in the TUI.**
   `App::update_progress_at` currently carries speed/ETA state across same-name
   samples regardless of direction. When the new sample's `sent` is lower, the
   speed sample baseline, smoothed rate, speed, and ETA reset (the region stays
   visible at the new position). Rationale: this is the safety net for any
   phase restart the server still reports (e.g., a file replayed after
   completion), and it keeps the "Estimate responds gradually" guarantee from
   being untangled with samples of another phase.
4. **Testing encodes the observed DBI request sequence.** A `server_test`
   drives: small offset-0 read (announced, progress at 0%); contiguous
   continuation; a trailing non-contiguous range ending at the file's last
   byte (no progress, no completion, position unchanged); a following
   continuation that still participates; a restart at offset 0 (new announce);
   then sequential ranges to EOF producing exactly one completion. App-level
   unit tests cover the backward-sample reset.

## Risks / Trade-offs

- [The pre-install trigger is inferred, not observed with hardware] → The
  mechanism is generic: any range that jumps the bar backward or completes the
  file without continuing the streamed sequence stops affecting the region. If
  re-testing on hardware shows a different request pattern, the
  participation rule still removes "any EOF-touching range completes a file".
- [Installs whose content is read non-contiguously (e.g., NCZ section
  scattering) would appear as aux reads, so the bar stalls at the last
  streamed sample instead of sweeping 0→100%] → Accepted for this change: the
  region persists (spec: "stay visible") and completion/summary simply wait
  for a qualifying range. If NSZ/NCZ or XCI installs are reported stalling,
  revisit the participation rule (e.g., allow forward jumps within the same
  read phase).
- [Strict continuation requires consecutive reads without gaps; padding or
  skipped bytes split the streamed sequence into aux reads] → Same mitigation
  as above; the observable failure mode is a silent pause, never a false 100%.
- [Announce timing shifts: "Sending:" now appears at the first offset-0 read
  rather than at the first byte range served] → Intentional and honest (the
  "Transfer summary in the log" requirement describes streaming starts; a
  start-announcement at the streamed sequence start is the spec's wording).
- [Aux reads after completion: entry dropped, so re-serving mid-file reads
  stay silent until a new offset-0 read restarts the file] → Desired.

## Migration Plan

No deployment steps: single Rust binary, no config or data migration. Roll
back by reverting the change. The behavior difference is log/report-only and
DBI-visible streams are unchanged, so a rollback cannot affect transfers.

## Open Questions

None. If a flash survives on real hardware after implementation, collecting
the backend's debug record of the range requests that precede the streaming
phase resolves which request shape is involved; that is an implementation
diagnostic, not a spec or task-list change.
