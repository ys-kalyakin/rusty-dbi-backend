# Spec Delta

## RENAMED Requirements

- FROM: `### Requirement: Directory scans include only NSP and NSZ files`
- TO: `### Requirement: Directory scans include only filtered files`
- FROM: `### Requirement: Adding a single file ignores extension filtering`
- TO: `### Requirement: A directly added file is added unfiltered`

## MODIFIED Requirements

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
