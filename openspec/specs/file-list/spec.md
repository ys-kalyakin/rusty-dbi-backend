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

### Requirement: Directory scans include only filtered files

When scanning a directory, the system SHALL add only regular files that the
configured filters accept, and SHALL ignore every other file and directory
entry. The filters' rules — the accepted extension set and the include and
exclude patterns — are defined by the `filters` capability.

#### Scenario: Non-matching files are skipped

- **WHEN** a directory containing `game.nsp`, `notes.txt`, and `cover.jpg` is added
- **THEN** only `game.nsp` is present in the file list

#### Scenario: Extension matching ignores case

- **WHEN** a directory containing `GAME.NSP` and `Patch.NsZ` is added
- **THEN** both files are present in the file list

#### Scenario: A file the filters reject is skipped

- **WHEN** a directory contains a file that the configured filters do not accept
- **THEN** that file is not present in the file list

### Requirement: A directly added file is added unfiltered

When a path that is a regular file is added directly, the system SHALL add that
file regardless of its extension and regardless of the configured include and
exclude patterns, because the user named that file explicitly.

#### Scenario: Explicit file with another extension is added

- **WHEN** a regular file named `readme.txt` is added directly
- **THEN** `readme.txt` is present in the file list

#### Scenario: An explicitly named file ignores the patterns

- **WHEN** an exclude pattern matches the name of a regular file that is added directly
- **THEN** that file is present in the file list

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
recursively over the share's listings, SHALL add only the files the configured
filters accept, and SHALL continue past entries or subdirectories that cannot
be listed, reporting each skipped path.

#### Scenario: A share directory's matching files are added with their sizes

- **WHEN** a share directory containing `a.nsp` and `sub/b.nsz` is added
- **THEN** both files are present in the file list with the sizes reported by the share listing

#### Scenario: An unlistable share subdirectory is skipped and reported

- **WHEN** a scanned share directory contains a subdirectory whose listing fails and a readable `a.nsp`
- **THEN** `a.nsp` is added, the failed subdirectory is reported, and the scan continues

#### Scenario: The same filters apply over a share

- **WHEN** a share directory contains a file the configured filters do not accept
- **THEN** that file is not added to the file list

### Requirement: A directly added share file ignores extension filtering

When a file on a connected share is added directly, the system SHALL add it
regardless of its extension.

#### Scenario: An explicit share file with another extension is added

- **WHEN** a share file named `readme.txt` is added directly
- **THEN** `readme.txt` is present in the file list

### Requirement: Directory scans stay finite

A directory add SHALL terminate even when a listing reports the self and
parent pseudo-entries `.` and `..` as directories: such entries SHALL NOT be
descended into. The reporting when a directory is skipped remains as defined
in the requirement "Directory scans tolerate unreadable entries".

#### Scenario: Dot pseudo-entries do not loop the walk

- **WHEN** a scanned directory's listing reports `.` and `..` as directories
- **THEN** the add terminates quickly and lists only the real matching files

#### Scenario: A directory with only dot entries adds nothing

- **WHEN** a directory is added whose listing contains only `.` and `..`
- **THEN** the add terminates quickly and the file list is unchanged

### Requirement: Entries carry the title identity

When a file is added, the system SHALL record the identity of its container —
the title id, the content type and the version — whenever one could be
determined, and SHALL make that identity available alongside the entry's name
and size. Reading the identity SHALL NOT keep the file from being added.

#### Scenario: The identity is recorded on add

- **WHEN** a file whose title id can be determined is added
- **THEN** the entry reports that title id, the content type and the version

#### Scenario: The identity is available for every entry

- **WHEN** the file list is read
- **THEN** each entry exposes an identity, empty for a file whose identity could not be determined

#### Scenario: A file that cannot be read for its identity is still added

- **WHEN** a file is added whose container cannot be read
- **THEN** the entry is present in the file list and carries no identity
