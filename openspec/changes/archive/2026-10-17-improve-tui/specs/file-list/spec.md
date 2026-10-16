# Spec Delta

## ADDED Requirements

### Requirement: Entries expose file size

When a file is added, the system SHALL record its size in bytes and SHALL make that size available alongside the entry's name and path, so callers can display it.

#### Scenario: Size is recorded on add

- **WHEN** a file of a known size is added
- **THEN** the entry reports that file's size in bytes

#### Scenario: Size is available for every entry

- **WHEN** the file list is read
- **THEN** each entry exposes a size value without re-reading the filesystem
