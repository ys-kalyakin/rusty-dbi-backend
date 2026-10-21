# Spec Delta

## ADDED Requirements

### Requirement: Share add form

Inside the share list, the UI SHALL map `a` to open the add-share form: a modal
with fields `name`, `server`, `share`, and `username`, plus optional `domain`
and `root`. The form SHALL NOT contain or ask for a password. While the form is
open, the UI SHALL show it and its key hints in place of the normal help line,
and Esc SHALL cancel the form, discarding everything typed.

#### Scenario: A opens the add form

- **WHEN** `a` is pressed while the share list is open
- **THEN** the add-share form opens with all fields empty

#### Scenario: The form has no password

- **WHEN** the add-share form is shown
- **THEN** none of its fields is a password and none is masked

#### Scenario: The form replaces the help line

- **WHEN** the add-share form is open
- **THEN** the UI shows the form and its key hints in place of the normal help line

#### Scenario: Esc cancels without saving

- **WHEN** the user presses Esc in the add-share form
- **THEN** the form closes, the share list is shown unchanged, and nothing was saved

### Requirement: Share form field entry and validation

The form SHALL let the user edit each field by typing and erasing, move focus
between fields with the keys its hints show, and submit with Enter. On submit
the form SHALL require `name`, `server`, `share`, and `username` to be
non-empty, SHALL accept `server` only as `host` or `host:port`, and SHALL
reject a `name` already used by a configured share. An invalid submit SHALL
keep the form open and say what is wrong; the typed values SHALL be kept.

#### Scenario: Editing and moving between fields

- **WHEN** the user types into a field, erases with backspace, and moves focus to another field
- **THEN** each field keeps its own typed text

#### Scenario: Empty required fields are rejected

- **WHEN** the form is submitted with an empty required field
- **THEN** the form stays open and names what is missing

#### Scenario: A bad server is rejected

- **WHEN** the form is submitted with `server` that is not `host` or `host:port`
- **THEN** the form stays open and reports the bad server

#### Scenario: A duplicate name is rejected

- **WHEN** the form is submitted with the name of an already-configured share
- **THEN** the form stays open and reports the name as taken

#### Scenario: Optional fields may be left empty

- **WHEN** the form is submitted with `domain` and `root` left empty
- **THEN** the added share has no domain and an empty root, like a config entry without them

### Requirement: Submitting the add form

Submitting valid data SHALL close the form and return to the share list, which
SHALL include the added share with the highlight on it. The form SHALL NOT
connect to the share or ask for a password.

#### Scenario: A valid submit adds the share

- **WHEN** the form is submitted with valid data
- **THEN** the share list is shown again, lists the added share, and highlights it

#### Scenario: Submit does not connect

- **WHEN** the form is submitted with valid data
- **THEN** no connection is attempted and no password is asked

## MODIFIED Requirements

### Requirement: Share list presentation

The share list SHALL show each configured share's name, server, share, and
username, SHALL cancel back to the normal view without connecting, and SHALL
show, when no shares are configured, a hint naming both the key that opens the
add-share form and the config file path.

#### Scenario: Shares are listed with their details

- **WHEN** the share list opens with configured shares
- **THEN** each row shows that share's name, server, share, and username

#### Scenario: Cancelling the share list connects to nothing

- **WHEN** the user cancels the share list
- **THEN** the UI returns to the normal view and no connection is attempted

#### Scenario: Empty share list names the config file

- **WHEN** the share list opens and no shares are configured
- **THEN** the view shows a hint naming both the key that opens the add-share form and the config file path
