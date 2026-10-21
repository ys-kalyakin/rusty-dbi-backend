//! TUI application state and the background server task.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config;
use crate::config::ShareConfig;
use crate::picker::{Kind, Picker};
use crate::theme;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use dbi_backend_core::filelist::{AddOutcome, FileList};
use dbi_backend_core::server::{Server, ServerUpdate};
use dbi_backend_core::source::{ListEntry, Source, SourceError, Sources};
use dbi_backend_core::transport::usb::UsbTransport;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Gauge, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

/// Maximum number of log lines retained (mirrors the Python 1000-line cap).
pub const MAX_LOG_LINES: usize = 1000;

/// Ignore samples closer together than this when estimating transfer speed.
const MIN_SPEED_SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

/// Time constant for exponentially smoothing the reported transfer speed.
const SPEED_SMOOTHING_TAU_SECS: f64 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServerStatus {
    Idle,
    Waiting,
    Running,
}

/// What the caller should do after a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Quit,
    /// Connect to a share by display name with its typed password. Closing
    /// this action drops the password; the hub is the only holder afterwards.
    Connect {
        share: String,
        password: String,
        selection: usize,
    },
    /// List a share directory through the store (the picker asked for it).
    List(Source),
    /// Add a share-sourced path to the file list (needs the store).
    Add(Source),
    /// Register a share the add form built: connectable at once in the same
    /// run and saved to the config file by the caller. No password is carried.
    AddShare {
        config: ShareConfig,
    },
}

/// A lifecycle or progress transition reported by the background server task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    Started,
    Disconnected,
    Ended,
    Progress {
        name: String,
        sent: u64,
        total: u64,
    },
    /// A file range was streamed in full; the progress region ends with it.
    Completed {
        name: String,
        bytes: u64,
    },
}

/// Which destructive action a confirmation prompt is gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKind {
    Quit,
    Clear,
}

/// Which view is active.
pub enum Mode {
    Normal,
    Shares(ShareList),
    Password(PasswordModal),
    Picker(Picker),
    ShareForm(ShareFormState),
    Confirm(ConfirmKind),
    Help,
}

/// The add-share form's field order: display name, SMB server, share, login
/// username, optional domain, and optional browsable root. No field is a
/// password and none is masked — passwords are never asked here.
pub const SHARE_FORM_FIELDS: usize = 6;
const FIELD_NAME: usize = 0;
const FIELD_SERVER: usize = 1;
const FIELD_SHARE: usize = 2;
const FIELD_USERNAME: usize = 3;
const FIELD_DOMAIN: usize = 4;
const FIELD_ROOT: usize = 5;

/// The add-share dialog state: six labeled buffers in field order, the focused
/// field, the errors from the last submit, and the share-list selection to
/// return to on Esc. Validation happens in `config::build_share`.
pub struct ShareFormState {
    buffers: [String; SHARE_FORM_FIELDS],
    focus: usize,
    errors: Vec<String>,
    return_to: usize,
}

impl ShareFormState {
    /// The empty form with the focus on the first field.
    pub fn open(return_to: usize) -> Self {
        Self {
            buffers: Default::default(),
            focus: 0,
            errors: Vec::new(),
            return_to,
        }
    }

    /// Type into the focused field.
    pub fn push(&mut self, c: char) {
        self.buffers[self.focus].push(c);
    }

    /// Erase the focused field's last typed character.
    pub fn pop(&mut self) {
        self.buffers[self.focus].pop();
    }

    pub fn next_field(&mut self) {
        if self.focus + 1 < SHARE_FORM_FIELDS {
            self.focus += 1;
        }
    }

    pub fn previous_field(&mut self) {
        self.focus = self.focus.saturating_sub(1);
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn field_value(&self, field: usize) -> &str {
        &self.buffers[field]
    }

    /// The share-list selection Esc returns to.
    pub fn return_selection(&self) -> usize {
        self.return_to
    }

    pub fn set_errors(&mut self, errors: Vec<String>) {
        self.errors = errors;
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Validate the typed values against the names already configured; the
    /// same rules the config loader applies.
    pub fn build(&self, existing_names: &[String]) -> Result<ShareConfig, Vec<String>> {
        config::build_share(
            &self.buffers[FIELD_NAME],
            &self.buffers[FIELD_SERVER],
            &self.buffers[FIELD_SHARE],
            &self.buffers[FIELD_USERNAME],
            &self.buffers[FIELD_DOMAIN],
            &self.buffers[FIELD_ROOT],
            existing_names,
        )
    }
}

/// The share list view state: which configured share is highlighted.
pub struct ShareList {
    pub selected: usize,
}

impl ShareList {
    pub fn open() -> Self {
        Self { selected: 0 }
    }
}

/// The password modal for one share: the display name and the typed buffer,
/// always rendered masked. The buffer is emptied on submit; the password
/// travels in the [`Action::Connect`] and lives on inside the hub only.
pub struct PasswordModal {
    pub share: String,
    password: String,
}

impl PasswordModal {
    pub fn for_share(share: &str) -> Self {
        Self {
            share: share.to_string(),
            password: String::new(),
        }
    }

    pub fn push(&mut self, c: char) {
        self.password.push(c);
    }

    pub fn pop(&mut self) {
        self.password.pop();
    }

    /// The typed password, if any; the modal's buffer is emptied either way.
    pub fn take(&mut self) -> Option<String> {
        if self.password.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.password))
        }
    }

    /// How many mask characters to render.
    pub fn typed_len(&self) -> usize {
        self.password.len()
    }
}

/// A retained log line with its age relative to application start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub at: Duration,
    pub message: String,
}

/// The latest transfer progress, with derived speed once two samples are seen.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressState {
    pub name: String,
    pub sent: u64,
    pub total: u64,
    pub speed: Option<u64>,
    /// Estimated seconds remaining, derivable once a transfer speed is known.
    pub eta: Option<u64>,
    /// Timestamp and byte count of the last sample accepted by the rate estimator.
    speed_sample_at: Instant,
    speed_sample_sent: u64,
    smoothed_speed: Option<f64>,
}

pub struct App {
    pub files: FileList,
    pub log: VecDeque<LogLine>,
    pub status: ServerStatus,
    pub mode: Mode,
    pub should_quit: bool,
    pub selected: usize,
    pub progress: Option<ProgressState>,
    /// Configured shares for the share list, sorted by display name.
    pub shares: Vec<ShareConfig>,
    /// Where the config file lives, for the empty-share-list hint.
    pub config_hint: Option<PathBuf>,
    server_started: bool,
    started_at: Instant,
}

impl App {
    /// The plain constructor: no shares configured. Also the test harness's
    /// default, hence the dead-code allowance for the server-only binary.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(files: FileList) -> Self {
        Self::with_shares(files, Vec::new())
    }

    /// An app that knows the configured shares for the share list view.
    pub fn with_shares(files: FileList, mut shares: Vec<ShareConfig>) -> Self {
        shares.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            files,
            log: VecDeque::new(),
            status: ServerStatus::Idle,
            mode: Mode::Normal,
            should_quit: false,
            selected: 0,
            progress: None,
            shares,
            config_hint: config::config_path(),
            server_started: false,
            started_at: Instant::now(),
        }
    }

    pub fn push_log(&mut self, line: String) {
        let at = self.started_at.elapsed();
        self.log.push_back(LogLine { at, message: line });
        while self.log.len() > MAX_LOG_LINES {
            self.log.pop_front();
        }
    }

    /// Apply a lifecycle or progress transition from the background server task.
    pub fn handle_server_event(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::Started => {
                self.status = ServerStatus::Running;
                // Progress belongs to a live connection; never revive an old bar.
                self.progress = None;
            }
            ServerEvent::Disconnected => {
                self.status = ServerStatus::Waiting;
                self.progress = None;
            }
            ServerEvent::Ended => {
                self.status = ServerStatus::Idle;
                self.server_started = false;
                self.progress = None;
            }
            ServerEvent::Progress { name, sent, total } => {
                self.update_progress(name, sent, total);
            }
            ServerEvent::Completed { name, bytes } => {
                self.progress = None;
                self.push_log(format!("Sent: {name} ({})", human_size(bytes)));
            }
        }
    }

    fn update_progress(&mut self, name: String, sent: u64, total: u64) {
        self.update_progress_at(name, sent, total, Instant::now());
    }

    fn update_progress_at(&mut self, name: String, sent: u64, total: u64, now: Instant) {
        let Some(previous) = self.progress.as_ref().filter(|state| state.name == name) else {
            self.progress = Some(ProgressState {
                name,
                sent,
                total,
                speed: None,
                eta: None,
                speed_sample_at: now,
                speed_sample_sent: sent,
                smoothed_speed: None,
            });
            return;
        };

        // A sample that moves the bar backward starts a new streamed phase: the
        // region follows it at the new position, and the previous phase's speed
        // and estimate history is discarded so nothing continues across the
        // restart.
        if sent < previous.sent {
            self.progress = Some(ProgressState {
                name,
                sent,
                total,
                speed: None,
                eta: None,
                speed_sample_at: now,
                speed_sample_sent: sent,
                smoothed_speed: None,
            });
            return;
        }

        let mut next = ProgressState {
            name,
            sent,
            total,
            speed: previous.speed,
            eta: previous.eta,
            speed_sample_at: previous.speed_sample_at,
            speed_sample_sent: previous.speed_sample_sent,
            smoothed_speed: previous.smoothed_speed,
        };

        if sent > previous.speed_sample_sent {
            let elapsed = now.saturating_duration_since(previous.speed_sample_at);
            if elapsed >= MIN_SPEED_SAMPLE_INTERVAL {
                let elapsed_secs = elapsed.as_secs_f64();
                let instant = (sent - previous.speed_sample_sent) as f64 / elapsed_secs;
                let smoothed = match previous.smoothed_speed {
                    Some(previous_speed) => {
                        let alpha = 1.0 - (-elapsed_secs / SPEED_SMOOTHING_TAU_SECS).exp();
                        previous_speed + alpha * (instant - previous_speed)
                    }
                    None => instant,
                };
                let speed = smoothed.round() as u64;
                next.speed = Some(speed);
                next.eta = Some(remaining_seconds(sent, total, speed));
                next.speed_sample_at = now;
                next.speed_sample_sent = sent;
                next.smoothed_speed = Some(smoothed);
            }
        }

        self.progress = Some(next);
    }

    /// Add a path to the file list, logging the outcome and closing the picker.
    fn select_path(&mut self, path: &Path) {
        match self.files.add_path(path) {
            Ok(outcome) => {
                self.push_log(format!("Added: {}", path.display()));
                for skipped in &outcome.skipped {
                    self.push_log(format!("Warning: skipped {skipped}: cannot be read"));
                }
                for replaced in &outcome.replaced {
                    self.push_log(format!(
                        "Warning: {replaced} replaced a previously added file"
                    ));
                }
            }
            Err(e) => self.push_log(format!("Failed to add {}: {e}", path.display())),
        }
        self.mode = Mode::Normal;
        self.clamp_selection();
    }

    /// Picker keys. Local adds close the picker inline; share adds and share
    /// listings come back as async actions for the caller to run.
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
                        if picker.current().map(|e| e.kind) == Some(Kind::Dir) {
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
            Outcome::Select(source) => match source {
                // Local adds complete instantly; share adds need the store.
                Source::Local(path) => {
                    self.select_path(&path);
                    Action::None
                }
                share => {
                    self.mode = Mode::Normal;
                    self.clamp_selection();
                    Action::Add(share)
                }
            },
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

    fn move_selection(&mut self, delta: isize) {
        let len = self.files.len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        let max = len - 1;
        let current = self.selected.min(max);
        self.selected = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            (current + delta as usize).min(max)
        };
    }

    fn clamp_selection(&mut self) {
        let len = self.files.len();
        self.selected = if len == 0 {
            0
        } else {
            self.selected.min(len - 1)
        };
    }

    fn remove_selected(&mut self) {
        let name = self.files.names().nth(self.selected).cloned();
        if let Some(name) = name {
            if self.files.remove(&name).is_some() {
                self.push_log(format!("Removed: {name}"));
                self.clamp_selection();
            }
        }
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

        // While the server task is active (waiting or running), edits are inert so
        // the offered list matches the server's snapshot.
        let running = self.server_started;
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
                if !self.server_started && !self.files.is_empty() {
                    self.server_started = true;
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
                if !self.shares.is_empty() {
                    list.selected = (list.selected + 1).min(self.shares.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                list.selected = list.selected.saturating_sub(1);
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
            KeyCode::Char(c) if c.is_ascii_graphic() || c == ' ' || !c.is_control() => {
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
            KeyCode::Char(c) if c.is_ascii_graphic() || c == ' ' || !c.is_control() => {
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
                Action::Connect {
                    share,
                    password,
                    selection,
                }
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

    /// The configured share by display name, for the picker's browsable root.
    pub fn share_config(&self, name: &str) -> Option<&ShareConfig> {
        self.shares.iter().find(|config| config.name == name)
    }

    /// Register a share the add form built: inserted among the sorted shares
    /// (replacing a same-named entry defensively, as the hub does), with the
    /// share list brought back up and the highlight on the added share.
    /// Returns that share's position in the list.
    pub fn share_added(&mut self, config: ShareConfig) -> usize {
        let position = self
            .shares
            .iter()
            .position(|other| other.name >= config.name)
            .unwrap_or(self.shares.len());
        if self
            .shares
            .get(position)
            .is_some_and(|s| s.name == config.name)
        {
            self.shares.remove(position);
        }
        self.shares.insert(position, config);
        self.mode = Mode::Shares(ShareList { selected: position });
        position
    }

    /// The app's answer to a failed share connection: stay on the share list
    /// (the caller logged the failure) and keep the share there for a retry.
    pub fn share_connection_failed(&mut self, share: &str) {
        let selection = self
            .shares
            .iter()
            .position(|s| s.name == share)
            .unwrap_or(0);
        self.mode = Mode::Shares(ShareList {
            selected: selection,
        });
    }

    /// A successful connection opens the picker at the share root. Its first
    /// listing is still pending: callers resolve it through
    /// [`App::pending_share_list`].
    pub fn open_share_picker(&mut self, share: &str, root: &str) {
        self.mode = Mode::Picker(Picker::at_share(share, root));
    }

    /// The listing the open share picker currently waits for.
    pub fn pending_share_list(&self) -> Option<Source> {
        match &self.mode {
            Mode::Picker(picker) => picker.reload_request(),
            _ => None,
        }
    }

    /// Apply a store listing to the open share picker.
    pub fn apply_share_listing(&mut self, source: &Source, result: Vec<ListEntry>) {
        let Mode::Picker(picker) = &mut self.mode else {
            return;
        };
        let Source::Smb(location) = source else {
            return;
        };
        picker.apply_share_listing(location, result);
    }

    /// Record a share listing failure: the picker shows the error and calms.
    pub fn share_listing_failed(&mut self, source: &Source, error: &str) {
        let Mode::Picker(picker) = &mut self.mode else {
            return;
        };
        if picker.is_busy() || picker.entries().is_empty() {
            picker.set_error(format!("cannot list {source}: {error}"));
        }
    }

    /// Log the outcome of an add, whether sync (local) or awaited (share).
    pub fn log_add_outcome(&mut self, display: &str, outcome: &AddOutcome) {
        self.push_log(format!("Added: {display}"));
        for skipped in &outcome.skipped {
            self.push_log(format!("Warning: skipped {skipped}: cannot be read"));
        }
        for replaced in &outcome.replaced {
            self.push_log(format!(
                "Warning: {replaced} replaced a previously added file"
            ));
        }
        self.clamp_selection();
    }

    /// Log a failed add.
    pub fn log_add_failure(&mut self, display: &str, error: &SourceError) {
        self.push_log(format!("Cannot add {display}: {error}"));
        self.clamp_selection();
    }
}

/// Decides which connection attempts reach the log.
///
/// The waiting state is logged once per waiting period, and a failing connection is
/// logged once while the same error persists.
#[derive(Debug, Default)]
struct ConnectLog {
    waiting_logged: bool,
    last_error: Option<String>,
}

impl ConnectLog {
    /// The message to log before a connection attempt, if the waiting state is new.
    fn waiting(&mut self) -> Option<&'static str> {
        if self.waiting_logged {
            None
        } else {
            self.waiting_logged = true;
            Some("Waiting for Switch...")
        }
    }

    /// The message to log for a failed attempt, if the error differs from the last one.
    fn failed(&mut self, error: &str) -> Option<String> {
        let message = format!("USB error: {error}");
        if self.last_error.as_deref() == Some(message.as_str()) {
            None
        } else {
            self.last_error = Some(message.clone());
            Some(message)
        }
    }

    /// Forget the waiting state and the last error once a connection is established.
    fn connected(&mut self) {
        self.waiting_logged = false;
        self.last_error = None;
    }
}

/// Spawn the background server loop. Logs are forwarded to `log_tx`, lifecycle
/// transitions and progress to `event_tx`.
pub fn spawn_server(
    files: FileList,
    store: Arc<dyn Sources>,
    log_tx: UnboundedSender<String>,
    event_tx: UnboundedSender<ServerEvent>,
) {
    tokio::spawn(async move {
        let mut connect_log = ConnectLog::default();
        loop {
            if let Some(message) = connect_log.waiting() {
                let _ = log_tx.send(message.into());
            }
            let transport = match UsbTransport::connect().await {
                Ok(t) => t,
                Err(e) => {
                    if let Some(message) = connect_log.failed(&e.to_string()) {
                        let _ = log_tx.send(message);
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            connect_log.connected();
            let _ = log_tx.send("Switch connected".into());
            let _ = event_tx.send(ServerEvent::Started);

            let mut server = Server::new(transport, files.clone(), store.clone());
            let log = log_tx.clone();
            let events = event_tx.clone();
            let mut on_update = move |update: ServerUpdate| match update {
                ServerUpdate::Log(line) => {
                    let _ = log.send(line);
                }
                ServerUpdate::Progress { name, sent, total } => {
                    let _ = events.send(ServerEvent::Progress { name, sent, total });
                }
                ServerUpdate::Completed { name, bytes } => {
                    let _ = events.send(ServerEvent::Completed { name, bytes });
                }
            };
            match server.run(&mut on_update).await {
                Ok(()) => {
                    let _ = log_tx.send("Server stopped".into());
                    let _ = event_tx.send(ServerEvent::Ended);
                    break;
                }
                Err(e) => {
                    let _ = log_tx.send(format!("Connection lost: {e}"));
                    let _ = event_tx.send(ServerEvent::Disconnected);
                    continue;
                }
            }
        }
    });
}

/// Hint line shown while the picker is open.
const PICKER_HINT: &str = "Esc cancel  Enter open  Space add  Bksp up  / filter";

/// Hint line shown while the share list is open.
const SHARES_HINT: &str = "Esc cancel  Enter connect  a add share";

/// Hint line shown while the add-share form is open.
const SHARE_FORM_HINT: &str = "Tab next  Enter save  Esc cancel";

/// The normal-view hint broken into keyed chip pairs: bracketed key text, label
/// remainder, and the key character (for locked-out detection).
const NORMAL_ACTIONS: [(&str, &str, char); 7] = [
    ("[a]", "dd", 'a'),
    ("[d]", "elete", 'd'),
    ("[c]", "lear", 'c'),
    ("[s]", "tart", 's'),
    ("[S]", "hares", 'S'),
    ("[q]", "uit", 'q'),
    ("[?]", " help", '?'),
];

/// The normal-mode hint as keyed chips. While the server task is active the
/// locked-out editing keys (`a`/`d`/`c`/`S`) are rendered dimmed.
fn normal_hint_line(server_active: bool) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    for (index, (key, label, key_char)) in NORMAL_ACTIONS.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let locked = server_active && matches!(key_char, 'a' | 'd' | 'c' | 'S');
        if locked {
            spans.push(Span::styled(key.to_string(), theme::muted_dim()));
            spans.push(Span::styled(label.to_string(), theme::muted_dim()));
        } else {
            spans.push(Span::styled(key.to_string(), theme::accent()));
            spans.push(Span::styled(label.to_string(), theme::text()));
        }
    }
    Line::from(spans)
}

pub fn ui(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let show_progress = app.status == ServerStatus::Running && app.progress.is_some();

    let mut constraints = vec![Constraint::Length(1), Constraint::Min(3)];
    if show_progress {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(3));
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_header(frame, app, chunks[0]);

    match &app.mode {
        Mode::Picker(picker) => {
            // The picker is only open while the server is idle, so no progress region exists.
            render_picker(frame, picker, chunks[1]);
            render_log(frame, app, chunks[2]);
            render_help(frame, app, chunks[3]);
        }
        Mode::Shares(list) => {
            // Same for the share list: only reachable while the server is idle.
            render_shares(frame, app, list, chunks[1]);
            render_log(frame, app, chunks[2]);
            render_help(frame, app, chunks[3]);
        }
        _ => {
            render_files(frame, app, chunks[1]);
            let mut log_index = 2;
            if show_progress {
                render_progress(frame, app, chunks[2]);
                log_index = 3;
            }
            render_log(frame, app, chunks[log_index]);
            render_help(frame, app, chunks[log_index + 1]);
        }
    }

    match &app.mode {
        Mode::Confirm(kind) => render_confirm(frame, *kind, area),
        Mode::Help => render_help_overlay(frame, area),
        Mode::Password(modal) => render_password(frame, modal, area),
        Mode::ShareForm(form) => render_share_form(frame, form, area),
        _ => {}
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let (state_text, color) = match app.status {
        ServerStatus::Idle => ("○ idle".to_string(), theme::IDLE),
        ServerStatus::Waiting => {
            let frames = theme::SPINNER_FRAMES;
            let position = (app.started_at.elapsed().as_millis() / 200) as usize;
            (
                format!("{} waiting…", frames[position % frames.len()]),
                theme::WARNING,
            )
        }
        ServerStatus::Running => ("● running".to_string(), theme::SUCCESS),
    };
    let left = Line::from(vec![
        Span::styled(
            theme::APP_NAME.to_string(),
            theme::accent().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(state_text, theme::fg(color)),
    ]);
    let count = if app.files.len() == 1 {
        "1 file".to_string()
    } else {
        format!("{} files", app.files.len())
    };
    let right = format!("{count} · total: {}", human_size(app.files.total_size()));
    frame.render_widget(Paragraph::new(left), halves[0]);
    frame.render_widget(
        Paragraph::new(right)
            .style(theme::muted())
            .alignment(Alignment::Right),
        halves[1],
    );
}

fn render_files(frame: &mut Frame, app: &App, area: Rect) {
    if app.files.is_empty() {
        render_empty_files_hint(frame, area);
        return;
    }
    let widths = [
        Constraint::Min(4),
        Constraint::Length(10),
        Constraint::Min(8),
    ];
    let rows: Vec<Row> = app
        .files
        .iter()
        .map(|(name, entry)| {
            Row::new(vec![
                Cell::from(Span::styled(name.clone(), theme::text())),
                Cell::from(
                    Line::styled(human_size(entry.size), theme::muted())
                        .alignment(Alignment::Right),
                ),
                Cell::from(Span::styled(
                    abbrev_path(
                        &entry.source.to_string(),
                        (area.width as usize).saturating_sub(20),
                    ),
                    theme::muted(),
                )),
            ])
        })
        .collect();
    let files = Table::new(rows, widths)
        .block(theme::block_title("Files"))
        .column_spacing(2)
        .row_highlight_style(theme::selection())
        .highlight_symbol("> ");
    let mut state = TableState::default();
    if !app.files.is_empty() {
        state.select(Some(app.selected.min(app.files.len() - 1)));
    }
    frame.render_stateful_widget(files, area, &mut state);
}

/// A centered onboarding hint shown when no files are offered (picker closed).
fn render_empty_files_hint(frame: &mut Frame, area: Rect) {
    let block = theme::block_title("Files");
    let inner = block.inner(area);
    let bands = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(1),
            Constraint::Percentage(55),
        ])
        .split(inner);
    let hint = Paragraph::new(Span::styled(
        "No files yet — press `a` to add files",
        theme::muted(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(block, area);
    frame.render_widget(hint, bands[1]);
}

fn render_progress(frame: &mut Frame, app: &App, area: Rect) {
    let Some(progress) = &app.progress else {
        return;
    };
    let ratio = if progress.total > 0 {
        (progress.sent as f64 / progress.total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let percent = (ratio * 100.0).round() as u64;
    let mut spans = vec![
        Span::styled("Sending ", theme::muted()),
        Span::styled(progress.name.clone(), theme::text()),
        Span::styled(format!("  {percent}%"), theme::text()),
        Span::styled(
            format!(
                "  {}/{}",
                human_size(progress.sent),
                human_size(progress.total)
            ),
            theme::muted(),
        ),
    ];
    if let Some(bytes) = progress.speed {
        spans.push(Span::styled(
            format!("  {}/s", human_size(bytes)),
            theme::muted(),
        ));
    }
    if let Some(eta) = progress.eta {
        spans.push(Span::styled(
            format!("  eta {}", format_elapsed(Duration::from_secs(eta))),
            theme::muted(),
        ));
    }
    let line = Line::from(spans);

    // One stable row: the statistics text leads, the bar fills the remainder of
    // that row (and disappears entirely when the text already spans the width).
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(line.width() as u16), Constraint::Min(0)])
        .split(area);
    frame.render_widget(Paragraph::new(line), halves[0]);

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(theme::SUCCESS))
        .ratio(ratio)
        .label("");
    frame.render_widget(gauge, halves[1]);
}

fn render_log(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let start = app.log.len().saturating_sub(inner_height);
    let log_lines: Vec<Line> = app
        .log
        .iter()
        .skip(start)
        .map(|line| {
            Line::styled(
                format!("{} {}", format_elapsed(line.at), line.message),
                Style::default().fg(log_color(&line.message)),
            )
        })
        .collect();
    let log = Paragraph::new(log_lines)
        .block(theme::block_title("Log"))
        .wrap(Wrap { trim: true });
    frame.render_widget(log, area);
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.mode {
        Mode::Picker(picker) => match picker.error() {
            Some(error) => Line::from(Span::styled(
                format!("Error: {error}"),
                theme::fg(theme::DANGER),
            )),
            None => Line::from(Span::styled(PICKER_HINT, theme::fg(theme::WARNING))),
        },
        Mode::Confirm(_) => Line::from(Span::styled(
            "Press y to confirm, any other key to cancel",
            theme::muted(),
        )),
        Mode::Password(_) => Line::from(Span::styled("Enter connect · Esc cancel", theme::muted())),
        Mode::Shares(_) => Line::from(Span::styled(SHARES_HINT, theme::fg(theme::WARNING))),
        Mode::ShareForm(_) => Line::from(Span::styled(SHARE_FORM_HINT, theme::fg(theme::WARNING))),
        _ => normal_hint_line(app.server_started),
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_picker(frame: &mut Frame, picker: &Picker, area: Rect) {
    let rows: Vec<Row> = picker
        .entries()
        .iter()
        .map(|entry| {
            let (name, name_style, size) = match entry.kind {
                Kind::Dir => (format!("{}/", entry.name), theme::accent(), String::new()),
                Kind::File => (
                    entry.name.clone(),
                    theme::text(),
                    entry.size.map(human_size).unwrap_or_default(),
                ),
            };
            Row::new(vec![
                Cell::from(Span::styled(name, name_style)),
                Cell::from(Line::styled(size, theme::muted()).alignment(Alignment::Right)),
            ])
        })
        .collect();
    let count = if picker.filter().is_empty() {
        if picker.entries().len() == 1 {
            "1 entry".to_string()
        } else {
            format!("{} entries", picker.entries().len())
        }
    } else if picker.entries().len() == 1 {
        "1 match".to_string()
    } else {
        format!("{} matches", picker.entries().len())
    };
    let title = if picker.is_busy() {
        format!("Pick a path: {} · listing…", picker.origin_display())
    } else if picker.error().is_some() {
        format!(
            "Pick a path: {} · {}",
            picker.origin_display(),
            picker.error().unwrap()
        )
    } else if picker.filter().is_empty() {
        format!("Pick a path: {} · {count}", picker.origin_display())
    } else {
        format!(
            "Pick a path: {}  [filter: {}] · {count}",
            picker.origin_display(),
            picker.filter()
        )
    };
    let widths = [Constraint::Min(8), Constraint::Length(10)];
    let list = Table::new(rows, widths)
        .block(theme::block_title(title))
        .column_spacing(2)
        .row_highlight_style(theme::selection())
        .highlight_symbol("> ");
    let mut state = TableState::default();
    if !picker.entries().is_empty() {
        state.select(Some(picker.cursor()));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

/// The share list: one row per configured share with its details, or a hint
/// naming the config file when nothing is configured. Renders in place of the
/// Files table (the server is idle while it is open).
fn render_shares(frame: &mut Frame, app: &App, list: &ShareList, area: Rect) {
    if app.shares.is_empty() {
        let block = theme::block_title("Shares");
        let inner = block.inner(area);
        let bands = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(45),
                Constraint::Length(1),
                Constraint::Percentage(55),
            ])
            .split(inner);
        let hint = match &app.config_hint {
            Some(path) => Paragraph::new(Span::styled(
                format!(
                    "No shares configured — press `a` to add one (saved to {})",
                    path.display()
                ),
                theme::muted(),
            ))
            .alignment(Alignment::Center),
            None => Paragraph::new(Span::styled(
                "No shares configured — press `a` to add one (no config path known)",
                theme::muted(),
            ))
            .alignment(Alignment::Center),
        };
        frame.render_widget(block, area);
        frame.render_widget(hint, bands[1]);
        return;
    }

    let rows: Vec<Row> = app
        .shares
        .iter()
        .map(|share| {
            Row::new(vec![
                Cell::from(Span::styled(share.name.clone(), theme::text())),
                Cell::from(Span::styled(
                    format!("{} · {}", share.host, share.share),
                    theme::muted(),
                )),
                Cell::from(Span::styled(share.username.clone(), theme::muted())),
            ])
        })
        .collect();
    let count = if app.shares.len() == 1 {
        "1 share".to_string()
    } else {
        format!("{} shares", app.shares.len())
    };
    let widths = [
        Constraint::Min(8),
        Constraint::Min(12),
        Constraint::Length(12),
    ];
    let table = Table::new(rows, widths)
        .block(theme::block_title(format!("Shares · {count}")))
        .column_spacing(2)
        .row_highlight_style(theme::selection())
        .highlight_symbol("> ");
    let mut state = TableState::default();
    state.select(Some(list.selected.min(app.shares.len() - 1)));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_confirm(frame: &mut Frame, kind: ConfirmKind, area: Rect) {
    let prompt = match kind {
        ConfirmKind::Quit => "Quit dbi-backend? [y/N]",
        ConfirmKind::Clear => "Clear the file list? [y/N]",
    };
    let popup = centered_fixed(prompt.len() as u16 + 2, 3, area);
    frame.render_widget(Clear, popup);
    let widget = Paragraph::new(prompt)
        .alignment(Alignment::Center)
        .block(theme::block_title(Span::styled(
            "Confirm",
            theme::fg(theme::WARNING),
        )))
        .style(theme::fg(theme::WARNING));
    frame.render_widget(widget, popup);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("a   open the path picker"),
        Line::from("d   remove the highlighted file"),
        Line::from("c   clear the file list (confirm)"),
        Line::from("s   start the server"),
        Line::from("S   list configured shares"),
        Line::from("Share list: Enter connect, a add share, Esc cancel"),
        Line::from("j/k or arrows   move the highlight"),
        Line::from("q   quit (confirm)"),
        Line::from("?   toggle this help"),
        Line::from(""),
        Line::from("Picker: / filter, Enter open, Space add, Bksp up, Esc cancel"),
    ];
    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);
    let widget = Paragraph::new(text)
        .block(theme::block_title("Help"))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, popup);
}

/// The password modal: a centered masked input for one share. Typed
/// characters render solely as `*`; the length is the only visible signal.
fn render_password(frame: &mut Frame, modal: &PasswordModal, area: Rect) {
    let masked = "*".repeat(modal.typed_len());
    let width = (modal.share.len() + 24 + masked.len().max(3)) as u16;
    let popup = centered_fixed(width, 4, area);
    frame.render_widget(Clear, popup);

    let widget = Paragraph::new(Line::from(vec![
        Span::styled(masked.clone(), theme::text()),
        Span::styled("▏", theme::fg(theme::ACCENT)),
    ]))
    .block(theme::block_title(format!("Password for {}", modal.share)));
    frame.render_widget(widget, popup);

    let hint = Paragraph::new(Span::styled("Enter connect · Esc cancel", theme::muted()))
        .alignment(Alignment::Center);
    let hint_area = Rect {
        x: popup.x,
        y: popup.y + popup.height.saturating_sub(1),
        width: popup.width,
        height: 1,
    };
    frame.render_widget(hint, hint_area);
}

/// The add-share dialog: every field rendered labeled in an editable row with
/// the focused one marked, submit errors shown inline under the fields, and no
/// password anywhere — nothing is masked because nothing is secret.
fn render_share_form(frame: &mut Frame, form: &ShareFormState, area: Rect) {
    let labels = ["name", "server", "share", "username", "domain", "root"];
    let mut rows: Vec<Line> = (0..SHARE_FORM_FIELDS)
        .map(|field| {
            let mut spans = vec![
                Span::styled(
                    format!("{:<9}", format!("{}:", labels[field])),
                    theme::muted(),
                ),
                Span::styled(form.field_value(field).to_string(), theme::text()),
            ];
            if field == form.focus() {
                spans.push(Span::styled("▏", theme::fg(theme::ACCENT)));
            }
            Line::from(spans)
        })
        .collect();

    let errors = form.errors();
    if !errors.is_empty() {
        rows.push(Line::from(""));
        rows.extend(
            errors
                .iter()
                .map(|error| Line::from(Span::styled(error.clone(), theme::fg(theme::DANGER)))),
        );
    }

    let width = rows
        .iter()
        .map(|row| row.width() as u16)
        .max()
        .unwrap_or(16)
        + 2 // the dialog border
        + 2;
    let popup = centered_fixed(width.min(area.width), rows.len() as u16 + 2, area);
    frame.render_widget(Clear, popup);

    let widget = Paragraph::new(rows).block(theme::block_title("Add share"));
    frame.render_widget(widget, popup);
}

fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn log_color(message: &str) -> Color {
    if message.starts_with("Warning:") {
        theme::WARNING
    } else if message.starts_with("Failed")
        || message.starts_with("USB error")
        || message.starts_with("Connection lost")
    {
        theme::DANGER
    } else if message.starts_with("Sent:") {
        theme::SUCCESS
    } else {
        theme::SUBTLE
    }
}

fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Estimated seconds remaining for a transfer at the given speed.
fn remaining_seconds(sent: u64, total: u64, speed: u64) -> u64 {
    if sent >= total || speed == 0 {
        0
    } else {
        (total - sent).div_ceil(speed)
    }
}

/// Truncate a path's display text from the middle, keeping both ends so the file
/// name tail stays visible. The ellipsis counts as one character; the result is
/// never wider than `max_width` characters.
fn abbrev_path(path: &str, max_width: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max_width {
        return path.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let keep = max_width - 1;
    if keep == 0 {
        return "…".to_string();
    }
    let head = keep - keep / 2;
    let tail = keep / 2;
    let head_str: String = chars[..head].iter().collect();
    let tail_str: String = chars[chars.len() - tail..].iter().collect();
    format!("{head_str}…{tail_str}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use dbi_backend_core::source::local::LocalStore;
    use dbi_backend_core::source::{EntryKind, SmbLocation};
    use dbi_backend_core::transport::{Transport, TransportError};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::fs;

    fn press(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app_with_picker(dir: &Path) -> App {
        let mut app = App::new(FileList::new());
        app.mode = Mode::Picker(Picker::at(dir));
        app
    }

    fn buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| ui(frame, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn render(app: &App, width: u16, height: u16) -> String {
        let buffer = buffer(app, width, height);
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
            out.push('\n');
        }
        out
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
            .collect()
    }

    #[test]
    fn push_log_caps_at_max_lines() {
        let mut app = App::new(FileList::new());
        for i in 0..(MAX_LOG_LINES + 50) {
            app.push_log(i.to_string());
        }
        assert_eq!(app.log.len(), MAX_LOG_LINES);
    }

    #[test]
    fn q_opens_confirmation_and_y_quits() {
        let mut app = App::new(FileList::new());
        assert_eq!(app.handle_key(press('q')), Action::None);
        assert!(matches!(app.mode, Mode::Confirm(ConfirmKind::Quit)));
        assert!(!app.should_quit);
        assert_eq!(app.handle_key(press('y')), Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn q_confirmation_can_be_cancelled() {
        let mut app = App::new(FileList::new());
        app.handle_key(press('q'));
        assert_eq!(app.handle_key(press('n')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
        assert!(!app.should_quit);
    }

    #[test]
    fn c_requires_confirmation() {
        let mut app = App::new(FileList::new());
        app.files.insert("a.nsp", "/tmp/a.nsp");
        assert_eq!(app.handle_key(press('c')), Action::None);
        assert!(matches!(app.mode, Mode::Confirm(ConfirmKind::Clear)));
        assert_eq!(app.files.len(), 1);
        app.handle_key(press('y'));
        assert!(app.files.is_empty());
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn c_confirmation_can_be_cancelled() {
        let mut app = App::new(FileList::new());
        app.files.insert("a.nsp", "/tmp/a.nsp");
        app.handle_key(press('c'));
        app.handle_key(press('x'));
        assert_eq!(app.files.len(), 1);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn help_overlay_toggles_with_question_mark() {
        let mut app = App::new(FileList::new());
        assert_eq!(app.handle_key(press('?')), Action::None);
        assert!(matches!(app.mode, Mode::Help));
        app.handle_key(press('?'));
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn s_starts_only_when_files_present_and_only_once() {
        let mut app = App::new(FileList::new());
        assert_eq!(app.handle_key(press('s')), Action::None);

        app.files.insert("a", "/tmp/a");
        assert_eq!(app.handle_key(press('s')), Action::Start);
        assert_eq!(app.handle_key(press('s')), Action::None);
    }

    #[test]
    fn s_sets_waiting_until_the_device_connects() {
        let mut app = App::new(FileList::new());
        app.files.insert("a", "/tmp/a");

        assert_eq!(app.handle_key(press('s')), Action::Start);
        assert_eq!(app.status, ServerStatus::Waiting);
    }

    #[test]
    fn started_makes_running_and_disconnected_returns_to_waiting() {
        let mut app = App::new(FileList::new());
        app.files.insert("a", "/tmp/a");
        app.handle_key(press('s'));

        app.handle_server_event(ServerEvent::Started);
        assert_eq!(app.status, ServerStatus::Running);

        app.handle_server_event(ServerEvent::Disconnected);
        assert_eq!(app.status, ServerStatus::Waiting);
    }

    #[test]
    fn progress_does_not_survive_disconnect_and_reconnect() {
        let mut app = App::new(FileList::new());
        app.handle_server_event(ServerEvent::Started);
        app.handle_server_event(ServerEvent::Progress {
            name: "game.nsp".into(),
            sent: 50,
            total: 100,
        });
        assert!(app.progress.is_some());

        app.handle_server_event(ServerEvent::Disconnected);
        assert!(app.progress.is_none());

        app.handle_server_event(ServerEvent::Started);
        assert!(app.progress.is_none());
        assert!(!render(&app, 120, 12).contains("Sending game.nsp"));
    }

    #[test]
    fn edits_stay_inert_while_waiting() {
        let mut app = App::new(FileList::new());
        app.files.insert("a", "/tmp/a");
        app.handle_key(press('s'));
        assert_eq!(app.status, ServerStatus::Waiting);

        assert_eq!(app.handle_key(press('a')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.handle_key(press('d')), Action::None);
        assert_eq!(app.files.len(), 1);
        assert_eq!(app.handle_key(press('c')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn ended_event_returns_to_idle_and_allows_restart() {
        let mut app = App::new(FileList::new());
        app.files.insert("a", "/tmp/a");

        assert_eq!(app.handle_key(press('s')), Action::Start);
        assert_eq!(app.status, ServerStatus::Waiting);

        app.handle_server_event(ServerEvent::Started);
        assert_eq!(app.status, ServerStatus::Running);

        app.handle_server_event(ServerEvent::Ended);
        assert_eq!(app.status, ServerStatus::Idle);

        assert_eq!(app.handle_key(press('s')), Action::Start);
    }

    #[test]
    fn edits_are_inert_while_the_server_runs() {
        let mut app = App::new(FileList::new());
        app.files.insert("keep", "/tmp/keep");
        assert_eq!(app.handle_key(press('s')), Action::Start);
        assert!(matches!(app.status, ServerStatus::Waiting));

        assert_eq!(app.handle_key(press('a')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.handle_key(press('c')), Action::None);
        assert_eq!(app.files.len(), 1);

        app.handle_server_event(ServerEvent::Started);
        assert!(matches!(app.status, ServerStatus::Running));
        assert_eq!(app.handle_key(press('d')), Action::None);
        assert_eq!(app.files.len(), 1);

        app.handle_server_event(ServerEvent::Ended);
        assert_eq!(app.handle_key(press('a')), Action::None);
        assert!(matches!(app.mode, Mode::Picker(_)));
    }

    #[test]
    fn log_shows_the_newest_lines() {
        let mut app = App::new(FileList::new());
        for i in 0..10 {
            app.push_log(format!("line-{i}"));
        }

        let view = render(&app, 40, 12);
        assert!(view.contains("line-9"));
        assert!(!view.contains("line-0"));
    }

    #[test]
    fn a_opens_the_picker() {
        let mut app = App::new(FileList::new());
        assert_eq!(app.handle_key(press('a')), Action::None);
        assert!(matches!(app.mode, Mode::Picker(_)));
    }

    #[test]
    fn global_keys_do_not_fire_while_picker_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_picker(dir.path());
        app.files.insert("keep", "/tmp/keep");

        assert_eq!(app.handle_key(press('c')), Action::None);
        assert_eq!(app.files.len(), 1);
        assert_eq!(app.handle_key(press('s')), Action::None);
        assert!(matches!(app.mode, Mode::Picker(_)));
    }

    #[test]
    fn enter_descends_into_a_highlighted_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        let mut app = app_with_picker(dir.path());
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Action::None);
        match &app.mode {
            Mode::Picker(picker) => assert_eq!(picker.dir(), sub.as_path()),
            _ => panic!("expected picker to stay open"),
        }
    }

    #[test]
    fn backspace_goes_up_in_the_picker() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        let mut app = app_with_picker(&sub);
        app.handle_key(key(KeyCode::Backspace));
        match &app.mode {
            Mode::Picker(picker) => assert_eq!(picker.dir(), dir.path()),
            _ => panic!("expected picker to stay open"),
        }
    }

    #[test]
    fn esc_cancels_the_picker() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_picker(dir.path());
        app.handle_key(key(KeyCode::Esc));
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn q_inside_picker_cancels_without_quitting() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_picker(dir.path());
        assert_eq!(app.handle_key(press('q')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
        assert!(!app.should_quit);
    }

    #[test]
    fn selecting_a_file_adds_it_directly() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.nsp"), b"g").unwrap();

        let mut app = app_with_picker(dir.path());
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.files.get("game.nsp").is_some());
    }

    #[test]
    fn selecting_an_existing_name_logs_a_replacement_warning() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.nsp"), b"g").unwrap();

        let mut app = app_with_picker(dir.path());
        app.files.insert("game.nsp", "/elsewhere/game.nsp");
        app.handle_key(key(KeyCode::Char(' ')));

        assert!(app.log.iter().any(|l| l.message.contains("replaced")));
    }

    #[test]
    fn enter_on_a_file_adds_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.nsp"), b"g").unwrap();

        let mut app = app_with_picker(dir.path());
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.files.get("game.nsp").is_some());
    }

    #[test]
    fn selecting_a_directory_adds_its_contents_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("titles");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("a.nsp"), b"a").unwrap();

        let mut app = app_with_picker(dir.path());
        app.handle_key(key(KeyCode::Char(' ')));
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.files.get("a.nsp").is_some());
    }

    #[test]
    fn picker_highlight_follows_the_cursor() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsp"), b"b").unwrap();

        let mut app = app_with_picker(dir.path());
        let first = render(&app, 40, 12);
        assert!(first.contains("> a.nsp"));
        assert!(!first.contains("> b.nsp"));

        app.handle_key(key(KeyCode::Down));
        let second = render(&app, 40, 12);
        assert!(second.contains("> b.nsp"));
        assert_ne!(first, second);
    }

    #[test]
    fn cancel_restores_the_normal_help_line() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_picker(dir.path());

        let picker_view = render(&app, 60, 12);
        assert!(picker_view.contains("Esc cancel"));

        app.handle_key(key(KeyCode::Esc));
        let normal_view = render(&app, 60, 12);
        assert!(normal_view.contains("[a]dd"));
        assert!(!normal_view.contains("Esc cancel"));
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = App::new(FileList::new());
        app.files.insert("a.nsp", "/tmp/a.nsp");
        app.files.insert("b.nsp", "/tmp/b.nsp");

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn file_list_scrolls_to_the_highlight() {
        let mut app = App::new(FileList::new());
        for i in 0..40 {
            app.files
                .insert(format!("f{i:02}.nsp"), format!("/tmp/f{i:02}.nsp"));
        }
        for _ in 0..39 {
            app.handle_key(key(KeyCode::Down));
        }
        assert_eq!(app.selected, 39);

        let view = render(&app, 60, 12);
        assert!(view.contains("> f39.nsp"));
        assert!(!view.contains("f00.nsp"));
    }

    #[test]
    fn d_removes_the_highlighted_file() {
        let mut app = App::new(FileList::new());
        app.files.insert("a.nsp", "/tmp/a.nsp");
        app.files.insert("b.nsp", "/tmp/b.nsp");

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
        assert_eq!(app.handle_key(press('d')), Action::None);
        assert!(app.files.get("b.nsp").is_none());
        assert!(app.files.get("a.nsp").is_some());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn removal_is_inert_while_the_server_runs() {
        let mut app = App::new(FileList::new());
        app.files.insert("a.nsp", "/tmp/a.nsp");
        assert_eq!(app.handle_key(press('s')), Action::Start);
        app.handle_key(press('d'));
        assert_eq!(app.files.len(), 1);
    }

    #[test]
    fn files_list_shows_sizes_and_total() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), vec![0u8; 2048]).unwrap();
        let mut app = App::new(FileList::new());
        app.files.add_path(dir.path()).unwrap();

        let view = render(&app, 80, 12);
        assert!(view.contains("2.0 KiB"));
        assert!(view.contains("total: 2.0 KiB"));
    }

    #[test]
    fn empty_file_list_shows_an_onboarding_hint() {
        let empty = App::new(FileList::new());
        let view = render(&empty, 60, 12);
        assert!(view.contains("No files yet"));
        assert!(view.contains("press `a` to add"));
        assert!(view.contains("No files yet — press `a` to add"));

        let mut populated = App::new(FileList::new());
        populated.files.insert("a.nsp", "/tmp/a.nsp");
        let view = render(&populated, 60, 12);
        assert!(!view.contains("No files yet"));
    }

    #[test]
    fn size_column_is_right_aligned_in_the_files_table() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), vec![0u8; 1024]).unwrap();
        fs::write(dir.path().join("verylonggamename.nsp"), vec![0u8; 2048]).unwrap();
        let mut app = App::new(FileList::new());
        app.files.add_path(dir.path()).unwrap();

        let buf = buffer(&app, 80, 12);
        // Files rows start below the header (y=0) and the top border (y=1); the last
        // character of a right-aligned size cell is the same 'B' x-position in both rows.
        let size_x = |y: u16| {
            (0..buf.area.width)
                .find(|x| buf.cell((*x, y)).unwrap().symbol() == "B")
                .expect("size cell 'B' in files row")
        };
        assert_eq!(size_x(2), size_x(3));
    }

    #[test]
    fn header_identifies_the_app_and_shows_counts() {
        let app = App::new(FileList::new());
        let view = render(&app, 80, 12);
        assert!(view.contains(theme::APP_NAME));
        assert!(view.contains("idle"));
        assert!(view.contains("0 files · total: 0 B"));

        let mut app = App::new(FileList::new());
        app.files.insert("a.nsp", "/tmp/a.nsp");
        app.files.insert("b.nsp", "/tmp/b.nsp");
        let view = render(&app, 80, 12);
        assert!(view.contains("2 files"));
    }

    #[test]
    fn hint_chips_dim_locked_keys_while_the_server_is_active() {
        let idle = App::new(FileList::new());
        let bottom = |app: &App| {
            let buf = buffer(app, 60, 12);
            let y = buf.area.height - 1;
            (0..buf.area.width)
                .map(|x| buf.cell((x, y)).unwrap().fg)
                .collect::<Vec<_>>()
        };

        let idle_fgs = bottom(&idle);
        assert!(
            idle_fgs.contains(&theme::ACCENT),
            "keys are accented when idle"
        );
        assert!(
            !idle_fgs.contains(&theme::MUTED),
            "no muted cells when idle"
        );

        let mut active = App::new(FileList::new());
        active.files.insert("a", "/tmp/a");
        active.handle_key(press('s'));
        assert!(active.server_started);
        let active_fgs = bottom(&active);
        assert!(
            active_fgs.iter().filter(|c| **c == theme::MUTED).count() >= 12,
            "locked a/d/c chips render dimmed"
        );
        assert!(
            active_fgs.contains(&theme::ACCENT),
            "unlocked keys stay accented"
        );
    }

    #[test]
    fn status_header_is_colored_by_state() {
        let app = App::new(FileList::new());
        // x=13 is the status capsule: "dbi-backend" (11 chars) + two spaces.
        assert_eq!(buffer(&app, 80, 12).cell((13, 0)).unwrap().fg, Color::Gray);
        let view = render(&app, 80, 12);
        assert!(view.contains("idle"));

        let mut app = App::new(FileList::new());
        app.files.insert("a", "/tmp/a");
        app.handle_key(press('s'));
        assert_eq!(app.status, ServerStatus::Waiting);
        let view = render(&app, 80, 12);
        assert!(view.contains("waiting"));
        // The spinner glyph varies with wall-clock time, so assert color only.
        assert_eq!(
            buffer(&app, 80, 12).cell((13, 0)).unwrap().fg,
            Color::Yellow
        );

        app.handle_server_event(ServerEvent::Started);
        assert_eq!(app.status, ServerStatus::Running);
        let view = render(&app, 80, 12);
        assert!(view.contains("running"));
        assert_eq!(buffer(&app, 80, 12).cell((13, 0)).unwrap().fg, Color::Green);
    }

    #[test]
    fn log_lines_are_timestamped_and_warnings_are_colored() {
        let mut app = App::new(FileList::new());
        app.push_log("Warning: bad thing".into());

        let view = render(&app, 60, 12);
        assert!(view.contains("00:00 Warning: bad thing"));
        assert_eq!(log_color("Warning: bad thing"), Color::Yellow);
        assert_eq!(log_color("Connection lost: x"), Color::Red);
    }

    #[test]
    fn remaining_seconds_divides_remainder_by_speed() {
        assert_eq!(remaining_seconds(75, 100, 25), 1);
        assert_eq!(remaining_seconds(0, 300, 100), 3);
        assert_eq!(remaining_seconds(100, 100, 25), 0);
        assert_eq!(remaining_seconds(50, 100, 0), 0);
    }

    #[test]
    fn progress_eta_comes_from_the_second_sample() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("a.nsp".into(), 50, 100, start);
        let first = app.progress.as_ref().unwrap();
        assert!(first.speed.is_none());
        assert_eq!(first.eta, None);

        app.update_progress_at("a.nsp".into(), 75, 100, start + Duration::from_secs(1));
        let second = app.progress.as_ref().unwrap();
        assert_eq!(second.speed, Some(25));
        assert_eq!(second.eta, Some(1));
    }

    #[test]
    fn progress_speed_is_smoothed_across_uneven_samples() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);

        app.update_progress_at("game.nsp".into(), 0, 10_000, start);
        app.update_progress_at(
            "game.nsp".into(),
            1_000,
            10_000,
            start + Duration::from_secs(1),
        );
        app.update_progress_at(
            "game.nsp".into(),
            1_200,
            10_000,
            start + Duration::from_millis(1_200),
        );
        assert_eq!(app.progress.as_ref().unwrap().speed, Some(1_000));

        // The final interval is much slower than the earlier ones. A single-interval
        // estimate would collapse to 100 B/s; the smoothed rate stays between the old
        // and new instantaneous values.
        app.update_progress_at(
            "game.nsp".into(),
            1_300,
            10_000,
            start + Duration::from_millis(2_200),
        );
        let state = app.progress.as_ref().unwrap();
        let speed = state.speed.unwrap();
        assert!(
            (101..1_000).contains(&speed),
            "smoothed speed {speed} should be between 100 and 1000"
        );
        assert_eq!(state.eta, Some(remaining_seconds(1_300, 10_000, speed)));
    }

    #[test]
    fn sub_100ms_samples_do_not_recompute_the_speed() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);

        app.update_progress_at("game.nsp".into(), 0, 10_000, start);
        app.update_progress_at(
            "game.nsp".into(),
            1_000,
            10_000,
            start + Duration::from_secs(1),
        );
        assert_eq!(app.progress.as_ref().unwrap().speed, Some(1_000));

        app.update_progress_at(
            "game.nsp".into(),
            1_500,
            10_000,
            start + Duration::from_millis(1_050),
        );
        let state = app.progress.as_ref().unwrap();
        assert_eq!(state.sent, 1_500);
        assert_eq!(state.speed, Some(1_000));
    }

    #[test]
    fn progress_eta_is_carried_when_no_bytes_advance() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);

        app.update_progress_at("game.nsp".into(), 500, 2_000, start);
        app.update_progress_at(
            "game.nsp".into(),
            1_000,
            2_000,
            start + Duration::from_secs(1),
        );
        let before = app.progress.as_ref().unwrap().clone();
        assert_eq!(before.speed, Some(500));
        assert_eq!(before.eta, Some(2));

        app.update_progress_at(
            "game.nsp".into(),
            1_000,
            2_000,
            start + Duration::from_secs(2),
        );
        let after = app.progress.as_ref().unwrap();
        assert_eq!(after.speed, before.speed);
        assert_eq!(after.eta, before.eta);
    }

    #[test]
    fn progress_eta_is_zero_when_complete() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);

        app.update_progress_at("game.nsp".into(), 900, 1_000, start);
        app.update_progress_at(
            "game.nsp".into(),
            1_000,
            1_000,
            start + Duration::from_secs(1),
        );
        let state = app.progress.as_ref().unwrap();
        assert_eq!(state.speed, Some(100));
        assert_eq!(state.eta, Some(0));

        // A streamed phase restarting at the file's beginning replaces the
        // finished fill in the region: the bar shows the new position and the
        // previous 100% fill is gone.
        app.update_progress_at("game.nsp".into(), 10, 1_000, start + Duration::from_secs(2));
        let view = render(&app, 120, 12);
        assert!(view.contains("Sending game.nsp"));
        assert!(
            view.contains("1%"),
            "the region follows the restart: {view}"
        );
        assert!(
            !view.contains("100%"),
            "the old fill must not linger: {view}"
        );

        // The restart logs nothing: only an explicit completed event produces
        // the file's success summary.
        assert!(app.log.is_empty());
        app.handle_server_event(ServerEvent::Completed {
            name: "game.nsp".into(),
            bytes: 1_000,
        });
        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].message.contains("Sent: game.nsp"));
    }

    #[test]
    fn progress_backward_sample_restarts_the_phase() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);

        // Build a measured speed in the first streamed phase.
        app.update_progress_at("game.nsp".into(), 500, 1_000, start);
        app.update_progress_at(
            "game.nsp".into(),
            1_000,
            1_000,
            start + Duration::from_secs(1),
        );
        assert!(app.progress.as_ref().unwrap().speed.is_some());

        // A backward sample streams again from the file's start: the bar follows
        // it and no estimate from the finished phase is carried over.
        app.update_progress_at("game.nsp".into(), 10, 1_000, start + Duration::from_secs(2));
        let state = app.progress.as_ref().unwrap();
        assert_eq!(state.name, "game.nsp");
        assert_eq!(state.sent, 10);
        assert_eq!(state.speed, None, "speed is discarded on a backward jump");
        assert_eq!(state.eta, None, "eta is discarded on a backward jump");

        // The next forward sample re-measures speed from the new baseline.
        app.update_progress_at(
            "game.nsp".into(),
            110,
            1_000,
            start + Duration::from_secs(3),
        );
        let state = app.progress.as_ref().unwrap();
        assert_eq!(state.speed, Some(100));
        assert_eq!(state.eta, Some(9));
    }

    /// A transport preset with the reads of a scripted DBI exchange; writes go
    /// nowhere because the assertions below look at the app state, not the wire.
    struct MockTransport {
        reads: VecDeque<Vec<u8>>,
    }

    #[async_trait::async_trait]
    impl Transport for MockTransport {
        async fn read(&mut self, _len: usize) -> Result<Vec<u8>, TransportError> {
            self.reads
                .pop_front()
                .ok_or_else(|| TransportError::Usb("no more reads".into()))
        }

        async fn write(&mut self, _data: &[u8]) -> Result<(), TransportError> {
            Ok(())
        }
    }

    fn range_req_header(name: &str, range_size: u32, offset: u64) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend_from_slice(&range_size.to_le_bytes());
        header.extend_from_slice(&offset.to_le_bytes());
        header.extend_from_slice(&(name.len() as u32).to_le_bytes());
        header.extend_from_slice(name.as_bytes());
        header
    }

    fn range_request(name: &str, range_size: u32, offset: u64) -> Vec<Vec<u8>> {
        let header = range_req_header(name, range_size, offset);
        let data_size = header.len() as u32;
        vec![
            dbi_backend_core::protocol::Header::new(
                dbi_backend_core::protocol::CMD_TYPE_REQUEST,
                dbi_backend_core::protocol::CMD_ID_FILE_RANGE,
                data_size,
            )
            .to_bytes()
            .to_vec(),
            header,
            vec![0u8; 16], // the payload acknowledgment (contents unrecognized)
        ]
    }

    // The end-to-end defect flow: the DBI preparation reads (start at offset
    // zero, a continuation, the silent trailing read ending at the file's last
    // byte) followed by the content stream restarting at zero and running to
    // the end. Drives real `Server` updates into an `App` the way spawn_server
    // does.
    #[tokio::test]
    async fn preparation_reads_and_restart_never_flash_a_full_bar_before_install() {
        let total = 100usize;
        let content: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game.nsp");
        fs::write(&path, &content).unwrap();

        let name = "game.nsp";
        let mut files = FileList::new();
        files.insert(name, &path);
        let store = Arc::new(LocalStore::new()) as Arc<dyn Sources>;

        let reads = [
            range_request(name, 10, 0),  // streamed start
            range_request(name, 10, 10), // streamed continuation
            range_request(name, 2, 98),  // trailing auxiliary read at the file's end
            range_request(name, 10, 0),  // restart: the install streams from zero
            range_request(name, 90, 10), // streamed continuation to the file's end
            vec![dbi_backend_core::protocol::Header::new(
                dbi_backend_core::protocol::CMD_TYPE_REQUEST,
                dbi_backend_core::protocol::CMD_ID_EXIT,
                0,
            )
            .to_bytes()
            .to_vec()],
        ]
        .concat();

        let mut app = App::new(FileList::new());
        app.handle_server_event(ServerEvent::Started);

        let mut install_phase = false;
        let mut sending_lines = 0usize;

        Server::new(MockTransport { reads: reads.into() }, files, store.clone())
            .run(|update| match update {
                ServerUpdate::Log(line) => {
                    if line.starts_with("Sending: ") {
                        sending_lines += 1;
                        if sending_lines == 2 {
                            // The second announcement opens the restarted, real
                            // install streaming.
                            install_phase = true;
                        }
                    }
                    app.push_log(line);
                }
                ServerUpdate::Progress { name, sent, total } => {
                    if !install_phase {
                        assert!(
                            sent < total,
                            "the region must not receive a full fill before the install streams: {sent}/{total}"
                        );
                    }
                    app.handle_server_event(ServerEvent::Progress { name, sent, total });
                }
                ServerUpdate::Completed { name, bytes } => {
                    app.handle_server_event(ServerEvent::Completed { name, bytes });
                }
            })
            .await
            .unwrap();

        assert_eq!(sending_lines, 2, "one announcement per streamed sequence");
        let sent_lines = app
            .log
            .iter()
            .filter(|line| line.message.starts_with("Sent: "))
            .count();
        assert_eq!(sent_lines, 1, "exactly one completion summary");
        assert!(
            app.progress.is_none(),
            "the region ends with the completion"
        );
    }

    #[test]
    fn progress_eta_resets_for_a_new_file() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("a.nsp".into(), 90, 100, start);
        app.update_progress_at("a.nsp".into(), 100, 100, start + Duration::from_secs(1));
        assert!(app.progress.as_ref().unwrap().speed.is_some());

        app.update_progress_at("b.nsp".into(), 10, 100, start + Duration::from_secs(2));
        let state = app.progress.as_ref().unwrap();
        assert_eq!(state.name, "b.nsp");
        assert_eq!(state.speed, None);
        assert_eq!(state.eta, None);
    }

    #[test]
    fn progress_event_updates_and_clears() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("game.nsp".into(), 50, 100, start);

        let view = render(&app, 120, 12);
        assert!(view.contains("game.nsp"));
        assert!(view.contains("50%"));
        // Byte counts are human-readable; no eta before a second sample.
        assert!(view.contains("50 B/100 B"));
        assert!(!view.contains("eta"));

        // A second sample on the same file measures a speed and derives an eta.
        app.update_progress_at("game.nsp".into(), 75, 100, start + Duration::from_secs(1));
        let view = render(&app, 120, 12);
        assert!(view.contains("75 B/100 B"));
        assert!(view.contains("eta"));

        app.handle_server_event(ServerEvent::Ended);
        let view = render(&app, 120, 12);
        assert!(!view.contains("Sending game.nsp"));
    }

    #[test]
    fn progress_renders_the_name_and_the_bar_on_one_row() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("game.nsp".into(), 100, 100, start);

        let buf = buffer(&app, 80, 12);
        let name_y = (0..buf.area.height)
            .find(|y| row_text(&buf, *y).contains("Sending game.nsp"))
            .expect("progress text row");

        assert!(
            row_text(&buf, name_y).contains('█'),
            "the bar fills the remainder of the same row"
        );

        let name_x = row_text(&buf, name_y).find("game.nsp").unwrap() as u16;
        let name_cell = buf.cell((name_x, name_y)).unwrap();
        assert_ne!(
            name_cell.bg,
            theme::SUCCESS,
            "the file name is not drawn on the success fill"
        );
    }

    #[test]
    fn progress_keeps_the_file_name_at_the_start_of_a_narrow_row() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("game.nsp".into(), 50, 100, start);

        let view = render(&app, 30, 10);
        let line = view
            .lines()
            .find(|line| line.contains("Sending"))
            .expect("progress text row");
        assert!(
            line.starts_with("Sending game.nsp"),
            "narrow progress row starts with the file name: {line:?}"
        );
        assert!(
            !view.contains('█'),
            "the bar yields to the statistics text when the row is too narrow"
        );
    }

    #[test]
    fn progress_bar_fills_from_reported_bytes() {
        let mut app = App::new(FileList::new());
        let start = Instant::now();
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("game.nsp".into(), 50, 100, start);

        // The bar occupies the row after the statistics text
        // ("Sending game.nsp  50%  50 B/100 B", 33 columns) and fills it by ratio.
        let bar_extent = |app: &App| {
            let buf = buffer(app, 60, 12);
            let y = (0..buf.area.height)
                .find(|y| row_text(&buf, *y).contains("Sending game.nsp"))
                .expect("progress row");
            let start_x = (0..buf.area.width)
                .find(|x| buf.cell((*x, y)).unwrap().symbol() == "█")
                .expect("the bar starts after the statistics text");
            let filled = (start_x..buf.area.width)
                .filter(|x| {
                    let cell = buf.cell((*x, y)).unwrap();
                    cell.symbol() == "█" || cell.bg == theme::SUCCESS
                })
                .count();
            (start_x, filled as u16)
        };

        let (bar_start_x, half_filled) = bar_extent(&app);
        let bar_width = 60 - bar_start_x;
        assert_eq!(bar_start_x, 33, "the bar starts right after the text");
        assert!(
            half_filled > 0 && half_filled < bar_width,
            "half transfer leaves a partially filled bar: {half_filled}/{bar_width}"
        );

        app.update_progress_at("game.nsp".into(), 100, 100, start + Duration::from_secs(1));
        let (bar_start_x, full_filled) = bar_extent(&app);
        assert_eq!(
            full_filled,
            60 - bar_start_x,
            "a completed bar fills its slot"
        );
    }

    #[test]
    fn completed_transfer_logs_a_summary_and_clears_the_progress() {
        let mut app = App::new(FileList::new());
        app.handle_server_event(ServerEvent::Started);
        app.handle_server_event(ServerEvent::Progress {
            name: "game.nsp".into(),
            sent: 100,
            total: 100,
        });
        assert!(render(&app, 120, 12).contains("100%"));

        app.handle_server_event(ServerEvent::Completed {
            name: "game.nsp".into(),
            bytes: 4096,
        });

        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].message.contains("Sent: game.nsp"));
        assert!(app.log[0].message.contains("4.0 KiB"));
        assert!(app.progress.is_none());

        let view = render(&app, 120, 12);
        assert!(view.contains("Sent: game.nsp"));
        assert!(!view.contains("100%"));
    }

    #[test]
    fn progress_persists_between_ranges_until_a_file_completes() {
        let mut app = App::new(FileList::new());
        app.handle_server_event(ServerEvent::Started);
        app.update_progress_at("game.nsp".into(), 50, 100, Instant::now());
        assert!(render(&app, 120, 12).contains("Sending game.nsp"));

        // A later range request of the same file arrives as another Progress:
        // the region stays up and only the statistics change, without layout churn.
        app.update_progress_at("game.nsp".into(), 75, 100, Instant::now());
        let view = render(&app, 120, 12);
        assert!(view.contains("Sending game.nsp"));
        assert!(view.contains("75%"));

        // Only the file-level completion ends the region, and it logs exactly one
        // success summary for the file.
        app.handle_server_event(ServerEvent::Completed {
            name: "game.nsp".into(),
            bytes: 100,
        });
        let view = render(&app, 120, 12);
        assert!(!view.contains("Sending game.nsp"));
        assert!(app.progress.is_none());
        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].message.contains("Sent: game.nsp"));
    }

    #[test]
    fn sending_lines_take_the_informational_log_color() {
        let mut app = App::new(FileList::new());
        app.push_log("Sending: game.nsp".into());

        assert_eq!(log_color("Sending: game.nsp"), theme::SUBTLE);

        let buffer = buffer(&app, 120, 12);
        let mut checked = false;
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
                .collect();
            if let Some(column) = row.find("Sending: game.nsp") {
                let cell = buffer.cell((column as u16, y)).unwrap();
                assert_eq!(cell.fg, theme::SUBTLE);
                checked = true;
            }
        }
        assert!(checked, "the start line should be rendered");
    }

    #[test]
    fn sent_lines_are_colored_as_success() {
        let mut app = App::new(FileList::new());
        app.handle_server_event(ServerEvent::Completed {
            name: "game.nsp".into(),
            bytes: 4096,
        });

        assert_eq!(log_color("Sent: game.nsp (4.0 KiB)"), Color::Green);

        let buffer = buffer(&app, 120, 12);
        let mut checked = false;
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
                .collect();
            if let Some(column) = row.find("Sent:") {
                let cell = buffer.cell((column as u16, y)).unwrap();
                assert_eq!(cell.fg, Color::Green);
                checked = true;
            }
        }
        assert!(checked, "the summary line should be rendered");
    }

    #[test]
    fn connect_log_reports_the_waiting_state_once_per_period() {
        let mut log = ConnectLog::default();
        assert_eq!(log.waiting(), Some("Waiting for Switch..."));
        assert_eq!(log.waiting(), None);

        // The next waiting period reports the state again.
        log.connected();
        assert_eq!(log.waiting(), Some("Waiting for Switch..."));
    }

    #[test]
    fn connect_log_reports_an_error_only_while_it_changes() {
        let mut log = ConnectLog::default();
        assert_eq!(
            log.failed("no device"),
            Some("USB error: no device".to_string())
        );
        assert_eq!(log.failed("no device"), None);
        assert_eq!(
            log.failed("permission denied"),
            Some("USB error: permission denied".to_string())
        );

        // A successful connection forgets the error, so it is reported again later.
        log.connected();
        assert_eq!(
            log.failed("no device"),
            Some("USB error: no device".to_string())
        );
    }

    #[test]
    fn slash_starts_filtering_and_edits_the_filter() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
        fs::write(dir.path().join("beta.nsp"), b"b").unwrap();

        let mut app = app_with_picker(dir.path());
        app.handle_key(press('/'));
        app.handle_key(press('b'));
        match &app.mode {
            Mode::Picker(picker) => {
                assert_eq!(picker.filter(), "b");
                assert_eq!(picker.entries().len(), 1);
            }
            _ => panic!("expected the picker to stay open"),
        }

        // Global actions stay inert while the picker is open.
        app.handle_key(press('c'));
        assert_eq!(app.files.len(), 0);
        assert!(matches!(app.mode, Mode::Picker(_)));
    }

    #[test]
    fn escape_clears_the_filter_before_cancelling() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();

        let mut app = app_with_picker(dir.path());
        app.handle_key(press('/'));
        app.handle_key(press('a'));
        app.handle_key(key(KeyCode::Esc));
        match &app.mode {
            Mode::Picker(picker) => assert_eq!(picker.filter(), ""),
            _ => panic!("picker should stay open after clearing the filter"),
        }

        app.handle_key(key(KeyCode::Esc));
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn abbrev_path_keeps_paths_that_fit() {
        assert_eq!(abbrev_path("/tmp/a.nsp", 20), "/tmp/a.nsp");
        assert_eq!(abbrev_path("/tmp/a.nsp", 10), "/tmp/a.nsp");
    }

    #[test]
    fn abbrev_path_truncates_from_the_middle() {
        let long = "/mnt/games/The Legend of Zelda Breath of the Wild/game.nsp";
        let out = abbrev_path(long, 20);
        assert_eq!(out.chars().count(), 20);
        assert!(out.contains('…'));
        assert!(out.starts_with("/mnt/games"));
        assert!(out.ends_with("game.nsp"));
    }

    #[test]
    fn abbrev_path_handles_degenerate_widths() {
        assert_eq!(abbrev_path("/very/long/path", 0), "");
        assert_eq!(abbrev_path("/very/long/path", 1), "…");
    }

    #[test]
    fn narrow_terminal_truncates_long_paths() {
        let dir = tempfile::tempdir().unwrap();
        let long_name = "a-nsp-file-with-a-remarkably-long-name.nsp";
        fs::write(dir.path().join(long_name), vec![0u8; 2048]).unwrap();
        let mut app = App::new(FileList::new());
        app.files.add_path(dir.path()).unwrap();

        let view = render(&app, 40, 12);
        assert!(view.contains('…'));
    }

    #[test]
    fn picker_rows_show_file_sizes_but_not_directory_sizes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), vec![0u8; 1024]).unwrap();
        fs::write(dir.path().join("verybig.nsp"), vec![0u8; 2048]).unwrap();
        fs::create_dir(dir.path().join("games")).unwrap();
        let mut app = App::new(FileList::new());
        app.mode = Mode::Picker(Picker::at(dir.path()));

        let buf = buffer(&app, 80, 16);
        let mut saw_file = false;
        let mut saw_dir = false;
        // Only the picker's inner rows (below the title border at y=1 and above the
        // bottom border of the list region) are scanned.
        for y in 2..6 {
            let row: String = (0..buf.area.width)
                .map(|x| buf.cell((x, y)).unwrap().symbol())
                .collect();
            let has_size = (0..buf.area.width).any(|x| buf.cell((x, y)).unwrap().symbol() == "B");
            if row.contains("games/") {
                saw_dir = true;
                assert!(!has_size, "directory row must have an empty size cell");
            }
            if row.contains("a.nsp") {
                saw_file = true;
                assert!(has_size, "file row must show its size");
            }
        }
        assert!(saw_file && saw_dir);
    }

    #[test]
    fn picker_title_shows_the_entry_count() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.nsp"), b"a").unwrap();
        fs::write(dir.path().join("b.nsp"), b"b").unwrap();
        let mut app = App::new(FileList::new());
        app.mode = Mode::Picker(Picker::at(dir.path()));

        let view = render(&app, 80, 12);
        assert!(view.contains("· 2 entries"));
    }

    #[test]
    fn picker_title_shows_the_match_count_while_filtering() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
        fs::write(dir.path().join("beta.nsp"), b"b").unwrap();
        let mut app = App::new(FileList::new());
        app.mode = Mode::Picker(Picker::at(dir.path()));
        app.handle_key(press('/'));
        app.handle_key(press('b'));
        match &app.mode {
            Mode::Picker(picker) => {
                assert_eq!(picker.entries().len(), 1);
                let view = render(&app, 80, 12);
                assert!(view.contains("· 1 match"));
            }
            _ => panic!("expected the picker to stay open"),
        }

        app.handle_key(press('a'));
        let view = render(&app, 80, 12);
        assert!(view.contains("· 0 matches"));
    }

    #[test]
    fn centered_fixed_centers_and_clamps() {
        let area = Rect::new(0, 0, 80, 24);
        assert_eq!(centered_fixed(20, 3, area), Rect::new(30, 10, 20, 3));

        let tiny = Rect::new(0, 0, 4, 2);
        assert_eq!(centered_fixed(20, 3, tiny), Rect::new(0, 0, 4, 2));
    }

    #[test]
    fn confirm_dialog_is_compact_for_both_prompts() {
        for (kind, prompt) in [
            (ConfirmKind::Quit, "Quit dbi-backend? [y/N]"),
            (ConfirmKind::Clear, "Clear the file list? [y/N]"),
        ] {
            let mut app = App::new(FileList::new());
            app.mode = Mode::Confirm(kind);
            let buf = buffer(&app, 80, 24);

            let mut min_x = u16::MAX;
            let mut max_x = 0;
            let mut rows = 0;
            for y in 0..buf.area.height {
                let mut row_has_dialog = false;
                for x in 0..buf.area.width {
                    // The themed dialog is drawn in the warning color; the muted
                    // hint line keeps this dialog-only scan unambiguous.
                    if buf.cell((x, y)).unwrap().fg == Color::Yellow {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                        row_has_dialog = true;
                    }
                }
                if row_has_dialog {
                    rows += 1;
                }
            }

            let width = max_x - min_x + 1;
            assert_eq!(width, prompt.len() as u16 + 2);
            assert_eq!(rows, 3);
            assert_eq!(min_x, (80 - width) / 2);
            assert!(width < 80 * 60 / 100);

            let view = render(&app, 80, 24);
            assert!(view.contains(prompt));
        }
    }

    fn share(name: &str, host: &str, share: &str, username: &str) -> ShareConfig {
        ShareConfig {
            name: name.to_string(),
            host: host.to_string(),
            port: 445,
            share: share.to_string(),
            username: username.to_string(),
            domain: None,
            root: String::new(),
        }
    }

    /// Opens the share list with two configured shares.
    fn app_with_shares() -> App {
        App::with_shares(
            FileList::new(),
            vec![
                share("Backup", "192.0.2.9", "backup", "player"),
                share("Games", "192.0.2.1", "games", "player"),
            ],
        )
    }

    #[test]
    fn capital_s_opens_the_share_list_and_lower_s_starts_instead() {
        let mut app = app_with_shares();

        // Lowercase `s` keeps its old meaning; with no files it changes nothing.
        app.handle_key(press('s'));
        assert!(matches!(app.mode, Mode::Normal));

        app.handle_key(press('S'));
        assert!(matches!(app.mode, Mode::Shares(_)));
    }

    #[test]
    fn s_is_inert_while_the_server_is_active() {
        let mut app = app_with_shares();
        app.files.insert("a.nsp", "/tmp/a.nsp");
        app.handle_key(press('s'));
        assert_eq!(app.status, ServerStatus::Waiting);

        assert_eq!(app.handle_key(press('S')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn s_chip_is_locked_while_the_server_is_active() {
        let active = {
            let mut app = app_with_shares();
            app.files.insert("a.nsp", "/tmp/a.nsp");
            app.handle_key(press('s'));
            app
        };
        let buf = buffer(&active, 120, 12);
        let y = buf.area.height - 1;
        let fgs: Vec<Color> = (0..buf.area.width)
            .map(|x| buf.cell((x, y)).unwrap().fg)
            .collect();
        // The `[S]hares` chip is 8 cells in total, all dimmed while locked.
        assert!(
            fgs.iter().filter(|c| **c == theme::MUTED).count() >= 20,
            "a/d/c and S are dimmed while the server is active"
        );
        // Idle: nothing of that chip is locked.
        let idle = app_with_shares();
        let buf = buffer(&idle, 120, 12);
        let y = buf.area.height - 1;
        let fgs: Vec<Color> = (0..buf.area.width)
            .map(|x| buf.cell((x, y)).unwrap().fg)
            .collect();
        assert!(fgs.contains(&theme::ACCENT), "S stays accented when idle");
    }

    #[test]
    fn cancelling_the_share_list_connects_to_nothing() {
        for cancel in [KeyCode::Esc, KeyCode::Char('q')] {
            let mut app = app_with_shares();
            app.mode = Mode::Shares(ShareList::open());
            app.handle_key(key(cancel));
            assert!(matches!(app.mode, Mode::Normal));
        }
    }

    #[test]
    fn j_and_k_move_the_share_selection_with_clamping() {
        let mut app = app_with_shares();
        app.mode = Mode::Shares(ShareList::open());

        app.handle_key(press('j'));
        app.handle_key(press('j'));
        match &app.mode {
            Mode::Shares(list) => {
                assert_eq!(list.selected, 1, "two downs stop at the last row")
            }
            _ => panic!("share list stays open"),
        }
        app.handle_key(key(KeyCode::Up));
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 0),
            _ => panic!("share list stays open"),
        }
        app.handle_key(press('k'));
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 0, "clamped at the top"),
            _ => panic!("share list stays open"),
        }
    }

    #[test]
    fn share_list_rows_show_the_details_sorted_by_name() {
        let mut app = app_with_shares();
        app.mode = Mode::Shares(ShareList::open());
        let view = render(&app, 100, 14);

        // alphabetical: Backup first.
        let backup = view.find("Backup").expect("share row shown");
        let games = view.find("Games").expect("share row shown");
        assert!(backup < games);
        assert!(view.contains("192.0.2.9 · backup"));
        assert!(view.contains("192.0.2.1 · games"));
        assert!(view.contains("player"));
        assert!(view.contains("Shares · 2 shares"));
    }

    #[test]
    fn empty_share_list_names_the_add_key_and_the_config_path() {
        let mut app = App::new(FileList::new());
        app.mode = Mode::Shares(ShareList::open());
        let view = render(&app, 120, 14);
        assert!(view.contains("No shares configured"));
        assert!(view.contains("`a`"), "the add key is named: {view}");
        assert!(
            view.contains("config.toml"),
            "names the config path: {view}"
        );
    }

    #[test]
    fn the_share_list_shows_its_own_hints_including_add() {
        let app = app_in_share_list();
        let view = render(&app, 100, 14);
        assert!(view.contains("a add share"), "the add chip shows: {view}");
        assert!(view.contains("Enter connect"));
        assert!(view.contains("Esc cancel"));
        assert!(
            !view.contains("[a]dd"),
            "the file chips do not show: {view}"
        );
    }

    #[test]
    fn the_help_overlay_mentions_the_share_list_keys() {
        let mut app = App::new(FileList::new());
        app.mode = Mode::Help;
        let view = render(&app, 80, 24);
        assert!(
            view.contains("Share list: Enter connect, a add share, Esc cancel"),
            "the share section lists its keys: {view}"
        );
    }

    // --- 5.2: the password modal -----------------------------------------

    fn app_with_password_modal(share: &str) -> App {
        let mut app = app_with_shares();
        app.mode = Mode::Password(PasswordModal::for_share(share));
        app
    }

    /// The share list already open, ready for keys.
    fn app_in_share_list() -> App {
        let mut app = app_with_shares();
        app.mode = Mode::Shares(ShareList::open());
        app
    }

    #[test]
    fn enter_opens_the_password_modal_for_the_selected_share() {
        let mut app = app_in_share_list();
        app.handle_key(key(KeyCode::Down)); // Games (sorted second)
        app.handle_key(key(KeyCode::Enter));
        match &app.mode {
            Mode::Password(modal) => assert_eq!(modal.share, "Games"),
            _ => panic!("expected the password modal"),
        }
    }

    #[test]
    fn typed_characters_mask_in_render_never_echoing_the_password() {
        let mut app = app_with_password_modal("Games");
        for c in "hunter2".chars() {
            app.handle_key(press(c));
        }
        let view = render(&app, 80, 10);
        assert!(view.contains("*******"), "renders one star per character");
        assert!(!view.contains("hunter2"), "the typed text never renders");
        assert!(view.contains("Password for Games"));
    }

    #[test]
    fn enter_submits_and_the_buffer_leaves_with_it() {
        let mut app = app_with_password_modal("Games");
        for c in "secret".chars() {
            app.handle_key(press(c));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        match action {
            Action::Connect {
                share,
                password,
                selection,
            } => {
                assert_eq!(share, "Games");
                assert_eq!(password, "secret");
                assert_eq!(selection, 1, "Games is the second sorted share");
            }
            other => panic!("expected a connect action, got {other:?}"),
        }
        // The modal is empty and closed; the share list is back underneath.
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 1),
            _ => panic!("expected the share list after submitting"),
        }
    }

    #[test]
    fn enter_on_an_empty_modal_does_nothing() {
        let mut app = app_with_password_modal("Games");
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Action::None);
        assert!(matches!(app.mode, Mode::Password(_)));
    }

    #[test]
    fn backspace_reshapes_the_mask() {
        let mut app = app_with_password_modal("Games");
        for c in "abc".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Backspace));
        let view = render(&app, 80, 10);
        assert!(view.contains("**"), "two stars left");
        assert!(!view.contains("***"), "the third star is gone");
    }

    #[test]
    fn esc_cancels_back_to_the_share_list_at_that_share() {
        let mut app = app_in_share_list();
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter)); // modal for Games
        app.handle_key(key(KeyCode::Esc));
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 1, "back at Games"),
            _ => panic!("expected the share list after cancelling"),
        }
    }

    #[test]
    fn a_failed_connection_stays_on_the_share_list_for_a_retry() {
        let mut app = app_in_share_list();
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter)); // modal for Games
        for c in "secret".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Enter)); // submit (action thrown away here)
        match &app.mode {
            Mode::Shares(_) => {}
            _ => panic!("expected the share list while connecting"),
        }

        // The caller logs and reports the failure; the list stays put.
        app.push_log("Cannot connect to Games: authentication failed".to_string());
        app.share_connection_failed("Games");
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 1),
            _ => panic!("expected the share list after the failed connect"),
        }
    }

    #[test]
    fn a_connected_share_opens_the_picker_at_its_root() {
        let mut app = app_with_shares();
        app.open_share_picker("Games", "library/switch");

        match &app.mode {
            Mode::Picker(picker) => {
                assert!(picker.is_busy(), "the root listing is pending");
                assert_eq!(picker.origin_display(), "Games:/library/switch");
            }
            _ => panic!("expected the picker after a successful connect"),
        }
        assert_eq!(
            app.pending_share_list(),
            Some(Source::Smb(SmbLocation {
                share: "Games".into(),
                path: "library/switch".into()
            }))
        );
    }

    // --- 5.3: share-sourced browsing and adds -----------------------------

    fn loc(path: &str) -> SmbLocation {
        SmbLocation {
            share: "Games".into(),
            path: path.into(),
        }
    }

    fn list_entry(name: &str, kind: EntryKind, size: u64) -> ListEntry {
        ListEntry {
            name: name.to_string(),
            kind,
            size,
        }
    }

    /// A picker over a listed share root, open in the app.
    fn app_with_share_picker() -> App {
        let mut app = app_with_shares();
        app.open_share_picker("Games", "");
        if let Some(source) = app.pending_share_list() {
            // Pretend the store answered with one directory and one file.
            app.apply_share_listing(
                &source,
                vec![
                    list_entry("Switch", EntryKind::Dir, 0),
                    list_entry("game.nsp", EntryKind::File, 42),
                ],
            );
        }
        app
    }

    #[test]
    fn a_share_listing_fills_the_picker() {
        let app = app_with_share_picker();
        match &app.mode {
            Mode::Picker(picker) => {
                assert!(!picker.is_busy());
                let names: Vec<&str> = picker.entries().iter().map(|e| e.name.as_str()).collect();
                assert_eq!(names, vec!["Switch", "game.nsp"]);
                assert_eq!(
                    picker.entries()[1].source,
                    Source::Smb(loc("game.nsp")),
                    "entries know their full share location"
                );
            }
            _ => panic!("expected the picker"),
        }
    }

    #[test]
    fn a_failed_share_listing_shows_an_error_and_unblocks() {
        let mut app = app_with_shares();
        app.open_share_picker("Games", "");
        let requested = app.pending_share_list().unwrap();
        app.share_listing_failed(&requested, "the share is gone");

        match &app.mode {
            Mode::Picker(picker) => {
                assert!(!picker.is_busy());
                let view = render(&app, 100, 14);
                assert!(view.contains("the share is gone"));
                assert!(view.contains("Games"));
            }
            _ => panic!("expected the picker to stay open with the error"),
        }
    }

    #[test]
    fn enter_on_a_share_directory_requests_its_listing_through_the_action() {
        let mut app = app_with_share_picker();
        // Switch (first row) is a directory.
        let action = app.handle_key(key(KeyCode::Enter));
        match action {
            Action::List(source) => assert_eq!(source, Source::Smb(loc("Switch"))),
            other => panic!("expected a list action, got {other:?}"),
        }
        // The picker is busy until the listing comes back.
        assert_eq!(app.pending_share_list(), Some(Source::Smb(loc("Switch"))),);
        // And the listing lands.
        app.apply_share_listing(
            &Source::Smb(loc("Switch")),
            vec![list_entry("game.nsp", EntryKind::File, 10)],
        );
        match &app.mode {
            Mode::Picker(picker) => {
                assert_eq!(picker.origin_display(), "Games:/Switch");
                assert_eq!(picker.entries().len(), 1);
                assert!(!picker.is_busy());
            }
            _ => panic!("expected the picker"),
        }
    }

    #[test]
    fn bksp_in_a_share_requests_the_parent_listing_and_stops_at_the_root() {
        let mut app = app_with_share_picker();
        app.handle_key(key(KeyCode::Enter)); // → Games:/Switch
        let action = app.handle_key(key(KeyCode::Backspace)); // → root
        match action {
            Action::List(source) => assert_eq!(source, Source::Smb(loc(""))),
            other => panic!("expected a list action, got {other:?}"),
        }

        // At the root, going up is a no-op.
        app.apply_share_listing(&Source::Smb(loc("")), vec![]);
        let action = app.handle_key(key(KeyCode::Backspace));
        assert_eq!(action, Action::None);
        match &app.mode {
            Mode::Picker(picker) => assert_eq!(picker.origin_display(), "Games:/"),
            _ => panic!("expected the picker"),
        }
    }

    #[test]
    fn space_on_a_share_entry_starts_an_add_and_closes_the_picker() {
        let mut app = app_with_share_picker();
        app.handle_key(key(KeyCode::Down)); // game.nsp
        let action = app.handle_key(press(' '));
        assert_eq!(action, Action::Add(Source::Smb(loc("game.nsp"))));
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn share_origins_show_in_the_files_table_share_qualified() {
        let mut app = App::new(FileList::new());
        app.files
            .insert_source("game.nsp", Source::Smb(loc("switch/game.nsp")), 42);
        let view = render(&app, 120, 12);
        assert!(
            view.contains("Games:/switch/game.nsp"),
            "share-qualified origin visible"
        );
    }

    #[test]
    fn the_help_overlay_lists_s() {
        let mut app = App::new(FileList::new());
        app.mode = Mode::Help;
        let view = render(&app, 80, 24);
        assert!(view.contains("S   list configured shares"));
    }

    // --- the add-share form ----------------------------------------------

    /// The share list open, ready to receive the form's opening key.
    fn app_in_form_from_share_list(selected: usize) -> App {
        let mut app = app_in_share_list();
        if selected > 0 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(press('a'));
        app
    }

    /// The form open over the share list at Games (selected second).
    fn app_with_form() -> App {
        app_in_form_from_share_list(1)
    }

    #[test]
    fn a_in_the_share_list_opens_the_add_form() {
        let mut app = app_in_share_list();
        assert_eq!(app.handle_key(press('a')), Action::None);
        match &app.mode {
            Mode::ShareForm(form) => {
                for field in 0..SHARE_FORM_FIELDS {
                    assert_eq!(form.field_value(field), "", "field {field} opens empty");
                }
                assert_eq!(form.focus(), 0, "the first field holds the focus");
            }
            _ => panic!("expected the add-share form"),
        }
    }

    #[test]
    fn a_in_the_share_list_keeps_the_entry_point_for_esc() {
        let app = app_with_form();
        match &app.mode {
            Mode::ShareForm(form) => assert_eq!(form.return_selection(), 1),
            _ => panic!("expected the form to remember the share list"),
        }
    }

    #[test]
    fn the_add_form_renders_as_a_dialog_with_its_hints() {
        let mut app = app_with_form();
        if let Mode::ShareForm(form) = &mut app.mode {
            form.push('h');
            form.push('o');
            form.push('s');
            form.push('t');
        }
        let view = render(&app, 80, 24);

        assert!(view.contains("Add share"), "the dialog title: {view}");
        for label in ["name", "server", "share", "username", "domain", "root"] {
            assert!(view.contains(label), "the {label} field is shown: {view}");
        }
        assert!(view.contains("host"), "the typed value shows as typed");
        assert!(view.contains("Tab next"));
        assert!(view.contains("Esc cancel"));
        assert!(!view.contains("[a]dd"), "the normal chips do not show");
        assert!(
            !view.contains("Password"),
            "no password in the form: {view}"
        );
    }

    #[test]
    fn each_field_keeps_its_own_typed_text() {
        let mut app = app_with_form();
        for c in "nb1".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Backspace));
        app.handle_key(key(KeyCode::Tab)); // → server
        for c in "nas.example".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Down)); // → share
        for c in "games".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Up)); // back to server, values stay
        app.handle_key(key(KeyCode::Up)); // back to name

        match &app.mode {
            Mode::ShareForm(form) => {
                assert_eq!(form.field_value(FIELD_NAME), "nb");
                assert_eq!(form.field_value(FIELD_SERVER), "nas.example");
                assert_eq!(form.field_value(FIELD_SHARE), "games");
            }
            _ => panic!("the form stays open while typing"),
        }
    }

    #[test]
    fn typing_shows_in_the_render_and_never_masks() {
        let mut app = app_with_form();
        for c in "nas".chars() {
            app.handle_key(press(c));
        }
        let view = render(&app, 80, 24);
        assert!(view.contains("nas"), "the value shows as typed: {view}");
        assert!(!view.contains("n**"), "no mask characters appear");
    }

    #[test]
    fn backspace_erases_within_the_focused_field_only() {
        let mut app = app_with_form();
        for c in "na".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Tab)); // → server
        for c in "se".chars() {
            app.handle_key(press(c));
        }
        app.handle_key(key(KeyCode::Backspace));

        match &app.mode {
            Mode::ShareForm(form) => {
                assert_eq!(
                    form.field_value(FIELD_SERVER),
                    "s",
                    "erases the focused field"
                );
                assert_eq!(
                    form.field_value(FIELD_NAME),
                    "na",
                    "other fields keep theirs"
                );
            }
            _ => panic!("the form stays open"),
        }
    }

    #[test]
    fn tab_and_shift_tab_move_focus_within_the_six_fields() {
        let mut app = app_with_form();
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(form_focus(&app), 1);
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(form_focus(&app), 3);

        let shift_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        app.handle_key(shift_tab);
        assert_eq!(form_focus(&app), 2, "Shift+Tab moves back");
    }

    fn form_focus(app: &App) -> usize {
        match &app.mode {
            Mode::ShareForm(form) => form.focus(),
            _ => panic!("expected the form open"),
        }
    }

    #[test]
    fn arrow_focus_moves_clamp_at_both_ends() {
        let mut app = app_with_form();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(form_focus(&app), 0, "clamped at the first field");
        for _ in 0..8 {
            app.handle_key(key(KeyCode::Down));
        }
        assert_eq!(form_focus(&app), 5, "clamped at the last field");
        app.handle_key(key(KeyCode::Up));
        assert_eq!(form_focus(&app), 4);
    }

    #[test]
    fn esc_returns_to_the_share_list_without_saving() {
        let mut app = app_with_form();
        for c in "whatever".chars() {
            app.handle_key(press(c));
        }
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Action::None);
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 1, "back at Games"),
            _ => panic!("expected the share list after Esc"),
        }
        assert_eq!(app.shares.len(), 2, "nothing was saved");
    }

    #[test]
    fn enter_on_empty_required_fields_keeps_the_form_and_names_what_is_missing() {
        let mut app = app_with_form();
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Action::None);
        match &app.mode {
            Mode::ShareForm(form) => {
                assert!(form.errors().contains(&"name is required".to_string()));
                assert!(form.errors().contains(&"server is required".to_string()));
                assert!(form.errors().contains(&"share is required".to_string()));
                assert!(form.errors().contains(&"username is required".to_string()));
            }
            _ => panic!("an invalid submit keeps the form open"),
        }
        let view = render(&app, 80, 24);
        assert!(
            view.contains("name is required"),
            "errors show inline: {view}"
        );
    }

    #[test]
    fn enter_with_a_bad_server_keeps_the_form_and_reports_it() {
        let mut app = app_with_form();
        fill_form(&mut app, ("Late", "host:bogus", "s", "u"));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Action::None);
        match &app.mode {
            Mode::ShareForm(form) => {
                assert!(form.errors().iter().any(|error| error.contains("bad port")));
            }
            _ => panic!("an invalid submit keeps the form open"),
        }
        // The typed values survive the rejected submit.
        match &app.mode {
            Mode::ShareForm(form) => {
                assert_eq!(form.field_value(FIELD_NAME), "Late");
                assert_eq!(form.field_value(FIELD_SERVER), "host:bogus");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn enter_with_a_duplicate_name_keeps_the_form_and_reports_it_taken() {
        let mut app = app_with_form();
        // Games is configured by the app's own share list.
        fill_form(&mut app, ("Games", "other.host", "other", "u"));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Action::None);
        match &app.mode {
            Mode::ShareForm(form) => {
                assert!(
                    form.errors().iter().any(
                        |error| error.contains("Games") && error.contains("already configured")
                    ),
                    "the taken name is reported: {:?}",
                    form.errors()
                );
            }
            _ => panic!("an invalid submit keeps the form open"),
        }
    }

    #[test]
    fn enter_with_valid_data_returns_the_add_share_action() {
        let mut app = app_with_form();
        fill_form(&mut app, ("Late", "nas.example", "backups", "me"));
        match app.handle_key(key(KeyCode::Enter)) {
            Action::AddShare { config } => {
                assert_eq!(config.name, "Late");
                assert_eq!(config.host, "nas.example");
                assert_eq!(config.port, 445);
                assert_eq!(config.share, "backups");
                assert_eq!(config.username, "me");
                assert_eq!(config.domain, None);
                assert_eq!(config.root, "");
            }
            other => panic!("expected an add-share action, got {other:?}"),
        }
        // The form stays open until the caller lands back on the share list.
        assert!(matches!(app.mode, Mode::ShareForm(_)));
    }

    #[test]
    fn enter_with_optional_fields_carries_them() {
        let mut app = app_with_form();
        fill_form(&mut app, ("Late", "nas.example:9445", "backup", "me"));
        if let Mode::ShareForm(form) = &mut app.mode {
            form.next_field(); // username → domain
            for c in "WORKGROUP".chars() {
                form.push(c);
            }
            form.next_field(); // domain → root
            for c in "library/switch".chars() {
                form.push(c);
            }
        }
        match app.handle_key(key(KeyCode::Enter)) {
            Action::AddShare { config } => {
                assert_eq!(config.port, 9445);
                assert_eq!(config.domain.as_deref(), Some("WORKGROUP"));
                assert_eq!(config.root, "library/switch");
            }
            other => panic!("expected an add-share action, got {other:?}"),
        }
    }

    /// Types the four required values field by field: name, server, share,
    /// username, ending after username.
    fn fill_form(app: &mut App, values: (&str, &str, &str, &str)) {
        for (value, is_last) in [
            (values.0, false),
            (values.1, false),
            (values.2, false),
            (values.3, true),
        ] {
            for c in value.chars() {
                app.handle_key(press(c));
            }
            if !is_last {
                app.handle_key(key(KeyCode::Tab));
            }
        }
    }
}
