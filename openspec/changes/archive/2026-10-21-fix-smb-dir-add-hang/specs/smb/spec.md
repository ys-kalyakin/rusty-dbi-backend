# Spec Delta

## ADDED Requirements

### Requirement: Share listings exclude self and parent entries

The system SHALL NOT include the `.` and `..` pseudo-entries in a share
directory listing, no matter what the share server returns, so that share
browsing matches the local filesystem behavior.

#### Scenario: A server that returns dot pseudo-entries yields a clean listing

- **WHEN** a share server returns `.` and `..` among a directory's entries and the picker displays that directory
- **THEN** the displayed entries contain no `.` or `..`

#### Scenario: An add cannot receive dot pseudo-entries

- **WHEN** a recursive add consumes the share listing of a directory whose server response contains `.` and `..` as directories
- **THEN** the listing handed to the walk contains neither of them
