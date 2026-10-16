# Spec Delta

## Purpose

Defines the command-line interface, the terminal UI, and the background server
lifecycle that together form the user-facing behavior of the backend.

## ADDED Requirements

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
resolved path.

#### Scenario: Log is capped

- **WHEN** more than 1000 log lines are emitted
- **THEN** only the most recent 1000 are retained

### Requirement: Keybindings

The UI SHALL map `a` to a path-entry prompt (add on Enter), `c` to clear the
list, `s` to start the server only when the list is non-empty and not already
started, and `q` to quit.

#### Scenario: Quit key requests exit

- **WHEN** `q` is pressed
- **THEN** the application is asked to quit

#### Scenario: Start requires files and happens once

- **WHEN** `s` is pressed with no files, then with files present, then again
- **THEN** the first press does nothing and later presses start the server at most once

#### Scenario: Path prompt adds on Enter

- **WHEN** a path is typed in the prompt and Enter is pressed
- **THEN** the file list attempts to add that path

### Requirement: Background server lifecycle

On start, the program SHALL run the server in a background task over the USB
transport, snapshot the current file list, forward server log lines to the UI,
retry the connection about once per second, and reconnect after a connection is
lost while keeping the process alive.

#### Scenario: Connection loss reconnects

- **WHEN** an established connection fails during the command loop
- **THEN** the loss is logged and the server attempts to connect again

#### Scenario: Server stops on EXIT

- **WHEN** the Switch issues `EXIT`
- **THEN** the server loop ends and the stop is logged
