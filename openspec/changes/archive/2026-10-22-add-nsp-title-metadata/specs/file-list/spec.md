# Spec Delta

## ADDED Requirements

### Requirement: Entries carry the title identity

When a file is added, the system SHALL record the identity of its container —
the title id, the content type and the version — whenever one could be
determined, and SHALL make that identity available alongside the entry's name
and size. Reading the identity SHALL NOT keep the file from being added.

#### Scenario: The identity is recorded on add

- **WHEN** a file whose title id can be determined is added
- **THEN** the entry reports that title id, the content type and the version

#### Scenario: The identity is available for every entry

- **WHEN** the file list is read
- **THEN** each entry exposes an identity, empty for a file whose identity could not be determined

#### Scenario: A file that cannot be read for its identity is still added

- **WHEN** a file is added whose container cannot be read
- **THEN** the entry is present in the file list and carries no identity
