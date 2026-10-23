# Spec Delta

## MODIFIED Requirements

### Requirement: Keybindings

Outside the picker the UI SHALL map `a` to open the path picker, `c` to clear the list, `d` to remove the highlighted file, `s` to start the USB server only when the list is non-empty and the USB server is not running, `H` to start the HTTP server only when the list is non-empty and the HTTP server is not running, and to stop the HTTP server while it runs, `/` to open the file list's search, `?` to toggle help, `j`/`k` and the arrow keys to move the highlight, and `q` to quit. While either server task is active, `a`, `c`, and `d` SHALL be inert so the offered list matches the running servers' snapshots, and the UI SHALL visibly mark those actions as unavailable. While the picker is open, keys SHALL drive the picker instead of these actions. While the search input is open, keys SHALL drive the search instead of these actions.

#### Scenario: Quit key requests exit

- **WHEN** `q` is pressed while the picker is closed and confirmed
- **THEN** the application is asked to quit

#### Scenario: Start requires files and happens once

- **WHEN** `s` is pressed with no files, then with files present, then again
- **THEN** the first press does nothing and later presses start the server at most once

#### Scenario: H starts and stops the HTTP server

- **WHEN** `H` is pressed with files offered, then pressed again while the HTTP server runs
- **THEN** the first press starts the HTTP server and the second stops it

#### Scenario: H requires files

- **WHEN** `H` is pressed with no files offered
- **THEN** the HTTP server is not started

#### Scenario: Editing is blocked while the server runs

- **WHEN** `a`, `c`, or `d` is pressed while either server task is active
- **THEN** the file list is unchanged

#### Scenario: Disabled actions are marked unavailable

- **WHEN** either server task is active (waiting or running)
- **THEN** the hint line visibly marks `a`, `c`, and `d` as unavailable while `s`, `q`, and navigation remain available

#### Scenario: Either server starts while the other runs

- **WHEN** `H` is pressed while the USB server task is active, or `s` while the HTTP server runs
- **THEN** the pressed key acts on its own server and the other server is unaffected

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

### Requirement: Server status presentation

The screen SHALL include a header that identifies the application by name, shows the offered file count and the total size of that list, shows the USB server's state as one of three visually distinct values — waiting for the device, running, or idle — and shows the HTTP server's state as running or idle, with the URL to enter on a console while it runs.

#### Scenario: Header identifies the application

- **WHEN** the main screen is drawn
- **THEN** the header text contains the application's name

#### Scenario: Counts follow the list

- **WHEN** files are added to or removed from the list
- **THEN** the header's file count and total size match the current list

#### Scenario: Waiting is distinct from running

- **WHEN** the server task is active but the device is not connected, versus when it is connected
- **THEN** the header shows two visibly different status values for those conditions

#### Scenario: The HTTP state and URL appear in the header

- **WHEN** the HTTP server is running, versus stopped
- **THEN** the header shows a running HTTP state together with a browsable URL in the first case, and an idle HTTP state with no URL in the second

### Requirement: Share connection key

Outside the picker, the UI SHALL map `S` to open the share list. While either
server task is active, `S` SHALL be inert and the hint line SHALL visibly mark
it unavailable like the other locked editing keys. While the share list or the
password modal is open, the UI SHALL show that view and its hints in place of
the normal help line.

#### Scenario: S opens the share list

- **WHEN** `S` is pressed while no other view is open and no server is active
- **THEN** the share list is displayed

#### Scenario: S is inert while the server is active

- **WHEN** `S` is pressed while either server task is active
- **THEN** no view opens and the hint line marks `S` unavailable

#### Scenario: Share views replace the help line

- **WHEN** the share list or the password modal is open
- **THEN** the UI shows that view and its key hints in place of the normal help line

## ADDED Requirements

### Requirement: HTTP download progress

While the HTTP server streams a file, the UI SHALL show that file in the same one-row progress region, and SHALL write the same start and completion summary lines, that a USB transfer produces. When both servers stream at once, the region SHALL show the file whose progress sample arrived most recently.

#### Scenario: An HTTP download shows in the progress region

- **WHEN** a file streams over HTTP
- **THEN** the progress region names that file and advances with its transferred bytes

#### Scenario: An HTTP completion is summarized once

- **WHEN** a file streams over HTTP to its end
- **THEN** the log shows exactly one success line naming the file and its size in human-readable units

#### Scenario: Concurrent streams follow the latest sample

- **WHEN** both servers stream different files at the same time
- **THEN** the progress region shows the file whose progress sample arrived most recently
