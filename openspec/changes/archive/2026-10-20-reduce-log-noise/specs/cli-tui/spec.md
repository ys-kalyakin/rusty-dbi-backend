# Spec Delta

## MODIFIED Requirements

### Requirement: Background server lifecycle

On start, the program SHALL run the server in a background task over the USB transport, snapshot the current file list, forward server log lines to the UI, retry the connection about once per second, and reconnect after a connection is lost while keeping the process alive. The program SHALL NOT write a log line per connection attempt: the waiting-for-device state SHALL be logged once per waiting period, and a failed connection SHALL be logged once while the same error persists. The UI SHALL report the task as waiting for the device while it is active but the connection is not established — including while retrying after a loss — as running once the device is connected, and as idle once the task ends. The UI SHALL allow starting the server again after a run ends.

#### Scenario: Waiting shown while connecting

- **WHEN** the user starts the server and the Switch has not connected yet
- **THEN** the UI reports the waiting-for-device state until the connection is established

#### Scenario: Waiting is logged once per period

- **WHEN** the device stays disconnected across several connection attempts
- **THEN** the waiting state is written to the log once and not repeated for each attempt

#### Scenario: Repeated connection failures are logged once

- **WHEN** consecutive connection attempts fail with the same error
- **THEN** the error is written to the log once while it persists

#### Scenario: Connection loss reconnects

- **WHEN** an established connection fails during the command loop
- **THEN** the loss is logged and the server attempts to connect again

#### Scenario: Waiting resumes after connection loss

- **WHEN** an established connection fails during the command loop
- **THEN** the loss is logged, the task attempts to connect again, and the UI reports the waiting-for-device state until the next connection

#### Scenario: Running shown while connected

- **WHEN** the device is connected and the task is serving
- **THEN** the UI reports the running state

#### Scenario: Server stops on EXIT

- **WHEN** the Switch issues `EXIT`
- **THEN** the server loop ends, the stop is logged, and the UI reports the idle state

#### Scenario: Status returns to idle after the server ends

- **WHEN** the server loop ends
- **THEN** the UI reports the idle status and files can be edited again

#### Scenario: Server can be started again

- **WHEN** `s` is pressed after a previous run has ended
- **THEN** a new server run starts

### Requirement: Transfer progress reporting

While serving, the server SHALL report the current file and the bytes transferred, and the UI SHALL show a progress indicator derived from them. The UI SHALL also show the bytes transferred and the file's total size in human-readable form, and, once a transfer speed is known, an estimated remaining time derived from that speed. The progress region SHALL disappear once the transfer completes.

#### Scenario: Progress reflects transfer

- **WHEN** the server transfers bytes of a file
- **THEN** the UI's progress indicator advances toward the file's total size

#### Scenario: Byte counts are shown

- **WHEN** a file is being served
- **THEN** the progress region shows the transferred bytes and the file's total size in human-readable units

#### Scenario: Estimate appears once speed is known

- **WHEN** a transfer speed has been measured for the current file
- **THEN** the progress region shows an estimated remaining time derived from that speed

#### Scenario: Estimate is absent before a speed is known

- **WHEN** no transfer speed has been measured for the current file
- **THEN** the progress region shows no remaining-time estimate

#### Scenario: Progress clears when serving ends

- **WHEN** the server stops serving
- **THEN** the progress region is no longer shown

#### Scenario: Progress clears when a transfer completes

- **WHEN** a file range finishes streaming successfully
- **THEN** the progress region is no longer shown

## ADDED Requirements

### Requirement: Transfer summary in the log

When a file range finishes streaming successfully, the UI SHALL write exactly one log
line naming the file and the number of bytes sent in human-readable units, and SHALL
NOT write a log line for the range request itself.

#### Scenario: A completed transfer is summarized once

- **WHEN** a file range finishes streaming successfully
- **THEN** the log shows one line naming the file and the bytes sent

#### Scenario: Requests are not logged

- **WHEN** a file range request is received
- **THEN** no log line describes the request
