# file-list Specification

## Purpose

Defines how paths are discovered and mapped to the file names offered to the
Switch, so that a user-provided directory exposes the installable titles it
contains.

## Requirements

### Requirement: Adding a directory scans it recursively

When a directory is added, the system SHALL include matching files from that
directory and from every nested subdirectory, at any depth.

#### Scenario: Matching files in nested subdirectories are included

- **WHEN** a directory is added that contains `a.nsp` and `sub/deep/b.nsz`
- **THEN** both `a.nsp` and `b.nsz` are present in the file list

#### Scenario: Adding an empty directory adds nothing

- **WHEN** a directory with no matching files at any depth is added
- **THEN** the add succeeds and the file list is unchanged

### Requirement: Directory scans include only NSP and NSZ files

When scanning a directory, the system SHALL add only regular files whose
extension is `.nsp` or `.nsz`, matching the extension case-insensitively, and
SHALL ignore every other file and directory entry.

#### Scenario: Non-matching files are skipped

- **WHEN** a directory containing `game.nsp`, `notes.txt`, and `cover.jpg` is added
- **THEN** only `game.nsp` is present in the file list

#### Scenario: Extension matching ignores case

- **WHEN** a directory containing `GAME.NSP` and `Patch.NsZ` is added
- **THEN** both files are present in the file list

### Requirement: Adding a single file ignores extension filtering

When a path that is a regular file is added directly, the system SHALL add that
file regardless of its extension.

#### Scenario: Explicit file with another extension is added

- **WHEN** a regular file named `readme.txt` is added directly
- **THEN** `readme.txt` is present in the file list

### Requirement: Entries are keyed by file name

The system SHALL key each entry by the file's name, and adding an entry whose
name already exists SHALL replace the previous path and report that a
replacement occurred.

#### Scenario: Duplicate names overwrite

- **WHEN** two files named `game.nsp` from different directories are added
- **THEN** the file list holds a single `game.nsp` entry and the replacement is reported

### Requirement: Directory scans tolerate unreadable entries

When scanning a directory, the system SHALL continue past an entry or
subdirectory that cannot be read, SHALL include every readable matching file,
and SHALL report the paths it skipped.

#### Scenario: Unreadable subdirectory is skipped

- **WHEN** a scanned directory contains an unreadable subdirectory and a readable `a.nsp`
- **THEN** `a.nsp` is added and the skipped subdirectory is reported

### Requirement: Entries expose file size

When a file is added, the system SHALL record its size in bytes and SHALL make that size available alongside the entry's name and path, so callers can display it.

#### Scenario: Size is recorded on add

- **WHEN** a file of a known size is added
- **THEN** the entry reports that file's size in bytes

#### Scenario: Size is available for every entry

- **WHEN** the file list is read
- **THEN** each entry exposes a size value without re-reading the filesystem

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
