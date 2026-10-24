# Proposal

## Why

DBI's install-over-USB audience lives mostly on Windows, but the project ships
nothing a Windows user can run: CI only runs fmt/clippy/test on Linux, there
are no release binaries, no `cargo publish` path (the crates carry none of the
metadata publishing requires), and the README documents Linux permissions only
— so a Windows user gets neither a build nor the WinUSB/Zadig driver notes
DBI's own documentation treats as required, and macOS, which nusb supports for
free, is not documented at all.

## What Changes

- A release workflow that, on a pushed `v*` tag, builds release binaries for
  Windows x86_64 (MSVC), Linux x86_64 (GNU), and macOS x86_64 and aarch64,
  packages each as one archive per platform (`.zip` on Windows, `.tar.gz`
  elsewhere) together with SHA-256 checksums, and publishes them as a GitHub
  release.
- CI grows from a single Linux job into a build/test matrix over the same
  platforms, so Windows and macOS breakage surfaces on every push and pull
  request instead of at release time; fmt and clippy stay on Linux.
- Both crates gain the metadata `cargo publish` requires — `description`,
  `repository`, `readme`, `keywords`, `categories` — and the release workflow
  publishes them to crates.io in dependency order (`dbi-backend-core` first,
  then `dbi-backend`) using a registry token secret.
- The README gains a Windows section — Zadig/WinUSB driver setup for USB
  installs in the spirit of DBI's docs, noting that HTTP "Home server" installs
  need no driver — a macOS section, and the config path spelled out per OS
  (today it is shown only as `~/.config/dbi-backend/`).
- Out of scope: distro packages (`.deb`/`.rpm`), Homebrew/Chocolatey, static
  musl builds, and Windows arm64.

## Capabilities

### New Capabilities

- `distribution`: how the backend is built, verified, and shipped across
  platforms — the platform matrix CI verifies on every change, the release
  artifacts a version tag produces, crates.io publishing readiness, and the
  platform-specific setup documentation (Windows drivers, macOS notes) users
  install by.

### Modified Capabilities

(none — no existing capability's requirements change; the product's runtime
behavior is untouched.)

## Impact

- `.github/workflows/ci.yml`: the check job becomes a per-platform matrix
  (fmt/clippy on Linux only, build and test everywhere).
- `.github/workflows/release.yml`: new — tag-triggered matrix builds,
  packaging, checksums, GitHub release, and crates.io publish.
- `Cargo.toml` (workspace and both crates): crates.io publish metadata only;
  versions stay workspace-managed.
- `README.md`: new Windows and macOS sections, config path per OS; the Linux
  permissions section stays as is.
- No product code changes are expected: `nusb` is cross-platform, the local
  source already has a non-unix `read_at` path, and the app resolves its
  config path per OS through `dirs` today.
