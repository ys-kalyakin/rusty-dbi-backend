# Spec Delta

## Purpose

Defines how paths are discovered and mapped to the file names offered to the
Switch, so that a user-provided directory exposes the installable titles it
contains.

## ADDED Requirements

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
name already exists SHALL replace the previous path.

#### Scenario: Duplicate names overwrite

- **WHEN** two files named `game.nsp` from different directories are added
- **THEN** the file list holds a single `game.nsp` entry
