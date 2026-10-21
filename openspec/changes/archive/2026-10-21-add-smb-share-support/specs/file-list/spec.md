# Spec Delta

## ADDED Requirements

### Requirement: Entries carry a source

Each entry SHALL carry a source identifying where its bytes come from — a
local filesystem path or a location on a connected share — and SHALL make that
source available alongside the entry's name and size, so the server can stream
from either kind.

#### Scenario: A local file records a local source

- **WHEN** a local file is added
- **THEN** the entry's source names its local path

#### Scenario: A share file records a share source

- **WHEN** a file is added from a connected share
- **THEN** the entry's source names that share and the file's path within it

### Requirement: Adding a share directory scans it over the share

When a directory on a connected share is added, the system SHALL scan it
recursively over the share's listings, SHALL add only `.nsp` and `.nsz` files
matching the extension case-insensitively, and SHALL continue past entries or
subdirectories that cannot be listed, reporting each skipped path.

#### Scenario: A share directory's matching files are added with their sizes

- **WHEN** a share directory containing `a.nsp` and `sub/b.nsz` is added
- **THEN** both files are present in the file list with the sizes reported by the share listing

#### Scenario: An unlistable share subdirectory is skipped and reported

- **WHEN** a scanned share directory contains a subdirectory whose listing fails and a readable `a.nsp`
- **THEN** `a.nsp` is added, the failed subdirectory is reported, and the scan continues

### Requirement: A directly added share file ignores extension filtering

When a file on a connected share is added directly, the system SHALL add it
regardless of its extension.

#### Scenario: An explicit share file with another extension is added

- **WHEN** a share file named `readme.txt` is added directly
- **THEN** `readme.txt` is present in the file list
