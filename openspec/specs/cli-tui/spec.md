# cli-tui Specification

## Purpose

Defines the command-line interface, the terminal UI, and the background server
lifecycle that together form the user-facing behavior of the backend.

## Requirements

### Requirement: Command-line paths

The program SHALL accept zero or more positional `PATH` arguments, each a file or
directory to offer, and SHALL reject unknown flags.

#### Scenario: Multiple paths are accepted

- **WHEN** invoked with a file and a directory path
- **THEN** both are parsed as paths to load

#### Scenario: Unknown flag is rejected

- **WHEN** invoked with an unrecognized option
- **THEN** argument parsing fails

### Requirement: Startup path loading

On startup the program SHALL add every given path to the file list, and a path
that cannot be added SHALL produce a warning without terminating the program.

#### Scenario: Bad path warns and continues

- **WHEN** a provided path cannot be read
- **THEN** a warning is printed and the program continues with the remaining paths

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

### Requirement: File list shows sizes

The file list SHALL display each file's size, and SHALL display the total size of the offered files.

#### Scenario: Size is shown per file

- **WHEN** a file is added to the list
- **THEN** the list shows that file's size alongside its name

#### Scenario: Total size reflects the list

- **WHEN** files are added or removed
- **THEN** the displayed total size equals the sum of the remaining files' sizes

### Requirement: File list selection

The file list SHALL keep a highlight within the list's bounds, SHALL move the highlight up and down, SHALL clamp it at the first and last entries, and SHALL scroll the listing so the highlighted entry stays visible.

#### Scenario: Moving past the last entry stops

- **WHEN** the highlight is on the last file and the user moves down again
- **THEN** the highlight stays on the last file

#### Scenario: Removing keeps the highlight valid

- **WHEN** the highlighted file is removed
- **THEN** the highlight remains within the remaining list's bounds

#### Scenario: Listing scrolls to the highlight

- **WHEN** the highlight moves outside the visible file list region
- **THEN** the listing scrolls so the highlighted file is visible

### Requirement: Confirmation for destructive actions

Clearing the list and quitting SHALL require explicit confirmation before taking effect, and SHALL be cancellable without changing the list or leaving the program.

#### Scenario: Quit is confirmed

- **WHEN** `q` is pressed and the confirmation is accepted
- **THEN** the application is asked to quit

#### Scenario: Quit can be cancelled

- **WHEN** `q` is pressed and the confirmation is dismissed
- **THEN** the program keeps running with the list unchanged

#### Scenario: Clear is confirmed

- **WHEN** `c` is pressed and the confirmation is accepted
- **THEN** the file list is emptied

#### Scenario: Clear can be cancelled

- **WHEN** `c` is pressed and the confirmation is dismissed
- **THEN** the file list is unchanged

### Requirement: Help overlay

The UI SHALL provide a help overlay that lists the available keybindings, and SHALL toggle it with `?` while the picker is closed.

#### Scenario: Help opens and closes

- **WHEN** `?` is pressed twice while the picker is closed
- **THEN** the help overlay is shown after the first press and hidden after the second

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

### Requirement: Transfer summary in the log

When a file starts streaming, the UI SHALL write one log line naming that file. When a file streams to its end, the UI SHALL write exactly one log line naming the file and its size in human-readable units, colored as a success. The UI SHALL NOT write a log line for the range request itself nor one per streamed range.

#### Scenario: A completed transfer is summarized once

- **WHEN** a file streams fully to its end
- **THEN** the log shows exactly one success line naming the file and its size in human-readable units

#### Scenario: A file's start is logged once

- **WHEN** a file starts streaming to the Switch
- **THEN** the log shows one line naming that file

#### Scenario: Ranges are not logged

- **WHEN** a file streams through several range requests without completing
- **THEN** the log gains no lines for those ranges beyond the file's start line

#### Scenario: Requests are not logged

- **WHEN** a file range request is received
- **THEN** no log line describes the request

### Requirement: Compact confirmation dialog

The confirmation prompt for clearing the list or quitting SHALL be presented in
a centered dialog sized to fit its prompt text plus a minimal border, and SHALL
NOT occupy a fixed fraction of the terminal. The prompt text and confirmation
semantics SHALL be unchanged.

#### Scenario: Dialog fits its prompt

- **WHEN** the quit or clear confirmation is shown
- **THEN** the dialog's width and height are just large enough for the prompt text and border, without a large empty area

#### Scenario: Both confirmations use the compact dialog

- **WHEN** either the quit or the clear confirmation is shown
- **THEN** both are presented in the same compact dialog rather than a terminal-relative fraction

#### Scenario: Small terminal still shows the prompt

- **WHEN** the terminal is smaller than the content-fitted dialog
- **THEN** the dialog is clamped to the available terminal area so the prompt remains visible

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
