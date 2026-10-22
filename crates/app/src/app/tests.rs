//! The application's tests: they drive the keys and read the rendered rows.

use super::format::{abbrev_path, log_color, remaining_seconds};
use super::server::ConnectLog;
use super::state::{FIELD_NAME, FIELD_SERVER, FIELD_SHARE, SHARE_FORM_FIELDS};
use super::ui::centered_fixed;
use super::*;
use crate::config::ShareConfig;
use crate::picker::Picker;
use crate::theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use dbi_backend_core::filelist::{AddOutcome, FileList};
use dbi_backend_core::filters::Filters;
use dbi_backend_core::metadata::ContentType;
use dbi_backend_core::protocol::{
    Header, CMD_ID_EXIT, CMD_ID_LIST, CMD_TYPE_ACK, CMD_TYPE_REQUEST,
};
use dbi_backend_core::server::{Server, ServerUpdate};
use dbi_backend_core::source::local::LocalStore;
use dbi_backend_core::source::{EntryKind, SmbLocation};
use dbi_backend_core::source::{ListEntry, Source, Sources};
use dbi_backend_core::transport::{Transport, TransportError};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Terminal;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// An app with no shares configured.
fn no_shares(files: FileList) -> App {
    App::with_shares(files, Vec::new())
}

fn press(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn app_with_picker(dir: &Path) -> App {
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
    for i in 0..(MAX_LOG_LINES + 50) {
        app.push_log(i.to_string());
    }
    assert_eq!(app.log.len(), MAX_LOG_LINES);
}

#[test]
fn q_opens_confirmation_and_y_quits() {
    let mut app = no_shares(FileList::new());
    assert_eq!(app.handle_key(press('q')), Action::None);
    assert!(matches!(app.mode, Mode::Confirm(ConfirmKind::Quit)));
    assert!(!app.should_quit);
    assert_eq!(app.handle_key(press('y')), Action::Quit);
    assert!(app.should_quit);
}

#[test]
fn q_confirmation_can_be_cancelled() {
    let mut app = no_shares(FileList::new());
    app.handle_key(press('q'));
    assert_eq!(app.handle_key(press('n')), Action::None);
    assert!(matches!(app.mode, Mode::Normal));
    assert!(!app.should_quit);
}

#[test]
fn c_requires_confirmation() {
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
    app.files.insert("a.nsp", "/tmp/a.nsp");
    app.handle_key(press('c'));
    app.handle_key(press('x'));
    assert_eq!(app.files.len(), 1);
    assert!(matches!(app.mode, Mode::Normal));
}

/// An app offering the given file names, all with a size.
fn app_with_files(names: &[&str]) -> App {
    let mut app = no_shares(FileList::new());
    for name in names {
        app.files.insert(*name, format!("/tmp/{name}"));
    }
    app
}

/// Type the given text into the file list's search.
fn type_search(app: &mut App, text: &str) {
    app.handle_key(press('/'));
    for c in text.chars() {
        app.handle_key(press(c));
    }
}

/// Run the add an action asks for, the way the caller does.
async fn run_add(app: &mut App, action: Action) {
    let Action::Add(source) = action else {
        panic!("the action adds a source: {action:?}");
    };
    app.add_source(&LocalStore::new(), &source).await;
}

/// The names the file list region shows.
fn visible_names(app: &App) -> Vec<String> {
    app.visible_files()
        .iter()
        .map(|(name, _)| (*name).clone())
        .collect()
}

#[test]
fn a_search_narrows_the_visible_list() {
    let mut app = app_with_files(&["game.nsp", "other.nsz"]);
    type_search(&mut app, "GAME");
    assert_eq!(visible_names(&app), vec!["game.nsp"]);
}

#[test]
fn a_search_with_no_matches_shows_an_empty_list() {
    let mut app = app_with_files(&["game.nsp"]);
    type_search(&mut app, "zzz");
    assert!(visible_names(&app).is_empty());
}

#[test]
fn clearing_the_search_restores_the_full_list() {
    let mut app = app_with_files(&["game.nsp", "other.nsz"]);
    type_search(&mut app, "game");
    assert_eq!(visible_names(&app).len(), 1);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(visible_names(&app).len(), 2);
}

#[test]
fn the_search_does_not_change_what_the_switch_receives() {
    let mut app = app_with_files(&["game.nsp", "other.nsz"]);
    type_search(&mut app, "game");
    assert_eq!(visible_names(&app).len(), 1);
    assert_eq!(
        app.files.len(),
        2,
        "the offered list keeps every file the scan accepted"
    );
}

#[test]
fn d_removes_the_visible_file() {
    let mut app = app_with_files(&["a.nsp", "b.nsp"]);
    type_search(&mut app, "b");
    // Enter keeps the search and closes the input; only then do the
    // editing keys act again.
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(press('d'));
    assert!(app.files.get("b.nsp").is_none(), "the visible file goes");
    assert!(
        app.files.get("a.nsp").is_some(),
        "the hidden file stays offered"
    );
}

#[test]
fn the_open_search_input_takes_the_keys() {
    let mut app = app_with_files(&["a.nsp"]);
    type_search(&mut app, "d");
    assert_eq!(app.search(), "d", "the key went into the search");
    assert!(
        app.files.get("a.nsp").is_some(),
        "typing does not remove a file"
    );
}

#[test]
fn the_highlight_stays_within_the_visible_list() {
    let mut app = app_with_files(&["a.nsp", "b.nsp"]);
    type_search(&mut app, "b");
    app.handle_key(press('j'));
    app.handle_key(press('k'));
    assert_eq!(app.selected, 0, "the highlight stays on the only match");
}

#[test]
fn the_search_works_while_the_server_runs() {
    let mut app = app_with_files(&["game.nsp"]);
    assert_eq!(app.handle_key(press('s')), Action::Start);
    app.handle_key(press('/'));
    assert!(app.is_searching(), "the search is a read-only view");
    app.handle_key(press('a'));
    assert!(
        matches!(app.mode, Mode::Normal),
        "the editing keys stay locked while the server runs"
    );
}

#[test]
fn the_add_log_line_names_the_accepted_and_rejected_counts() {
    let mut app = no_shares(FileList::new());
    let outcome = AddOutcome {
        added: vec!["a.nsp".into(), "b.nsp".into()],
        rejected: 5,
        ..AddOutcome::default()
    };

    app.log_add_outcome("/games", &outcome);

    let line = app.log.back().unwrap().message.clone();
    assert!(
        line.contains("2 files"),
        "the accepted count is named: {line}"
    );
    assert!(
        line.contains("5 rejected"),
        "the rejected count is named: {line}"
    );
}

#[test]
fn a_scan_that_rejects_everything_says_so() {
    let mut app = no_shares(FileList::new());
    let outcome = AddOutcome {
        rejected: 3,
        ..AddOutcome::default()
    };

    app.log_add_outcome("/games", &outcome);

    let line = app.log.back().unwrap().message.clone();
    assert!(line.contains("0 files"), "no file was accepted: {line}");
    assert!(
        line.contains("3 rejected"),
        "the rejected count is named: {line}"
    );
}

#[test]
fn the_picker_takes_the_search_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_picker(dir.path());
    app.handle_key(press('/'));
    assert!(!app.is_searching(), "the picker handles the key");
}

#[test]
fn the_file_list_renders_the_matching_rows() {
    let mut app = app_with_files(&["game.nsp", "other.nsz"]);
    type_search(&mut app, "game");
    let text = render(&app, 80, 20);
    assert!(text.contains("game.nsp"), "the match is shown: {text}");
    assert!(
        !text.contains("other.nsz"),
        "the hidden file is not shown: {text}"
    );
}

#[test]
fn the_hint_line_names_the_search_key() {
    let app = app_with_files(&["game.nsp"]);
    let text = render(&app, 100, 20);
    assert!(text.contains("search"), "the hint names the key: {text}");
}

#[test]
fn a_scan_without_a_match_says_so() {
    let mut app = app_with_files(&["game.nsp"]);
    type_search(&mut app, "zzz");
    let text = render(&app, 80, 20);
    assert!(
        text.contains("No file matches"),
        "the region explains itself: {text}"
    );
    assert!(!text.contains("game.nsp"), "no file is shown: {text}");
}

/// A transport that hands out the given reads and records the writes.
struct RecordingTransport {
    reads: std::collections::VecDeque<Vec<u8>>,
    writes: Vec<Vec<u8>>,
}

#[async_trait::async_trait]
impl Transport for RecordingTransport {
    async fn read(&mut self, _len: usize) -> Result<Vec<u8>, TransportError> {
        self.reads
            .pop_front()
            .ok_or_else(|| TransportError::Usb("no more reads".into()))
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.writes.push(data.to_vec());
        Ok(())
    }
}

#[tokio::test]
async fn the_switch_receives_the_whole_offered_list_while_a_search_narrows_the_view() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.nsp", "b.nsp", "notes.txt", "cover.jpg"] {
        fs::write(dir.path().join(name), name).unwrap();
    }

    let mut files = FileList::with_filters(Filters::with_extensions(["nsp"]));
    let outcome = files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();
    assert_eq!(outcome.added.len(), 2, "the scan accepted two files");
    assert_eq!(outcome.rejected, 2, "the filters rejected two files");

    let mut app = no_shares(files);
    type_search(&mut app, "a");
    assert_eq!(visible_names(&app), vec!["a.nsp"], "the view is narrowed");
    assert_eq!(
        app.files.len(),
        2,
        "the offered list keeps every file the scan accepted"
    );

    // The server answers a LIST with every offered name, not only the
    // matching one.
    let reads = vec![
        Header::new(CMD_TYPE_REQUEST, CMD_ID_LIST, 0)
            .to_bytes()
            .to_vec(),
        Header::new(CMD_TYPE_ACK, 0, 0).to_bytes().to_vec(),
        Header::new(CMD_TYPE_REQUEST, CMD_ID_EXIT, 0)
            .to_bytes()
            .to_vec(),
    ];
    let mut server = Server::new(
        RecordingTransport {
            reads: reads.into(),
            writes: Vec::new(),
        },
        app.files.clone(),
        Arc::new(LocalStore::new()),
    );
    server.run(|_update| {}).await.unwrap();
    let transport = server.into_transport();

    assert_eq!(transport.writes[1], b"a.nsp\nb.nsp\n");
}

#[test]
fn help_overlay_toggles_with_question_mark() {
    let mut app = no_shares(FileList::new());
    assert_eq!(app.handle_key(press('?')), Action::None);
    assert!(matches!(app.mode, Mode::Help));
    app.handle_key(press('?'));
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn s_starts_only_when_files_present_and_only_once() {
    let mut app = no_shares(FileList::new());
    assert_eq!(app.handle_key(press('s')), Action::None);

    app.files.insert("a", "/tmp/a");
    assert_eq!(app.handle_key(press('s')), Action::Start);
    assert_eq!(app.handle_key(press('s')), Action::None);
}

#[test]
fn s_sets_waiting_until_the_device_connects() {
    let mut app = no_shares(FileList::new());
    app.files.insert("a", "/tmp/a");

    assert_eq!(app.handle_key(press('s')), Action::Start);
    assert_eq!(app.status, ServerStatus::Waiting);
}

#[test]
fn started_makes_running_and_disconnected_returns_to_waiting() {
    let mut app = no_shares(FileList::new());
    app.files.insert("a", "/tmp/a");
    app.handle_key(press('s'));

    app.handle_server_event(ServerEvent::Started);
    assert_eq!(app.status, ServerStatus::Running);

    app.handle_server_event(ServerEvent::Disconnected);
    assert_eq!(app.status, ServerStatus::Waiting);
}

#[test]
fn progress_does_not_survive_disconnect_and_reconnect() {
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
    for i in 0..10 {
        app.push_log(format!("line-{i}"));
    }

    let view = render(&app, 40, 12);
    assert!(view.contains("line-9"));
    assert!(!view.contains("line-0"));
}

#[test]
fn a_opens_the_picker() {
    let mut app = no_shares(FileList::new());
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

#[tokio::test]
async fn selecting_a_file_adds_it_directly() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("game.nsp"), b"g").unwrap();

    let mut app = app_with_picker(dir.path());
    let action = app.handle_key(key(KeyCode::Char(' ')));
    assert!(matches!(app.mode, Mode::Normal));
    run_add(&mut app, action).await;
    assert!(app.files.get("game.nsp").is_some());
}

#[tokio::test]
async fn selecting_an_existing_name_logs_a_replacement_warning() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("game.nsp"), b"g").unwrap();

    let mut app = app_with_picker(dir.path());
    app.files.insert("game.nsp", "/elsewhere/game.nsp");
    let action = app.handle_key(key(KeyCode::Char(' ')));
    run_add(&mut app, action).await;

    assert!(app.log.iter().any(|l| l.message.contains("replaced")));
}

#[tokio::test]
async fn enter_on_a_file_adds_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("game.nsp"), b"g").unwrap();

    let mut app = app_with_picker(dir.path());
    let action = app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::Normal));
    run_add(&mut app, action).await;
    assert!(app.files.get("game.nsp").is_some());
}

#[tokio::test]
async fn selecting_a_directory_adds_its_contents_recursively() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("titles");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("a.nsp"), b"a").unwrap();

    let mut app = app_with_picker(dir.path());
    let action = app.handle_key(key(KeyCode::Char(' ')));
    assert!(matches!(app.mode, Mode::Normal));
    run_add(&mut app, action).await;
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
    app.files.insert("a.nsp", "/tmp/a.nsp");
    assert_eq!(app.handle_key(press('s')), Action::Start);
    app.handle_key(press('d'));
    assert_eq!(app.files.len(), 1);
}

#[tokio::test]
async fn the_flow_reads_the_identities_and_keeps_the_offer_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Game [0100000000010000][v65536].nsp"), b"a").unwrap();
    fs::write(dir.path().join("Update [0100000000010800].nsp"), b"b").unwrap();
    fs::write(dir.path().join("notes.txt"), b"c").unwrap();

    let mut files = FileList::new();
    let outcome = files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();
    assert_eq!(outcome.added.len(), 2, "two titles were accepted");
    assert_eq!(outcome.rejected, 1, "the notes file was rejected");

    let app = no_shares(files);
    let base = app
        .files
        .get("Game [0100000000010000][v65536].nsp")
        .unwrap();
    assert_eq!(base.identity.title_id.as_deref(), Some("0100000000010000"));
    assert_eq!(base.identity.content_type, Some(ContentType::Base));
    assert_eq!(base.identity.version, Some((1, 0, 0)));
    let update = app.files.get("Update [0100000000010800].nsp").unwrap();
    assert_eq!(
        update.identity.title_id.as_deref(),
        Some("0100000000010800")
    );
    assert_eq!(update.identity.content_type, Some(ContentType::Update));

    // The offer to the Switch is unchanged: the same names, in the same
    // order, whatever the identities say.
    let reads = vec![
        Header::new(CMD_TYPE_REQUEST, CMD_ID_LIST, 0)
            .to_bytes()
            .to_vec(),
        Header::new(CMD_TYPE_ACK, 0, 0).to_bytes().to_vec(),
        Header::new(CMD_TYPE_REQUEST, CMD_ID_EXIT, 0)
            .to_bytes()
            .to_vec(),
    ];
    let mut server = Server::new(
        RecordingTransport {
            reads: reads.into(),
            writes: Vec::new(),
        },
        app.files.clone(),
        Arc::new(LocalStore::new()),
    );
    server.run(|_update| {}).await.unwrap();
    let transport = server.into_transport();

    assert_eq!(
        transport.writes[1],
        b"Game [0100000000010000][v65536].nsp\nUpdate [0100000000010800].nsp\n"
    );
}

#[tokio::test]
async fn the_file_list_shows_the_title_identity() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Game [0100000000010000][v65536].nsp"), b"x").unwrap();
    let mut app = no_shares(FileList::new());
    app.files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();

    let view = render(&app, 120, 12);
    assert!(view.contains("0100000000010000"), "the id is shown: {view}");
    assert!(view.contains("base"), "the kind is shown: {view}");
    assert!(view.contains("1.0.0"), "the version is shown: {view}");
}

#[test]
fn a_file_without_an_identity_shows_only_its_name() {
    let mut app = no_shares(FileList::new());
    app.files.insert("plain.nsp", "/tmp/plain.nsp");

    let view = render(&app, 120, 12);
    assert!(view.contains("plain.nsp"), "the name is shown: {view}");
    assert!(!view.contains("base"), "no kind is shown: {view}");
    assert!(!view.contains("1.0.0"), "no version is shown: {view}");
}

#[tokio::test]
async fn a_narrow_terminal_drops_the_identity_but_keeps_the_name() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Game [0100000000010000][v65536].nsp"), b"x").unwrap();
    let mut app = no_shares(FileList::new());
    app.files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();

    let view = render(&app, 40, 12);
    assert!(view.contains("Game"), "the name stays: {view}");
    assert!(!view.contains("1.0.0"), "the identity is dropped: {view}");
}

#[tokio::test]
async fn files_list_shows_sizes_and_total() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.nsp"), vec![0u8; 2048]).unwrap();
    let mut app = no_shares(FileList::new());
    app.files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();

    let view = render(&app, 80, 12);
    assert!(view.contains("2.0 KiB"));
    assert!(view.contains("total: 2.0 KiB"));
}

#[test]
fn empty_file_list_shows_an_onboarding_hint() {
    let empty = no_shares(FileList::new());
    let view = render(&empty, 60, 12);
    assert!(view.contains("No files yet"));
    assert!(view.contains("press `a` to add"));
    assert!(view.contains("No files yet — press `a` to add"));

    let mut populated = no_shares(FileList::new());
    populated.files.insert("a.nsp", "/tmp/a.nsp");
    let view = render(&populated, 60, 12);
    assert!(!view.contains("No files yet"));
}

#[tokio::test]
async fn size_column_is_right_aligned_in_the_files_table() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.nsp"), vec![0u8; 1024]).unwrap();
    fs::write(dir.path().join("verylonggamename.nsp"), vec![0u8; 2048]).unwrap();
    let mut app = no_shares(FileList::new());
    app.files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();

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
    let app = no_shares(FileList::new());
    let view = render(&app, 80, 12);
    assert!(view.contains(theme::APP_NAME));
    assert!(view.contains("idle"));
    assert!(view.contains("0 files · total: 0 B"));

    let mut app = no_shares(FileList::new());
    app.files.insert("a.nsp", "/tmp/a.nsp");
    app.files.insert("b.nsp", "/tmp/b.nsp");
    let view = render(&app, 80, 12);
    assert!(view.contains("2 files"));
}

#[test]
fn hint_chips_dim_locked_keys_while_the_server_is_active() {
    let idle = no_shares(FileList::new());
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

    let mut active = no_shares(FileList::new());
    active.files.insert("a", "/tmp/a");
    active.handle_key(press('s'));
    assert!(active.status != ServerStatus::Idle);
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
    let app = no_shares(FileList::new());
    // x=13 is the status capsule: "dbi-backend" (11 chars) + two spaces.
    assert_eq!(buffer(&app, 80, 12).cell((13, 0)).unwrap().fg, Color::Gray);
    let view = render(&app, 80, 12);
    assert!(view.contains("idle"));

    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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

    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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

#[tokio::test]
async fn narrow_terminal_truncates_long_paths() {
    let dir = tempfile::tempdir().unwrap();
    let long_name = "a-nsp-file-with-a-remarkably-long-name.nsp";
    fs::write(dir.path().join(long_name), vec![0u8; 2048]).unwrap();
    let mut app = no_shares(FileList::new());
    app.files
        .add_path(dir.path(), &LocalStore::new())
        .await
        .unwrap();

    let view = render(&app, 40, 12);
    assert!(view.contains('…'));
}

#[test]
fn picker_rows_show_file_sizes_but_not_directory_sizes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.nsp"), vec![0u8; 1024]).unwrap();
    fs::write(dir.path().join("verybig.nsp"), vec![0u8; 2048]).unwrap();
    fs::create_dir(dir.path().join("games")).unwrap();
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
    app.mode = Mode::Picker(Picker::at(dir.path()));

    let view = render(&app, 80, 12);
    assert!(view.contains("· 2 entries"));
}

#[test]
fn picker_title_shows_the_match_count_while_filtering() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("alpha.nsp"), b"a").unwrap();
    fs::write(dir.path().join("beta.nsp"), b"b").unwrap();
    let mut app = no_shares(FileList::new());
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
        let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
        Action::Connect { share, password } => {
            assert_eq!(share, "Games");
            assert_eq!(password, "secret");
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
    let mut app = no_shares(FileList::new());
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
    let mut app = no_shares(FileList::new());
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
                form.errors()
                    .iter()
                    .any(|error| error.contains("Games") && error.contains("already configured")),
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
