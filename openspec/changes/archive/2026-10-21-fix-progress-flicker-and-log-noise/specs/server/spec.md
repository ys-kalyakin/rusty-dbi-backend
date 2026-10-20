# Spec Delta

## MODIFIED Requirements

### Requirement: Transfer log verbosity

The server SHALL NOT log every file-range request it receives, SHALL NOT log per-request
range details such as the requested size, offset, or name length, and SHALL NOT report
completion per streamed range. When a file starts streaming, the server SHALL announce
that file once. When a streamed range reaches the file's end (its offset plus size
reaches or passes the file's total size), the server SHALL report that file's completion
once, naming the file and its size. Failures SHALL still be logged.

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
