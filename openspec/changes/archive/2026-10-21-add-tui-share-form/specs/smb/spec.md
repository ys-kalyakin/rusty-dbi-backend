# Spec Delta

## ADDED Requirements

### Requirement: A share added through the UI is saved to the config file

When a share is added through the share-add UI, the system SHALL save it as a
new `[[shares]]` entry in the user's config file, carrying the entered `name`,
`server`, `share`, `username`, and `domain`/`root` when those were entered.
The system SHALL keep the config file free of passwords and SHALL retain the
shares already present in the file.

#### Scenario: The file gains the added share

- **WHEN** a share is added through the UI and the file can be written
- **THEN** the config file contains a `[[shares]]` entry with that share's name, server, share, username, and any entered domain or root, and no password

#### Scenario: Existing shares survive a save

- **WHEN** a share is added while the config file already contains another share
- **THEN** the saved file contains both shares

### Requirement: A share added mid-run is usable without restarting

A share added through the UI SHALL be usable in the same run: it appears in
the share list and connecting to it proceeds as for a share present at
startup, asking for the password first.

#### Scenario: A share added mid-run connects in the same run

- **WHEN** a share is added through the UI and the user connects to it in the same run
- **THEN** the password is asked first and the connection proceeds as usual

### Requirement: A failed save keeps the share for the run

A failed save SHALL be reported as a warning, leave the config file unchanged,
and still keep the added share listed and connectable for the current run.

#### Scenario: A failed save warns and keeps the share

- **WHEN** the config file cannot be written while a share is added through the UI
- **THEN** a warning is logged, the file is unchanged, and the share stays listed and connectable for the current run

