# Spec Delta

## MODIFIED Requirements

### Requirement: Entries are keyed by file name

The system SHALL key each entry by the file's name, and adding an entry whose
name already exists SHALL replace the previous path and report that a
replacement occurred.

#### Scenario: Duplicate names overwrite

- **WHEN** two files named `game.nsp` from different directories are added
- **THEN** the file list holds a single `game.nsp` entry and the replacement is reported

## ADDED Requirements

### Requirement: Directory scans tolerate unreadable entries

When scanning a directory, the system SHALL continue past an entry or
subdirectory that cannot be read, SHALL include every readable matching file,
and SHALL report the paths it skipped.

#### Scenario: Unreadable subdirectory is skipped

- **WHEN** a scanned directory contains an unreadable subdirectory and a readable `a.nsp`
- **THEN** `a.nsp` is added and the skipped subdirectory is reported
