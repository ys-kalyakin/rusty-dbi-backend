# Design

## Context

The TUI already has a `Mode::Shares` list (key `S`) with a password modal and no way to
create a share: `App::with_shares` receives shares parsed once at startup, the file
`~/.config/dbi-backend/config.toml` is only ever read, and `ShareHub` is built once with
the startup list in `main.rs` (`crates/core/src/source/smb.rs`). The form therefore needs
three new pieces: a view, config-file writing, and a way for the hub to learn a share
after startup. `toml` 0.9 is already a dependency and also serializes; no new deps.

## Goals / Non-Goals

**Goals:**

- A share definition (no password anywhere in it) can be created, validated, and saved
  from the TUI without hand-editing the config file.
- A saved share is connectable immediately in the same run.
- The config file keeps every share it already had across saves.

**Non-Goals:**

- Editing or deleting existing shares (in-TUI or CLI).
- Preserving comments/formatting of hand-edited TOML across a save (see Decision 3).
- Making the running app notice manual config-file edits done while it is open.

## Decisions

### 1. Form view as a new mode, keyed like the picker's add flow

- New `Mode::ShareForm(ShareFormState)`: six labeled string buffers (`name`, `server`,
  `share`, `username`, `domain`, `root`), a focused-field index, and an error list.
- Keys inside the share list: `a` opens the form (scoped to that mode like the file
  list's `a`; no global-key conflict since modes consume their own keys first).
- Keys inside the form: typing/backspace edits the focused field, `Tab`/arrows move
  focus, Enter validates and submits, Esc returns to the share list. Chosen over a
  one-field-at-a-time wizard to keep all fields visible and editable without modal
  sequencing; alternative (sequential prompts) was discarded as slower to re-enter.
- Rendering reuses the compact centered-dialog pattern (`Clear` + `theme::block_title`),
  with focused field highlighted and errors shown in `theme::DANGER` under the fields.

### 2. Validation lives beside parsing in `config.rs`

- `config.rs` gains a build-and-validate function that turns the six form fields into
  `Result<ShareConfig, Vec<String>>`: field-level errors ("X is required", "bad port in
  server"), reusing the existing `parse_server`. Duplicate-name checking compares against
  the names already configured. Reusing the module that reads the file keeps future
  format rules in one place; the alternative (validation in `app.rs`) would drift from
  the parser.

### 3. Save: semantic round-trip, atomic write, canonical form

- Writing shares out only from the app is new behavior; the loader stays read-only.
- Save procedure: parse the current file text into a generic `toml::Table`
  (top-level keys other than `shares` are kept untouched), replace the `shares` array
  with the in-memory list serialized via `toml::to_string`, write to a temp file in the
  same directory, then rename over the target (atomic; a crash leaves the old file).
- The `server` field is written canonically: `host` when the port is 445, otherwise
  `host:port`; `domain`/`root` entries are omitted when empty. Entries are normalized to
  exactly what the loader yields.
- The in-memory share list is the source of truth for the `[[shares]]` array when saving.
- If the existing file cannot be parsed, unknown top-level content cannot be carried
  over; saving then writes the canonical document from the in-memory list (the file was
  already unusable). IO failures are logged as warnings and change nothing.

### 4. The hub learns new shares at runtime

- `ShareHub::add_config(&mut self, config)` inserts an unconnected entry (replacing a
  same-named entry deterministically; the form forbids duplicates anyway). Rebuilding the
  hub was rejected: it holds live sessions and the in-memory passwords of other shares.

### 5. Submit flows through an async action like `Action::Connect`

- `handle_key` validates the form; on success it keeps the form open and returns
  `Action::AddShare { config }`, so all side effects stay in `run_action` (same shape as
  `Action::Connect`): register the config in the hub, append to `App`'s list, save the
  file, log the outcome, and return to the share list with the highlight on the added
  share. Save failure logs a warning and still lands on the share list.
- When no config path is known (`config_path()` is `None`), the warning is logged and the
  share is usable for the run — covered by the failed-save requirement.

### 6. Discoverability edits

- Empty share-list hint becomes e.g. `No shares configured — press a to add one (saved
  to <path>)`; the file's path suffix stays for manual editors.
- The share-list view's hint line gains `a add` next to the existing hints; the help
  overlay's share section likewise (exact wording settled at implementation time).

## Risks / Trade-offs

- [Comments and formatting of a hand-edited config.toml are lost when the app saves] →
  Accepted for the canonical `[[shares]]` list; unknown top-level keys survive. If
  comment preservation ever matters, `toml::DocumentMut` is the upgrade path.
- [Manual config edits done while the app is running are overwritten by a later save] →
  Single-instance usage is the current operating mode; the log line names the file on
  every save failure so surprises are visible.
- [Saving writes user data] → Atomic rename keeps partial writes out of the file; the
  file's permissions are preserved by keeping the write in-place via temp+rename in the
  same directory.

## Migration Plan

Additive only: existing `[[shares]]` files keep loading unchanged, and not using the
form leaves behavior identical. Rollback is reverting the commits; the config format is
unchanged by design (the form only produces what the loader already accepts).

## Open Questions

None.
