# Spec Delta

## MODIFIED Requirements

### Requirement: Transfer progress reporting

While serving, the server SHALL report the current file and the bytes transferred. The UI SHALL show a one-row progress region naming the file and displaying the percentage, the transferred and total bytes, and, once a speed is known, an estimated remaining time derived from a smoothed rate over recent samples, with a progress bar derived from the reported bytes filling the remainder of that same row. That region SHALL stay visible across the consecutive range requests serving the same file, and SHALL end when that file completes, the connection is lost, or the server ends; stale progress SHALL NOT reappear after a reconnect. A progress sample for the file currently shown whose transferred count is lower than the shown count SHALL start a new streamed phase for that file: the bar and byte counters follow the new sample, and the speed and remaining-time sample history is discarded so no estimate continues from the previous phase.

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
- **THEN** the file name and statistics remain readable on the same single row as the bar

#### Scenario: Progress persists across ranges of one file

- **WHEN** one file is streamed through consecutive range requests without completing it
- **THEN** the progress region remains continuously shown between those ranges

#### Scenario: Progress clears when a transfer completes

- **WHEN** a file's streaming reaches that file's end
- **THEN** the progress region is no longer shown

#### Scenario: Progress clears when serving ends

- **WHEN** the server stops serving
- **THEN** the progress region is no longer shown

#### Scenario: Progress does not survive a disconnect

- **WHEN** the connection is lost during a transfer and the device later reconnects
- **THEN** no progress from the interrupted transfer is shown until a new progress update arrives

#### Scenario: A backward sample restarts the transfer statistics

- **WHEN** a progress sample for the file currently shown reports fewer transferred bytes than the region already shows
- **THEN** the bar and byte counters follow the new sample and no speed or remaining-time estimate from the previous streamed phase is shown until a fresh speed is measured

#### Scenario: A restarted phase does not fill the bar prematurely

- **WHEN** a new streamed phase for a file starts at the file's beginning after previous progress for that file
- **THEN** the region shows the new phase's current position rather than the previously reached 100% fill
