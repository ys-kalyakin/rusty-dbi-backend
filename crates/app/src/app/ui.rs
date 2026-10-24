//! The rendering: what the terminal shows for each view.

use dbi_backend_core::metadata::TitleIdentity;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Gauge, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

use std::time::Duration;

use super::format::{abbrev_path, format_elapsed, human_size, log_color, plural};
use super::state::{
    App, ConfirmKind, HttpServerStatus, Mode, PasswordModal, ServerStatus, ShareFormState,
    ShareList, SHARE_FORM_FIELDS,
};
use crate::picker::Picker;
use crate::theme;
use dbi_backend_core::source::EntryKind;

/// Hint line shown while the picker is open.
const PICKER_HINT: &str = "Esc cancel  Enter open  Space add  Bksp up  / filter";

/// Hint line shown while the share list is open.
const SHARES_HINT: &str = "Esc cancel  Enter connect  a add share";

/// Hint line shown while the search input is open.
const SEARCH_HINT: &str = "Esc clear  Enter keep  Bksp erase";

/// Hint line shown while the add-share form is open.
const SHARE_FORM_HINT: &str = "Tab next  Enter save  Esc cancel";

/// The normal-view hint broken into keyed chip pairs: bracketed key text, label
/// remainder, and the key character (for locked-out detection).
const NORMAL_ACTIONS: [(&str, &str, char); 9] = [
    ("[a]", "dd", 'a'),
    ("[d]", "elete", 'd'),
    ("[c]", "lear", 'c'),
    ("[s]", "tart", 's'),
    ("[H]", "ttp", 'H'),
    ("[S]", "hares", 'S'),
    ("[/]", "search", '/'),
    ("[q]", "uit", 'q'),
    ("[?]", " help", '?'),
];

/// The normal-mode hint as keyed chips. While either server task is active
/// the locked-out editing keys (`a`/`d`/`c`/`S`) are rendered dimmed.
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
    let either_running =
        app.status == ServerStatus::Running || app.http == HttpServerStatus::Running;
    let show_progress = either_running && app.progress.is_some();

    let mut constraints = vec![Constraint::Length(1), Constraint::Min(3)];
    if show_progress {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Max(LOG_PANE_MAX_HEIGHT));
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
    let count = plural(app.files.len(), "file", "files");
    let mut right_spans = vec![Span::raw(format!(
        "{count} · total: {}",
        human_size(app.files.total_size())
    ))];
    match app.http {
        HttpServerStatus::Running => {
            let url = app.http_url.as_deref().unwrap_or_default();
            right_spans.push(Span::raw("  "));
            right_spans.push(Span::styled(
                format!("● http {url}"),
                theme::fg(theme::SUCCESS),
            ));
        }
        HttpServerStatus::Idle => {
            right_spans.push(Span::raw("  "));
            right_spans.push(Span::styled("○ http idle", theme::muted()));
        }
    }
    frame.render_widget(Paragraph::new(left), halves[0]);
    frame.render_widget(
        Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right),
        halves[1],
    );
}

fn render_files(frame: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_files();
    if visible.is_empty() {
        if app.files.is_empty() {
            render_empty_files_hint(frame, area);
        } else {
            render_no_match_hint(frame, area);
        }
        return;
    }
    // The identity needs room of its own; below that width it is dropped
    // rather than squeezing the name out of the list.
    let show_identity = area.width >= IDENTITY_MIN_WIDTH;
    let mut widths = vec![Constraint::Min(4), Constraint::Length(10)];
    if show_identity {
        widths.push(Constraint::Length(IDENTITY_WIDTH));
    }
    widths.push(Constraint::Min(8));
    let rows: Vec<Row> = visible
        .iter()
        .map(|(name, entry)| {
            let mut cells = vec![
                Cell::from(Span::styled((*name).clone(), theme::text())),
                Cell::from(
                    Line::styled(human_size(entry.size), theme::muted())
                        .alignment(Alignment::Right),
                ),
            ];
            if show_identity {
                cells.push(Cell::from(Span::styled(
                    identity_text(&entry.identity),
                    theme::muted(),
                )));
            }
            cells.push(Cell::from(Span::styled(
                abbrev_path(
                    &entry.source.to_string(),
                    (area.width as usize).saturating_sub(20),
                ),
                theme::muted(),
            )));
            Row::new(cells)
        })
        .collect();
    let selected = (!visible.is_empty()).then(|| app.selected.min(visible.len() - 1));
    render_table(frame, "Files".to_string(), rows, widths, selected, area);
}

/// The one way a centered hint is drawn against a titled list block: the
/// three-band layout that puts the message one line above the middle.
fn render_centered_hint(frame: &mut Frame, title: &'static str, text: String, area: Rect) {
    let block = theme::block_title(title);
    let inner = block.inner(area);
    let bands = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(1),
            Constraint::Percentage(55),
        ])
        .split(inner);
    let hint = Paragraph::new(Span::styled(text, theme::muted())).alignment(Alignment::Center);
    frame.render_widget(block, area);
    frame.render_widget(hint, bands[1]);
}

/// The one way a list is drawn: a titled table with the column spacing, the
/// highlight style and the `>` cursor the app uses everywhere, with the
/// caller's own widths, rows and clamped selection.
fn render_table(
    frame: &mut Frame,
    title: String,
    rows: Vec<Row>,
    widths: impl IntoIterator<Item = Constraint>,
    selected: Option<usize>,
    area: Rect,
) {
    let table = Table::new(rows, widths)
        .block(theme::block_title(title))
        .column_spacing(2)
        .row_highlight_style(theme::selection())
        .highlight_symbol("> ");
    let mut state = TableState::default();
    state.select(selected);
    frame.render_stateful_widget(table, area, &mut state);
}

/// A centered hint shown when the search matches none of the offered files.
fn render_no_match_hint(frame: &mut Frame, area: Rect) {
    render_centered_hint(
        frame,
        "Files",
        "No file matches the search".to_string(),
        area,
    );
}

/// A centered onboarding hint shown when no files are offered (picker closed).
fn render_empty_files_hint(frame: &mut Frame, area: Rect) {
    render_centered_hint(
        frame,
        "Files",
        "No files yet — press `a` to add files".to_string(),
        area,
    );
}

/// The width the identity column takes: the title id, the kind of content and
/// the version, separated by spaces.
const IDENTITY_WIDTH: u16 = 28;

/// The narrowest terminal that still shows the identity column.
const IDENTITY_MIN_WIDTH: u16 = 64;

/// The identity as the list shows it: the title id, the kind of content and
/// the version. A file without an identity shows nothing.
fn identity_text(identity: &TitleIdentity) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(title_id) = &identity.title_id {
        parts.push(title_id.clone());
    }
    if let Some(content_type) = identity.content_type {
        parts.push(content_type.label().to_string());
    }
    if let Some((major, minor, patch)) = identity.version {
        parts.push(format!("{major}.{minor}.{patch}"));
    }
    parts.join(" ")
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

/// The most rows the log pane ever takes: its titled border plus up to 6 log
/// lines. Every row beyond it belongs to the primary region; on a terminal
/// too short for this, the log yields first.
const LOG_PANE_MAX_HEIGHT: u16 = 8;

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
        Mode::Normal if app.is_searching() => Line::from(vec![
            Span::styled(format!("search: {}", app.search()), theme::accent()),
            Span::raw("  "),
            Span::styled(SEARCH_HINT, theme::muted()),
        ]),
        _ => {
            normal_hint_line(app.status != ServerStatus::Idle || app.http != HttpServerStatus::Idle)
        }
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_picker(frame: &mut Frame, picker: &Picker, area: Rect) {
    let rows: Vec<Row> = picker
        .entries()
        .iter()
        .map(|entry| {
            let (name, name_style, size) = match entry.kind {
                EntryKind::Dir => (format!("{}/", entry.name), theme::accent(), String::new()),
                EntryKind::File => (
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
        plural(picker.entries().len(), "entry", "entries")
    } else {
        plural(picker.entries().len(), "match", "matches")
    };
    // Keep the count (and filter) inside the panel however long the path is.
    let reserve = if picker.filter().is_empty() { 32 } else { 48 };
    let origin = abbrev_path(
        &picker.origin().display().to_string(),
        (area.width as usize).saturating_sub(reserve),
    );
    let title = if picker.is_busy() {
        format!("Pick a path: {origin} · listing…")
    } else if picker.error().is_some() {
        format!("Pick a path: {origin} · {}", picker.error().unwrap())
    } else if picker.filter().is_empty() {
        format!("Pick a path: {origin} · {count}")
    } else {
        format!(
            "Pick a path: {origin}  [filter: {}] · {count}",
            picker.filter()
        )
    };
    let widths = [Constraint::Min(8), Constraint::Length(10)];
    let selected = (!picker.entries().is_empty()).then(|| picker.cursor());
    render_table(frame, title, rows, widths, selected, area);
}

/// The share list: one row per configured share with its details, or a hint
/// naming the config file when nothing is configured. Renders in place of the
/// Files table (the server is idle while it is open).
fn render_shares(frame: &mut Frame, app: &App, list: &ShareList, area: Rect) {
    if app.shares.is_empty() {
        match &app.config_hint {
            Some(path) => render_centered_hint(
                frame,
                "Shares",
                format!(
                    "No shares configured — press `a` to add one (saved to {})",
                    abbrev_path(
                        &path.display().to_string(),
                        (area.width as usize).saturating_sub(58)
                    )
                ),
                area,
            ),
            None => render_centered_hint(
                frame,
                "Shares",
                "No shares configured — press `a` to add one (no config path known)".to_string(),
                area,
            ),
        }
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
    let count = plural(app.shares.len(), "share", "shares");
    render_table(
        frame,
        format!("Shares · {count}"),
        rows,
        [
            Constraint::Min(8),
            Constraint::Min(12),
            Constraint::Length(12),
        ],
        Some(list.selected.min(app.shares.len() - 1)),
        area,
    );
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
        Line::from("s   start the USB server"),
        Line::from("H   start/stop the HTTP server"),
        Line::from("S   list configured shares"),
        Line::from("Share list: Enter connect, a add share, Esc cancel"),
        Line::from("j/k or arrows   move the highlight"),
        Line::from("/   search the file list"),
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

pub(crate) fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
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
