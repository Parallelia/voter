mod app;
mod ui;

use std::io;

use crossterm::event::{Event, EventStream};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use nostr_sdk::prelude::Keys;
use tracing::{info, warn};

use app::{Action, App, Screen, ShouldQuit};
use voter::config::{self, AppConfig};
use voter::identity;
use voter::nostr::client::{NostrAction, NostrVoterClient, VoterCommand};
use voter::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for debug builds — writes to a log file to avoid TUI interference.
    // Never logs sensitive data (tokens, nonces, keys).
    if cfg!(debug_assertions) {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
        let log_path = config::config_dir().join("voter.log");
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::File::create(&log_path) {
            Ok(log_file) => {
                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_writer(log_file)
                    .with_ansi(false)
                    .init();
            }
            Err(_) => {
                // Fallback to stderr if log file cannot be created
                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_writer(std::io::stderr)
                    .with_ansi(false)
                    .init();
            }
        }
    }

    // Load configuration
    let config_path = config::config_path();
    let config = AppConfig::load(&config_path)?;
    info!(config_path = %config_path.display(), "configuration loaded");
    let persistent_state = AppState::load(&config.state_path())?;

    // Create action channel
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();

    // Determine initial screen
    let identity_path = &config.identity.path.clone();
    let initial_screen = if identity::identity_exists(identity_path) {
        if identity::identity_is_encrypted(identity_path) {
            Screen::PasswordPrompt
        } else {
            // Load identity directly
            Screen::ElectionList
        }
    } else {
        Screen::Welcome
    };

    let mut app = App::new(config, persistent_state, action_tx.clone());
    app.screen = initial_screen.clone();
    info!(screen = ?initial_screen, "starting application");

    // If we can load identity directly, do it
    if matches!(initial_screen, Screen::ElectionList) {
        match identity::load_identity(None, identity_path) {
            Ok(keys) => app.keys = Some(keys),
            Err(_) => app.screen = Screen::Welcome,
        }
    }

    // Connect to Nostr relays if identity is available
    let mut nostr_task: Option<tokio::task::JoinHandle<()>> = None;
    if let Some(ref keys) = app.keys {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<VoterCommand>();
        app.cmd_tx = Some(cmd_tx);
        let nostr_tx = action_tx.clone();
        let nostr_keys = keys.clone();
        let nostr_config = app.config.clone();
        nostr_task = Some(tokio::spawn(async move {
            connect_nostr(&nostr_keys, &nostr_config, nostr_tx, cmd_rx).await;
        }));
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stderr();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Spawn crossterm event reader
    let tx = action_tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            match event {
                Event::Key(key) => {
                    let _ = tx.send(Action::KeyPress(key.code));
                }
                Event::Resize(_, _) => {
                    let _ = tx.send(Action::Resize);
                }
                _ => {}
            }
        }
    });

    // Main loop
    loop {
        terminal.draw(|f| ui::render(&app, f))?;

        if let Some(action) = action_rx.recv().await {
            // On identity creation/unlock, connect to Nostr relays
            let should_connect = matches!(
                action,
                Action::IdentityCreated(_) | Action::IdentityUnlocked
            );

            if app.update(action) == ShouldQuit::Yes {
                break;
            }

            if should_connect && let Some(ref keys) = app.keys {
                if let Some(handle) = nostr_task.take() {
                    handle.abort();
                }
                let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<VoterCommand>();
                app.cmd_tx = Some(cmd_tx);
                let nostr_tx = app.action_tx.clone();
                let nostr_keys = keys.clone();
                let nostr_config = app.config.clone();
                nostr_task = Some(tokio::spawn(async move {
                    connect_nostr(&nostr_keys, &nostr_config, nostr_tx, cmd_rx).await;
                }));
            }
        }
    }

    // Cleanup
    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Save state
    let state_path = app.state_path.clone();
    app.persistent_state.save(&state_path)?;

    Ok(())
}

/// Connect to Nostr relays with exponential backoff reconnection.
async fn connect_nostr(
    keys: &Keys,
    config: &AppConfig,
    action_tx: mpsc::UnboundedSender<Action>,
    mut cmd_rx: mpsc::UnboundedReceiver<VoterCommand>,
) {
    let mut backoff_secs = 1u64;
    let max_backoff = 60u64;

    loop {
        info!("connecting to Nostr relays");
        match NostrVoterClient::connect(keys, config).await {
            Ok(client) => {
                backoff_secs = 1;
                let _ = action_tx.send(Action::Nostr(NostrAction::ConnectionStatus(true)));

                if let Err(e) = client.subscribe().await {
                    warn!(error = %e, "subscription failed, will reconnect");
                    let _ = action_tx.send(Action::Nostr(NostrAction::Error(format!(
                        "Subscription failed: {e}"
                    ))));
                    client.disconnect().await;
                    let _ = action_tx.send(Action::Nostr(NostrAction::ConnectionStatus(false)));
                } else {
                    // Bridge NostrAction channel into main Action channel
                    let (nostr_tx, mut nostr_rx) = mpsc::unbounded_channel::<NostrAction>();
                    let bridge_tx = action_tx.clone();
                    tokio::spawn(async move {
                        while let Some(nostr_action) = nostr_rx.recv().await {
                            if bridge_tx.send(Action::Nostr(nostr_action)).is_err() {
                                break;
                            }
                        }
                    });

                    // Listen blocks until disconnection; process app commands
                    // (register / request-token / cast-vote) concurrently.
                    tokio::select! {
                        res = client.listen(nostr_tx.clone()) => {
                            if let Err(e) = res {
                                warn!(error = %e, "listener disconnected");
                            }
                        }
                        _ = client.process_commands(&mut cmd_rx, nostr_tx.clone(), config) => {}
                    }

                    client.disconnect().await;
                    let _ = action_tx.send(Action::Nostr(NostrAction::ConnectionStatus(false)));
                }
            }
            Err(e) => {
                warn!(error = %e, backoff_secs, "connection failed, retrying");
                let _ = action_tx.send(Action::Nostr(NostrAction::Error(format!(
                    "Relay connection failed: {e}"
                ))));
            }
        }

        // Exponential backoff before reconnect
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}
