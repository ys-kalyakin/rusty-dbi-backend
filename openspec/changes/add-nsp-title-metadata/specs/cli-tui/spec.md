# Spec Delta

## ADDED Requirements

### Requirement: The file list shows the title identity

The file list SHALL show, next to each file that has an identity, its title
id, its content type and its version, and SHALL show the file's name alone for
a file without an identity.

#### Scenario: The identity is shown next to the name

- **WHEN** a file with a title id, a content type and a version is listed
- **THEN** the list shows that title id, that content type and that version next to the file's name

#### Scenario: A file without an identity shows only its name

- **WHEN** a file without an identity is listed
- **THEN** the list shows the file's name and no title id, content type or version
