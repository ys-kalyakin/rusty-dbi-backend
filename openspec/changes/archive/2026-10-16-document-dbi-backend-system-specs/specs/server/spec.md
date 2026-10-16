# Spec Delta

## Purpose

Defines how the backend answers requests from the Switch over a transport: the
command loop, the `EXIT`, `LIST`, and `FILE_RANGE` handlers, file-byte streaming,
and error handling.

## ADDED Requirements

### Requirement: Command loop dispatch

The server SHALL read a 16-byte header, dispatch `EXIT`, `FILE_RANGE`, and
`LIST`, ignore headers without the `DBI0` magic and unknown command ids, and
continue looping until `EXIT` is received or the transport fails.

#### Scenario: Unknown command is ignored

- **WHEN** a valid header with an unrecognized command id is received
- **THEN** the server takes no action and keeps reading

#### Scenario: Unframed data is ignored

- **WHEN** a header does not start with the `DBI0` magic
- **THEN** the server skips it and keeps reading

### Requirement: EXIT handling

On `EXIT`, the server SHALL reply with a `RESPONSE`/`EXIT` header carrying no
payload and then stop the command loop.

#### Scenario: Exit response then stop

- **WHEN** an `EXIT` request is received
- **THEN** the server writes a `DBI0, RESPONSE, EXIT, 0` header and returns

### Requirement: LIST handling

On `LIST`, the server SHALL build the newline-separated payload of current file
names, send a `RESPONSE`/`LIST` header carrying the payload length, and, when
the length is non-zero, wait for a 16-byte acknowledgment before sending the
payload.

#### Scenario: Names are sent after acknowledgment

- **WHEN** a `LIST` request is received and files are present
- **THEN** the server writes the response header, waits for the 16-byte ack, then writes the names

#### Scenario: Empty list skips the acknowledgment

- **WHEN** a `LIST` request is received and no files are present
- **THEN** the server writes a zero-length response header and sends no payload

### Requirement: FILE_RANGE handling

On `FILE_RANGE`, the server SHALL send an `ACK` carrying the requested header
size, read that many bytes as the file-range header, send a
`RESPONSE`/`FILE_RANGE` header carrying `range_size`, wait for a 16-byte
acknowledgment, then open the named file, seek to `range_offset`, and stream
`range_size` bytes in chunks of at most 1 MiB.

#### Scenario: Requested bytes are streamed

- **WHEN** a valid range request names a known file
- **THEN** the server sends exactly the requested byte range in order

#### Scenario: Range larger than one chunk is split

- **WHEN** the requested range exceeds the 1 MiB chunk size
- **THEN** the server sends it as multiple writes each no larger than 1 MiB

### Requirement: FILE_RANGE error handling

The server SHALL log and skip an unknown file name or a file open/seek failure
without ending the command loop, and SHALL reject a range header larger than the
accepted maximum as a protocol error.

#### Scenario: Unknown file is logged and skipped

- **WHEN** a range request names a file that is not in the list
- **THEN** the server logs the unknown name and continues serving

#### Scenario: Oversized range header is rejected

- **WHEN** a `FILE_RANGE` header larger than the accepted maximum is received
- **THEN** the server reports a protocol error and stops the loop
