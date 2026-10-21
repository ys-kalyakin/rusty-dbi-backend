# Spec Delta

## Purpose

Defines how the backend is configured with Samba shares, how it connects to
them lazily with a password asked per connection, and how files on a connected
share are browsed and read from within the terminal UI.

## ADDED Requirements

### Requirement: Shares are configured in a config file

The system SHALL read share definitions from the user's configuration file
(`~/.config/dbi-backend/config.toml` by default), where each share carries a
display name, a server, a share name, and a username, and MAY name the share's
browsable root inside the share. The configuration SHALL NOT contain
passwords, and a malformed share entry SHALL produce a warning without hiding
the remaining shares.

#### Scenario: A configured share is listed

- **WHEN** the config file defines one valid share
- **THEN** the share list shows that share's name, server, share, and username

#### Scenario: Missing config leaves an empty share list

- **WHEN** the config file does not exist
- **THEN** the share list is empty and its hint names the config file path

#### Scenario: A malformed entry warns without hiding the rest

- **WHEN** the config file contains one malformed entry and one valid share
- **THEN** the valid share is listed, the malformed entry is reported, and the program continues

### Requirement: Connecting asks for the password every time

The system SHALL ask for a share's password every time that share is
connected. The password SHALL NOT be accepted from command-line arguments,
SHALL NOT be written to any file, and SHALL NOT appear in any log line,
including error messages.

#### Scenario: A fresh connection prompts for the password

- **WHEN** the user connects to a share
- **THEN** the system asks for that share's password before connecting

#### Scenario: A failed login does not leak the password

- **WHEN** connecting fails because the password is wrong
- **THEN** the logged failure names the share but not the password

### Requirement: Password entry is masked and cancellable

The system SHALL collect the password in a modal input that does not echo the
typed characters, SHALL submit it on Enter, and SHALL cancel the connection on
Esc, leaving the share disconnected.

#### Scenario: Typed characters are not displayed

- **WHEN** the user types a password into the password input
- **THEN** the UI does not display the typed characters

#### Scenario: Esc cancels without connecting

- **WHEN** the user presses Esc in the password input
- **THEN** the modal closes and no connection is attempted

### Requirement: Shares connect only on demand

The system SHALL connect to a share only when the user asks for it from the
share list, and SHALL NOT open any share connection at startup or while the
user works only with local files.

#### Scenario: Startup connects to nothing

- **WHEN** the program starts with configured shares and the user adds local files
- **THEN** no share connection is made

#### Scenario: Connecting again asks again

- **WHEN** the user connects to a share and later connects to the same share again in the same run
- **THEN** the password is asked again before the connection is re-established

### Requirement: A connected share is browsed like the local filesystem

Once a share is connected, the picker SHALL start at that share's browsable
root (its configured root, or the share root) and SHALL use the same
navigation, filtering, and add semantics as local browsing. Navigating up from
the share's browsable root SHALL stay there.

#### Scenario: The picker opens at the share root

- **WHEN** a share connects
- **THEN** the picker displays that share's browsable root and its entries

#### Scenario: Going up stops at the share root

- **WHEN** the displayed directory is the share's browsable root and the user navigates up
- **THEN** the displayed directory is unchanged

### Requirement: A dead share session is re-established when opening a file

When a file on a share is opened and that share's session is no longer valid,
the system SHALL re-establish the session using the password held in memory
for the current run and retry the open. If re-establishing fails, the open
SHALL fail as any other open failure.

#### Scenario: A rebooted NAS reconnects transparently

- **WHEN** the server restarts between adding files and the first install read
- **THEN** the open re-establishes the session and the read proceeds

#### Scenario: A failed reconnection fails the open

- **WHEN** the session cannot be re-established
- **THEN** the open fails and the failure is logged as any other open failure

### Requirement: Passwords live only for the application run

The system SHALL keep an entered password in memory only for the current
application run and SHALL discard it when the application exits.

#### Scenario: A new run asks again

- **WHEN** the user quits and starts the application again
- **THEN** connecting to the same share asks for the password again
