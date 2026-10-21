# Spec Delta

## MODIFIED Requirements

### Requirement: Command-line paths

The program SHALL accept zero or more positional `PATH` arguments, each a file or
directory to offer, SHALL accept the filter options that configure the accepted
extension set and the include and exclude patterns, and SHALL reject unknown
flags.

#### Scenario: Multiple paths are accepted

- **WHEN** invoked with a file and a directory path
- **THEN** both are parsed as paths to load

#### Scenario: Unknown flag is rejected

- **WHEN** invoked with an unrecognized option
- **THEN** argument parsing fails

#### Scenario: Filter options are accepted

- **WHEN** invoked with an accepted extension set, an include pattern, and an exclude pattern
- **THEN** the options are parsed and become the filter settings in effect

### Requirement: Keybindings

Outside the picker the UI SHALL map `a` to open the path picker, `c` to clear the list, `d` to remove the highlighted file, `s` to start the server only when the list is non-empty and no server is running, `/` to open the file list's search, `?` to toggle help, `j`/`k` and the arrow keys to move the highlight, and `q` to quit. While the server task is active, `a`, `c`, and `d` SHALL be inert so the offered list matches the server's snapshot, and the UI SHALL visibly mark those actions as unavailable. While the picker is open, keys SHALL drive the picker instead of these actions. While the search input is open, keys SHALL drive the search instead of these actions.

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

#### Scenario: The open search takes the keys

- **WHEN** the search input is open and a printable key is pressed
- **THEN** the key goes into the search and triggers no global action

#### Scenario: The search is closed before the editing keys act again

- **WHEN** the search input is open and Enter is pressed
- **THEN** the search input closes, the search stays, and the editing keys act again

## ADDED Requirements

### Requirement: The file list can be searched

The UI SHALL let the user narrow the file list with a search string, matched
against the file names case-insensitively, and SHALL show only the matching
files while a search is active. The search SHALL NOT change the files offered
to the Switch: the list the server hands over keeps every file the scan
accepted. While the picker is open, the search key SHALL drive the picker
instead.

#### Scenario: A search narrows the visible list

- **WHEN** a search string is entered in the file list
- **THEN** only the files whose name contains that string, ignoring case, are shown

#### Scenario: A search with no matches shows an empty list

- **WHEN** a search string matches no file
- **THEN** no file is shown and no file is highlighted

#### Scenario: Clearing the search restores the full list

- **WHEN** the search is cleared
- **THEN** every file of the list is shown again

#### Scenario: The search does not change what the Switch receives

- **WHEN** a search is active and the server starts
- **THEN** the list offered to the Switch contains every file the scan accepted, not only the matching ones

#### Scenario: The search works while the server runs

- **WHEN** the server task is active
- **THEN** a search can still be entered and cleared, and the editing keys stay unavailable

#### Scenario: The picker takes the search key

- **WHEN** the picker is open and the search key is pressed
- **THEN** the picker handles the key and the file list's search is not started
