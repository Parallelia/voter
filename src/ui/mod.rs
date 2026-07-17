pub mod election_detail;
pub mod election_list;
pub mod help;
pub mod password;
pub mod results;
pub mod settings;
pub mod vote;
pub mod welcome;
pub mod widgets;

use ratatui::Frame;

use crate::app::{App, Screen};

/// Render the current screen based on app state.
pub fn render(app: &App, frame: &mut Frame) {
    match &app.screen {
        Screen::Welcome => welcome::render(app, frame),
        Screen::PasswordPrompt => password::render(app, frame),
        Screen::ElectionList => election_list::render(app, frame),
        Screen::ElectionDetail { election_id } => {
            election_detail::render(app, frame, election_id);
        }
        Screen::Vote { election_id } => vote::render(app, frame, election_id),
        Screen::Results { election_id } => results::render(app, frame, election_id),
        Screen::Settings => settings::render(app, frame),
    }

    // Render help overlay
    if app.show_help {
        help::render_overlay(frame);
    }

    // Status bar at the bottom
    widgets::status_bar::render(app, frame);
}

#[cfg(test)]
pub mod test_support {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::app::App;
    use voter::config::AppConfig;
    use voter::crypto::token::VotingToken;
    use voter::nostr::events::{Candidate, Election, ElectionStatus};
    use voter::state::{AppState, VoterRegistration};

    /// Build an App with default config/state and a dangling action channel.
    ///
    /// Persistence is redirected into the returned tempdir so any
    /// save_state()-triggering test can never write to the user's real config
    /// dir; keep the guard bound (e.g. `_dir`) so it lives for the test.
    pub fn test_app() -> (App, tempfile::TempDir) {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(AppConfig::default(), AppState::default(), tx);
        let dir = tempfile::tempdir().expect("create tempdir");
        app.set_state_path(dir.path().join("state.json"));
        (app, dir)
    }

    /// Draw a single frame with the given render closure on a test backend.
    pub fn render_to_terminal<F>(width: u16, height: u16, mut render_fn: F) -> Terminal<TestBackend>
    where
        F: FnMut(&mut ratatui::Frame),
    {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render_fn(frame)).unwrap();
        terminal
    }

    /// Flatten the rendered buffer into a newline-separated string.
    pub fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let width = buffer.area.width as usize;
        let mut text = String::new();
        for (i, cell) in buffer.content().iter().enumerate() {
            text.push_str(cell.symbol());
            if (i + 1) % width == 0 {
                text.push('\n');
            }
        }
        text
    }

    /// Render and return the buffer contents as text in one step.
    pub fn render_to_text<F>(width: u16, height: u16, render_fn: F) -> String
    where
        F: FnMut(&mut ratatui::Frame),
    {
        buffer_text(&render_to_terminal(width, height, render_fn))
    }

    /// An election with three candidates: Alice (1), Bob (2), Carol (3).
    pub fn sample_election(
        election_id: &str,
        name: &str,
        status: ElectionStatus,
        rules_id: &str,
    ) -> Election {
        Election {
            election_id: election_id.to_string(),
            name: name.to_string(),
            start_time: 1_752_000_000,
            end_time: 1_752_086_400,
            status,
            rules_id: rules_id.to_string(),
            rsa_pub_key: "dW51c2Vk".to_string(),
            candidates: vec![
                Candidate {
                    id: 1,
                    name: "Alice".to_string(),
                },
                Candidate {
                    id: 2,
                    name: "Bob".to_string(),
                },
                Candidate {
                    id: 3,
                    name: "Carol".to_string(),
                },
            ],
            ec_pubkey: None,
        }
    }

    pub fn registration() -> VoterRegistration {
        VoterRegistration {
            registered: true,
            registered_at: "1752000000".to_string(),
        }
    }

    pub fn voting_token(consumed: bool) -> VotingToken {
        VotingToken {
            nonce_b64: "bm9uY2U=".to_string(),
            h_n: "ab".repeat(32),
            signature_b64: "c2ln".to_string(),
            randomizer_b64: None,
            consumed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{render_to_text, test_app};
    use crate::app::Screen;

    #[test]
    fn render_dispatches_to_welcome_screen_with_status_bar() {
        // Arrange
        let (app, _dir) = test_app();

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert
        assert!(text.contains("Welcome to Parallelia Voter"));
        assert!(text.contains("Setup"));
    }

    #[test]
    fn render_dispatches_to_password_prompt_screen() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::PasswordPrompt;

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert: prompt is clipped by the modal width at 80x24
        assert!(text.contains("Enter password to unlock"));
        assert!(text.contains("Unlock"));
    }

    #[test]
    fn render_dispatches_to_election_list_screen() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::ElectionList;

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert
        assert!(text.contains("No elections found. Waiting for announcements..."));
        assert!(text.contains("Elections"));
    }

    #[test]
    fn render_dispatches_to_election_detail_screen() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::ElectionDetail {
            election_id: "missing".to_string(),
        };

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert
        assert!(text.contains("Election not found"));
        assert!(text.contains("Detail"));
    }

    #[test]
    fn render_dispatches_to_vote_screen() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::Vote {
            election_id: "missing".to_string(),
        };

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert: unknown election renders nothing but the status bar
        assert!(text.contains("Vote"));
    }

    #[test]
    fn render_dispatches_to_results_screen() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::Results {
            election_id: "missing".to_string(),
        };

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert
        assert!(text.contains("Results: Unknown"));
        assert!(text.contains("Results not yet available."));
    }

    #[test]
    fn render_dispatches_to_settings_screen() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::Settings;

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert
        assert!(text.contains("Settings"));
        assert!(text.contains("Nostr Relays"));
    }

    #[test]
    fn render_draws_help_overlay_when_toggled_on() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::ElectionList;
        app.show_help = true;

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert
        assert!(text.contains("Keyboard Shortcuts"));
    }

    #[test]
    fn render_always_shows_connection_indicator_in_status_bar() {
        // Arrange
        let (app, _dir) = test_app();

        // Act
        let text = render_to_text(80, 24, |f| super::render(&app, f));

        // Assert: disconnected by default
        assert!(text.contains("○"));
    }
}
