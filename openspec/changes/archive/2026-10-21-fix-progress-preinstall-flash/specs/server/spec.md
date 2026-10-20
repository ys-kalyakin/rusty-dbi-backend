# Spec Delta

## MODIFIED Requirements

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

## ADDED Requirements

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
