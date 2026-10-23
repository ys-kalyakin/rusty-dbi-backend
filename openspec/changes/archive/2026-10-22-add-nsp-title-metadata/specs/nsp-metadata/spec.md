# Spec Delta

## Purpose

Defines how the backend learns what a title is: the title id, the content type
and the version of an offered NSP/NSZ file, where those come from, and what is
deliberately left unread.

## ADDED Requirements

### Requirement: A container's identity is read from its own metadata

For a file whose container exposes its content metadata, the system SHALL
determine the title id, the content type and the version from that metadata.

#### Scenario: The title id comes from the container

- **WHEN** a file's container exposes content metadata naming a title id
- **THEN** the file's identity carries that title id

#### Scenario: The version comes from the container

- **WHEN** a file's container exposes content metadata carrying a version
- **THEN** the file's identity carries that version

#### Scenario: The content type comes from the container

- **WHEN** a file's container exposes content metadata carrying a content type
- **THEN** the file's identity carries that content type

### Requirement: The identity falls back to the file name

When a file's container does not expose its content metadata, the system SHALL
derive the identity from the file name, reading the title id and the version
from the bracketed parts of the conventional dump name
`Name [TitleId][vVersion].nsp`.

#### Scenario: The title id comes from the file name

- **WHEN** a file is named `Game [0100000000010000].nsp` and its container exposes no metadata
- **THEN** the file's identity carries the title id `0100000000010000`

#### Scenario: The version comes from the file name

- **WHEN** a file is named `Game [0100000000010000][v65536].nsp` and its container exposes no metadata
- **THEN** the file's identity carries the version 1.0.0

#### Scenario: A name without the convention carries no identity

- **WHEN** a file is named `game.nsp` and its container exposes no metadata
- **THEN** the file's identity carries no title id and no version

#### Scenario: A bracketed part that is not a title id is ignored

- **WHEN** a file is named `Game [notes].nsp`
- **THEN** the file's identity carries no title id

#### Scenario: A name with no name part keeps the file name

- **WHEN** a file is named `[0100000000010000].nsp`
- **THEN** the name shown for it is `[0100000000010000].nsp`

### Requirement: The content type is derived from the title id

When an identity carries a title id but no content type, the system SHALL
classify it from the title id's last three digits: `000` is a base game, `800`
is an update, and any other value is downloadable content.

#### Scenario: A base game's title id is classified

- **WHEN** an identity's title id ends in `000`
- **THEN** the identity's content type is a base game

#### Scenario: An update's title id is classified

- **WHEN** an identity's title id ends in `800`
- **THEN** the identity's content type is an update

#### Scenario: A downloadable content's title id is classified

- **WHEN** an identity's title id ends in a value other than `000` or `800`
- **THEN** the identity's content type is downloadable content

### Requirement: The title name is not read from the container

The system SHALL NOT read a title's human-readable name out of the container,
and SHALL take the name it shows from the file name instead.

#### Scenario: The name comes from the file name

- **WHEN** a file is named `Some Game [0100000000010000].nsp`
- **THEN** the name shown for it is `Some Game`

#### Scenario: A file without a name part keeps its file name

- **WHEN** a file is named `game.nsp`
- **THEN** the name shown for it is `game.nsp`

### Requirement: An unreadable container does not keep a file out of the list

Reading a container's metadata SHALL NOT decide whether a file is offered: a
file whose container cannot be read or understood SHALL be added without an
identity, and the failure SHALL NOT be reported as a read error.

#### Scenario: A file that is not a container is added

- **WHEN** a file whose contents are not a known container is added
- **THEN** the file is present in the file list and carries no identity

#### Scenario: A broken container is added

- **WHEN** a file's container starts with a known container marker but cannot be read past it
- **THEN** the file is present in the file list and carries no identity, and no read error is reported
