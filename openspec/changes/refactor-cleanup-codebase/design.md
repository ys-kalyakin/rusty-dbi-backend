# Design

## Context

See proposal.md for the motivation. The facts that shape the approach:

- The workspace is two crates: `crates/core` (DBI0 protocol, server command
  loop, file list, sources, metadata) and `crates/app` (ratatui TUI, CLI,
  config, picker). The app crate is a binary; `crates/app/src/main.rs`
  declares `mod app; mod cli; mod config; mod picker; mod theme;`.
- `crates/core/src/source/mod.rs:6` gates `pub mod testutil` behind
  `#[cfg(feature = "test-util")]`, and `crates/core/Cargo.toml:11-12` declares
  that feature. Nothing in the workspace enables it: `crates/app/Cargo.toml:12`
  depends on core without features, core has no self dev-dependency, and
  `.github/workflows/ci.yml:20` runs `cargo test --workspace`. The block is
  therefore never compiled. `cargo test -p dbi-backend-core --features
  test-util` compiles and passes (118 tests against 104 without the feature),
  so the block is sound and only unreachable.
- `crates/app/src/app.rs` is 4291 lines: 1825 production and 2466 test. Its
  production half holds, in one file, the state types (`Mode`, `Action`,
  `ServerEvent`, `ConfirmKind`, `ServerStatus`, `LogLine`, `ProgressState`,
  `ShareFormState`, `ShareList`, `PasswordModal`), the `App` struct and its
  state mutators, the key handlers, the fourteen `render_*` functions, the
  background server task and the formatting helpers.
- `crates/core/src/protocol.rs:11` defines `CMD_ID_LIST_OLD` and nothing
  references it; the server's command loop matches `CMD_ID_EXIT`,
  `CMD_ID_FILE_RANGE` and `CMD_ID_LIST` and ignores everything else
  (`crates/core/src/server.rs:87-99`), so the constant already describes
  behaviour the program does not have.
- `crates/core/src/source/mod.rs:38-55` and `:56-73` define `Source::join` and
  `Source::parent`; neither is called anywhere. The app reimplements the same
  arithmetic locally in `crates/app/src/picker.rs:357-363` (`join_share`) and
  `:367-375` (`parent_share_path`).
- `crates/core/src/server.rs:153` clones the whole `FileEntry` and `:157`
  clones its `Source` a second time; `:207` allocates a fresh zeroed buffer of
  up to 1 MiB for every `FILE_RANGE` request; `:118` materialises a
  `Vec<String>` of every file name before copying it into the payload.
- The repo's checks are `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings` and `cargo test --workspace` (`.github/workflows/ci.yml`).
  They are all clean today, so every change below has to stay clean.

## Goals / Non-Goals

**Goals:**

- Every line of code in the workspace is either reachable or is a test.
- The application code is laid out so that a change to the rendering does not
  touch the key handling, and a change to the key handling does not touch the
  server task.
- The parts that must agree — the offer rules, the share validation rules, the
  share-path arithmetic, the version decoding — are written once.
- The observable behaviour of every spec requirement is unchanged. Where a
  consolidation could change behaviour, it is not done.

**Non-Goals:**

- Fixing the behavioural problems the analysis also found. They are real and
  they are listed at the end of this document, but each one changes what a
  user sees and therefore needs its own change with its own decision.
- Adding, removing or upgrading a dependency.
- Changing the command line, the config file format, or the DBI0 wire
  protocol.
- Restructuring `crates/core` into more crates, or splitting `crates/app`'s
  test module across the new modules.

## Decisions

### 1. The change declares `skip_specs: true`

Every requirement in `cli-tui`, `file-list`, `filters`, `nsp-metadata`,
`path-picker`, `protocol`, `server`, `smb` and `transport` keeps its wording
and its observable behaviour. A spec delta would have to invent a requirement
that does not exist yet, which the schema's proposal guidance forbids.

*Alternative considered:* a new `code-quality` capability with requirements
like "the test suite covers every spec requirement". Rejected: that is a claim
about the repository, not about the program's behaviour, and it would have to
be re-argued every time a spec is added.

### 2. The unreachable test block is compiled into the crate's own test build

`crates/core/src/source/mod.rs` gates the module with
`#[cfg(feature = "test-util")]` and `crates/core/src/filelist.rs:753` gates
`add_source_tests` the same way. The feature becomes `#[cfg(test)]` on both,
and `crates/core/Cargo.toml` loses the `test-util` feature.

*Alternative considered:* keep the feature and turn it on in CI with
`cargo test --workspace --all-features`. Rejected: a feature that exists only
to switch on code that should always be compiled is a footgun — the next
contributor adds a test that needs the fake store and it silently does not
run. A crate cannot enable its own feature for its own unit tests through a
self dev-dependency, so the `cfg(test)` gate is the way to get the tests into
`cargo test --workspace` with no flag.

*Alternative considered:* move the fake store to `crates/core/tests/common/`
and port the ten tests to the integration-test crate. Rejected: an
integration test cannot reach a `pub` item of the library that is private to
the library's unit-test build, and the tests exercise `FileList::add_source`
against a store, which is exactly what a library unit test is for. The fake
store is only used by `filelist.rs`; `crates/core/tests/server_test.rs` does
not use it.

### 3. `crates/app/src/app.rs` becomes `crates/app/src/app/`

```
crates/app/src/app/mod.rs      the module declarations and the re-exports
                                main.rs uses (`App`, `Action`, `ServerEvent`,
                                `ui`, `spawn_server`)
crates/app/src/app/state.rs     App, Mode, Action, ServerEvent, ConfirmKind,
                                ServerStatus, LogLine, ProgressState,
                                ShareFormState, ShareList, PasswordModal, and
                                the state-mutating `impl App`
crates/app/src/app/input.rs     the key handling: `handle_key` and the six
                                per-mode handlers
crates/app/src/app/ui.rs        `ui` and the fourteen `render_*` functions,
                                the layout helpers and the table/hint helpers
crates/app/src/app/server.rs    `spawn_server` and `ConnectLog`
crates/app/src/app/format.rs    `human_size`, `format_elapsed`,
                                `remaining_seconds`, `abbrev_path`, `log_color`
crates/app/src/app/tests.rs     the existing `#[cfg(test)] mod tests`, moved
                                wholesale
```

`main.rs` keeps `use app::{Action, App, ServerEvent};` and
`app::ui(frame, &app)` unchanged, because `mod.rs` re-exports them.

*Privacy.* Rust's restricted visibility is "visible in this module and its
descendants". A sibling module cannot see a private item of another module, so
the two items the split would strand are made `pub(crate)`:
`App::started_at` (read by `ui.rs` at what is today `app.rs:1233`) and the
`App` fields the key handlers read. `App::server_started` disappears instead
(decision 5). Everything the test module touches is already `pub`
(`App::files`, `App::log`, `App::mode`, `App::progress`, `App::status`,
`App::selected`, `App::shares`, `App::should_quit`, `App::search`,
`App::is_searching`, and the `impl App` methods it calls), so
`app/tests.rs` needs no new access. `Picker::dir` is already `#[cfg(test)]` in
`crates/app/src/picker.rs` and stays reachable from the app's test build.

*Alternative considered:* keep one file and only extract the server task and
the formatting helpers. Rejected: it leaves the state, the key handling and
the rendering in the same 3500-line file, which is where the churn actually
is.

*Alternative considered:* distribute the 2466 test lines across the new
modules. Rejected: the tests exercise the `App` as a whole (a key press, then
a rendered row, then a log line), and splitting them by file would either
duplicate the fixtures or scatter one behaviour across three files. One test
module, moved unchanged, keeps the split a pure move that a reviewer can read
as a move.

### 4. The test module moves unchanged, and the three dead-code removals in it are updated

`crates/app/src/app.rs:1826-4291` becomes `crates/app/src/app/tests.rs`. The
`use` block at the top of the module and the `#[cfg(test)]` attributes come
across as they are. Three call sites change because the code they call is
being deleted: `App::new` → `App::with_shares(files, Vec::new())`, the
`Action::Connect { selection }` assertion → the share-list selection the app
actually sets, and the `app.server_started` assertion → `app.status`.

### 5. `App::server_started` is deleted; the status carries it

`server_started` and `status` are set at the same five points and never
diverge:

| point | `status` | `server_started` |
|---|---|---|
| construction (`app.rs:296`, `:305`) | `Idle` | `false` |
| start pressed (`app.rs:733-734`) | `Waiting` | `true` |
| switch connected (`app.rs:322`) | `Running` | stays `true` |
| switch disconnected (`app.rs:327`) | `Waiting` | stays `true` |
| run ended (`app.rs:331-332`) | `Idle` | `false` |

So `server_started == (status != ServerStatus::Idle)` holds at every
reachable state, and the four read sites (`app.rs:691`, `:732`, `:1476` and
the test at `:2711`) read the derived value instead.

*Alternative considered:* keep the field and drop `status`. Rejected:
`status` distinguishes `Running` from `Waiting` for the header, and the
"server task is up" question is only one of the three things `status`
answers.

### 6. The share-path arithmetic moves to `SmbLocation`

`crates/core/src/source/mod.rs` gains:

```rust
impl SmbLocation {
    /// `<self.path>/<name>`; the share root has no leading or trailing slash.
    pub fn join(&self, name: &str) -> SmbLocation
    /// The parent path inside the share. `None` at the share root itself; a
    /// top-level entry's parent is the root, the empty path.
    pub fn parent_path(&self) -> Option<String>
}
```

`Source::parent` is deleted. `Source::join` stays — the share walk calls it at
`crates/core/src/filelist.rs:296` — but its share arm now delegates to
`SmbLocation::join`, so the path arithmetic exists once.

`SmbLocation::join` is exactly `join_share` (`picker.rs:357-363`): both yield
`name` when the path is empty and `{path}/{name}` otherwise.

`parent_path` is exactly `parent_share_path` (`picker.rs:367-375`) and is
**not** `Source::parent`: for a separator-less path such as `"a"`,
`parent_share_path` yields `Some("")` — the share root — while
`Source::parent` yields `None`. The picker's "go up" depends on the former:
from a top-level entry it must land on the share root rather than refuse to
move. `Source::parent` therefore had a different contract from the only
caller that existed, and it was the dead one.

### 7. Each consolidation is checked against the behaviour it replaces

The consolidations below are only done where the replacement is provably
equivalent. The ones that are not provably equivalent are left alone.

| consolidation | equivalence argument |
|---|---|
| `render_centered_hint(title, text, area)` replaces the skeleton at `app.rs:1323-1336`, `:1341-1354` and `:1546-1572` | the three blocks are the same twelve lines with a different title and message; the 45/1/55 band split and the centred `Paragraph` are identical |
| `render_table(...)` replaces the table builders at `app.rs:1309-1318`, `:1529-1538` and `:1600-1607` | all three build `Table::new(rows, widths).block(theme::block_title(title)).column_spacing(2).row_highlight_style(theme::selection()).highlight_symbol("> ")` and set a `TableState`; only the widths, the rows and the clamped index differ |
| `plural(count, singular, plural)` replaces the "N file(s)" forms at `app.rs:1249-1253`, `:1590-1594` and the four-way variant at `:1500-1510` | the three sites already agree on the singular/plural split; the helper keeps each site's own wording |
| one printable-character guard replaces the copies at `app.rs:835` and `:883` | `c.is_ascii_graphic() \|\| c == ' ' \|\| !c.is_control()` is exactly `!c.is_control()`: `is_control()` is true precisely for the C0 and C1 controls, and every one of those is neither ASCII-graphic nor a space, so the first two clauses never change the result |
| one share-validation function replaces `config::parse_share` (`config.rs:48-82`) and `config::build_share` (`config.rs:102-158`) | both require `name`/`server`/`share`/`username`, default the port to 445, parse `host:port` through the shared `parse_server`, treat an empty domain as `None` and trim `/` from `root`; the helper returns the same fields and each caller keeps its own error shape (`String` versus `Vec<String>`) and its own message wording |
| one `FileList::put` replaces the map insertion in `insert` (`filelist.rs:56-69`), `insert_source` (`:73-89`) and `record` (`:211-241`) | only the insertion itself is shared; `insert` and `insert_source` keep not canonicalising and not reporting a replacement, and `record` keeps doing both, because `insert` is used by `crates/core/tests/server_test.rs` and `insert_source` by both test crates |
| one reconnect helper replaces the three copies in `ShareHub` (`smb.rs:179-195`, `:200-216`, `:219-235`) | the three differ only in which session call they make and whether the listing is filtered; the "ensure live, try, rebuild once if the session died" sequence is byte-for-byte the same in all three |
| `SmbLocation::join` / `parent_path` replace `join_share` / `parent_share_path` | see decision 6 |
| one `split_version(raw: u32) -> (u8, u8, u8)` in `crates/core/src/metadata/mod.rs` replaces the copies at `cnmt.rs:52-56` and `filename.rs:80` | both expressions are `(raw >> 16) as u8, (raw >> 8) as u8, raw as u8` |
| one `extension_of` replaces `filters::extension_of` (`filters.rs:165-169`) and the copy inside `metadata::is_container_file` (`metadata/mod.rs:69-75`) | both take the last extension of the name and lowercase it; the shared helper keeps the *untrimmed* form, because `filters::normalize_extension` (`:157-162`) trims and is used on the config side, where a configured extension may carry a stray dot or space |
| one `MemoryHandle` replaces the two identical fakes at `metadata/mod.rs:111-137` and `metadata/container.rs:183-208` | the two `read_at` implementations are the same clamp-and-copy |
| one `meta_bytes` fixture replaces `metadata/mod.rs:160-171` and `cnmt.rs:76-87` | the two builders emit the same 0x20-byte content-meta header |
| one parameterised PFS0 builder replaces the four builders in `metadata/container.rs:213-293` and the one at `metadata/mod.rs:140-157` | all five write the same 0x10-byte header and the same member table; they differ only in whether the names are UTF-16 and whether the table is padded |

The consolidations that are **not** done, and why:

- **The accepted-extension set and the include patterns stay intersected.**
  `Filters::accepts` (`filters.rs:110-131`) requires the extension to be in the
  set *and* the name to match an include pattern, so `--ext xci` together with
  `include = ["*.nsp"]` currently accepts nothing. Changing that to a union
  would change which files are offered — a behaviour change.
- **The picker's symlink policy stays different from the scanner's.**
  `picker.rs:385-400` classifies a directory symlink as a directory and
  `picker.rs:302-311` descends into it, while `source/local.rs:96-99` and
  `filelist.rs:150-162` deliberately refuse to descend into directory
  symlinks. Unifying them would change what the picker lists, and the
  picker's version can walk a cycle.
- **`log_color`'s `"Failed"` branch stays.** `crates/core/src/server.rs:163`
  emits `"Failed to open {source}: {e}"`, and `spawn_server` forwards it to
  the app's log through `ServerUpdate::Log`, so the branch is live.
- **The startup warning stays in two places.** `main.rs:49-54` prints it to
  stderr before the TUI takes over the screen and `app.rs:1013-1020` logs it
  again so it survives in the alternate screen. The two spellings differ in
  capitalisation (`"warning: "` versus `"Warning: "`) because `log_color`
  matches the capitalised prefix. Only the message construction is shared;
  both strings stay byte-identical.
- **`ServerError::Protocol` and `ServerError::Range` stay separate
  variants.** They carry the same payload but render different text, and the
  text is what the log shows.

### 8. `Server::process_file_range` is split into named steps

`crates/core/src/server.rs:129-263` is 135 lines holding four concerns: the
length guard and the acknowledgement, the range-header parse, the stream-state
bookkeeping (`:174-191`), the streaming loop (`:208-243`) and the completion
bookkeeping (`:247-261`). It becomes:

- `ack_range(data_size)` — the guard and the acknowledgement;
- `read_range_header(data_size)` — the parse, returning `ServerError::Protocol`;
- `range_stream_state(&mut self, range) -> (bool, bool)` — the `(track,
  announce_start)` decision, unchanged;
- `stream_range(&mut self, handle, source, range, track, log)` — the loop;
- `finish_range(&mut self, range, sent, total, track, log)` — the completion.

Three excesses go with the split, and all three are behaviour-preserving:

- `self.files.get(&range.name).cloned()` (`:153`) clones the whole `FileEntry`
  — the `Source` plus the identity's optional strings — and `:157` clones the
  `Source` again. The entry is borrowed, the `Source` is cloned once, and the
  size is copied out; the identity is never needed.
- `let mut buffer = vec![0u8; remaining.min(BUFFER_SEGMENT_DATA_SIZE)]`
  (`:207`) allocates and zeroes up to 1 MiB for every request. The buffer
  becomes a field on `Server` and is reused. Only `buffer[..n]` is ever
  written to the transport, so the bytes sent are the same.
- `build_list(self.files.names().cloned())` (`:118`) materialises a
  `Vec<String>` of every name and then copies each name into the payload
  again. `build_list` takes `impl IntoIterator<Item = impl AsRef<str>>` and
  `FileList::names` yields `&str`.

### 9. The reconnect helper takes the session call as a closure

`ShareHub::open`, `list_dir` and `stat` all follow the same shape: resolve the
path, find the share entry, ensure the session is live, call the session, and
on failure rebuild once if the session died. The helper takes the session call
as an async closure over the session so the three callers keep their own
request and their own post-processing:

```rust
async fn with_reconnect<T>(
    &mut self,
    source: &Source,
    call: impl for<'a> FnOnce(&'a mut dyn ShareSession) -> BoxFuture<'a, Result<T, SourceError>>,
) -> Result<T, SourceError>
```

The "is this a share?" check runs once instead of twice: `share_path`
(`smb.rs:284-289`) fails with `"… is not a share"` before
`entry_for_source` (`smb.rs:273-281`) is reached, so the second check's error
arm is unreachable and is deleted.

*Alternative considered:* a macro over the three call sites. Rejected: the
borrow of `self.shares` across an `await` inside a macro is harder to read
than an explicit helper.

### 10. The dead-code deletions

| item | where | why it is safe |
|---|---|---|
| `CMD_ID_LIST_OLD` | `protocol.rs:11` | the command loop already ignores every id it does not match, and the protocol spec says unknown ids are ignored |
| `Source::join`, `Source::parent` | `source/mod.rs:38-73` | `Source::parent` has no call site and its contract differed from the only caller that existed, so it is deleted; `Source::join` is called by the share walk (`filelist.rs:296`) and stays, but its share arm delegates to `SmbLocation::join` so the path arithmetic exists once |
| `App::new` and its `allow(dead_code)` | `app.rs:285-288` | the only production constructor is `App::with_shares`; `App::new` is called from the test module only |
| `Action::Connect.selection` | `app.rs:52` | `main.rs:149` discards it and the app already sets the share-list selection itself at `app.rs:901-903`; only a test read it |
| `App::share_added`'s return value | `app.rs:935` | `main.rs:182` discards it; the function's effect is the mutation |
| `App::share_added`'s "defensive replace" branch | `app.rs:941-947` | `config::build_share` already rejects a duplicate name (`config.rs:124-126`) and `share_added` is only ever called with a form-built config |
| `picker::Kind` and its mapping | `picker.rs:11-16`, `:168-171` | `core::source::EntryKind` is the same two-variant enum |
| `Picker::origin_display` | `picker.rs:224-226` | a one-line pass-through to `Origin::display`; the app calls `origin_display` in one place, so the call site uses `Origin::display` directly |
| `ShareHub::is_connected` becomes test-only | `smb.rs:162-172` | the only callers are `smb.rs`'s own tests; the app tracks connection state in the UI |
| `App::search` / `App::searching` getters | `app.rs:611-618` | the fields are `pub` and read directly at `app.rs:1471-1472`; the getters and the fields are two ways to read one value, so the fields become private and the getters become the only way in |

## Risks / Trade-offs

- **[The module split is a large diff] →** The move itself is mechanical and
  `git diff -M` shows it as renames; it lands as its own commit, before any
  behaviour-adjacent edit, so a reviewer can read it as a move. The
  re-exports in `app/mod.rs` keep `main.rs` untouched, which keeps the
  blast radius at one directory.
- **[A consolidation changes a message or a number by accident] →** Every
  message string and every rendered number is asserted by the existing tests;
  the consolidations are only done where the replacement text is identical,
  and the tests are the check. The full suite must stay green at every step,
  not only at the end.
- **[Removing `server_started` breaks a case the tests do not cover] →** The
  equivalence is argued from the five assignment sites in decision 5, and it
  holds by construction rather than by coincidence: the field is only ever
  written next to a status assignment. The one test that reads it
  (`app.rs:2711`) is rewritten to read the status.
- **[Wiring up the previously unreachable tests fails in CI] →** The block
  passes today (`cargo test -p dbi-backend-core --features test-util` is
  green), and CI runs the same tests. The risk is a platform difference, not
  a logic problem; the tests use only `tempfile` and the in-memory store.
- **[The `ShareHub` reconnect helper does not borrow-check] →** The helper
  holds `&mut self.shares` across an `await` inside a boxed future; if the
  closure form fights the borrow checker, the fallback is a macro that
  expands to the same three-step body at each call site — which is what the
  code looks like today, so the fallback is a no-op relative to the current
  state.
- **[The reusable buffer in the server hides stale bytes] →** Only
  `buffer[..n]` is written to the transport, and `n` is the number of bytes
  the read actually produced, so a stale tail is never sent. The existing
  multi-chunk and short-transfer tests cover the loop.
- **[Consolidating the test fixtures touches many tests] →** The fixtures are
  test-only, so a mistake there is caught by the test suite immediately and
  never reaches a user.
- **[The `Filters` intersection and the picker's symlink policy stay
  inconsistent] →** They are recorded as follow-ups rather than fixed here.
  Leaving them visible is the point: this change makes the codebase easier
  to change, and those two need a decision about behaviour, not a refactor.

## Migration Plan

There is no migration: no data, no config format, no protocol and no
dependency changes. The change is applied in the order given by `tasks.md`,
with `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` green after every numbered group. To roll back, revert
the commits; nothing outside the repository is affected.

## Open Questions

None. The decisions that would have changed the approach — the scope of the
cleanup, the fate of the unreachable test block, and the size of the module
split — were settled before this document was written.
