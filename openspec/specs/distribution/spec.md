# distribution Specification

## Purpose

How the backend reaches users on Windows, macOS, and Linux: the platform
matrix CI verifies on every change, the release artifacts and crates.io
publishes a version tag produces, and the platform-specific setup
documentation a user installs by.

## Requirements

### Requirement: CI builds and tests every supported platform

On every push and pull request, the project SHALL build the workspace and run
its test suite on each supported platform: Windows x86_64, Linux x86_64, macOS
x86_64, and macOS aarch64. The formatting and lint checks SHALL continue to
run on Linux.

#### Scenario: A pull request runs tests on every platform

- **WHEN** a pull request is opened
- **THEN** the test suite runs on Windows x86_64, Linux x86_64, macOS x86_64, and macOS aarch64

#### Scenario: A Windows-only break fails the pull request

- **WHEN** a change compiles on Linux but not on Windows
- **THEN** the pull request's check fails on the Windows leg

#### Scenario: Formatting and lint stay Linux-only

- **WHEN** the checks run
- **THEN** the format check and the clippy check each run on exactly one platform, Linux

### Requirement: A version tag produces a per-platform release

Pushing a tag of the form `v<version>` SHALL build the release binary for
every supported platform and publish a GitHub release named after the tag
holding one archive per platform — a `.zip` on Windows, a `.tar.gz` elsewhere,
the binary carrying a `.exe` suffix only on Windows. No release SHALL be
produced from builds that are not triggered by such a tag.

#### Scenario: A tag publishes every platform's archive

- **WHEN** a `v<version>` tag is pushed
- **THEN** the release holds four archives, one each for Windows x86_64, Linux x86_64, macOS x86_64, and macOS aarch64

#### Scenario: The Windows archive carries an exe

- **WHEN** the Windows archive is unpacked
- **THEN** it contains the backend's binary named with a `.exe` suffix

#### Scenario: An untagged build publishes nothing

- **WHEN** CI runs on an ordinary push or pull request
- **THEN** no release or release artifacts are published

### Requirement: Release archives are checksummed and reproducible

Each release SHALL carry one checksum file covering every published archive
with its SHA-256 digest. Release builds SHALL use the workspace's committed
lockfile and SHALL fail if the lockfile would change.

#### Scenario: Checksums cover every archive

- **WHEN** the release's checksum file is checked
- **THEN** it names every published archive and each named SHA-256 matches that archive

#### Scenario: A stale lockfile fails the release

- **WHEN** a tagged build would change the committed lockfile
- **THEN** the build fails and no release is published

### Requirement: A release publishes the crates to crates.io

A release triggered by a version tag SHALL publish both crates to crates.io in
dependency order — the core library first, then the application — and each
crate SHALL carry the metadata crates.io requires: a description, a license,
a repository link, and a readme. Publishing SHALL NOT run on pushes or pull
requests.

#### Scenario: A tag publishes core before the application

- **WHEN** a release triggered by a version tag publishes
- **THEN** the core library is published first and the application crate is published after it

#### Scenario: A dry-run publish passes with the crate metadata

- **WHEN** the crate metadata is validated with a publish dry-run
- **THEN** it succeeds without complaining about missing description, license, repository, or readme

#### Scenario: An untagged build publishes no crates

- **WHEN** CI runs on an ordinary push or pull request
- **THEN** nothing is sent to crates.io

### Requirement: Windows USB setup is documented

The documentation SHALL explain that USB installs on Windows need a
WinUSB-compatible driver bound to the Switch device (vendor `0x057E`, product
`0x3000`), that the driver is installed with Zadig by choosing WinUSB or
libusbK — following DBI's own guidance — and that HTTP "Home server" installs
need no driver at all.

#### Scenario: The README explains the Zadig step

- **WHEN** the README's Windows section is read
- **THEN** it names Zadig, the WinUSB or libusbK driver choice, and the Switch device it applies to

#### Scenario: The driver-free path is called out

- **WHEN** the README's Windows section is read
- **THEN** it states that installing over HTTP needs no driver setup

### Requirement: macOS support is documented

The documentation SHALL state that the backend runs on macOS, on both Intel
and Apple Silicon Macs, over USB and network installs alike, and that no
driver installation or udev-style permission setup is needed there — plugging
the cable is the whole setup.

#### Scenario: The README covers both Mac architectures

- **WHEN** the README's macOS section is read
- **THEN** it names both x86_64 and aarch64 Macs as supported

#### Scenario: No-setup claim is stated

- **WHEN** the README's macOS section is read
- **THEN** it states that no driver or permission setup is required on macOS

### Requirement: The config file location is documented per OS

The documentation SHALL state where the configuration file lives on each
supported operating system rather than showing a Unix path alone.

#### Scenario: Each OS gets its config path

- **WHEN** the README's configuration location is read
- **THEN** it names the path on Linux, on macOS, and on Windows
