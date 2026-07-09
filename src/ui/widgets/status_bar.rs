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
