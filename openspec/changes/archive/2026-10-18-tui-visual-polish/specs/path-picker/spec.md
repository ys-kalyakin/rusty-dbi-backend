# Spec Delta

## MODIFIED Requirements

### Requirement: Picker lists directories and files

The picker SHALL list the subdirectories and regular files of the displayed directory, SHALL mark directories as enterable, and SHALL ignore failed individual entries without discarding the rest of the listing. The picker SHALL display each regular file's size alongside its name, and SHALL show the number of listed entries in the picker region; while a filter is active, it SHALL show how many entries match the filter instead.

#### Scenario: Directories and files are listed

- **WHEN** the displayed directory contains subdirectories and regular files
- **THEN** both appear in the listing, with directories distinguishable from files

#### Scenario: An unreadable entry is skipped

- **WHEN** one entry of the displayed directory cannot be inspected
- **THEN** the remaining entries are still listed

#### Scenario: File size is shown

- **WHEN** a regular file is listed
- **THEN** its size is shown alongside its name

#### Scenario: Entry count is shown

- **WHEN** the picker displays a directory
- **THEN** the number of listed entries is visible in the picker region

#### Scenario: Filtering shows a match count

- **WHEN** a filter is active
- **THEN** the picker region shows the number of entries matching the filter
