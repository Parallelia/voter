use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Overlay help on top of the current screen.
pub fn render_overlay(frame: &mut Frame) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);
    render_help_content(frame, area);
}

fn render_help_content(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Keyboard Shortcuts ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            "Global",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        shortcut("q", "Quit"),
        shortcut("?", "Toggle help"),
        shortcut("Esc", "Go back / cancel"),
        Line::default(),
        Line::from(Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        shortcut("j / ↓", "Move down"),
        shortcut("k / ↑", "Move up"),
        shortcut("Enter", "Select / confirm"),
        Line::default(),
        Line::from(Span::styled(
            "Election List",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        shortcut("s", "Open settings"),
        Line::default(),
        Line::from(Span::styled(
            "Election Detail",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        shortcut("Enter", "Enter registration token"),
        shortcut("t", "Request voting token"),
        shortcut("v", "Cast vote (if token available)"),
        shortcut("r", "View results"),
        Line::default(),
        Line::from(Span::styled(
            "Voting (STV)",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        shortcut("Enter/Space", "Add candidate to ranking"),
        shortcut("d", "Remove from ranking"),
        shortcut("s", "Submit vote (with confirmation)"),
    ];

    let help = Paragraph::new(lines);
    frame.render_widget(help, inner);
}

fn shortcut<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{key:>12}"), Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::raw(desc),
    ])
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
    use super::render_overlay;
    use crate::ui::test_support::render_to_text;

    #[test]
    fn overlay_lists_all_section_headers_and_shortcuts() {
        // Arrange / Act: tall terminal so every help line fits in the overlay
        let text = render_to_text(100, 40, render_overlay);

        // Assert: sections
        assert!(text.contains("Keyboard Shortcuts"));
        assert!(text.contains("Global"));
        assert!(text.contains("Navigation"));
        assert!(text.contains("Election List"));
        assert!(text.contains("Election Detail"));
        assert!(text.contains("Voting (STV)"));
        // Assert: representative shortcuts
        assert!(text.contains("Quit"));
        assert!(text.contains("Toggle help"));
        assert!(text.contains("Go back / cancel"));
        assert!(text.contains("Move down"));
        assert!(text.contains("Move up"));
        assert!(text.contains("Open settings"));
        assert!(text.contains("Enter registration token"));
        assert!(text.contains("Request voting token"));
        assert!(text.contains("Cast vote (if token available)"));
        assert!(text.contains("View results"));
        assert!(text.contains("Add candidate to ranking"));
        assert!(text.contains("Remove from ranking"));
        assert!(text.contains("Submit vote (with confirmation)"));
    }
}
