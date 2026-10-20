# Spec Delta

## MODIFIED Requirements

### Requirement: Terminal UI layout

The UI SHALL show a status/header region, a file list region, a log region capped at 1000 retained lines, and a status/help line, and SHALL render each file as its name and resolved path. The log region SHALL display the most recent retained lines. While the server is serving a file, the UI SHALL show a progress region for the current file. While the picker is open, the UI SHALL show the picker's listing and its key hints in place of the normal help line. When no files are offered, the file list region SHALL show an onboarding hint that names the key which adds files.

#### Scenario: Log is capped

- **WHEN** more than 1000 log lines are emitted
- **THEN** only the most recent 1000 are retained

#### Scenario: Log shows the newest lines

- **WHEN** more log lines are emitted than the log region can display
- **THEN** the most recent lines are visible without scrolling

#### Scenario: Picker view replaces the help line

- **WHEN** the picker is open
- **THEN** the UI shows the picker listing and picker key hints

#### Scenario: Progress region appears while serving

- **WHEN** the server is serving a file range to the Switch
- **THEN** the UI shows the current file's name and its progress percentage

#### Scenario: Empty list shows an onboarding hint

- **WHEN** no files are offered and the picker is closed
- **THEN** the file list region displays a hint that names the key for adding files

### Requirement: Keybindings

Outside the picker the UI SHALL map `a` to open the path picker, `c` to clear the list, `d` to remove the highlighted file, `s` to start the server only when the list is non-empty and no server is running, `?` to toggle help, `j`/`k` and the arrow keys to move the highlight, and `q` to quit. While the server task is active, `a`, `c`, and `d` SHALL be inert so the offered list matches the server's snapshot, and the UI SHALL visibly mark those actions as unavailable. While the picker is open, keys SHALL drive the picker instead of these actions.

#### Scenario: Quit key requests exit

- **WHEN** `q` is pressed while the picker is closed and confirmed
- **THEN** the application is asked to quit

#### Scenario: Start requires files and happens once

- **WHEN** `s` is pressed with no files, then with files present, then again
- **THEN** the first press does nothing and later presses start the server at most once

#### Scenario: Editing is blocked while the server runs

- **WHEN** `a`, `c`, or `d` is pressed while the server task is active
- **THEN** the file list is unchanged

#### Scenario: Disabled actions are marked unavailable

- **WHEN** the server task is active (waiting or running)
- **THEN** the hint line visibly marks `a`, `c`, and `d` as unavailable while `s`, `q`, and navigation remain available

#### Scenario: Path prompt adds on Enter

- **WHEN** a file is highlighted in the picker and Enter is pressed
- **THEN** the file list attempts to add that file

#### Scenario: Add key opens the picker

- **WHEN** `a` is pressed while the picker is closed and no server is running
- **THEN** the path picker opens

#### Scenario: Remove key targets the highlighted file

- **WHEN** `d` is pressed while a file is highlighted and no server is running
- **THEN** that file is removed from the list

#### Scenario: Picker keys are not global actions

- **WHEN** the picker is open and a key that is a global action is pressed
- **THEN** the key is interpreted by the picker and does not trigger the global action

### Requirement: Background server lifecycle

On start, the program SHALL run the server in a background task over the USB transport, snapshot the current file list, forward server log lines to the UI, retry the connection about once per second, and reconnect after a connection is lost while keeping the process alive. The UI SHALL report the task as waiting for the device while it is active but the connection is not established — including while retrying after a loss — as running once the device is connected, and as idle once the task ends. The UI SHALL allow starting the server again after a run ends.

#### Scenario: Waiting shown while connecting

- **WHEN** the user starts the server and the Switch has not connected yet
- **THEN** the UI reports the waiting-for-device state until the connection is established

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

While serving, the server SHALL report the current file and the bytes transferred, and the UI SHALL show a progress indicator derived from them. The UI SHALL also show the bytes transferred and the file's total size in human-readable form, and, once a transfer speed is known, an estimated remaining time derived from that speed.

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

## ADDED Requirements

### Requirement: Server status presentation

The screen SHALL include a header that identifies the application by name, shows the offered file count and the total size of that list, and shows the server state as one of three visually distinct values: waiting for the device, running, or idle.

#### Scenario: Header identifies the application

- **WHEN** the main screen is drawn
- **THEN** the header text contains the application's name

#### Scenario: Counts follow the list

- **WHEN** files are added to or removed from the list
- **THEN** the header's file count and total size match the current list

#### Scenario: Waiting is distinct from running

- **WHEN** the server task is active but the device is not connected, versus when it is connected
- **THEN** the header shows two visibly different status values for those conditions
