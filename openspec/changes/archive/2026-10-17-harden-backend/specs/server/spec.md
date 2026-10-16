# Spec Delta

## MODIFIED Requirements

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
