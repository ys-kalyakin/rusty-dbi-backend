//! How a number, a duration or a path is written out for the terminal.

use std::time::Duration;

use ratatui::style::Color;

use crate::theme;

pub(crate) fn log_color(message: &str) -> Color {
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

pub(crate) fn format_elapsed(elapsed: Duration) -> String {
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

pub(crate) fn human_size(bytes: u64) -> String {
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
pub(crate) fn remaining_seconds(sent: u64, total: u64, speed: u64) -> u64 {
    if sent >= total || speed == 0 {
        0
    } else {
        (total - sent).div_ceil(speed)
    }
}

/// Truncate a path's display text from the middle, keeping both ends so the file
/// name tail stays visible. The ellipsis counts as one character; the result is
/// never wider than `max_width` characters.
pub(crate) fn abbrev_path(path: &str, max_width: usize) -> String {
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

/// A counted noun: "1 file" or "5 files". A zero count is plural, as English
/// words it.
pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    match count {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}
