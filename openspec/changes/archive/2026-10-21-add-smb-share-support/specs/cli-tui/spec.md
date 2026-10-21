# Spec Delta

## ADDED Requirements

### Requirement: Share connection key

Outside the picker, the UI SHALL map `S` to open the share list. While the
server task is active, `S` SHALL be inert and the hint line SHALL visibly mark
it unavailable like the other locked editing keys. While the share list or the
password modal is open, the UI SHALL show that view and its hints in place of
the normal help line.

#### Scenario: S opens the share list

- **WHEN** `S` is pressed while no other view is open and the server is not active
- **THEN** the share list is displayed

#### Scenario: S is inert while the server is active

- **WHEN** `S` is pressed while the server task is active
- **THEN** no view opens and the hint line marks `S` unavailable

#### Scenario: Share views replace the help line

- **WHEN** the share list or the password modal is open
- **THEN** the UI shows that view and its key hints in place of the normal help line

### Requirement: Share list presentation

The share list SHALL show each configured share's name, server, share, and
username, SHALL cancel back to the normal view without connecting, and SHALL
show a hint naming the config file path when no shares are configured.

#### Scenario: Shares are listed with their details

- **WHEN** the share list opens with configured shares
- **THEN** each row shows that share's name, server, share, and username

#### Scenario: Cancelling the share list connects to nothing

- **WHEN** the user cancels the share list
- **THEN** the UI returns to the normal view and no connection is attempted

#### Scenario: Empty share list names the config file

- **WHEN** the share list opens and no shares are configured
- **THEN** the view shows a hint naming the config file path

### Requirement: Share-qualified origin in the file list

The file list SHALL render a share file's resolved path as the share's display
name followed by the file's path within the share, and SHALL render a local
file's resolved path as before.

#### Scenario: A share file shows its origin

- **WHEN** a file added from a share named `nas` is displayed
- **THEN** its resolved path is shown as `nas:<path within the share>`

#### Scenario: A local file keeps its plain path

- **WHEN** a local file is displayed
- **THEN** its resolved path is shown as before, without a share prefix
