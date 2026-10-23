//! The terminal UI: the state it shows, the keys it answers and the
//! background server tasks that talk to the Switch — over USB and over HTTP.

mod format;
mod input;
mod server;
mod state;
mod ui;

#[cfg(test)]
mod tests;

pub use server::{spawn_http_server, spawn_server};
pub use state::{Action, App, ServerEvent};
pub use ui::ui;

#[cfg(test)]
pub use state::{
    ConfirmKind, HttpServerStatus, Mode, PasswordModal, ServerStatus, ShareList, MAX_LOG_LINES,
};
