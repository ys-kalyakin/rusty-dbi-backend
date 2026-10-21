mod app;
mod cli;
mod config;
mod picker;
mod theme;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use crossterm::event::Event;
use dbi_backend_core::filelist::FileList;
use dbi_backend_core::source::smb::{ShareHub, Smb2SessionFactory};
use dbi_backend_core::source::CombinedStore;
use dbi_backend_core::source::Source;
use tokio::sync::mpsc;

use app::{Action, App, ServerEvent};
use config::load_user_config;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut files = FileList::new();
    for path in &cli.paths {
        match files.add_path(path) {
            Ok(outcome) => {
                for skipped in &outcome.skipped {
                    eprintln!("warning: skipped {skipped}: cannot be read");
                }
                for replaced in &outcome.replaced {
                    eprintln!("warning: {replaced} replaced a previously added file");
                }
            }
            Err(e) => eprintln!("warning: failed to add {}: {e}", path.display()),
        }
    }

    let loaded = load_user_config();
    for warning in &loaded.warnings {
        eprintln!("warning: {warning}");
    }
    let shares = Arc::new(tokio::sync::Mutex::new(ShareHub::new(
        Arc::new(Smb2SessionFactory),
        loaded.shares.clone(),
    )));
    let store: Arc<dyn dbi_backend_core::source::Sources> =
        Arc::new(CombinedStore::new(shares.clone()));

    let mut app = App::with_shares(files, loaded.shares);

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
                    Some(Event::Key(key)) => {
                        let action = app.handle_key(key);
                        if run_action(
                            action,
                            &mut app,
                            &shares,
                            store.clone(),
                            &log_tx,
                            &event_tx,
                        )
                        .await
                        {
                            break;
                        }
                    }
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

/// Run the async consequences of a key action: share connects, share listings,
/// and share adds. Returns `true` when the app should quit.
async fn run_action(
    action: Action,
    app: &mut App,
    shares: &Arc<tokio::sync::Mutex<ShareHub>>,
    store: Arc<dyn dbi_backend_core::source::Sources>,
    log_tx: &mpsc::UnboundedSender<String>,
    event_tx: &mpsc::UnboundedSender<ServerEvent>,
) -> bool {
    match action {
        Action::None => {}
        Action::Start => {
            app::spawn_server(
                app.files.clone(),
                store.clone(),
                log_tx.clone(),
                event_tx.clone(),
            );
        }
        Action::Quit => return true,
        Action::Connect {
            share, password, ..
        } => {
            let result = shares.lock().await.connect(&share, &password).await;
            // `password` drops here; the hub is the only holder afterwards.
            match result {
                Ok(()) => {
                    let root = app
                        .share_config(&share)
                        .map(|config| config.root.clone())
                        .unwrap_or_default();
                    app.push_log(format!("Connected to {share}"));
                    app.open_share_picker(&share, &root);
                    if let Some(source) = app.pending_share_list() {
                        list_and_apply(app, store.as_ref(), &source).await;
                    }
                }
                Err(e) => {
                    app.push_log(format!("Cannot connect to {share}: {e}"));
                    app.share_connection_failed(&share);
                }
            }
        }
        Action::List(source) => {
            list_and_apply(app, store.as_ref(), &source).await;
        }
        Action::Add(source) => {
            let display = source.to_string();
            match app.files.add_source(store.as_ref(), &source).await {
                Ok(outcome) => app.log_add_outcome(&display, &outcome),
                Err(e) => app.log_add_failure(&display, &e),
            }
        }
        Action::AddShare { config } => {
            let name = config.name.clone();
            // The hub serves the share in the same run; the app lists it with
            // the highlight on it; the config file is written from the list.
            shares.lock().await.add_config(config.clone());
            app.share_added(config);
            match app.config_hint.clone() {
                Some(path) => match config::save_shares(&app.shares, &path) {
                    Ok(()) => {
                        app.push_log(format!("Added share {name} (saved to {})", path.display()));
                    }
                    Err(e) => {
                        app.push_log(format!("Added share {name}"));
                        app.push_log(format!("Warning: cannot save {}: {e}", path.display()));
                    }
                },
                None => {
                    app.push_log(format!("Added share {name}"));
                    app.push_log("Warning: cannot save: no config file location known".into());
                }
            }
        }
    }
    false
}

/// Run a share directory listing and hand it over to the open picker.
async fn list_and_apply(
    app: &mut App,
    store: &dyn dbi_backend_core::source::Sources,
    source: &Source,
) {
    match store.list_dir(source).await {
        Ok(entries) => app.apply_share_listing(source, entries),
        Err(e) => app.share_listing_failed(source, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::Mode;
    use dbi_backend_core::source::smb::ShareHub;
    use std::path::PathBuf;

    /// A factory the add flow never calls: adding a share must not attempt a
    /// connection or ask for a password.
    struct NothingConnects;

    #[async_trait::async_trait]
    impl dbi_backend_core::source::smb::SessionFactory for NothingConnects {
        async fn create(
            &self,
            _setup: &dbi_backend_core::source::smb::ShareSetup,
        ) -> Result<
            Box<dyn dbi_backend_core::source::smb::ShareSessionLike>,
            dbi_backend_core::source::SourceError,
        > {
            Err(dbi_backend_core::source::SourceError::new(
                "adding must not connect",
            ))
        }
    }

    /// An app whose records the form is about to extend.
    fn app_for_add(config_dir: Option<PathBuf>) -> (App, Arc<tokio::sync::Mutex<ShareHub>>) {
        let mut app = App::new(FileList::new());
        app.config_hint = config_dir;
        let hub = Arc::new(tokio::sync::Mutex::new(ShareHub::new(
            Arc::new(NothingConnects),
            Vec::new(),
        )));
        (app, hub)
    }

    /// Adding never touches files, so any store satisfies `run_action` here.
    fn noop_store() -> Arc<dyn dbi_backend_core::source::Sources> {
        Arc::new(dbi_backend_core::source::local::LocalStore::new())
    }

    fn form_submission(name: &str, host: &str) -> Action {
        Action::AddShare {
            config: config::ShareConfig {
                name: name.into(),
                host: host.into(),
                port: 445,
                share: "games".into(),
                username: "me".into(),
                domain: None,
                root: String::new(),
            },
        }
    }

    #[tokio::test]
    async fn adding_registers_lists_highlights_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config.toml");
        let (mut app, hub) = app_for_add(Some(config_file.clone()));

        run_action(
            form_submission("Late", "nas.example"),
            &mut app,
            &hub,
            noop_store(),
            &mpsc::unbounded_channel().0,
            &mpsc::unbounded_channel().0,
        )
        .await;

        // The hub serves the added share in the same run.
        let names: Vec<String> = hub
            .lock()
            .await
            .configs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, vec!["Late"]);
        // The app list includes the share with the highlight on it.
        assert_eq!(
            app.shares
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
            vec!["Late"]
        );
        match &app.mode {
            Mode::Shares(list) => assert_eq!(list.selected, 0, "the added share is highlighted"),
            _ => panic!("the share list comes back after adding"),
        }
        // The form does not connect: the factory was never called and no
        // password ever appeared.
        assert!(app
            .log
            .iter()
            .all(|line| !line.message.contains("password")));
        // The config file gains the entry with no password anywhere in it.
        let contents = std::fs::read_to_string(&config_file).unwrap();
        assert!(contents.contains("[[shares]]"));
        assert!(contents.contains("name = \"Late\""));
        assert!(contents.contains("username = \"me\""));
        assert!(!contents.to_lowercase().contains("password"));
        assert!(
            app.log
                .iter()
                .any(|line| line.message.contains("Added share Late")),
            "the outcome is logged: {:?}",
            app.log
        );
    }

    #[tokio::test]
    async fn a_failed_save_warns_and_keeps_the_share_for_the_run() {
        // A file where the save needs a directory: writing refuses.
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let blocked = blocker.join("config.toml");
        let (mut app, hub) = app_for_add(Some(blocked.clone()));

        run_action(
            form_submission("Late", "nas.example"),
            &mut app,
            &hub,
            noop_store(),
            &mpsc::unbounded_channel().0,
            &mpsc::unbounded_channel().0,
        )
        .await;

        let names: Vec<String> = hub
            .lock()
            .await
            .configs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, vec!["Late"], "the share is registered");
        assert_eq!(
            app.shares
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
            vec!["Late"],
            "the share stays listed"
        );
        assert!(matches!(app.mode, Mode::Shares(_)));
        assert!(
            std::fs::read_to_string(&blocked).is_err(),
            "no file was written"
        );
        assert!(
            app.log
                .iter()
                .any(|line| line.message.starts_with("Warning: cannot save")),
            "the failure warns: {:?}",
            app.log
        );
    }

    #[tokio::test]
    async fn adding_without_a_config_path_warns_and_keeps_the_share() {
        let (mut app, hub) = app_for_add(None);

        run_action(
            form_submission("Late", "nas.example"),
            &mut app,
            &hub,
            noop_store(),
            &mpsc::unbounded_channel().0,
            &mpsc::unbounded_channel().0,
        )
        .await;

        assert_eq!(
            app.shares
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
            vec!["Late"]
        );
        assert!(
            app.log
                .iter()
                .any(|line| line.message.starts_with("Warning: cannot save")),
            "the missing location warns: {:?}",
            app.log
        );
    }
}
