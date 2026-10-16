# Spec Delta

## ADDED Requirements

### Requirement: Picker listing can be filtered

The picker SHALL allow the user to enter a filter string and SHALL show only the entries whose name contains that string, case-insensitively, keeping the highlight within the filtered listing's bounds. Clearing the filter SHALL restore the full listing.

#### Scenario: Filter narrows the listing

- **WHEN** a filter string is entered
- **THEN** only entries whose name contains that string, ignoring case, are listed

#### Scenario: Filter with no matches

- **WHEN** the filter string matches no entry
- **THEN** the listing is empty and no entry is highlighted

#### Scenario: Clearing the filter restores the listing

- **WHEN** the filter is cleared
- **THEN** all entries of the displayed directory are listed again

#### Scenario: Navigation stays within the filtered listing

- **WHEN** the highlight is on the last filtered entry and the user moves down
- **THEN** the highlight stays on that last entry
