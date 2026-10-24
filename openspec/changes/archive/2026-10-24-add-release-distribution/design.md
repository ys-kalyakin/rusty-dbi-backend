# Design

## Context

CI today is one ubuntu job running fmt, clippy, and tests (`.github/workflows/ci.yml`).
The workspace holds two crates — `dbi-backend-core` (lib) and `dbi-backend` (bin) — with
workspace-inherited `version`/`license` but none of the metadata `cargo publish` needs
(no `description`, `repository`, `readme`). The product code is already portable: `nusb`
ships WinUSB/macOS backends, the local source has a `cfg(not(unix))` `read_at` path, the
only unix-gated non-test code is one permission test, and the config path resolves per OS
through `dirs::config_dir()`. The remote is `github.com/ys-kalyakin/rusty-dbi-backend`,
so GitHub Actions is the existing CI substrate. As of 2026-10-24 the crates.io API
returns 404 for `dbi-backend`, so the crate name is free.

## Goals / Non-Goals

**Goals:**

- One `git push origin v<x.y.z>` produces everything: four platform archives, checksums,
  a GitHub release, and both crates on crates.io.
- Platform breakage surfaces on every push/PR, not at release time.
- The pipeline is plain, readable GitHub Actions YAML with no external generator.

**Non-Goals:**

- Code signing (Windows binaries will be unsigned and may trip SmartScreen).
- Changelog generation and automated version bumps (`release-plz`-style).
- `cargo-binstall` metadata, distro packages, musl static builds (proposal lists these out
  of scope).
- Any change to the product's runtime behavior.

## Decisions

### Hand-rolled workflow matrix, not cargo-dist

A `release.yml` with a per-target matrix plus a small packaging job does everything the
specs ask in a readable, dependency-free form. `cargo-dist` would also emit installer
scripts and cargo-binstall support, but it brings a generator whose conventions must be
learned and kept in sync, for a project that needs four archives and a checksum file.
`release-please`/`release-plz` automate changelogs and version bumps we are not doing.

### Native runners, no cross-compilation

Targets: `x86_64-pc-windows-msvc` on `windows-latest`, `x86_64-unknown-linux-gnu` on
`ubuntu-latest`, and both `x86_64-apple-darwin` and `aarch64-apple-darwin` in one
`macos-latest` job. Cross-compiling MSVC or Apple targets from Linux needs SDK plumbing
that costs more than the native minutes; an Apple Silicon runner builds both Apple targets
natively (the x86_64 target cross-builds on arm64 macOS with the stock toolchain). All
builds run `cargo build --release --locked`; a tag whose tree needs lockfile changes fails
instead of releasing something unreviewed.

### Release flow: build → one packaging job → one publish job

The tag-triggered matrix uploads raw binaries; a single `release` job on ubuntu downloads
all four, packs `dbi-backend-<version>-<target>.zip` (Windows, `Compress-Archive`) or
`.tar.gz` (unix, `tar -czf`), writes one `SHA256SUMS.txt` covering every archive, and
creates the GitHub release via `gh release create`. A separate `publish` job then runs
`cargo publish -p dbi-backend-core` and, after it, `-p dbi-backend`, authenticated by a
`CARGO_REGISTRY_TOKEN` secret. Splitting publish from packaging means a packaging failure
leaves nothing on crates.io, and the publish order lives in one obvious place. A first
step asserts the tag name equals the workspace version, so the tag, the GitHub release,
and the crates.io version can never disagree.

### Metadata through workspace inheritance

`[workspace.package]` gains `description`, `repository`, `keywords`, `categories`, and
`readme = "../../README.md"`; both crates set the corresponding `*.workspace = true`.
Cargo has accepted a readme outside the package root since 1.44, and both members sit at
the same depth (`crates/<crate>`), so one inherited path serves both. Ubuntu CI gains a
`cargo publish --dry-run` step (`-p` each crate) so metadata regressions fail the pull
request, not the release.

### CI: a test matrix beside a Linux-only lint job

`ci.yml` keeps its lint job exactly as is (fmt + clippy + the toolchain components on
ubuntu) and turns the test run into a matrix over the four targets. The suite is expected
to be portable: the unix-only permission test is `#[cfg(unix)]`-gated and integration
tests use tempdirs and mock sources, not device access.

### Docs: a Platform notes umbrella in the README

The "Linux permissions" section joins new "Windows" and "macOS" sections under one
"Platform notes" heading. Windows covers Zadig (Options → List All Devices, pick the
Switch, VID `057E` PID `3000`, install WinUSB or libusbK — DBI's own docs treat either as
valid) and states that HTTP "Home server" installs skip driver setup entirely. macOS
states both architectures are supported and no driver or permission setup exists there.
The config path is listed per OS (`~/.config/dbi-backend/`,
`~/Library/Application Support/dbi-backend/`, `%APPDATA%\dbi-backend\`), and the SMB
section's `~/.config/...` mention plus the same path in `config.rs`'s doc comment move to
the neutral "the config file" wording with the per-OS list nearby.

## Risks / Trade-offs

- [New platform legs may expose latent unix assumptions in tests] → The matrix lands
  before any release tag; discovered issues are fixed or `cfg`-gated the way the existing
  permission test is.
- [Re-running the publish job after a partial failure hits "already exists"] → The job
  fails loudly on the second run of the same tag; the fix is publishing the missing crate
  alone, and the error text says exactly that. Accepted over skip-if-exists logic, which
  could mask real failures.
- [Publishing the app right after core fails while the sparse registry index catches
  up] → The app publish step retries five times, 30 seconds apart.
- [Unsigned Windows exe trips SmartScreen] → Documented in the release notes template;
  real signing is out of scope until someone owns a certificate.
- [Linux gnu binary needs a recent glibc (ubuntu-latest runner)] → Accepted; musl static
  builds stay a non-goal. Anyone stuck on an old distro can build from source or use
  `cargo install`.
- [crates.io name taken between now and the first release] → Unlikely and cheap to
  detect: the CI dry-run keeps confirming the metadata; if `dbi-backend` is squatted, only
  the bin crate's `name` and the docs change.

## Migration Plan

Additive: two workflow files change, `Cargo.toml`s gain metadata fields, README gains
sections. Nothing about local development changes. Rollout: merge, set the
`CARGO_REGISTRY_TOKEN` secret, then push `v0.1.0` (after bumping the workspace version if
0.1.0 was already consumed by local builds — the tag/version assert catches a mismatch
before anything publishes). Rollback is `git revert` of the workflow changes; published
crates and releases are permanent, which is why publish is the last, separately-failing
job.

## Open Questions

- Exact Zadig wording to mirror (DBI's docs describe both WinUSB and libusbK; the README
  will recommend one and mention the other) — content detail, resolvable while writing the
  docs without touching the approach.
