# Spec Delta

## ADDED Requirements

### Requirement: Compact confirmation dialog

The confirmation prompt for clearing the list or quitting SHALL be presented in
a centered dialog sized to fit its prompt text plus a minimal border, and SHALL
NOT occupy a fixed fraction of the terminal. The prompt text and confirmation
semantics SHALL be unchanged.

#### Scenario: Dialog fits its prompt

- **WHEN** the quit or clear confirmation is shown
- **THEN** the dialog's width and height are just large enough for the prompt text and border, without a large empty area

#### Scenario: Both confirmations use the compact dialog

- **WHEN** either the quit or the clear confirmation is shown
- **THEN** both are presented in the same compact dialog rather than a terminal-relative fraction

#### Scenario: Small terminal still shows the prompt

- **WHEN** the terminal is smaller than the content-fitted dialog
- **THEN** the dialog is clamped to the available terminal area so the prompt remains visible
