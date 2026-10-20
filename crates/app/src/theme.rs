//! Shared visual palette and block styling for the TUI.
//!
//! Every widget pulls its colors from this module so the interface stays visually
//! coherent. The palette uses ANSI-16 colors so terminals without extended color
//! support still render sensibly.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType};

/// The application name shown in the header.
pub const APP_NAME: &str = "dbi-backend";

/// Default foreground color for regular text.
pub const TEXT: Color = Color::Reset;
/// Soft foreground color for bulk text that should not dominate (log lines).
pub const SUBTLE: Color = Color::Gray;
/// Dimmed foreground color for secondary details (paths, labels, counts).
pub const MUTED: Color = Color::DarkGray;
/// Accent color for interactive elements: keys, directories, the app name.
pub const ACCENT: Color = Color::Cyan;
/// Positive or connected states.
pub const SUCCESS: Color = Color::Green;
/// Attention states: warnings, hints, waiting for the device.
pub const WARNING: Color = Color::Yellow;
/// Failure states.
pub const DANGER: Color = Color::Red;
/// Idle server state.
pub const IDLE: Color = Color::Gray;

/// Foreground of the highlighted row in the file list and picker.
pub const SELECTION_FG: Color = Color::Black;
/// Background of the highlighted row in the file list and picker.
pub const SELECTION_BG: Color = Color::Cyan;

/// Spinner frames shown while waiting for the device; one frame per 200 ms tick.
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Style for regular text.
pub fn text() -> Style {
    Style::default().fg(TEXT)
}

/// Style for secondary details.
pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

/// Style for disabled elements (also dims for terminals that support it).
pub fn muted_dim() -> Style {
    Style::default().fg(MUTED).add_modifier(Modifier::DIM)
}

/// Style for interactive elements.
pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

/// Style for the highlighted row in the file list and picker.
pub fn selection() -> Style {
    Style::default()
        .fg(SELECTION_FG)
        .bg(SELECTION_BG)
        .add_modifier(Modifier::BOLD)
}

/// Style with an explicit foreground color.
pub fn fg(color: Color) -> Style {
    Style::default().fg(color)
}

/// A rounded block with a muted title, used by every bordered region.
pub fn block_title(title: impl Into<Span<'static>>) -> Block<'static> {
    let mut span = title.into();
    span.style = muted();
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(span)
}
