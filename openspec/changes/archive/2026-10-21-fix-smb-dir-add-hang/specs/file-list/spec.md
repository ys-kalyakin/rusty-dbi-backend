# Spec Delta

## ADDED Requirements

### Requirement: Directory scans stay finite

A directory add SHALL terminate even when a listing reports the self and
parent pseudo-entries `.` and `..` as directories: such entries SHALL NOT be
descended into. The reporting when a directory is skipped remains as defined
in the requirement "Directory scans tolerate unreadable entries".

#### Scenario: Dot pseudo-entries do not loop the walk

- **WHEN** a scanned directory's listing reports `.` and `..` as directories
- **THEN** the add terminates quickly and lists only the real matching files

#### Scenario: A directory with only dot entries adds nothing

- **WHEN** a directory is added whose listing contains only `.` and `..`
- **THEN** the add terminates quickly and the file list is unchanged
