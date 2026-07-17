use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame) {
    let area = centered_rect(50, 30, frame.area());

    let block = Block::default()
        .title(" Unlock Identity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .split(inner);

    let prompt = Paragraph::new("Enter password to unlock your identity:");
    frame.render_widget(prompt, chunks[0]);

    let masked: String = "*".repeat(app.password_input.len());
    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::styled(masked, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("█", Style::default().fg(Color::Gray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(input, chunks[1]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" to submit"),
    ]));
    frame.render_widget(hint, chunks[2]);

    if let Some(ref err) = app.error_message {
        let error = Paragraph::new(Span::styled(err.as_str(), Style::default().fg(Color::Red)));
        frame.render_widget(error, chunks[3]);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::ui::test_support::{render_to_text, test_app};

    // The dialog is a 50%x30% centered rect: at 80x24 the input row is
    // squeezed out, so these tests render on a 100x40 terminal.

    #[test]
    fn renders_prompt_with_empty_input() {
        // Arrange
        let (app, _dir) = test_app();

        // Act
        let text = render_to_text(100, 40, |f| render(&app, f));

        // Assert
        assert!(text.contains("Unlock Identity"));
        assert!(text.contains("Enter password to unlock your identity:"));
        assert!(text.contains("Enter to submit"));
        assert!(text.contains("█"));
        assert!(!text.contains('*'));
    }

    #[test]
    fn masks_typed_password_with_asterisks() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.password_input = "secret".to_string();

        // Act
        let text = render_to_text(100, 40, |f| render(&app, f));

        // Assert: one asterisk per character, plaintext never shown
        assert!(text.contains("******"));
        assert!(!text.contains("secret"));
    }

    #[test]
    fn renders_error_message_when_unlock_fails() {
        // Arrange
        let (mut app, _dir) = test_app();
        app.error_message = Some("Unlock failed: bad password".to_string());

        // Act
        let text = render_to_text(100, 40, |f| render(&app, f));

        // Assert
        assert!(text.contains("Unlock failed: bad password"));
    }
}
