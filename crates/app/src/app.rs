//! TUI application state and the background server task.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::server::Server;
use dbi_backend_core::transport::usb::UsbTransport;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

/// Maximum number of log lines retained (mirrors the Python 1000-line cap).
pub const MAX_LOG_LINES: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Idle,
    Running,
}

/// What the caller should do after a key press.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Start,
    Quit,
}

pub struct App {
    pub files: FileList,
    pub log: VecDeque<String>,
    pub status: ServerStatus,
    pub input: Option<String>,
    pub should_quit: bool,
    server_started: bool,
}

impl App {
    pub fn new(files: FileList) -> Self {
        Self {
            files,
            log: VecDeque::new(),
            status: ServerStatus::Idle,
            input: None,
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

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }

        if let Some(input) = self.input.as_mut() {
            match key.code {
                KeyCode::Enter => {
                    let raw = self.input.take().unwrap();
                    let trimmed = raw.trim().to_string();
                    if !trimmed.is_empty() {
                        match self.files.add_path(Path::new(&trimmed)) {
                            Ok(()) => self.push_log(format!("Added: {trimmed}")),
                            Err(e) => self.push_log(format!("Failed to add {trimmed}: {e}")),
                        }
                    }
                }
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                Action::Quit
            }
            KeyCode::Char('a') => {
                self.input = Some(String::new());
                Action::None
            }
            KeyCode::Char('c') => {
                self.files.clear();
                self.push_log("List cleared".into());
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

/// Spawn the background server loop. Logs are forwarded to `log_tx`.
pub fn spawn_server(files: FileList, log_tx: UnboundedSender<String>) {
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

            let mut server = Server::new(transport, files.clone());
            let tx = log_tx.clone();
            let mut log = move |line: String| {
                let _ = tx.send(line);
            };
            match server.run(&mut log).await {
                Ok(()) => {
                    let _ = log_tx.send("Server stopped".into());
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

pub fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|(name, path)| ListItem::new(format!("{}  ->  {}", name, path.display())))
        .collect();
    let files = List::new(items).block(Block::default().borders(Borders::ALL).title("Files"));
    frame.render_widget(files, chunks[0]);

    let log_lines: Vec<Line> = app.log.iter().map(|l| Line::from(l.clone())).collect();
    let log = Paragraph::new(log_lines)
        .block(Block::default().borders(Borders::ALL).title("Log"))
        .wrap(Wrap { trim: true });
    frame.render_widget(log, chunks[1]);

    let status = if let Some(input) = &app.input {
        format!("Add path: {input}")
    } else {
        let state = match app.status {
            ServerStatus::Idle => "idle",
            ServerStatus::Running => "running",
        };
        format!(
            "[a]dd  [c]lear  [s]tart  [q]uit | files: {} | status: {}",
            app.files.len(),
            state
        )
    };
    let status_widget = Paragraph::new(status).style(Style::default().fg(Color::Yellow));
    frame.render_widget(status_widget, chunks[2]);
}
