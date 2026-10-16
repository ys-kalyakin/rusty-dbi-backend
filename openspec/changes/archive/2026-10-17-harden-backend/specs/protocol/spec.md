# Spec Delta

## MODIFIED Requirements

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
