use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame) {
    let area = centered_rect(60, 50, frame.area());

    let block = Block::default()
        .title(" Parallelia Voter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(0),
    ])
    .split(inner);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Welcome to Parallelia Voter",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]))
    .centered();
    frame.render_widget(title, chunks[0]);

    let subtitle = Paragraph::new("Set up your voting identity to get started.").centered();
    frame.render_widget(subtitle, chunks[1]);

    let opt1 = Paragraph::new(Line::from(vec![
        Span::styled("[g] ", Style::default().fg(Color::Yellow)),
        Span::raw("Generate new identity"),
    ]));
    frame.render_widget(opt1, chunks[2]);

    let opt2 = Paragraph::new(Line::from(vec![
        Span::styled("[i] ", Style::default().fg(Color::Yellow)),
        Span::raw("Import existing identity"),
    ]));
    frame.render_widget(opt2, chunks[3]);

    if let Some(ref err) = app.error_message {
        let error = Paragraph::new(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(error, chunks[4]);
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

    #[test]
    fn renders_title_and_identity_options() {
        // Arrange
        let app = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("Parallelia Voter"));
        assert!(text.contains("Welcome to Parallelia Voter"));
        assert!(text.contains("Set up your voting identity to get started."));
        assert!(text.contains("[g]"));
        assert!(text.contains("Generate new identity"));
        assert!(text.contains("[i]"));
        assert!(text.contains("Import existing identity"));
    }

    #[test]
    fn renders_error_message_when_present() {
        // Arrange
        let mut app = test_app();
        app.error_message = Some("Failed to save identity".to_string());

        // Act: taller terminal so the error chunk gets rows
        let text = render_to_text(80, 40, |f| render(&app, f));

        // Assert
        assert!(text.contains("Failed to save identity"));
    }

    #[test]
    fn omits_error_line_when_no_error_is_set() {
        // Arrange
        let app = test_app();

        // Act
        let text = render_to_text(80, 40, |f| render(&app, f));

        // Assert
        assert!(!text.contains("Failed"));
    }
}
