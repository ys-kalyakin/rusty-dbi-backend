# Tasks

## 1. crates.io publish metadata

- [x] 1.1 Add publish metadata to `[workspace.package]` (description, repository, `readme = "README.md"` — resolved from the workspace root, keywords) and inherit it in both crates via `*.workspace = true`, giving the app a `categories` override; verify `cargo publish --dry-run -p dbi-backend-core` succeeds and a `cargo metadata --no-deps` field check shows description/license/readme/repository on both crates (the app's own dry-run cannot resolve the not-yet-published core dep, so it is covered by the metadata check until the first release)
- [x] 1.2 Confirm the version-tag contract is expressible: add the tag-vs-workspace-version check as a small script step source (shell, readable by the release job) and verify locally that it passes for a tag matching `version.workspace` and fails for one that does not

## 2. CI: per-platform matrix and publish readiness

- [x] 2.1 Turn the test run in `.github/workflows/ci.yml` into a matrix — `windows-latest` (x86_64-pc-windows-msvc), `ubuntu-latest` (x86_64-unknown-linux-gnu), `macos-latest` (both x86_64- and aarch64-apple-darwin) — keeping fmt and clippy on the existing Linux job; verify locally that `cargo check --workspace --all-targets` passes for all three non-host targets (the symlink test needed a `#[cfg(unix)]` gate) — the on-CI observation lands with task 5.2 once the branch is pushed
- [x] 2.2 Add a `publish-readiness` CI job: a full `cargo publish --dry-run -p dbi-backend-core` plus a `cargo metadata --no-deps` field check over both crates (the app crate's own dry-run cannot resolve the unpublished core dep), and verify both commands locally — the on-CI run lands with task 5.2

## 3. Release workflow

- [x] 3.1 Create `.github/workflows/release.yml` with a `v*` tag trigger, the tag/version assertion, and a build matrix (`cargo build --release --locked`) uploading each platform's binary as an artifact, and verify the YAML parses and the job graph (guard → build → release → publish) is well-formed, e.g. with actionlint or an equivalent YAML parse
- [x] 3.2 Add the packaging job: download all four artifacts, pack `dbi-backend-<version>-<target>.zip` (Windows) / `.tar.gz` (unix), write `SHA256SUMS.txt`, and create the GitHub release via `gh release create`; verify by inspecting the job that checksums are computed over the final archives and that the Windows archive is the only one carrying an `.exe`
- [x] 3.3 Add the crates.io publish job (`CARGO_REGISTRY_TOKEN` secret; `-p dbi-backend-core` before `-p dbi-backend`, retried while the sparse index catches up) depending on the release job, and verify in the workflow file that it is reachable only from tag builds and ordered after release creation

## 4. Documentation

- [x] 4.1 Reorganize the README's platform material under a "Platform notes" heading, add the Windows section — Zadig (List All Devices, the Switch at VID `057E`/PID `3000`), WinUSB or libusbK per DBI's docs, and the note that HTTP installs need no driver — and verify the device IDs quoted match the udev rule and the README states the driver-free HTTP path
- [x] 4.2 Add the macOS section (Intel and Apple Silicon supported, USB and network installs, no driver or permission setup) and verify every claim it makes is covered by the CI matrix legs from task 2.1
- [x] 4.3 List the config file location per OS (`~/.config/dbi-backend/`, `~/Library/Application Support/dbi-backend/`, `%APPDATA%\dbi-backend\`), neutralize the bare `~/.config/...` mention in the SMB section and in `config.rs`'s doc comment, and verify the README's toml blocks still load warning-free in the config tests
- [x] 4.4 Add an "Install" section pointing at GitHub Releases (per-platform archive names, checksum verification) and `cargo install dbi-backend`, and verify the archive names quoted match the release job's naming from task 3.2

## 5. Integration verification

- [x] 5.1 Run the full gate — `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` — and verify all three pass
- [ ] 5.2 Open the pull request and verify the checks show one lint job and four platform test legs, all green, plus the publish dry-run step

## Workflow follow-up

- Set the `CARGO_REGISTRY_TOKEN` secret on the repository before the first tagged release.
- Push the first version tag and verify the release holds all four archives plus `SHA256SUMS.txt` with matching digests, and both crates appear on crates.io (core first).
- Field-test on Windows: Zadig driver install per the README, then a USB install and an HTTP install against a real console.
- Archive the change after the release verification.
