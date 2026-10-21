# Spec Delta

## MODIFIED Requirements

### Requirement: FILE_RANGE handling

On `FILE_RANGE`, the server SHALL send an `ACK` carrying the requested header
size, read that many bytes as the file-range header, send a
`RESPONSE`/`FILE_RANGE` header carrying `range_size`, wait for a 16-byte
acknowledgment, then open the named file through the file list entry's source
— the local filesystem for a local entry, the entry's share for a share entry
— and stream `range_size` bytes starting at `range_offset` in chunks of at
most 1 MiB.

#### Scenario: Requested bytes are streamed

- **WHEN** a valid range request names a known file
- **THEN** the server sends exactly the requested byte range in order

#### Scenario: Range larger than one chunk is split

- **WHEN** the requested range exceeds the 1 MiB chunk size
- **THEN** the server sends it as multiple writes each no larger than 1 MiB

#### Scenario: A share file is streamed from its range

- **WHEN** a valid range request names a file whose source is a share
- **THEN** the streamed bytes are that file's bytes starting at `range_offset` for `range_size` bytes, read from the share
