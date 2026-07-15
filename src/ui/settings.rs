use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(8),
        Constraint::Length(4),
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    // Title
    let title = Paragraph::new(Line::from(Span::styled(
        " Settings ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Relays
    let relay_items: Vec<ListItem> = app
        .config
        .nostr
        .relays
        .iter()
        .map(|r| ListItem::new(format!("  {r}")))
        .collect();

    let relays = List::new(relay_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Nostr Relays "),
    );
    frame.render_widget(relays, chunks[1]);

    // Theme
    let theme_text = format!("  Current: {:?}", app.config.ui.theme);
    let theme =
        Paragraph::new(theme_text).block(Block::default().borders(Borders::ALL).title(" Theme "));
    frame.render_widget(theme, chunks[2]);

    // Identity
    let pubkey = app
        .keys
        .as_ref()
        .map(crate::identity::export_public_key)
        .unwrap_or_else(|| "Not loaded".to_string());
    let identity_text = format!("  Public key: {}...", &pubkey[..16.min(pubkey.len())]);
    let identity = Paragraph::new(identity_text)
        .block(Block::default().borders(Borders::ALL).title(" Identity "));
    frame.render_widget(identity, chunks[3]);

    // Key hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" back  "),
        Span::styled("?", Style::default().fg(Color::Yellow)),
        Span::raw(" help"),
    ]));
    frame.render_widget(hints, chunks[5]);
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::ui::test_support::{render_to_text, test_app};

    #[test]
    fn renders_relays_theme_and_placeholder_identity() {
        // Arrange
        let app = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("Settings"));
        assert!(text.contains("Nostr Relays"));
        assert!(text.contains("wss://relay.mostro.network"));
        assert!(text.contains("wss://nos.lol"));
        assert!(text.contains("Current: Dark"));
        assert!(text.contains("Public key: Not loaded..."));
    }

    #[test]
    fn renders_truncated_public_key_when_identity_is_loaded() {
        // Arrange
        let mut app = test_app();
        let keys = voter::identity::generate_keypair();
        let expected_prefix = voter::identity::export_public_key(&keys)[..16].to_string();
        app.keys = Some(keys);

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains(&format!("Public key: {expected_prefix}...")));
        assert!(!text.contains("Not loaded"));
    }
}
