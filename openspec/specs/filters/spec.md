# filters Specification

## Purpose

Defines how the set of files offered to the Switch is narrowed: which
extensions a directory scan accepts, which include and exclude patterns
apply, where those settings come from, and how a pattern is matched.

## Requirements

### Requirement: The accepted extension set is configurable

The system SHALL narrow every directory scan to regular files whose extension
is in an accepted extension set, matched case-insensitively. The accepted set
SHALL default to `nsp` and `nsz`.

#### Scenario: The default set accepts NSP and NSZ files

- **WHEN** no accepted extension set is configured
- **THEN** a directory scan accepts files ending in `nsp` or `nsz`, ignoring case, and skips every other file

#### Scenario: A configured set replaces the default

- **WHEN** an accepted extension set of `xci` and `nsp` is configured
- **THEN** a directory scan accepts files ending in `xci` or `nsp` and skips files ending in `nsz`

#### Scenario: An unset or empty set means the default

- **WHEN** the accepted extension set is unset or empty
- **THEN** the accepted set is `nsp` and `nsz`

#### Scenario: The extension is matched case-insensitively

- **WHEN** a directory contains `GAME.NSP` and `Patch.NsZ`
- **THEN** both files are accepted by the default extension set

### Requirement: Include and exclude patterns narrow a scan

The system SHALL apply include and exclude patterns to the files a directory
scan considers. A pattern is matched against the file name, ignoring case,
using `*`, `?` and character-class syntax. A file matching an exclude pattern
SHALL be skipped. When at least one include pattern is configured, a file
matching no include pattern SHALL be skipped.

#### Scenario: An excluded file is skipped

- **WHEN** an exclude pattern `*PATCH*` is configured and a directory contains `Game [PATCH].nsp` and `Game.nsp`
- **THEN** `Game [PATCH].nsp` is not added to the file list and `Game.nsp` is

#### Scenario: A bracket is a character class, not a literal

- **WHEN** an exclude pattern `*[PATCH]*` is configured
- **THEN** the pattern matches a name holding any of the characters `P`, `A`, `T`, `C` or `H`, because a bracket introduces a character class

#### Scenario: Only included files are candidates

- **WHEN** an include pattern `*.nsp` is configured and a directory contains `Game.nsp` and `Game.nsz`
- **THEN** only `Game.nsp` is added to the file list

#### Scenario: No include pattern keeps every file

- **WHEN** no include pattern is configured and a directory contains `Game.nsp` and `Game.nsz`
- **THEN** both files are offered to the scan, subject to the extension set and the exclude patterns

#### Scenario: A pattern is matched against the file name

- **WHEN** an exclude pattern `games` is configured and the file `games` sits in a directory named `collection`
- **THEN** the file is excluded, and a file whose name does not match is not

#### Scenario: The file name is matched without its directory

- **WHEN** an include pattern `*/games/*` is configured
- **THEN** no file is matched by it, because only the file name is matched

### Requirement: Filter settings come from the command line and the config file

The system SHALL read the accepted extension set and the include and exclude
patterns from the command line and from a `filters` section of the user's
configuration file, and SHALL use the command line's value whenever it gives
one. An unset or empty value SHALL mean the default.

#### Scenario: The command line wins over the config file

- **WHEN** the config file sets an accepted extension set and the command line sets another
- **THEN** the command line's set is in effect

#### Scenario: The config file supplies the settings

- **WHEN** only the config file sets an accepted extension set and patterns
- **THEN** those settings are in effect

#### Scenario: No setting anywhere means the defaults

- **WHEN** neither the command line nor the config file sets any filter
- **THEN** the accepted extension set is `nsp` and `nsz`, and no include or exclude pattern applies

#### Scenario: A malformed filter entry warns and keeps the rest

- **WHEN** the config file holds one malformed filter entry and one valid share
- **THEN** the malformed entry is reported, the valid share is still listed, and the program continues

### Requirement: A scan reports what the filters rejected

When a directory scan runs, the system SHALL report how many files it accepted
and how many the filters rejected, so that an empty result is distinguishable
from a scan that never looked at the file.

#### Scenario: Accepted and rejected counts are reported

- **WHEN** a directory scan accepts 2 files and rejects 5
- **THEN** the outcome reports 2 accepted files and 5 rejected files

#### Scenario: A scan that rejects everything is reported

- **WHEN** a directory scan accepts no file
- **THEN** the outcome reports 0 accepted files and the number of files the filters rejected
