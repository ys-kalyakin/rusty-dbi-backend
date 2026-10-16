mod app;
mod cli;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use crossterm::event::Event;
use dbi_backend_core::filelist::FileList;
use tokio::sync::mpsc;

use app::{Action, App};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut files = FileList::new();
    for path in &cli.paths {
        if let Err(e) = files.add_path(path) {
            eprintln!("warning: failed to add {}: {e}", path.display());
        }
    }

    let mut app = App::new(files);

    let (input_tx, mut input_rx) = mpsc::channel::<Event>(64);
    std::thread::spawn(move || input_thread(input_tx));

    let (log_tx, mut log_rx) = mpsc::unbounded_channel::<String>();

    let mut terminal = ratatui::init();
    let mut ticker = tokio::time::interval(Duration::from_millis(200));

    loop {
        terminal.draw(|frame| app::ui(frame, &app))?;

        tokio::select! {
            maybe_event = input_rx.recv() => {
                if let Some(Event::Key(key)) = maybe_event {
                    match app.handle_key(key) {
                        Action::Start => app::spawn_server(app.files.clone(), log_tx.clone()),
                        Action::Quit => break,
                        Action::None => {}
                    }
                }
            }
            Some(line) = log_rx.recv() => app.push_log(line),
            _ = ticker.tick() => {}
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}

fn input_thread(tx: mpsc::Sender<Event>) {
    while let Ok(event) = crossterm::event::read() {
        if tx.blocking_send(event).is_err() {
            break;
        }
    }
}
