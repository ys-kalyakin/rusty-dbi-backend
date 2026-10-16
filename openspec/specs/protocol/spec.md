# protocol Specification

## Purpose

Defines the DBI0 wire framing exchanged between the Nintendo Switch and the
backend: message headers, command identifiers, the file-range request payload,
and the file listing payload.

## Requirements

### Requirement: Message framing

Every DBI0 message SHALL begin with a 16-byte little-endian header consisting of
the 4-byte ASCII magic `DBI0`, then `cmd_type`, `cmd_id`, and `data_size` as
32-bit unsigned little-endian integers.

#### Scenario: Header round-trips

- **WHEN** a header is built with a command type, id, and data size
- **THEN** serializing it to bytes and parsing it back yields the same fields

#### Scenario: Header shorter than 16 bytes is rejected

- **WHEN** a parser is given fewer than 16 bytes
- **THEN** parsing returns no header

#### Scenario: Framing without DBI0 magic is invalid

- **WHEN** a 16-byte header does not start with `DBI0`
- **THEN** it is reported as invalid so the caller can skip it

### Requirement: Command identifiers

The protocol SHALL define command types `REQUEST = 0`, `RESPONSE = 1`,
`ACK = 2`, and command ids `EXIT = 0`, `LIST_OLD = 1`, `FILE_RANGE = 2`,
`LIST = 3`.

#### Scenario: Known command ids are recognized

- **WHEN** a header carries one of the defined command ids
- **THEN** the server dispatches it to the matching handler

### Requirement: File-range request payload

A FILE_RANGE payload SHALL encode `range_size` as a 32-bit unsigned integer,
`range_offset` as a 64-bit unsigned integer, `name_len` as a 32-bit unsigned
integer, and the file name as exactly `name_len` UTF-8 bytes, all little-endian.
Parsing SHALL reject a payload shorter than the fixed part, shorter than
`name_len` bytes of name, or whose name is not valid UTF-8.

#### Scenario: Fields are parsed

- **WHEN** a payload of these fields plus a name is parsed
- **THEN** the range size, offset, name length, and name are returned

#### Scenario: Short payload is rejected

- **WHEN** a payload shorter than the 16-byte fixed part is parsed
- **THEN** parsing returns no file-range header

#### Scenario: Declared name length is validated

- **WHEN** a payload declares a `name_len` larger than the bytes that follow
- **THEN** parsing returns no file-range header

#### Scenario: Invalid UTF-8 name is rejected

- **WHEN** the name bytes are not valid UTF-8
- **THEN** parsing returns no file-range header

### Requirement: File listing payload

The LIST payload SHALL contain each file name followed by a newline, UTF-8
encoded, in the order they are offered.

#### Scenario: Names are newline-terminated

- **WHEN** the payload is built from names `a.nsp` and `b.nsp`
- **THEN** the bytes are `a.nsp\nb.nsp\n`

### Requirement: Transfer chunk size

The protocol SHALL define a maximum bulk transfer chunk size of `0x100000`
bytes (1 MiB) used when streaming a file range.

#### Scenario: Large ranges are split into chunks

- **WHEN** a requested range exceeds 1 MiB
- **THEN** the stream is sent as multiple chunks no larger than the chunk size
