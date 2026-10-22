//! The application's state: what it shows, what it offers, and what the
//! background server task reports back.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config;
use crate::config::ShareConfig;
use crate::picker::Picker;

use super::format::{human_size, remaining_seconds};
use dbi_backend_core::filelist::{AddOutcome, FileEntry, FileList};
use dbi_backend_core::source::{ListEntry, Source, SourceError, Sources};

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
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Quit,
    /// Connect to a share by display name with its typed password. Closing
    /// this action drops the password; the hub is the only holder afterwards.
    Connect {
        share: String,
        password: String,
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
#[derive(Debug, Clone)]
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
pub(crate) const FIELD_NAME: usize = 0;
pub(crate) const FIELD_SERVER: usize = 1;
pub(crate) const FIELD_SHARE: usize = 2;
pub(crate) const FIELD_USERNAME: usize = 3;
pub(crate) const FIELD_DOMAIN: usize = 4;
pub(crate) const FIELD_ROOT: usize = 5;

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
#[derive(Debug, Clone)]
pub struct LogLine {
    pub at: Duration,
    pub message: String,
}

/// The latest transfer progress, with derived speed once two samples are seen.
#[derive(Debug, Clone)]
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
    /// The search narrowing the visible file list. It never changes the files
    /// offered to the Switch.
    search: String,
    /// Whether the search input is open.
    searching: bool,
    /// Configured shares for the share list, sorted by display name.
    pub shares: Vec<ShareConfig>,
    /// Where the config file lives, for the empty-share-list hint.
    pub config_hint: Option<PathBuf>,
    pub(crate) started_at: Instant,
}

impl App {
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
            search: String::new(),
            searching: false,
            shares,
            config_hint: config::config_path(),
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

    pub(crate) fn update_progress_at(&mut self, name: String, sent: u64, total: u64, now: Instant) {
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

    /// Add a source to the file list and log the outcome.
    ///
    /// A local path is walked on the filesystem, a share path over the share;
    /// both read each accepted file's identity through the store.
    pub async fn add_source(&mut self, store: &dyn Sources, source: &Source) {
        let display = source.to_string();
        let outcome = match source {
            Source::Local(path) => self
                .files
                .add_path(path, store)
                .await
                .map_err(|e| SourceError::new(e.to_string())),
            share => self.files.add_source(store, share).await,
        };
        match outcome {
            Ok(outcome) => self.log_add_outcome(&display, &outcome),
            Err(e) => self.log_add_failure(&display, &e),
        }
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        let len = self.visible_files().len();
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

    pub(crate) fn clamp_selection(&mut self) {
        let len = self.visible_files().len();
        self.selected = if len == 0 {
            0
        } else {
            self.selected.min(len - 1)
        };
    }

    pub(crate) fn remove_selected(&mut self) {
        let name = {
            let visible = self.visible_files();
            visible.get(self.selected).map(|(name, _)| (*name).clone())
        };
        if let Some(name) = name {
            if self.files.remove(&name).is_some() {
                self.push_log(format!("Removed: {name}"));
                self.clamp_selection();
            }
        }
    }

    /// The files the list region shows: the offered files whose name matches
    /// the active search, ignoring case. The search never changes the files
    /// offered to the Switch — that is the whole list.
    pub fn visible_files(&self) -> Vec<(&String, &FileEntry)> {
        let needle = self.search.to_lowercase();
        self.files
            .iter()
            .filter(|(name, _)| needle.is_empty() || name.to_lowercase().contains(&needle))
            .collect()
    }

    /// Whether the search input is open.
    pub fn is_searching(&self) -> bool {
        self.searching
    }

    /// The active search string.
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Open the search input, keeping the search it holds.
    pub fn start_search(&mut self) {
        self.searching = true;
    }

    /// Close the search input, keeping the search.
    pub fn stop_search(&mut self) {
        self.searching = false;
    }

    /// Type into the search.
    pub fn push_search(&mut self, c: char) {
        self.search.push(c);
        self.clamp_selection();
    }

    /// Erase the search's last character.
    pub fn pop_search(&mut self) {
        self.search.pop();
        self.clamp_selection();
    }

    /// Clear the search, so every file is visible again.
    pub fn clear_search(&mut self) {
        self.search.clear();
        self.clamp_selection();
    }

    /// The configured share by display name, for the picker's browsable root.
    pub fn share_config(&self, name: &str) -> Option<&ShareConfig> {
        self.shares.iter().find(|config| config.name == name)
    }

    /// Register a share the add form built: inserted among the sorted shares,
    /// with the share list brought back up and the highlight on the added
    /// share. The form refuses a duplicate name, so there is nothing to
    /// replace here.
    pub fn share_added(&mut self, config: ShareConfig) {
        let position = self
            .shares
            .iter()
            .position(|other| other.name >= config.name)
            .unwrap_or(self.shares.len());
        self.shares.insert(position, config);
        self.mode = Mode::Shares(ShareList { selected: position });
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
        let rejected = if outcome.rejected > 0 {
            format!(", {} rejected by the filters", outcome.rejected)
        } else {
            String::new()
        };
        self.push_log(format!(
            "Added: {display} ({} files{rejected})",
            outcome.added.len()
        ));
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
