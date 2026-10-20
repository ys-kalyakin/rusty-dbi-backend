# Spec Delta

## ADDED Requirements

### Requirement: Transfer log verbosity

The server SHALL NOT log every file-range request it receives, and SHALL NOT log
per-request range details such as the requested size, offset, or name length. After a
range is streamed successfully, the server SHALL report the file name and the number
of bytes sent once. Failures SHALL still be logged.

#### Scenario: Requests are not logged

- **WHEN** a file-range request is received
- **THEN** the server emits no log entry describing the request itself

#### Scenario: A completed transfer is reported once

- **WHEN** a file range is streamed successfully
- **THEN** the server reports the file name and the number of bytes sent exactly once

#### Scenario: Failures are still logged

- **WHEN** a file range fails
- **THEN** the server logs the failure as before
