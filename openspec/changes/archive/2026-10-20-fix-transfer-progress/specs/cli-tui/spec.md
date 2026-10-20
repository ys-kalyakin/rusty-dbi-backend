# Spec Delta

## MODIFIED Requirements

### Requirement: Transfer progress reporting

While serving, the server SHALL report the current file and the bytes transferred. The UI SHALL show a progress region with a text row naming the file and displaying the percentage, transferred and total bytes, and, once a speed is known, an estimated remaining time derived from a smoothed rate over recent samples. A progress bar derived from the reported bytes SHALL be rendered separately from that text. The progress region SHALL disappear when the range completes, the connection is lost, or the server ends, and stale progress SHALL NOT reappear after a reconnect.

#### Scenario: Progress reflects transfer

- **WHEN** the server transfers bytes of a file
- **THEN** the UI's progress bar advances toward the file's total size

#### Scenario: Byte counts are shown

- **WHEN** a file is being served
- **THEN** the progress region shows the transferred bytes and the file's total size in human-readable units

#### Scenario: Estimate appears once speed is known

- **WHEN** a transfer speed has been measured for the current file
- **THEN** the progress region shows an estimated remaining time derived from that speed

#### Scenario: Estimate is absent before a speed is known

- **WHEN** no transfer speed has been measured for the current file
- **THEN** the progress region shows no remaining-time estimate

#### Scenario: Estimate responds gradually to uneven samples

- **WHEN** progress samples for the same file arrive at uneven intervals
- **THEN** the remaining-time estimate is derived from a smoothed rate over recent samples rather than from a single short interval

#### Scenario: File name stays readable over the bar

- **WHEN** the progress bar is completely filled
- **THEN** the file name and statistics remain readable on the text row above the bar

#### Scenario: Progress clears when serving ends

- **WHEN** the server stops serving
- **THEN** the progress region is no longer shown

#### Scenario: Progress clears when a transfer completes

- **WHEN** a file range finishes streaming successfully
- **THEN** the progress region is no longer shown

#### Scenario: Progress does not survive a disconnect

- **WHEN** the connection is lost during a transfer and the device later reconnects
- **THEN** no progress from the interrupted transfer is shown until a new progress update arrives
