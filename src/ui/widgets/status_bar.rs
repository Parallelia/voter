use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Screen};

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    // Status bar occupies the very last line
    let bar_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };

    let screen_name = match &app.screen {
        Screen::Welcome => "Setup",
        Screen::PasswordPrompt => "Unlock",
        Screen::ElectionList => "Elections",
        Screen::ElectionDetail { .. } => "Detail",
        Screen::Vote { .. } => "Vote",
        Screen::Results { .. } => "Results",
        Screen::Settings => "Settings",
    };

    let connection_color = if app.connected {
        Color::Green
    } else {
        Color::Red
    };
    let connection_text = if app.connected { "●" } else { "○" };

    // The tracing warning about a missing EC pin is invisible in release
    // builds (tracing is only initialized in debug); the status bar is the
    // one place the user is guaranteed to see it.
    let unpinned = app.config.nostr.ec_pubkey.is_none();

    let chunks = Layout::horizontal([
        Constraint::Length(3),
        Constraint::Length(if unpinned { 12 } else { 0 }),
        Constraint::Min(0),
        Constraint::Length(20),
    ])
    .split(bar_area);

    // Connection indicator
    let conn = Paragraph::new(Span::styled(
        connection_text,
        Style::default().fg(connection_color),
    ));
    frame.render_widget(conn, chunks[0]);

    if unpinned {
        let warn = Paragraph::new(Span::styled(
            "UNPINNED ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(warn, chunks[1]);
    }

    // Status message or screen name
    let status_text = app.status_message.as_deref().unwrap_or(screen_name);
    let status = Paragraph::new(Span::styled(
        status_text,
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(status, chunks[2]);

    // Screen name on the right
    let right = Paragraph::new(Line::from(vec![Span::styled(
        screen_name,
        Style::default().fg(Color::DarkGray),
    )]))
    .right_aligned();
    frame.render_widget(right, chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::app::Screen;
    use crate::ui::test_support::{render_to_text, test_app};

    #[test]
    fn shows_filled_dot_when_connected() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.connected = true;

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("●"));
        assert!(!text.contains("○"));
    }

    #[test]
    fn shows_empty_dot_when_disconnected() {
        // Arrange
        let (app, _dir) = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("○"));
        assert!(!text.contains("●"));
    }

    #[test]
    fn warns_unpinned_when_ec_pubkey_is_not_configured() {
        // Arrange: default config has no pinned EC pubkey
        let (app, _dir) = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("UNPINNED"));
    }

    #[test]
    fn omits_unpinned_warning_when_ec_pubkey_is_pinned() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.config.nostr.ec_pubkey = Some("deadbeef".to_string());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(!text.contains("UNPINNED"));
    }

    #[test]
    fn status_message_overrides_screen_name_in_middle_section() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.screen = Screen::ElectionList;
        app.status_message = Some("Connected to relays".to_string());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert: message in the middle, screen name still on the right
        assert!(text.contains("Connected to relays"));
        assert!(text.contains("Elections"));
    }

    #[test]
    fn shows_screen_name_for_every_screen_variant() {
        // Arrange
        let cases = [
            (Screen::Welcome, "Setup"),
            (Screen::PasswordPrompt, "Unlock"),
            (Screen::ElectionList, "Elections"),
            (
                Screen::ElectionDetail {
                    election_id: "e1".to_string(),
                },
                "Detail",
            ),
            (
                Screen::Vote {
                    election_id: "e1".to_string(),
                },
                "Vote",
            ),
            (
                Screen::Results {
                    election_id: "e1".to_string(),
                },
                "Results",
            ),
            (Screen::Settings, "Settings"),
        ];

        for (screen, expected) in cases {
            let (mut app, _dir) = test_app();
            app.screen = screen;

            // Act
            let text = render_to_text(80, 24, |f| render(&app, f));

            // Assert
            assert!(text.contains(expected), "missing screen name: {expected}");
        }
    }
}
