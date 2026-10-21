# server Specification

## Purpose

Defines how the backend answers requests from the Switch over a transport: the
command loop, the `EXIT`, `LIST`, and `FILE_RANGE` handlers, file-byte streaming,
and error handling.

## Requirements

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

### Requirement: FILE_RANGE error handling

The server SHALL reject a range header larger than the accepted maximum as a
protocol error. When the named file is unknown, cannot be opened or seeked, or a
read yields fewer bytes than `range_size`, the server SHALL log the failure and
end the connection instead of leaving the peer waiting for an incomplete range.

#### Scenario: Unknown file is logged and skipped

- **WHEN** a range request names a file that is not in the list
- **THEN** the server logs the unknown name, does not stream it, and ends the connection

#### Scenario: Oversized range header is rejected

- **WHEN** a `FILE_RANGE` header larger than the accepted maximum is received
- **THEN** the server reports a protocol error and stops the loop

#### Scenario: Short read aborts the connection

- **WHEN** a file yields fewer bytes than the requested `range_size`
- **THEN** the server logs the short transfer and ends the connection

#### Scenario: File open failure aborts the connection

- **WHEN** the named file cannot be opened
- **THEN** the server logs the failure and ends the connection

### Requirement: Transfer log verbosity

The server SHALL NOT log every file-range request it receives, SHALL NOT log per-request range details such as the requested size, offset, or name length, and SHALL NOT report completion per streamed range. When a file's streamed sequence starts, the server SHALL announce that file once, before the sequence's next completion report. When a streamed range reaches the file's end, the server SHALL report that file's completion once, naming the file and its size. A range served outside the file's streamed sequence SHALL be streamed as requested while leaving the announcement state, the streamed position, and the completion state untouched. Failures SHALL still be logged.

#### Scenario: Requests are not logged

- **WHEN** a file-range request is received
- **THEN** the server emits no log entry describing the request itself

#### Scenario: A file's start is announced once

- **WHEN** streaming of a file begins on the connection
- **THEN** the server announces the file's name exactly once before its next completion report

#### Scenario: A completed transfer is reported once

- **WHEN** a streamed range reaches the file's end
- **THEN** the server reports that file's completion exactly once, with the file name and its size

#### Scenario: Mid-file ranges are not reported as complete

- **WHEN** a streamed range ends before the file's end and no further range of that file is served
- **THEN** the server emits no completion report for that file

#### Scenario: Failures are still logged

- **WHEN** a file range fails
- **THEN** the server logs the failure as before

#### Scenario: A trailing auxiliary read does not report completion

- **WHEN** a range that is not part of the file's streamed sequence reaches the file's end
- **THEN** the server streams that range as requested but emits no completion report and no announcement for the file

### Requirement: Streaming progress semantics

The server SHALL track, per connection and file, the byte position reached by the file's streamed ranges. A file-range request SHALL be part of that streamed sequence when it begins at the file's offset zero or continues at the streamed position; any other range for that file SHALL be an auxiliary read, served as requested but SHALL NOT advance the position, emit progress, or signal completion.

#### Scenario: Stream starts at the file's beginning

- **WHEN** a file-range request for a file without a streamed position begins at offset zero
- **THEN** the server streams it as part of the file's streamed sequence and reports progress while it is served

#### Scenario: Streaming continues across ranges

- **WHEN** a file-range request begins at the byte position the file's streamed ranges already reached
- **THEN** the server streams it as part of the file's streamed sequence and its progress continues from that position

#### Scenario: Auxiliary reads are silent

- **WHEN** a file-range request for a file with a streamed position begins neither at offset zero nor at the streamed position
- **THEN** the server serves that range as requested, emits no progress sample, no announcement, and no completion for it, and the streamed position stays unchanged

#### Scenario: A restart at offset zero begins a new streamed sequence

- **WHEN** a file-range request begins at offset zero for a file whose streaming had progressed or completed
- **THEN** the streamed position restarts at zero, the file is announced once, and progress is reported anew for that file
