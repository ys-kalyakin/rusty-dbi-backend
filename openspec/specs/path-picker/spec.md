# path-picker Specification

## Purpose

Lets a user browse the filesystem inside the terminal UI and choose a directory
or file to add to the file list, instead of typing an exact path from memory.

## Requirements

### Requirement: Picker opens at the current working directory

Opening the picker SHALL display the process's current working directory and its
entries.

#### Scenario: Picker shows the current directory

- **WHEN** the picker is opened
- **THEN** the displayed directory is the process's current working directory

#### Scenario: Current directory cannot be read

- **WHEN** the current working directory cannot be read
- **THEN** the picker reports an error and remains open at that directory

### Requirement: Picker lists directories and files

The picker SHALL list the subdirectories and regular files of the displayed
directory, SHALL mark directories as enterable, and SHALL ignore failed
individual entries without discarding the rest of the listing.

#### Scenario: Directories and files are listed

- **WHEN** the displayed directory contains subdirectories and regular files
- **THEN** both appear in the listing, with directories distinguishable from files

#### Scenario: An unreadable entry is skipped

- **WHEN** one entry of the displayed directory cannot be inspected
- **THEN** the remaining entries are still listed

### Requirement: Cursor movement

The picker SHALL let the user move the highlight up and down among the listed
entries and SHALL keep the highlight within the listing's bounds.

#### Scenario: Moving past the end stops

- **WHEN** the highlight is on the last entry and the user moves down again
- **THEN** the highlight stays on the last entry

#### Scenario: Moving above the start stops

- **WHEN** the highlight is on the first entry and the user moves up again
- **THEN** the highlight stays on the first entry

### Requirement: Navigating into a directory

When the highlighted entry is a directory, the picker SHALL be able to make it
the displayed directory and reset the highlight to the first entry.

#### Scenario: Descending into a directory

- **WHEN** a directory is highlighted and the user descends
- **THEN** that directory's entries are displayed and the highlight resets

### Requirement: Navigating up to the parent directory

The picker SHALL be able to move from the displayed directory to its parent
directory, and SHALL do nothing when the displayed directory is the filesystem
root.

#### Scenario: Going up from a subdirectory

- **WHEN** the displayed directory has a parent and the user navigates up
- **THEN** the parent directory's entries are displayed

#### Scenario: Going up from the filesystem root

- **WHEN** the displayed directory is the filesystem root and the user navigates up
- **THEN** the displayed directory is unchanged

### Requirement: Selecting an entry adds it to the file list

Selecting the highlighted entry SHALL add it to the file list using the existing
add-path semantics: a selected directory is scanned recursively and a selected
file is added directly, and the picker SHALL close after the attempt and log its
outcome.

#### Scenario: Selecting a directory adds its contents

- **WHEN** a directory is selected
- **THEN** the directory is added recursively and the picker closes

#### Scenario: Selecting a file adds that file

- **WHEN** a regular file is selected
- **THEN** that file is added directly and the picker closes

#### Scenario: A failed selection is logged

- **WHEN** the selected entry cannot be added
- **THEN** the failure is logged and the picker closes

### Requirement: Cancelling the picker

Cancelling the picker SHALL close it and leave the file list unchanged.

#### Scenario: Cancel discards the selection

- **WHEN** the picker is open and the user cancels
- **THEN** the picker closes and the file list is unchanged
