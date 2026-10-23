//! The terminal UI: the state it shows, the keys it answers and the
//! background server task that talks to the Switch.

mod format;
mod input;
mod server;
mod state;
mod ui;

#[cfg(test)]
mod tests;

pub use server::spawn_server;
pub use state::{Action, App, ServerEvent};
pub use ui::ui;

#[cfg(test)]
pub use state::{ConfirmKind, Mode, PasswordModal, ServerStatus, ShareList, MAX_LOG_LINES};
