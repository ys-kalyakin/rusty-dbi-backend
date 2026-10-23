//! The key handling: one handler per view, dispatched from the normal view.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use dbi_backend_core::source::Source;

use super::state::{
    clamped_step, Action, App, ConfirmKind, Mode, PasswordModal, ServerStatus, ShareFormState,
    ShareList,
};
use crate::picker::Picker;
use dbi_backend_core::source::EntryKind;

/// The character a typed value accepts: any character that is not a control
/// character. Today's keys send characters in that set and nothing else; the
/// written-out `is_ascii_graphic()` and space clauses never changed its
/// result, because every control character is neither graphic nor a space.
fn printable(c: char) -> bool {
    !c.is_control()
}

impl App {
    /// Picker keys. A selection comes back as an async action for the caller
    /// to run, so a local file is read the same way as a share file.
    fn handle_picker_key(&mut self, key: KeyEvent) -> Action {
        enum Outcome {
            None,
            Select(Source),
            /// Request a store listing for this share directory.
            List(Source),
            Cancel,
        }

        let outcome = {
            let Mode::Picker(picker) = &mut self.mode else {
                return Action::None;
            };
            if picker.is_filtering() {
                match key.code {
                    KeyCode::Char(c) => {
                        picker.push_filter(c);
                        Outcome::None
                    }
                    KeyCode::Backspace => {
                        picker.pop_filter();
                        Outcome::None
                    }
                    KeyCode::Enter => {
                        picker.stop_filter();
                        Outcome::None
                    }
                    KeyCode::Esc => {
                        picker.clear_filter();
                        picker.stop_filter();
                        Outcome::None
                    }
                    KeyCode::Up => {
                        picker.move_up();
                        Outcome::None
                    }
                    KeyCode::Down => {
                        picker.move_down();
                        Outcome::None
                    }
                    _ => Outcome::None,
                }
            } else {
                match key.code {
                    KeyCode::Char('/') => {
                        picker.start_filter();
                        Outcome::None
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        picker.move_down();
                        Outcome::None
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        picker.move_up();
                        Outcome::None
                    }
                    KeyCode::Char(' ') => match picker.current() {
                        Some(entry) => Outcome::Select(entry.source.clone()),
                        None => Outcome::None,
                    },
                    KeyCode::Enter => {
                        if picker.current().map(|e| e.kind) == Some(EntryKind::Dir) {
                            picker.descend();
                            // Only share origins list asynchronously.
                            match picker.reload_request() {
                                Some(source) => Outcome::List(source),
                                None => Outcome::None,
                            }
                        } else {
                            match picker.current() {
                                Some(entry) => Outcome::Select(entry.source.clone()),
                                None => Outcome::None,
                            }
                        }
                    }
                    KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                        if let Some(source) = picker.go_up() {
                            Outcome::List(source)
                        } else {
                            Outcome::None
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') => Outcome::Cancel,
                    _ => Outcome::None,
                }
            }
        };

        match outcome {
            Outcome::Select(source) => {
                // The add needs the store, so it comes back as an action.
                self.mode = Mode::Normal;
                self.clamp_selection();
                Action::Add(source)
            }
            Outcome::List(source) => Action::List(source),
            Outcome::Cancel => {
                self.mode = Mode::Normal;
                Action::None
            }
            Outcome::None => Action::None,
        }
    }

    fn handle_confirm_key(&mut self, key: KeyEvent, kind: ConfirmKind) -> Action {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            match kind {
                ConfirmKind::Quit => {
                    self.should_quit = true;
                    self.mode = Mode::Normal;
                    return Action::Quit;
                }
                ConfirmKind::Clear => {
                    self.files.clear();
                    self.clamp_selection();
                    self.push_log("List cleared".into());
                }
            }
        }
        self.mode = Mode::Normal;
        Action::None
    }
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }

        match &self.mode {
            Mode::Picker(_) => {
                return self.handle_picker_key(key);
            }
            Mode::Shares(_) => {
                self.handle_shares_key(key);
                return Action::None;
            }
            Mode::Password(_) => {
                return self.handle_password_key(key);
            }
            Mode::ShareForm(_) => {
                return self.handle_share_form_key(key);
            }
            Mode::Confirm(kind) => {
                let kind = *kind;
                return self.handle_confirm_key(key, kind);
            }
            Mode::Help => {
                if matches!(
                    key.code,
                    KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')
                ) {
                    self.mode = Mode::Normal;
                }
                return Action::None;
            }
            Mode::Normal => {}
        }

        // The open search input takes the keys while it is open, in every
        // view it can be opened from.
        if self.is_searching() {
            return self.handle_search_key(key);
        }

        // While the server task is active (waiting or running), edits are inert so
        // the offered list matches the server's snapshot.
        let running = self.status != ServerStatus::Idle;
        match key.code {
            KeyCode::Char('q') => {
                self.mode = Mode::Confirm(ConfirmKind::Quit);
                Action::None
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                Action::None
            }
            KeyCode::Char('a') => {
                if running {
                    Action::None
                } else {
                    self.mode = Mode::Picker(Picker::open());
                    Action::None
                }
            }
            KeyCode::Char('c') => {
                if running {
                    Action::None
                } else {
                    self.mode = Mode::Confirm(ConfirmKind::Clear);
                    Action::None
                }
            }
            KeyCode::Char('d') => {
                if !running {
                    self.remove_selected();
                }
                Action::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                Action::None
            }
            KeyCode::Char('s') => {
                if self.status == ServerStatus::Idle && !self.files.is_empty() {
                    self.status = ServerStatus::Waiting;
                    Action::Start
                } else {
                    Action::None
                }
            }
            KeyCode::Char('S') => {
                if !running {
                    self.mode = Mode::Shares(ShareList::open());
                }
                Action::None
            }
            KeyCode::Char('/') => {
                self.start_search();
                Action::None
            }
            KeyCode::Esc => {
                if !self.search().is_empty() {
                    self.clear_search();
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Keys of the open search input: type to narrow the list, Backspace to
    /// erase, Enter to keep the search and close the input, Esc to clear the
    /// search, and the arrows to move the highlight. The search is a
    /// read-only view: it works while the server runs.
    fn handle_search_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char(c) => {
                self.push_search(c);
                Action::None
            }
            KeyCode::Backspace => {
                self.pop_search();
                Action::None
            }
            KeyCode::Enter => {
                self.stop_search();
                Action::None
            }
            KeyCode::Esc => {
                self.clear_search();
                self.stop_search();
                Action::None
            }
            KeyCode::Up => {
                self.move_selection(-1);
                Action::None
            }
            KeyCode::Down => {
                self.move_selection(1);
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Keys of the share list view: move the highlight, open the password
    /// modal for the selected share on Enter, open the add-share form with
    /// `a`, cancel back to normal. Nothing connects from this view itself —
    /// cancelling connects to nothing at all.
    fn handle_shares_key(&mut self, key: KeyEvent) {
        let Mode::Shares(list) = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                list.selected =
                    clamped_step(list.selected, self.shares.len(), 1).unwrap_or(list.selected);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                list.selected =
                    clamped_step(list.selected, self.shares.len(), -1).unwrap_or(list.selected);
            }
            KeyCode::Enter => {
                if let Some(share) = self.shares.get(list.selected).map(|s| s.name.clone()) {
                    self.mode = Mode::Password(PasswordModal::for_share(&share));
                }
            }
            KeyCode::Char('a') => {
                self.mode = Mode::ShareForm(ShareFormState::open(list.selected));
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    /// Keys of the add-share form: type into the focused field, arrows and Tab
    /// move the focus between fields, Esc cancels back to the share list. No
    /// field asks for or masks a password.
    fn handle_share_form_key(&mut self, key: KeyEvent) -> Action {
        let Mode::ShareForm(form) = &mut self.mode else {
            return Action::None;
        };
        match key.code {
            KeyCode::Char(c) if printable(c) => {
                form.push(c);
            }
            KeyCode::Backspace => {
                form.pop();
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    form.previous_field();
                } else {
                    form.next_field();
                }
            }
            KeyCode::Down => {
                form.next_field();
            }
            KeyCode::Up => {
                form.previous_field();
            }
            KeyCode::Enter => {
                let names: Vec<String> = self.shares.iter().map(|s| s.name.clone()).collect();
                match form.build(&names) {
                    Ok(config) => {
                        // The form stays open until the caller has registered
                        // the share and brought the share list back up.
                        return Action::AddShare { config };
                    }
                    Err(errors) => form.set_errors(errors),
                }
            }
            KeyCode::Esc => {
                let return_to = form.return_selection();
                self.mode = Mode::Shares(ShareList {
                    selected: return_to,
                });
            }
            _ => {}
        }
        Action::None
    }

    /// Keys of the password modal: type printable keys, Enter submits (via
    /// [`Action::Connect`]), Esc cancels leaving the share disconnected.
    fn handle_password_key(&mut self, key: KeyEvent) -> Action {
        let Mode::Password(modal) = &mut self.mode else {
            return Action::None;
        };
        match key.code {
            KeyCode::Char(c) if printable(c) => {
                modal.push(c);
                Action::None
            }
            KeyCode::Backspace => {
                modal.pop();
                Action::None
            }
            KeyCode::Enter => {
                let Some(password) = modal.take() else {
                    return Action::None;
                };
                let share = modal.share.clone();
                let selection = self
                    .shares
                    .iter()
                    .position(|s| s.name == share)
                    .unwrap_or(0);
                self.mode = Mode::Shares(ShareList {
                    selected: selection,
                });
                Action::Connect { share, password }
            }
            KeyCode::Esc => {
                let share = modal.share.clone();
                let selection = self
                    .shares
                    .iter()
                    .position(|s| s.name == share)
                    .unwrap_or(0);
                self.mode = Mode::Shares(ShareList {
                    selected: selection,
                });
                Action::None
            }
            _ => Action::None,
        }
    }
}
