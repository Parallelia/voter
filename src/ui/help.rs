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
