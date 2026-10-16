# Spec Delta

## MODIFIED Requirements

### Requirement: Terminal UI layout

The UI SHALL show a file list region, a log region capped at 1000 retained
lines, and a status/help line, and SHALL render each file as its name and
resolved path. While the picker is open, the UI SHALL show the picker's listing
and its key hints in place of the normal help line.

#### Scenario: Log is capped

- **WHEN** more than 1000 log lines are emitted
- **THEN** only the most recent 1000 are retained

#### Scenario: Picker view replaces the help line

- **WHEN** the picker is open
- **THEN** the UI shows the picker listing and picker key hints

### Requirement: Keybindings

Outside the picker the UI SHALL map `a` to open the path picker, `c` to clear
the list, `s` to start the server only when the list is non-empty and not
already started, and `q` to quit. While the picker is open, keys SHALL drive the
picker instead of these actions.

#### Scenario: Quit key requests exit

- **WHEN** `q` is pressed while the picker is closed
- **THEN** the application is asked to quit

#### Scenario: Start requires files and happens once

- **WHEN** `s` is pressed with no files, then with files present, then again
- **THEN** the first press does nothing and later presses start the server at most once

#### Scenario: Path prompt adds on Enter

- **WHEN** a file is highlighted in the picker and Enter is pressed
- **THEN** the file list attempts to add that file

#### Scenario: Add key opens the picker

- **WHEN** `a` is pressed while the picker is closed
- **THEN** the path picker opens

#### Scenario: Picker keys are not global actions

- **WHEN** the picker is open and a key that is a global action is pressed
- **THEN** the key is interpreted by the picker and does not trigger the global action
