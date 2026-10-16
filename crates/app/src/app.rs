//! TUI application state and the background server task.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::picker::{Kind, Picker};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::server::{Server, ServerUpdate};
use dbi_backend_core::transport::usb::UsbTransport;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

/// Maximum number of log lines retained (mirrors the Python 1000-line cap).
pub const MAX_LOG_LINES: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServerStatus {
    Idle,
    Running,
}

/// What the caller should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Quit,
}

/// A lifecycle or progress transition reported by the background server task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    Started,
    Ended,
    Progress { name: String, sent: u64, total: u64 },
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
    Picker(Picker),
    Confirm(ConfirmKind),
    Help,
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
    last_at: Instant,
}

pub struct App {
    pub files: FileList,
    pub log: VecDeque<LogLine>,
    pub status: ServerStatus,
    pub mode: Mode,
    pub should_quit: bool,
    pub selected: usize,
    pub progress: Option<ProgressState>,
    server_started: bool,
    started_at: Instant,
}

impl App {
    pub fn new(files: FileList) -> Self {
        Self {
            files,
            log: VecDeque::new(),
            status: ServerStatus::Idle,
            mode: Mode::Normal,
            should_quit: false,
            selected: 0,
            progress: None,
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
            ServerEvent::Started => self.status = ServerStatus::Running,
            ServerEvent::Ended => {
                self.status = ServerStatus::Idle;
                self.server_started = false;
                self.progress = None;
            }
            ServerEvent::Progress { name, sent, total } => {
                self.update_progress(name, sent, total);
            }
        }
    }

    fn update_progress(&mut self, name: String, sent: u64, total: u64) {
        let now = Instant::now();
        let speed = match &self.progress {
            Some(previous) if previous.name == name && sent > previous.sent => {
                let elapsed = now.duration_since(previous.last_at).as_secs_f64();
                if elapsed > 0.0 {
                    Some(((sent - previous.sent) as f64 / elapsed) as u64)
                } else {
                    previous.speed
                }
            }
            Some(previous) if previous.name == name => previous.speed,
            _ => None,
        };
        self.progress = Some(ProgressState {
            name,
            sent,
            total,
            speed,
            last_at: now,
        });
    }

    /// Add a path to the file list, logging the outcome and closing the picker.
    fn select_path(&mut self, path: &Path) {
        match self.files.add_path(path) {
            Ok(outcome) => {
                self.push_log(format!("Added: {}", path.display()));
                for skipped in &outcome.skipped {
                    self.push_log(format!(
                        "Warning: skipped {}: cannot be read",
                        skipped.display()
                    ));
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

    fn handle_picker_key(&mut self, key: KeyEvent) {
        enum Outcome {
            None,
            Select(PathBuf),
            Cancel,
        }

        let outcome = {
            let Mode::Picker(picker) = &mut self.mode else {
                return;
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
                        Some(entry) => Outcome::Select(entry.path.clone()),
                        None => Outcome::None,
                    },
                    KeyCode::Enter => {
                        if picker.current().map(|e| e.kind) == Some(Kind::Dir) {
                            picker.descend();
                            Outcome::None
                        } else {
                            match picker.current() {
                                Some(entry) => Outcome::Select(entry.path.clone()),
                                None => Outcome::None,
                            }
                        }
                    }
                    KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                        picker.go_up();
                        Outcome::None
                    }
                    KeyCode::Esc | KeyCode::Char('q') => Outcome::Cancel,
                    _ => Outcome::None,
                }
            }
        };

        match outcome {
            Outcome::Select(path) => self.select_path(&path),
            Outcome::Cancel => self.mode = Mode::Normal,
            Outcome::None => {}
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
                self.handle_picker_key(key);
                return Action::None;
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

        let running = self.status == ServerStatus::Running;
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
                    self.status = ServerStatus::Running;
                    Action::Start
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }
}

/// Spawn the background server loop. Logs are forwarded to `log_tx`, lifecycle
/// transitions and progress to `event_tx`.
pub fn spawn_server(
    files: FileList,
    log_tx: UnboundedSender<String>,
    event_tx: UnboundedSender<ServerEvent>,
) {
    tokio::spawn(async move {
        loop {
            let _ = log_tx.send("Waiting for Switch...".into());
            let transport = match UsbTransport::connect().await {
                Ok(t) => t,
                Err(e) => {
                    let _ = log_tx.send(format!("USB error: {e}"));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let _ = log_tx.send("Switch connected".into());
            let _ = event_tx.send(ServerEvent::Started);

            let mut server = Server::new(transport, files.clone());
            let log = log_tx.clone();
            let events = event_tx.clone();
            let mut on_update = move |update: ServerUpdate| match update {
                ServerUpdate::Log(line) => {
                    let _ = log.send(line);
                }
                ServerUpdate::Progress { name, sent, total } => {
                    let _ = events.send(ServerEvent::Progress { name, sent, total });
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
                    continue;
                }
            }
        }
    });
}

/// Hint line shown while the picker is open.
const PICKER_HINT: &str = "Esc cancel  Enter open  Space add  Bksp up  / filter";

/// Hint line shown in the normal view.
const NORMAL_HINT: &str = "[a]dd  [d]elete  [c]lear  [s]tart  [q]uit  [?] help";

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
        _ => {}
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let state = match app.status {
        ServerStatus::Idle => "idle",
        ServerStatus::Running => "running",
    };
    let text = format!(
        "status: {state} | files: {} | total: {}",
        app.files.len(),
        human_size(app.files.total_size())
    );
    let widget = Paragraph::new(text).style(Style::default().fg(status_color(app.status)));
    frame.render_widget(widget, area);
}

fn render_files(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|(name, entry)| {
            ListItem::new(format!(
                "{}  {}  ->  {}",
                name,
                human_size(entry.size),
                entry.path.display()
            ))
        })
        .collect();
    let files = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if !app.files.is_empty() {
        state.select(Some(app.selected.min(app.files.len() - 1)));
    }
    frame.render_stateful_widget(files, area, &mut state);
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
    let speed = progress
        .speed
        .map(|bytes| format!("  {}/s", human_size(bytes)))
        .unwrap_or_default();
    let label = format!("Sending {}  {}%{}", progress.name, percent, speed);
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, area);
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
        .block(Block::default().borders(Borders::ALL).title("Log"))
        .wrap(Wrap { trim: true });
    frame.render_widget(log, area);
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let (text, color) = match &app.mode {
        Mode::Picker(picker) => match picker.error() {
            Some(error) => (format!("Error: {error}"), Color::Red),
            None => (PICKER_HINT.to_string(), Color::Yellow),
        },
        Mode::Confirm(_) => (
            "Press y to confirm, any other key to cancel".to_string(),
            Color::Yellow,
        ),
        _ => (NORMAL_HINT.to_string(), Color::Yellow),
    };
    let widget = Paragraph::new(text).style(Style::default().fg(color));
    frame.render_widget(widget, area);
}

fn render_picker(frame: &mut Frame, picker: &Picker, area: Rect) {
    let items: Vec<ListItem> = picker
        .entries()
        .iter()
        .map(|entry| {
            let suffix = if entry.kind == Kind::Dir { "/" } else { "" };
            ListItem::new(format!("{}{}", entry.name, suffix))
        })
        .collect();
    let title = if picker.filter().is_empty() {
        format!("Pick a path: {}", picker.dir().display())
    } else {
        format!(
            "Pick a path: {}  [filter: {}]",
            picker.dir().display(),
            picker.filter()
        )
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if !picker.entries().is_empty() {
        state.select(Some(picker.cursor()));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_confirm(frame: &mut Frame, kind: ConfirmKind, area: Rect) {
    let prompt = match kind {
        ConfirmKind::Quit => "Quit dbi-backend? [y/N]",
        ConfirmKind::Clear => "Clear the file list? [y/N]",
    };
    let popup = centered_fixed(prompt.len() as u16 + 2, 3, area);
    frame.render_widget(Clear, popup);
    let widget = Paragraph::new(prompt)
        .style(Style::default().fg(Color::Black).bg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Confirm"));
    frame.render_widget(widget, popup);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("a   open the path picker"),
        Line::from("d   remove the highlighted file"),
        Line::from("c   clear the file list (confirm)"),
        Line::from("s   start the server"),
        Line::from("j/k or arrows   move the highlight"),
        Line::from("q   quit (confirm)"),
        Line::from("?   toggle this help"),
        Line::from(""),
        Line::from("Picker: / filter, Enter open, Space add, Bksp up, Esc cancel"),
    ];
    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);
    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });
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

fn status_color(status: ServerStatus) -> Color {
    match status {
        ServerStatus::Idle => Color::Gray,
        ServerStatus::Running => Color::Green,
    }
}

fn log_color(message: &str) -> Color {
    if message.starts_with("Warning:") {
        Color::Yellow
    } else if message.starts_with("Failed")
        || message.starts_with("USB error")
        || message.starts_with("Connection lost")
    {
        Color::Red
    } else {
        Color::Gray
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
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
    fn ended_event_returns_to_idle_and_allows_restart() {
        let mut app = App::new(FileList::new());
        app.files.insert("a", "/tmp/a");

        assert_eq!(app.handle_key(press('s')), Action::Start);
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

        assert_eq!(app.handle_key(press('a')), Action::None);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.handle_key(press('c')), Action::None);
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
    fn status_header_is_colored_by_state() {
        let mut app = App::new(FileList::new());
        assert_eq!(buffer(&app, 40, 12).cell((0, 0)).unwrap().fg, Color::Gray);

        app.files.insert("a", "/tmp/a");
        app.handle_key(press('s'));
        assert_eq!(app.status, ServerStatus::Running);
        assert_eq!(buffer(&app, 40, 12).cell((0, 0)).unwrap().fg, Color::Green);
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
    fn progress_event_updates_and_clears() {
        let mut app = App::new(FileList::new());
        app.handle_server_event(ServerEvent::Started);
        app.handle_server_event(ServerEvent::Progress {
            name: "game.nsp".into(),
            sent: 50,
            total: 100,
        });

        let view = render(&app, 60, 12);
        assert!(view.contains("game.nsp"));
        assert!(view.contains("50%"));

        app.handle_server_event(ServerEvent::Ended);
        let view = render(&app, 60, 12);
        assert!(!view.contains("50%"));
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
                    if buf.cell((x, y)).unwrap().bg == Color::Yellow {
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
}
