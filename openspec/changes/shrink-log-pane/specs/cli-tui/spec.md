# Spec Delta

## MODIFIED Requirements

### Requirement: Terminal UI layout

The UI SHALL show a status/header region, a file list region, a log region capped at 1000 retained lines, and a status/help line, and SHALL render each file as its name and resolved path. The log region SHALL display the most recent retained lines. The log region's on-screen height SHALL NOT exceed 8 rows including its border (at most 6 visible log lines), and every remaining vertical row SHALL go to the primary region — the file list, the picker, or the share list. On a terminal too short for that cap alongside the fixed rows and the primary region's minimum height, the log region SHALL shrink below the cap while the primary region keeps at least 3 rows. While the server is serving a file, the UI SHALL show a progress region for the current file. While the picker is open, the UI SHALL show the picker's listing and its key hints in place of the normal help line. When no files are offered, the file list region SHALL show an onboarding hint that names the key which adds files.

#### Scenario: Log is capped

- **WHEN** more than 1000 log lines are emitted
- **THEN** only the most recent 1000 are retained

#### Scenario: Log shows the newest lines

- **WHEN** more log lines are emitted than the log region can display
- **THEN** the most recent lines are visible without scrolling

#### Scenario: The log pane's height is capped

- **WHEN** the main screen is drawn on a terminal taller than the fixed rows plus 8 log rows
- **THEN** the log region is at most 8 rows high and the primary region takes every remaining row

#### Scenario: The log yields space on short terminals

- **WHEN** the terminal is too short for the header, the primary region's minimum height, an 8-row log region, and the help line
- **THEN** the log region shrinks below 8 rows while the primary region keeps at least 3 rows

#### Scenario: Picker view replaces the help line

- **WHEN** the picker is open
- **THEN** the UI shows the picker listing and picker key hints

#### Scenario: Progress region appears while serving

- **WHEN** the server is serving a file range to the Switch
- **THEN** the UI shows the current file's name and its progress percentage

#### Scenario: Empty list shows an onboarding hint

- **WHEN** no files are offered and the picker is closed
- **THEN** the file list region displays a hint that names the key for adding files
