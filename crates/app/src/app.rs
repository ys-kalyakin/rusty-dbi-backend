//! TUI application state and the background server task.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::picker::{Kind, Picker};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::server::Server;
use dbi_backend_core::transport::usb::UsbTransport;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
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

/// A lifecycle transition reported by the background server task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerEvent {
    Started,
    Ended,
}

/// Which view is active.
pub enum Mode {
    Normal,
    Picker(Picker),
}

pub struct App {
    pub files: FileList,
    pub log: VecDeque<String>,
    pub status: ServerStatus,
    pub mode: Mode,
    pub should_quit: bool,
    server_started: bool,
}

impl App {
    pub fn new(files: FileList) -> Self {
        Self {
            files,
            log: VecDeque::new(),
            status: ServerStatus::Idle,
            mode: Mode::Normal,
            should_quit: false,
            server_started: false,
        }
    }

    pub fn push_log(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > MAX_LOG_LINES {
            self.log.pop_front();
        }
    }

    /// Apply a lifecycle transition from the background server task.
    pub fn handle_server_event(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::Started => self.status = ServerStatus::Running,
            ServerEvent::Ended => {
                self.status = ServerStatus::Idle;
                self.server_started = false;
            }
        }
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
            match key.code {
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
        };

        match outcome {
            Outcome::Select(path) => self.select_path(&path),
            Outcome::Cancel => self.mode = Mode::Normal,
            Outcome::None => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }

        if let Mode::Picker(_) = self.mode {
            self.handle_picker_key(key);
            return Action::None;
        }

        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                Action::Quit
            }
            KeyCode::Char('a') => {
                if self.status == ServerStatus::Running {
                    Action::None
                } else {
                    self.mode = Mode::Picker(Picker::open());
                    Action::None
                }
            }
            KeyCode::Char('c') => {
                if self.status == ServerStatus::Running {
                    Action::None
                } else {
                    self.files.clear();
                    self.push_log("List cleared".into());
                    Action::None
                }
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

/// Spawn the background server loop. Logs are forwarded to `log_tx` and
/// lifecycle transitions to `event_tx`.
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
            let tx = log_tx.clone();
            let mut log = move |line: String| {
                let _ = tx.send(line);
            };
            match server.run(&mut log).await {
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
const PICKER_HINT: &str = "Esc cancel  Enter open  Space add  Bksp up";

pub fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_log(frame, app, chunks[1]);

    match &app.mode {
        Mode::Picker(picker) => {
            render_picker(frame, picker, chunks[0]);
            let status = match picker.error() {
                Some(e) => format!("Error: {e}"),
                None => PICKER_HINT.to_string(),
            };
            let widget = Paragraph::new(status).style(Style::default().fg(Color::Yellow));
            frame.render_widget(widget, chunks[2]);
        }
        Mode::Normal => {
            let items: Vec<ListItem> = app
                .files
                .iter()
                .map(|(name, path)| ListItem::new(format!("{}  ->  {}", name, path.display())))
                .collect();
            let files =
                List::new(items).block(Block::default().borders(Borders::ALL).title("Files"));
            frame.render_widget(files, chunks[0]);

            let state = match app.status {
                ServerStatus::Idle => "idle",
                ServerStatus::Running => "running",
            };
            let status = format!(
                "[a]dd  [c]lear  [s]tart  [q]uit | files: {} | status: {}",
                app.files.len(),
                state
            );
            let widget = Paragraph::new(status).style(Style::default().fg(Color::Yellow));
            frame.render_widget(widget, chunks[2]);
        }
    }
}

fn render_log(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let start = app.log.len().saturating_sub(inner_height);
    let log_lines: Vec<Line> = app
        .log
        .iter()
        .skip(start)
        .map(|l| Line::from(l.clone()))
        .collect();
    let log = Paragraph::new(log_lines)
        .block(Block::default().borders(Borders::ALL).title("Log"))
        .wrap(Wrap { trim: true });
    frame.render_widget(log, area);
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
    let title = format!("Pick a path: {}", picker.dir().display());
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

    fn render(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| ui(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
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
    fn q_requests_quit() {
        let mut app = App::new(FileList::new());
        assert_eq!(app.handle_key(press('q')), Action::Quit);
        assert!(app.should_quit);
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

        assert!(app.log.iter().any(|l| l.contains("replaced")));
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
}
