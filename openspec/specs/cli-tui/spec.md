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

The UI SHALL show a file list region, a log region capped at 1000 retained
lines, and a status/help line, and SHALL render each file as its name and
resolved path. The log region SHALL display the most recent retained lines.
While the picker is open, the UI SHALL show the picker's listing and its key
hints in place of the normal help line.

#### Scenario: Log is capped

- **WHEN** more than 1000 log lines are emitted
- **THEN** only the most recent 1000 are retained

#### Scenario: Log shows the newest lines

- **WHEN** more log lines are emitted than the log region can display
- **THEN** the most recent lines are visible without scrolling

#### Scenario: Picker view replaces the help line

- **WHEN** the picker is open
- **THEN** the UI shows the picker listing and picker key hints

### Requirement: Keybindings

Outside the picker the UI SHALL map `a` to open the path picker, `c` to clear
the list, `s` to start the server only when the list is non-empty and no server
is running, and `q` to quit. While a server is running, `a` and `c` SHALL be
inert so the offered list matches the server's snapshot. While the picker is
open, keys SHALL drive the picker instead of these actions.

#### Scenario: Quit key requests exit

- **WHEN** `q` is pressed while the picker is closed
- **THEN** the application is asked to quit

#### Scenario: Start requires files and happens once

- **WHEN** `s` is pressed with no files, then with files present, then again
- **THEN** the first press does nothing and later presses start the server at most once

#### Scenario: Editing is blocked while the server runs

- **WHEN** `a` or `c` is pressed while a server is running
- **THEN** the file list is unchanged

#### Scenario: Path prompt adds on Enter

- **WHEN** a file is highlighted in the picker and Enter is pressed
- **THEN** the file list attempts to add that file

#### Scenario: Add key opens the picker

- **WHEN** `a` is pressed while the picker is closed and no server is running
- **THEN** the path picker opens

#### Scenario: Picker keys are not global actions

- **WHEN** the picker is open and a key that is a global action is pressed
- **THEN** the key is interpreted by the picker and does not trigger the global action

### Requirement: Background server lifecycle

On start, the program SHALL run the server in a background task over the USB
transport, snapshot the current file list, forward server log lines to the UI,
retry the connection about once per second, and reconnect after a connection is
lost while keeping the process alive. The UI SHALL report the server as running
while the task is active and as idle once it ends, and SHALL allow starting the
server again after a run ends.

#### Scenario: Connection loss reconnects

- **WHEN** an established connection fails during the command loop
- **THEN** the loss is logged and the server attempts to connect again

#### Scenario: Server stops on EXIT

- **WHEN** the Switch issues `EXIT`
- **THEN** the server loop ends and the stop is logged

#### Scenario: Status returns to idle after the server ends

- **WHEN** the server loop ends
- **THEN** the UI reports the idle status and files can be edited again

#### Scenario: Server can be started again

- **WHEN** `s` is pressed after a previous run has ended
- **THEN** a new server run starts
