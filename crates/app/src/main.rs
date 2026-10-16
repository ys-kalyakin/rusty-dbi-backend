mod app;
mod cli;
mod picker;

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use crossterm::event::Event;
use dbi_backend_core::filelist::FileList;
use tokio::sync::mpsc;

use app::{Action, App, ServerEvent};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut files = FileList::new();
    for path in &cli.paths {
        match files.add_path(path) {
            Ok(outcome) => {
                for skipped in &outcome.skipped {
                    eprintln!("warning: skipped {}: cannot be read", skipped.display());
                }
                for replaced in &outcome.replaced {
                    eprintln!("warning: {replaced} replaced a previously added file");
                }
            }
            Err(e) => eprintln!("warning: failed to add {}: {e}", path.display()),
        }
    }

    let mut app = App::new(files);

    let (input_tx, mut input_rx) = mpsc::channel::<Event>(64);
    std::thread::spawn(move || input_thread(input_tx));

    let (log_tx, mut log_rx) = mpsc::unbounded_channel::<String>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<ServerEvent>();

    let mut terminal = ratatui::init();
    let mut ticker = tokio::time::interval(Duration::from_millis(200));

    loop {
        if let Err(e) = terminal.draw(|frame| app::ui(frame, &app)) {
            ratatui::restore();
            return Err(e.into());
        }

        tokio::select! {
            maybe_event = input_rx.recv() => {
                match maybe_event {
                    Some(Event::Key(key)) => match app.handle_key(key) {
                        Action::Start => {
                            app::spawn_server(app.files.clone(), log_tx.clone(), event_tx.clone())
                        }
                        Action::Quit => break,
                        Action::None => {}
                    },
                    Some(_) => {}
                    None => break,
                }
            }
            Some(line) = log_rx.recv() => app.push_log(line),
            Some(event) = event_rx.recv() => app.handle_server_event(event),
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
