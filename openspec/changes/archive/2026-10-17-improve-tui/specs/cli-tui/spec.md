# Spec Delta

## MODIFIED Requirements

### Requirement: Terminal UI layout

The UI SHALL show a status/header region, a file list region, a log region capped at 1000 retained lines, and a status/help line, and SHALL render each file as its name and resolved path. The log region SHALL display the most recent retained lines. While the server is serving a file, the UI SHALL show a progress region for the current file. While the picker is open, the UI SHALL show the picker's listing and its key hints in place of the normal help line.

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

### Requirement: Keybindings

Outside the picker the UI SHALL map `a` to open the path picker, `c` to clear the list, `d` to remove the highlighted file, `s` to start the server only when the list is non-empty and no server is running, `?` to toggle help, `j`/`k` and the arrow keys to move the highlight, and `q` to quit. While a server is running, `a`, `c`, and `d` SHALL be inert so the offered list matches the server's snapshot. While the picker is open, keys SHALL drive the picker instead of these actions.

#### Scenario: Quit key requests exit

- **WHEN** `q` is pressed while the picker is closed and confirmed
- **THEN** the application is asked to quit

#### Scenario: Start requires files and happens once

- **WHEN** `s` is pressed with no files, then with files present, then again
- **THEN** the first press does nothing and later presses start the server at most once

#### Scenario: Editing is blocked while the server runs

- **WHEN** `a`, `c`, or `d` is pressed while a server is running
- **THEN** the file list is unchanged

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

## ADDED Requirements

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

While serving, the server SHALL report the current file and the bytes transferred, and the UI SHALL show a progress indicator derived from them.

#### Scenario: Progress reflects transfer

- **WHEN** the server transfers bytes of a file
- **THEN** the UI's progress indicator advances toward the file's total size

#### Scenario: Progress clears when serving ends

- **WHEN** the server stops serving
- **THEN** the progress region is no longer shown
